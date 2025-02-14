use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};

#[derive(Deserialize, Debug)]
pub struct UploadParams {
    pub(crate) msg_id: i32,
    pub(crate) chat_id: i64,
    pub(crate) posted_at: DateTime<Utc>,
}

#[derive(Serialize)]
pub struct UploadResponse {
    pub(crate) msg_id: i32,
    pub(crate) chat_id: i64,
    pub(crate) posted_at: DateTime<Utc>,
}

#[derive(Serialize,Deserialize)]
pub struct SearchResult {
    pub(crate) msg_id: i32,
    pub(crate) chat_id: i64,
    pub(crate) posted_at: DateTime<Utc>,
    pub(crate) similarity: f32,
}
