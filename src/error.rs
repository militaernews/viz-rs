use axum::Json;
use axum::response::IntoResponse;
use qdrant_client::QdrantError;
use serde_json::json;
use thiserror::Error;

#[derive(Error, Debug)]
pub enum AppError {
    #[error("Database error: {0}")]
    DatabaseError(#[from] QdrantError),

    #[error("Image processing error: {0}")]
    ImageProcessingError(#[from] image::ImageError),

    #[error("Model loading error: {0}")]
    ModelLoadingError(#[from] tch::TchError),

    #[error("Nested error: {0}")]
    NestedError(#[from] anyhow::Error),

    #[error("Multipart field error: {0}")]
    MultipartError(#[from] axum::extract::multipart::MultipartError),

    #[error("Incorrect Metadata provided: {0}")]
    MetadataError(#[from] serde_json::Error),


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
            _ => http::StatusCode::INTERNAL_SERVER_ERROR
        };
        (status_code, Json(json!({ "error": self.to_string() }))).into_response()
    }
}
