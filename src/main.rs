
use std::env::var;
use axum::{
    extract::{Multipart, Query, State},
    routing::post,
    response::{Json, IntoResponse},
    Router,
};
use anyhow::{anyhow, Result};
use image::DynamicImage;
use pgvector::Vector;
use serde::{Deserialize, Serialize};
use sqlx::postgres::PgPoolOptions;
use sqlx::{PgPool, Pool};
use sqlx::Postgres;
use std::net::SocketAddr;
use axum::extract::FromRef;
use tch::{nn, nn::Module, vision::resnet, Device, Kind, Tensor};
use chrono::{DateTime, Utc};
use dotenvy::dotenv;
use tch::nn::{ModuleT, VarStore};
use tch::vision::imagenet;
use tch::vision::resnet::{resnet18, resnet50};
use tokio::net::TcpListener;
use tracing::{log::info, error};
use thiserror::Error;

#[derive(Error, Debug)]
pub enum AppError {
    #[error("Database error: {0}")]
    DatabaseError(#[from] sqlx::Error),

    #[error("Image processing error: {0}")]
    ImageProcessingError(#[from] image::ImageError),

    #[error("Model loading error: {0}")]
    ModelLoadingError(#[from] tch::TchError),

    #[error("Multipart field error: {0}")]
    MultipartError(#[from] axum::extract::multipart::MultipartError),

    #[error("No image uploaded")]
    NoImageUploaded,

    #[error("Unknown error occurred")]
    Unknown,
}

impl IntoResponse for AppError {
    fn into_response(self) -> axum::response::Response {
        let status_code = match &self {
            AppError::DatabaseError(_) => http::StatusCode::INTERNAL_SERVER_ERROR,
            AppError::ImageProcessingError(_) => http::StatusCode::BAD_REQUEST,
            AppError::ModelLoadingError(_) => http::StatusCode::INTERNAL_SERVER_ERROR,
            AppError::MultipartError(_) => http::StatusCode::BAD_REQUEST,
            AppError::NoImageUploaded => http::StatusCode::BAD_REQUEST,
            AppError::Unknown => http::StatusCode::INTERNAL_SERVER_ERROR,
        };
        (status_code, Json(serde_json::json!({ "error": self.to_string() }))).into_response()
    }
}

#[derive(Clone, FromRef)]
pub struct AppState {
    pub db_pool: PgPool,
}

#[derive(Deserialize)]
struct UploadParams {
    msg_id: String,
    chat_id: String,
    timestamp: Option<DateTime<Utc>>,
}

#[derive(Serialize)]
struct UploadResponse {
    msg_id: String,
    chat_id: String,
    timestamp: DateTime<Utc>,
}

#[derive(Serialize)]
struct SearchResult {
    msg_id: String,
    chat_id: String,
    distance: f32,
}

// Extract Image Features
fn extract_features(img: DynamicImage) -> Result<Vec<f32>, AppError> {
    let mut vs = VarStore::new(Device::cuda_if_available());
    let model = resnet50(&vs.root(), 1000);

    // Attempt to load the model
  vs.load("D:\\dev\\tools\\resnet50.safetensors").map_err(AppError::ModelLoadingError)?;

    let img = img.resize_exact(224, 224, image::imageops::FilterType::CatmullRom);
    let rgb = img.to_rgb8();
    let (width, height) = rgb.dimensions();

    // Convert image to tensor
    let img_tensor = Tensor::from_data_size(
        &rgb.as_flat_samples().samples,
        &[1, 3, height as i64, width as i64],
        Kind::Uint8,
    )
        .to_kind(Kind::Float) / 255.0;

    // Normalize the tensor (ImageNet normalization)
    let mean = Tensor::from_slice(&[0.485, 0.456, 0.406]).view([1, 3, 1, 1]);
    let std = Tensor::from_slice(&[0.229, 0.224, 0.225]).view([1, 3, 1, 1]);
    let img_tensor = (img_tensor - mean) / std;

    // Apply the forward pass of the model to get the output
    let output = img_tensor.apply_t(&model, false);

    // Here we extract the features from the second-to-last layer, before the final classification head
    let features = output.flatten(1, 1); // Flatten the output into a 1D feature vector

    let mut vec_f32: Vec<f32> = vec![0.0; features.numel()];
    features.copy_data(&mut vec_f32, features.numel());

    Ok(vec_f32)




/*
    let tensor = Tensor::try_from(&img)?;
    let vs = nn::VarStore::new(Device::cuda_if_available());
    let resnet = resnet::resnet50_no_final_layer(&vs.root());
    vs.load("path/to/resnet50_weights.ot")?;

    // Preprocess the image tensor
    let image_tensor = tensor.unsqueeze(0).to_device(vs.device()); // Add batch dimension and move to device

    // Extract features
    let features = resnet.forward_t(&image_tensor, false);
*/


}

// Upload or Update Image in Database
async fn upload_image(
    mut multipart: Multipart,
    State(db_pool): State<PgPool>,
    Query(params): Query<UploadParams>,
) -> Result<Json<UploadResponse>, AppError> {
    let timestamp = params.timestamp.unwrap_or(Utc::now());

    while let Some(field) = multipart.next_field().await.map_err(AppError::MultipartError)? {
        if let Some(filename) = field.file_name() {
            println!("Received file: {}", filename);

            let data = field.bytes().await.map_err(AppError::MultipartError)?;
            let img = image::load_from_memory(&data).map_err(AppError::ImageProcessingError)?;
            let features = extract_features(img)?;

            // Insert or Update Image in DB
            sqlx::query!(
                "INSERT INTO images (msg_id, chat_id, vector, timestamp)
                 VALUES ($1, $2, $3, $4)
                 ON CONFLICT (msg_id, chat_id)
                 DO UPDATE SET vector = EXCLUDED.vector, timestamp = EXCLUDED.timestamp",
                params.msg_id,
                params.chat_id,
                &features as &[f32],
                timestamp
            )
                .execute(&db_pool)
                .await
                .map_err(AppError::DatabaseError)?;

            return Ok(Json(UploadResponse {
                msg_id: params.msg_id,
                chat_id: params.chat_id,
                timestamp,
            }));
        }
    }

    Err(AppError::NoImageUploaded)
}

// Search for Similar Images
async fn search_similar_images(
    State(db_pool): State<PgPool>,
    mut multipart: Multipart,
) -> Result<Json<Vec<SearchResult>>, AppError> {

    while let Some(mut field) = multipart.next_field().await.map_err(AppError::MultipartError)? {
        let name = field.name().unwrap().to_string();
        let data = field.bytes().await.map_err(AppError::MultipartError)?;

        println!("Length of `{}` is {} bytes", name, data.len());

        let img = image::load_from_memory(&data).map_err(AppError::ImageProcessingError)?;
        let features = extract_features(img)?;

        // Query DB for similar images
        let similar_images = find_similar_images(&db_pool, features, 16).await?;

        return Ok(Json(similar_images));
    }

    Ok(Json(vec![])) // Return empty if no image found
}

// Find Similar Images in Database
async fn find_similar_images(
    db_pool: &PgPool,
    query_vector: Vec<f32>,
    limit: i64,
) -> Result<Vec<SearchResult>, sqlx::Error> {
    let rows = sqlx::query!(
        "SELECT msg_id, chat_id, vector <=> $1 AS distance FROM images ORDER BY distance LIMIT $2",
        &query_vector as &[f32],
        limit
    )
        .fetch_all(db_pool)
        .await?;

    Ok(rows
        .iter()
        .map(|row| SearchResult {
            msg_id: row.msg_id.clone(),
            chat_id: row.chat_id.clone(),
            distance: row.distance.unwrap_or(0.0) as f32,
        })
        .collect())
}

// Set Up Axum Server
#[tokio::main]
async fn main() -> Result<(), AppError> {

    dotenv().ok();

    let database_url = var("DATABASE_URL").expect("Database Url has to be provided");

    // Initialize Database Pool
    let db_pool = PgPoolOptions::new()
        .max_connections(5)
        .connect(&*database_url)
        .await
        .map_err(AppError::DatabaseError)?;

    let state = AppState { db_pool };

    let app = Router::new()
        .route("/search", post(search_similar_images)) // Search Images
        .with_state(state);

    let addr = SocketAddr::from(([127, 0, 0, 1], 3000));
    let listener = TcpListener::bind(&addr)
        .await
        .map_err(|e| AppError::Unknown)?;

    info!("Server running on {}", addr);
    axum::serve(listener, app).await.map_err(|e| AppError::Unknown)?;

    Ok(())
}
