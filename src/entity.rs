use std::collections::HashMap;
use std::iter::Map;
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};

#[derive(Deserialize, Debug)]
pub struct UploadParams {
    pub msg_id: i32,
    pub chat_id: i64,
    pub posted_at: DateTime<Utc>,
    pub collection:String,
}

#[derive(Serialize)]
pub struct UploadResponse {
    pub msg_id: i32,
    pub chat_id: i64,
    pub posted_at: DateTime<Utc>,
    pub collection: String,
}

#[derive(Serialize, Deserialize)]
pub struct SearchResult {
    pub msg_id: i32,
    pub chat_id: i64,
    pub posted_at: DateTime<Utc>,
    pub similarity: f32,
    pub display_name: String,
    pub bias: Option<String>,
    pub user_name: Option<String>,
    pub invite_hash: Option<String>,
    pub tags: Vec<String>,
    pub img: String,
}

#[derive(Serialize  )]
pub struct MetadataResponse {
    pub datasets: HashMap<String,u64>,
}



#[derive(Deserialize, Debug)]
pub struct TextSearchParams {
    pub tags: Vec<String>,
    pub limit: Option<u64>,
    pub posted_before: Option<DateTime<Utc>>,
    pub posted_after: Option<DateTime<Utc>>,
    pub collection: String,
}

#[derive(Deserialize, Debug)]
pub struct ImageSearchParams {
    pub limit: Option<u64>,
    pub posted_before: Option<DateTime<Utc>>,
    pub posted_after: Option<DateTime<Utc>>,
    pub collection: String,
}