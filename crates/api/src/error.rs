use axum::Json;
use axum::http::StatusCode;
use axum::response::{IntoResponse, Response};
use serde_json::json;

#[derive(Debug)]
pub enum ApiError {
    BadRequest(String),
    BadImage,
    PayloadTooLarge,
    Unauthorized,
    RateLimited,
    /// The real error is logged server-side; the client only ever sees "internal error".
    Internal,
}

impl ApiError {
    pub fn internal<E: std::fmt::Display>(e: E) -> Self {
        tracing::error!(error = %e, "internal error");
        ApiError::Internal
    }

    /// Invalid media is the client's fault; anything else is ours.
    pub fn from_media(e: anyhow::Error) -> Self {
        if viz_core::error::is_invalid_media(&e) {
            tracing::debug!(error = %e, "rejected upload");
            ApiError::BadImage
        } else {
            ApiError::internal(format!("{e:#}"))
        }
    }
}

impl From<anyhow::Error> for ApiError {
    fn from(e: anyhow::Error) -> Self {
        ApiError::internal(format!("{e:#}"))
    }
}

impl From<tokio::task::JoinError> for ApiError {
    fn from(e: tokio::task::JoinError) -> Self {
        ApiError::internal(e)
    }
}

impl IntoResponse for ApiError {
    fn into_response(self) -> Response {
        let (status, msg) = match self {
            ApiError::BadRequest(m) => (StatusCode::BAD_REQUEST, m),
            ApiError::BadImage => (StatusCode::UNPROCESSABLE_ENTITY, "could not decode media".into()),
            ApiError::PayloadTooLarge => (StatusCode::PAYLOAD_TOO_LARGE, "upload too large".into()),
            ApiError::Unauthorized => (StatusCode::UNAUTHORIZED, "unauthorized".into()),
            ApiError::RateLimited => (StatusCode::TOO_MANY_REQUESTS, "slow down".into()),
            ApiError::Internal => (StatusCode::INTERNAL_SERVER_ERROR, "internal error".into()),
        };
        (status, Json(json!({ "error": msg }))).into_response()
    }
}
