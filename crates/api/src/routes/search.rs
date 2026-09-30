use axum::Json;
use axum::extract::{Multipart, Query, State};
use serde::Deserialize;
use std::sync::Arc;
use viz_core::types::SearchHit;
use viz_core::{db, ffmpeg, hash, media};

use super::{HashParams, MAX_RESULTS, Ranked, add_vector_hits, build_search_hits, read_file_field};
use crate::error::ApiError;
use crate::state::AppState;

/// Frames taken from an uploaded video, lower than ingest's cap to bound request latency.
const MAX_QUERY_FRAMES: usize = 20;
const MAX_QUERY_CHARS: usize = 200;
const FTS_CANDIDATES: i64 = 20;

pub async fn search_image(
    State(state): State<Arc<AppState>>,
    Query(params): Query<HashParams>,
    multipart: Multipart,
) -> Result<Json<Vec<SearchHit>>, ApiError> {
    let bytes = read_file_field(multipart).await?;
    if !media::is_image(&bytes) {
        return Err(ApiError::BadImage);
    }
    let (img, dhash) = tokio::task::spawn_blocking(move || {
        let img = media::decode_image(&bytes)?;
        let dhash = hash::dhash(&img);
        Ok::<_, anyhow::Error>((img, dhash))
    })
    .await?
    .map_err(ApiError::from_media)?;

    let mut ranked = Ranked::default();
    for (item_id, distance) in state.hash_index.read().await.find(dhash, params.max_distance()) {
        ranked.add(item_id, hash::similarity(distance));
    }
    // Near-duplicates win outright; semantic neighbours only when nothing matched by hash.
    if ranked.is_empty() {
        let vision = Arc::clone(&state.vision);
        let vector = tokio::task::spawn_blocking(move || vision.embed(&img)).await??;
        add_vector_hits(&state, vector, &mut ranked).await?;
    }
    Ok(Json(build_search_hits(&state, &ranked.into_sorted()).await?))
}

pub async fn search_video(
    State(state): State<Arc<AppState>>,
    Query(params): Query<HashParams>,
    multipart: Multipart,
) -> Result<Json<Vec<SearchHit>>, ApiError> {
    let bytes = read_file_field(multipart).await?;
    if !media::is_video(&bytes) {
        return Err(ApiError::BadImage);
    }
    // Per-request temp dir, removed on drop (also when the request times out).
    let dir = tempfile::tempdir().map_err(ApiError::internal)?;
    let path = dir.path().join("upload");
    tokio::fs::write(&path, &bytes).await.map_err(ApiError::internal)?;
    drop(bytes);

    let frames =
        ffmpeg::extract_scene_frames(&state.ffmpeg_bin, &path, state.frame_extract_timeout_secs, MAX_QUERY_FRAMES)
            .await
            .map_err(ApiError::from_media)?;
    let frames = tokio::task::spawn_blocking(move || {
        frames
            .into_iter()
            .filter_map(|(_, png)| media::decode_image(&png).ok())
            .map(|img| {
                let dhash = hash::dhash(&img);
                (img, dhash)
            })
            .collect::<Vec<_>>()
    })
    .await?;
    if frames.is_empty() {
        return Err(ApiError::BadImage);
    }

    let mut ranked = Ranked::default();
    {
        let index = state.hash_index.read().await;
        for (_, dhash) in &frames {
            for (item_id, distance) in index.find(*dhash, params.max_distance()) {
                ranked.add(item_id, hash::similarity(distance));
            }
        }
    }
    if ranked.is_empty() {
        let vision = Arc::clone(&state.vision);
        let vectors = tokio::task::spawn_blocking(move || {
            frames.iter().map(|(img, _)| vision.embed(img)).collect::<anyhow::Result<Vec<_>>>()
        })
        .await??;
        for vector in vectors {
            add_vector_hits(&state, vector, &mut ranked).await?;
        }
    }
    Ok(Json(build_search_hits(&state, &ranked.into_sorted()).await?))
}

#[derive(Debug, Deserialize)]
pub struct TextQuery {
    pub q: String,
}

pub async fn search_text(
    State(state): State<Arc<AppState>>,
    Json(query): Json<TextQuery>,
) -> Result<Json<Vec<SearchHit>>, ApiError> {
    let q = query.q.trim().to_string();
    if q.is_empty() || q.chars().count() > MAX_QUERY_CHARS {
        return Err(ApiError::BadRequest(format!("query must be 1-{MAX_QUERY_CHARS} characters")));
    }

    let text = Arc::clone(&state.text);
    let clip_query = q.clone();
    let (fts_ids, vector) = tokio::try_join!(
        async { Ok::<_, ApiError>(db::search_caption_fts(&state.pool, &q, FTS_CANDIDATES).await?) },
        async { Ok(tokio::task::spawn_blocking(move || text.embed_text(&clip_query)).await??) },
    )?;

    let mut clip = Ranked::default();
    add_vector_hits(&state, vector, &mut clip).await?;

    // Caption/OCR matches rank above CLIP neighbours, whose cosine scores stay below 1.0.
    let mut merged: Vec<(i64, f32)> = fts_ids.iter().map(|id| (*id, 1.0)).collect();
    merged.extend(clip.into_sorted().into_iter().filter(|(id, _)| !fts_ids.contains(id)));
    merged.truncate(MAX_RESULTS);
    Ok(Json(build_search_hits(&state, &merged).await?))
}
