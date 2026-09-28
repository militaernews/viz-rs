use anyhow::Result;
use viz_core::config::{optional, parsed_or, required};

pub struct Config {
    pub bind_addr: String,
    pub allowed_origin: String,
    pub api_key: String,
    pub max_upload_bytes: usize,
    pub database_url: String,
    pub db_max_connections: u32,
    pub qdrant_url: String,
    pub qdrant_collection: String,
    pub public_base_url: String,
    pub clip_vision_model_path: String,
    pub clip_text_model_path: String,
    pub clip_tokenizer_path: String,
    pub ffmpeg_bin: String,
    pub frame_extract_timeout_secs: u64,
}

impl Config {
    pub fn from_env() -> Result<Self> {
        Ok(Self {
            bind_addr: optional("API_BIND_ADDR").unwrap_or_else(|| "0.0.0.0:8080".into()),
            allowed_origin: required("API_ALLOWED_ORIGIN")?,
            api_key: required("BACKEND_API_KEY")?,
            max_upload_bytes: parsed_or("API_MAX_UPLOAD_BYTES", 25 * 1024 * 1024)?,
            database_url: required("DATABASE_URL")?,
            db_max_connections: parsed_or("API_DB_MAX_CONNECTIONS", 10)?,
            qdrant_url: required("QDRANT_URL")?,
            qdrant_collection: required("QDRANT_COLLECTION")?,
            public_base_url: required("S3_PUBLIC_BASE_URL")?,
            clip_vision_model_path: required("CLIP_VISION_MODEL_PATH")?,
            clip_text_model_path: required("CLIP_TEXT_MODEL_PATH")?,
            clip_tokenizer_path: required("CLIP_TOKENIZER_PATH")?,
            ffmpeg_bin: optional("FFMPEG_BIN").unwrap_or_else(|| "ffmpeg".into()),
            frame_extract_timeout_secs: parsed_or("FRAME_EXTRACT_TIMEOUT_SECS", 60)?,
        })
    }
}
