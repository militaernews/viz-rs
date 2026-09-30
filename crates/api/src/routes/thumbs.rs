use axum::extract::{Path, State};
use axum::http::header;
use axum::response::{IntoResponse, Response};
use std::sync::Arc;
use viz_core::storage::ThumbStore;

use crate::error::ApiError;
use crate::state::AppState;

/// Streams a thumbnail out of the bucket, for deployments where the bucket isn't public.
/// Keys are content-addressed, so responses can be cached forever.
pub async fn get_thumb(State(state): State<Arc<AppState>>, Path(key): Path<String>) -> Result<Response, ApiError> {
    let Some(store) = &state.thumbs else {
        return Err(ApiError::NotFound);
    };
    if !ThumbStore::is_thumb_key(&key) {
        return Err(ApiError::NotFound);
    }
    let bytes = store.get(&key).await?.ok_or(ApiError::NotFound)?;
    Ok(([(header::CONTENT_TYPE, "image/webp"), (header::CACHE_CONTROL, "public, max-age=31536000, immutable")], bytes)
        .into_response())
}
