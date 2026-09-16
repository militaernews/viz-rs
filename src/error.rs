use axum::http::StatusCode;
use axum::response::{IntoResponse, Response};
use axum::Json;
use serde_json::json;
use thiserror::Error;

#[derive(Debug, Error)]
pub enum AppError {
    #[error("Vector database error: {0}")]
    VectorDatabaseError(#[from] qdrant_client::QdrantError),

    #[error("Telegram database error: {0}")]
    TelegramDatabaseError(#[from] sqlx::Error),

    #[error("Metadata parsing error: {0}")]
    MetadataError(#[from] serde_json::Error),

    #[error("Multipart form error: {0}")]
    MultipartError(#[from] axum::extract::multipart::MultipartError),

    #[error("No image was uploaded")]
    NoImageUploaded,

    #[error("Collection '{0}' not found")]
    CollectionNotFound(String),

    #[error("Unknown error occurred")]
    Unknown,
}

impl IntoResponse for AppError {
    fn into_response(self) -> Response {
        let (status, error_message): (StatusCode, String) = match self {
            AppError::VectorDatabaseError(_) => (StatusCode::INTERNAL_SERVER_ERROR, "Vector database error".parse().unwrap()),
            AppError::TelegramDatabaseError(_) => (StatusCode::INTERNAL_SERVER_ERROR, "Database error".parse().unwrap()),
            AppError::MetadataError(_) => (StatusCode::BAD_REQUEST, "Invalid metadata format".parse().unwrap()),
            AppError::MultipartError(_) => (StatusCode::BAD_REQUEST, "Invalid multipart data".parse().unwrap()),
            AppError::NoImageUploaded => (StatusCode::BAD_REQUEST, "No image uploaded".parse().unwrap()),
            AppError::CollectionNotFound(ref name) => (StatusCode::NOT_FOUND, format!("Collection '{}' not found", name)),
            AppError::Unknown => (StatusCode::INTERNAL_SERVER_ERROR, "Internal server error".parse().unwrap()),
        };

        let body = Json(json!({
            "error": error_message,
            "details": self.to_string()
        }));

        (status, body).into_response()
    }
}

// Manual From implementations for errors that should map to Unknown. The API
// response only ever says "Internal server error" (by design, to avoid
// leaking internals to callers) but that previously meant the real cause was
// discarded entirely, including server-side in logs - making failures like
// "every backfilled image fails to extract features" undiagnosable. Log it.
impl From<tch::TchError> for AppError {
    fn from(err: tch::TchError) -> Self {
        log::error!("tch error: {err}");
        AppError::Unknown
    }
}

impl From<anyhow::Error> for AppError {
    fn from(err: anyhow::Error) -> Self {
        log::error!("error: {err:?}");
        AppError::Unknown
    }
}