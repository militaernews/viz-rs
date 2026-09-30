use anyhow::{Context, Result, bail};
use grammers_client::Client;
use grammers_client::media::Media;
use sqlx::PgPool;
use std::sync::Arc;
use tokio::sync::{Mutex, mpsc};
use tokio::task::JoinHandle;
use uuid::Uuid;
use viz_core::embed::ClipVisionEmbedder;
use viz_core::ocr::Ocr;
use viz_core::storage::{ThumbStore, encode_thumbnail};
use viz_core::types::{MediaKind, NewMediaItem, NewSighting};
use viz_core::vector::VectorIndex;
use viz_core::{db, ffmpeg, hash, media};

use crate::telegram::{MediaClass, PendingMedia};

/// The Telegram-independent half of ingest: turns image bytes into a searchable item.
pub struct Indexer {
    pub pool: PgPool,
    pub vision: Arc<ClipVisionEmbedder>,
    pub vectors: VectorIndex,
    pub thumbs: ThumbStore,
    pub ocr: Ocr,
}

pub struct PipelineCtx {
    pub indexer: Indexer,
    pub client: Client,
    pub ffmpeg_bin: String,
    pub frame_timeout_secs: u64,
    pub max_media_bytes: usize,
}

pub fn spawn_workers(ctx: Arc<PipelineCtx>, rx: mpsc::Receiver<PendingMedia>, count: usize) -> Vec<JoinHandle<()>> {
    let rx = Arc::new(Mutex::new(rx));
    (0..count)
        .map(|_| {
            let (ctx, rx) = (Arc::clone(&ctx), Arc::clone(&rx));
            tokio::spawn(async move {
                loop {
                    let next = rx.lock().await.recv().await;
                    match next {
                        Some(item) => handle(&ctx, item).await,
                        None => break,
                    }
                }
            })
        })
        .collect()
}

/// Processes one message and records the outcome; never fails, so one bad file can't stop a worker.
pub async fn handle(ctx: &PipelineCtx, item: PendingMedia) {
    let (channel_id, message_id) = (item.channel_id, item.message_id);
    match process_one(ctx, item).await {
        Ok(indexed) => {
            if indexed > 0 {
                tracing::info!(channel_id, message_id, visuals = indexed, "indexed message");
            }
            if let Err(e) = db::clear_failure(&ctx.indexer.pool, channel_id, message_id).await {
                tracing::warn!(channel_id, message_id, error = %e, "clearing failure record failed");
            }
        }
        Err(e) => {
            let error = format!("{e:#}");
            tracing::error!(channel_id, message_id, %error, "processing media failed");
            if let Err(e) = db::record_failure(&ctx.indexer.pool, channel_id, message_id, &error).await {
                tracing::error!(channel_id, message_id, error = %e, "recording failure failed");
            }
        }
    }
}

/// Returns the number of visuals (photo or video frames) recorded for the message.
pub async fn process_one(ctx: &PipelineCtx, item: PendingMedia) -> Result<usize> {
    if db::sighting_exists(&ctx.indexer.pool, item.channel_id, item.message_id).await? {
        return Ok(0);
    }
    if let Some(size) = media_size(&item.media).filter(|&size| size > ctx.max_media_bytes) {
        bail!("media is {size} bytes, above INGEST_MAX_MEDIA_BYTES={}", ctx.max_media_bytes);
    }

    // Temp dir is removed on drop, whatever happens below.
    let dir = tempfile::tempdir()?;
    let path = dir.path().join("media");
    ctx.client.download_media(&item.media, &path).await.context("downloading media")?;

    let visuals: Vec<(Option<i64>, Vec<u8>)> = match item.class {
        MediaClass::Video => {
            ffmpeg::extract_scene_frames(&ctx.ffmpeg_bin, &path, ctx.frame_timeout_secs, ffmpeg::MAX_FRAMES_PER_VIDEO)
                .await?
                .into_iter()
                .map(|(offset_ms, png)| (Some(offset_ms as i64), png))
                .collect()
        }
        MediaClass::Photo => vec![(None, tokio::fs::read(&path).await?)],
    };

    let mut sightings = Vec::with_capacity(visuals.len());
    for (frame_offset_ms, bytes) in visuals {
        let media_item_id = ctx.indexer.index_visual(item.caption.clone(), frame_offset_ms, bytes).await?;
        sightings.push(NewSighting {
            media_item_id,
            channel_id: item.channel_id,
            channel_username: item.channel_username.clone(),
            message_id: item.message_id,
            is_backup_channel: item.is_backup,
            posted_at: item.posted_at,
        });
    }
    db::insert_sightings(&ctx.indexer.pool, &sightings).await?;
    Ok(sightings.len())
}

impl Indexer {
    /// Hash, dedup-check, embed, thumbnail and persist one photo or frame; returns its item id.
    pub async fn index_visual(
        &self,
        caption: Option<String>,
        frame_offset_ms: Option<i64>,
        bytes: Vec<u8>,
    ) -> Result<i64> {
        let (bytes, img, dhash) = tokio::task::spawn_blocking(move || -> Result<_> {
            let img = media::decode_image(&bytes)?;
            let dhash = hash::dhash(&img);
            Ok((bytes, img, dhash))
        })
        .await??;

        if let Some(existing) = db::find_item_by_dhash(&self.pool, dhash, frame_offset_ms).await? {
            return Ok(existing);
        }

        let vision = Arc::clone(&self.vision);
        let (embedding, thumbnail) =
            tokio::task::spawn_blocking(move || -> Result<_> { Ok((vision.embed(&img)?, encode_thumbnail(&img)?)) })
                .await??;

        let ocr_text = match self.ocr.recognize(&bytes).await {
            Ok(text) => text,
            Err(e) => {
                tracing::debug!(error = %e, "OCR failed; continuing without text");
                None
            }
        };

        let new_item = NewMediaItem {
            dhash,
            embedding_id: Uuid::new_v4(),
            thumb_key: ThumbStore::key_for_hash(dhash),
            caption,
            ocr_text,
            media_kind: if frame_offset_ms.is_some() { MediaKind::VideoFrame } else { MediaKind::Photo },
            frame_offset_ms,
        };

        // The row only becomes visible once the vector and thumbnail exist.
        let mut tx = self.pool.begin().await?;
        match db::insert_media_item(&mut tx, &new_item).await? {
            Some(id) => {
                self.vectors.upsert(new_item.embedding_id, embedding, dhash).await?;
                self.thumbs.upload(&new_item.thumb_key, thumbnail).await?;
                tx.commit().await?;
                Ok(id)
            }
            None => {
                tx.rollback().await?;
                db::find_item_by_dhash(&self.pool, dhash, frame_offset_ms)
                    .await?
                    .context("media item disappeared after a dedup conflict")
            }
        }
    }
}

fn media_size(media: &Media) -> Option<usize> {
    match media {
        Media::Photo(photo) => photo.size(),
        Media::Document(doc) => doc.size(),
        _ => None,
    }
}
