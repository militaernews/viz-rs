use crate::database::{insert_images, search_vectors};
use crate::embedding::extract_features;
use crate::entity::{SearchResult, UploadParams, UploadResponse};
use crate::AppError;
use anyhow::Result;
use axum::extract::{FromRef, Multipart, State};
use axum::routing::get;
use axum::routing::post;
use axum::{Json, Router};
use http::{header, HeaderValue, Method};
use log::info;
use qdrant_client::Qdrant;
use sqlx::{PgPool, Postgres};
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
            vectors = Some(extract_features(&*data)?);
        }
    }

    println!("Done receiving: {:?}", metadata);

    if let (Some(metadata), Some(vectors)) = (metadata, vectors) {
        insert_images(&state.qdrant, &metadata, vectors).await?;

        Ok(Json(UploadResponse {
            msg_id: metadata.msg_id,
            chat_id: metadata.chat_id,
            posted_at: metadata.posted_at,
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
    while let Some(field) = multipart
        .next_field()
        .await
        .map_err(AppError::MultipartError)?
    {
        let name = field.name().unwrap().to_string();
        let data = field.bytes().await.map_err(AppError::MultipartError)?;

        println!("Length of `{}` is {} bytes", name, data.len());

        let features = extract_features(&*data)?;

        println!("Extracted features `{:?}`", features);


        // Query DB for similar images
        let similar_images = search_vectors(&state.qdrant, &state.pg_pool, features, 36).await?;

        return Ok(Json(similar_images));
    }

    Ok(Json(vec![])) // Return empty if no image found
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
        .route("/search", post(search_similar_images)) // Search Images
        .route("/upload", post(upload_image)) // Search Images

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
