use anyhow::Result;
use sqlx::PgPool;
use std::sync::Arc;
use std::time::Duration;
use tokio::sync::RwLock;
use viz_core::embed::{ClipTextEmbedder, ClipVisionEmbedder};
use viz_core::hash::BkTree;
use viz_core::storage::ThumbStore;
use viz_core::vector::VectorIndex;

use crate::config::Config;

const HASH_INDEX_REFRESH: Duration = Duration::from_secs(5 * 60);

pub struct AppState {
    pub pool: PgPool,
    pub vectors: VectorIndex,
    pub hash_index: RwLock<BkTree>,
    pub vision: Arc<ClipVisionEmbedder>,
    pub text: Arc<ClipTextEmbedder>,
    pub public_base_url: String,
    pub api_key: String,
    pub ffmpeg_bin: String,
    pub frame_extract_timeout_secs: u64,
    pub thumbs: Option<ThumbStore>,
}

impl AppState {
    pub async fn init(cfg: &Config) -> Result<Self> {
        // Own pool, separate from ingest's, so search load can't starve ingest writes.
        let pool = viz_core::db::connect(&cfg.database_url, cfg.db_max_connections).await?;
        let vectors = VectorIndex::connect(&cfg.qdrant_url, &cfg.qdrant_collection)?;
        let tree = build_hash_index(&pool).await?;

        let (vision_path, text_path, tokenizer_path) =
            (cfg.clip_vision_model_path.clone(), cfg.clip_text_model_path.clone(), cfg.clip_tokenizer_path.clone());
        let (vision, text) = tokio::task::spawn_blocking(move || -> Result<_> {
            Ok((ClipVisionEmbedder::load(&vision_path)?, ClipTextEmbedder::load(&text_path, &tokenizer_path)?))
        })
        .await??;
        vectors.ensure_collection(vision.dim()).await?;
        let thumbs = match &cfg.thumbs {
            Some(settings) => Some(ThumbStore::new(settings).await?),
            None => None,
        };

        Ok(Self {
            pool,
            vectors,
            hash_index: RwLock::new(tree),
            vision: Arc::new(vision),
            text: Arc::new(text),
            public_base_url: cfg.public_base_url.clone(),
            api_key: cfg.api_key.clone(),
            ffmpeg_bin: cfg.ffmpeg_bin.clone(),
            frame_extract_timeout_secs: cfg.frame_extract_timeout_secs,
            thumbs,
        })
    }
}

async fn build_hash_index(pool: &PgPool) -> Result<BkTree> {
    let rows = viz_core::db::fetch_all_dhashes(pool).await?;
    Ok(tokio::task::spawn_blocking(move || {
        let mut tree = BkTree::new();
        for (item_id, dhash) in rows {
            tree.insert(dhash, item_id);
        }
        tree
    })
    .await?)
}

/// Rebuilds the BK-tree periodically so newly ingested items become searchable without a
/// restart. The tree is built off-lock and swapped in, so searches only wait for the swap.
pub fn spawn_hash_index_refresh(state: Arc<AppState>) {
    tokio::spawn(async move {
        let mut interval = tokio::time::interval(HASH_INDEX_REFRESH);
        interval.tick().await;
        loop {
            interval.tick().await;
            match build_hash_index(&state.pool).await {
                Ok(tree) => {
                    let size = tree.len();
                    *state.hash_index.write().await = tree;
                    tracing::info!(items = size, "hash index refreshed");
                }
                Err(e) => {
                    tracing::warn!(error = %format!("{e:#}"), "hash index refresh failed; keeping the old one")
                }
            }
        }
    });
}
