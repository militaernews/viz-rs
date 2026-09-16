use crate::database::{create_collection, insert_images, search_by_tags, search_vectors};
use crate::embedding::extract_features;
use crate::entity::{ImageSearchParams, MetadataResponse, SearchResult, TextSearchParams, UploadParams, UploadResponse};
use crate::AppError;
use anyhow::Result;
use axum::extract::{FromRef, Multipart, Request, State};
use axum::middleware::{self, Next};
use axum::response::Response;
use axum::routing::get;
use axum::routing::post;
use axum::{Json, Router};
use base64::engine::general_purpose::STANDARD;
use base64::Engine;
use http::{header, HeaderValue, Method, StatusCode};
use log::{debug, info, warn};
use qdrant_client::Qdrant;
use sqlx::{PgPool, Postgres};
use std::collections::HashMap;
use std::env::var;
use std::iter::Map;
use std::net::SocketAddr;
use std::sync::Arc;
use tokio::net::TcpListener;
use tower_http::cors::CorsLayer;
use tower_http::trace::TraceLayer;

#[derive(Clone, FromRef)]
pub struct AppState {
    pub qdrant: Arc<Qdrant>,
    pub pg_pool: PgPool,
}

async fn upload_image(
    State(state): State<AppState>,
    mut multipart: Multipart,
) -> Result<Json<UploadResponse>, AppError> {
    let mut metadata: Option<UploadParams> = None;
    let mut vectors: Option<Vec<f32>> = None;
    let mut base64str:String=String::new();

    while let Some(field) = multipart
        .next_field()
        .await
        .map_err(AppError::MultipartError)?
    {
        let name = field.name().unwrap().to_string();
        println!("Received file: {name}");
        let data = field.bytes().await.map_err(AppError::MultipartError)?;

        if name == "meta" {
            metadata = serde_json::from_slice(&data).map_err(AppError::MetadataError)?;
            println!("Done receiving: {:?} - data: {:?}", metadata, data);
        } else if name == "image" {
            vectors = Some(extract_features(&data)?);
            base64str = STANDARD.encode(&data) ;
        }
    }

    println!("Done receiving: {:?}", metadata);

    if let (Some(metadata), Some(vectors)) = (metadata, vectors) {
        insert_images(&state.qdrant, &metadata, vectors, base64str).await?;

        Ok(Json(UploadResponse {
            msg_id: metadata.msg_id,
            chat_id: metadata.chat_id,
            posted_at: metadata.posted_at,
            collection:metadata.collection,
        }))
    } else {
        Err(AppError::NoImageUploaded)
    }
}

// Search for Similar Images
async fn search_similar_images(
    State(state): State<AppState>,
    mut multipart: Multipart,
) -> Result<Json<Vec<SearchResult>>, AppError> {
    let mut image_data: Option<Vec<u8>> = None;
    let mut search_params: Option<ImageSearchParams> = None;

    while let Some(field) = multipart
        .next_field()
        .await
        .map_err(AppError::MultipartError)?
    {
        let name = field.name().unwrap().to_string();
        let data = field.bytes().await.map_err(AppError::MultipartError)?;

        debug!("Received field '{}' with {} bytes", name, data.len());

        if(data.len()==0){
            return Err(AppError::NoImageUploaded);
        }

        match name.as_str() {
            "image" => {

                image_data = Some(data.to_vec());
            }
            "params" => {
                search_params = serde_json::from_slice(&data)
                    .map_err(AppError::MetadataError)?;
                dbg!(&search_params);
            }
            _ => {
                warn!("Unknown field in multipart data: {}", name);
            }
        }
    }

    if let Some(data) = image_data {
        let features = extract_features(&data)?;
        debug!("Extracted {} features from image", features.len());

        let params = search_params.unwrap(); // todo:return error

        let similar_images = search_vectors(
            &state.qdrant,
            &state.pg_pool,
            features,
            params.limit.unwrap_or(20),
            params.posted_after,
            params.posted_before,
            params.collection
        ).await?;

        info!("Found {} similar images", similar_images.len());
        return Ok(Json(similar_images));
    }

    warn!("No image data found in request");
    Ok(Json(vec![]))
}

// Search by text tags
async fn search_by_text_tags(
    State(state): State<AppState>,
    Json(search_params): Json<TextSearchParams>,
) -> Result<Json<Vec<SearchResult>>, AppError> {
    info!("Searching for tags: {:?}", search_params.tags);
    debug!("Search params: limit={:?}, start_from={:?}, start_after={:?}",
           search_params.limit, search_params.posted_before, search_params.posted_after);

    let similar_images = search_by_tags(
        &state.qdrant,
        &state.pg_pool,
        search_params.tags,
        search_params.limit.unwrap_or(36),
        search_params.posted_before,
        search_params.posted_after,
        search_params.collection
    ).await?;

    info!("Found {} images matching tags", similar_images.len());
    Ok(Json(similar_images))
}

// Create a new collection
async fn create_new_collection(
    State(state): State<AppState>,
    Json(collection_name): Json<String>,
) -> Result<Json<String>, AppError> {
    info!("Request to create collection: {}", collection_name);

    create_collection(&state.qdrant, &collection_name).await?;

    Ok(Json(format!("Collection '{}' created successfully", collection_name)))
}

const API_KEY_HEADER: &str = "x-api-key";

/// Byte-length-independent-timing comparison; avoids leaking the shared secret
/// through response-time differences.
fn constant_time_eq(a: &[u8], b: &[u8]) -> bool {
    if a.len() != b.len() {
        return false;
    }
    a.iter().zip(b.iter()).fold(0u8, |diff, (x, y)| diff | (x ^ y)) == 0
}

/// Only viz-sv's SvelteKit server (which holds BACKEND_API_KEY as a private env var)
/// is meant to reach this API directly - end-user browsers never see this URL or key.
async fn require_api_key(request: Request, next: Next) -> Result<Response, StatusCode> {
    let expected = var("BACKEND_API_KEY").map_err(|_| {
        log::error!("BACKEND_API_KEY is not set; refusing all requests");
        StatusCode::INTERNAL_SERVER_ERROR
    })?;

    let provided = request
        .headers()
        .get(API_KEY_HEADER)
        .and_then(|v| v.to_str().ok())
        .unwrap_or("");

    if constant_time_eq(provided.as_bytes(), expected.as_bytes()) {
        Ok(next.run(request).await)
    } else {
        Err(StatusCode::UNAUTHORIZED)
    }
}

/// Cross-origin browser access isn't needed now that only the SvelteKit backend calls
/// this API server-to-server, but CORS_ALLOWED_ORIGINS stays configurable for local
/// debugging (e.g. hitting the API directly from a browser during development).
fn build_cors_layer() -> CorsLayer {
    let configured = var("CORS_ALLOWED_ORIGINS").unwrap_or_default();
    let origins: Vec<HeaderValue> = configured
        .split(',')
        .map(str::trim)
        .filter(|s| !s.is_empty())
        .filter_map(|s| s.parse().ok())
        .collect();

    let mut cors = CorsLayer::new()
        .allow_methods([Method::GET, Method::POST])
        .allow_headers([header::CONTENT_TYPE, header::HeaderName::from_static(API_KEY_HEADER)])
        .allow_credentials(true);

    cors = if origins.is_empty() {
        // No origins configured: default to the local dev frontend ports only.
        cors.allow_origin([
            "http://localhost:3011".parse().unwrap(),
            "http://localhost:5173".parse().unwrap(),
        ])
    } else {
        cors.allow_origin(origins)
    };

    cors
}

pub async fn serve(qdrant: Qdrant, pg_pool: PgPool) -> Result<(), AppError> {
    let state = AppState {
        qdrant: Arc::from(qdrant),
        pg_pool,
    };

    let protected = Router::new()
        .route("/search/images", post(search_similar_images))
        .route("/search/tags", post(search_by_text_tags))
        .route("/meta", get(get_metadata))
        .route("/upload", post(upload_image))
        .route_layer(middleware::from_fn(require_api_key));

    let app = Router::new()
        .route("/", get(root))
        .merge(protected)
        .with_state(state)
        .layer(TraceLayer::new_for_http())
        .layer(build_cors_layer());

    println!("app: {app:?}");

    let addr = SocketAddr::from(([0,0,0,0], 3000));
    let listener = TcpListener::bind(&addr)
        .await
        .map_err(|e| {
            println!("listener {:?}", e);
            AppError::Unknown
        }
        )?;

    println!("Listening on: {}", addr);

    info!("Server running on {}", addr);
    axum::serve(listener, app)
        .await
        .map_err(|e| {
            println!("serve {:?}", e);
            AppError::Unknown
        }
        )?;

    Ok(())
}

async fn root() -> &'static str {
    println!("root get");
    "Functional call is root"
}

async fn get_metadata(State(state): State<AppState>) -> Result<Json<MetadataResponse>, AppError> {
    let collections = state.qdrant.list_collections().await?.collections;

    let mut collection_metadata = HashMap::new();

    for cd in collections {
        let collection = state.qdrant.collection_info(cd.name.clone()).await?;

        let points_count = collection
            .result
            .unwrap()
            .points_count
            .unwrap_or_default();

        collection_metadata.insert(cd.name.clone(), points_count);
    }

    Ok(Json(MetadataResponse {
        datasets: collection_metadata,
    }))
}