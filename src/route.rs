use std::net::SocketAddr;
use std::sync::Arc;
use axum::extract::{FromRef, Multipart, State};
use axum::{Json, Router};
use axum::routing::post;
use http::{header, HeaderValue, Method};
use log::info;
use qdrant_client::Qdrant;
use tokio::net::TcpListener;
use tower_http::cors::CorsLayer;
use crate::{AppError,};
use crate::database::{insert_images, search_vectors};
use crate::embedding::extract_features;
use crate::entity::{SearchResult, UploadParams, UploadResponse};



#[derive(Clone, FromRef)]
pub struct AppState {
    pub qdrant: Arc<Qdrant>,
}


async fn upload_image(
    State(state): State<AppState>,
    mut multipart: Multipart,
) -> anyhow::Result<Json<UploadResponse>, AppError> {
    let mut metadata: Option<UploadParams> = None;
    let mut vectors: Option<Vec<f32>> = None;

    while let Some(field) = multipart.next_field().await.map_err(AppError::MultipartError)? {
        let name = field.name().unwrap().to_string();
        println!("Received file: {}", name);
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

    }else{
        Err(AppError::NoImageUploaded)
    }


}



// Search for Similar Images
async fn search_similar_images(
    State(state): State<AppState>,
    mut multipart: Multipart,
) -> anyhow::Result<Json<Vec<SearchResult>>, AppError> {

    while let Some( field) = multipart.next_field().await.map_err(AppError::MultipartError)? {
        let name = field.name().unwrap().to_string();
        let data = field.bytes().await.map_err(AppError::MultipartError)?;

        println!("Length of `{}` is {} bytes", name, data.len());


        let features = extract_features(&*data)?;

        // Query DB for similar images
        let similar_images = search_vectors(&state.qdrant, features, 16).await?;

        return Ok(Json(similar_images));
    }

    Ok(Json(vec![])) // Return empty if no image found
}

pub async fn serve(qdrant:Qdrant)->Result<(),AppError> {
    let state = AppState {
        qdrant:Arc::from(qdrant)
    };

    let cors = CorsLayer::new()
        // Allow requests from your frontend origin
        .allow_origin("http://localhost:5173".parse::<HeaderValue>().unwrap())
        .allow_methods([Method::POST])
        // Allow the Content-Type header for multipart form data
        .allow_headers([header::CONTENT_TYPE])
        .allow_credentials(true);




    let app = Router::new()
        .route("/search", post(search_similar_images)) // Search Images
        .route("/upload", post(upload_image)) // Search Images
        .with_state(state)
        .layer(cors);

    let addr = SocketAddr::from(([127, 0, 0, 1], 3000));
    let listener = TcpListener::bind(&addr)
        .await
        .map_err(|e| AppError::Unknown)?;

    info!("Server running on {}", addr);
    axum::serve(listener, app).await.map_err(|e| AppError::Unknown)?;

    Ok(())
}