use std::collections::HashMap;
use std::iter::Map;
use crate::database::{insert_images, search_vectors, search_by_tags, create_collection};
use crate::embedding::extract_features;
use crate::entity::{MetadataResponse, SearchResult, UploadParams, UploadResponse, TextSearchParams, ImageSearchParams};
use crate::AppError;
use anyhow::Result;
use axum::extract::{FromRef, Multipart, State};
use axum::routing::get;
use axum::routing::post;
use axum::{Json, Router};
use http::{header, HeaderValue, Method};
use log::{info, debug, warn};
use qdrant_client::Qdrant;
use sqlx::{PgPool, Postgres};
use std::net::SocketAddr;
use std::sync::Arc;
use base64::Engine;
use base64::engine::general_purpose::STANDARD;
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

        match name.as_str() {
            "image" => {
                image_data = Some(data.to_vec());
            }
            "params" => {
                search_params = serde_json::from_slice(&data)
                    .map_err(AppError::MetadataError)?;
            }
            _ => {
                warn!("Unknown field in multipart data: {}", name);
            }
        }
    }

    if let Some(data) = image_data {
        let features = extract_features(&data)?;
        debug!("Extracted {} features from image", features.len());

        let params = search_params.unwrap(); // return error

        let similar_images = search_vectors(
            &state.qdrant,
            &state.pg_pool,
            features,
            params.limit.unwrap_or(36),
            params.posted_before,
            params.posted_after,
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

pub async fn serve(qdrant: Qdrant, pg_pool: PgPool) -> Result<(), AppError> {
    let state = AppState {
        qdrant: Arc::from(qdrant),
        pg_pool,
    };

    let origins = [
        "http://localhost:3011".parse().unwrap(),
        "http://localhost:5173".parse().unwrap(),
        "http://rnimu-2003-d2-6f0f-5d7f-2cdc-89a-6491-94ff.a.free.pinggy.link"
            .parse()
            .unwrap(),
    ];

    let cors = CorsLayer::new()
        // Allow requests from your frontend origin
        .allow_origin(origins)
        .allow_methods([Method::POST])
        // Allow the Content-Type header for multipart form data
        .allow_headers([header::CONTENT_TYPE])
        .allow_credentials(true);

    let app = Router::new()
        .route("/", get(root))
        .route("/search/images", post(search_similar_images))
        .route("/search/tags", post(search_by_text_tags))
        .route("/meta", get(get_metadata))
        .route("/upload", post(upload_image))
        .with_state(state)
        .layer(TraceLayer::new_for_http())
        .layer(cors);

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