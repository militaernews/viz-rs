pub mod health;
pub mod search;

use axum::extract::Multipart;
use serde::Deserialize;
use std::collections::HashMap;
use viz_core::types::{Link, SearchHit};
use viz_core::{db, hash, storage};

use crate::error::ApiError;
use crate::state::AppState;

/// Upper bound on hits returned by any search.
pub const MAX_RESULTS: usize = 50;
/// Candidates fetched from Qdrant per query vector.
pub const VECTOR_CANDIDATES: u64 = 20;
const MAX_DISTANCE_CAP: u32 = 16;

#[derive(Debug, Deserialize)]
pub struct HashParams {
    /// Hamming radius for the perceptual hash match (default 8, capped at 16).
    pub max_distance: Option<u32>,
}

impl HashParams {
    pub fn max_distance(&self) -> u32 {
        self.max_distance.unwrap_or(hash::DEFAULT_MAX_DISTANCE).min(MAX_DISTANCE_CAP)
    }
}

/// Reads the multipart field named "file". The overall size is bounded by `DefaultBodyLimit`.
pub async fn read_file_field(mut multipart: Multipart) -> Result<Vec<u8>, ApiError> {
    while let Some(field) = multipart.next_field().await.map_err(multipart_error)? {
        if field.name() == Some("file") {
            let bytes = field.bytes().await.map_err(multipart_error)?;
            if bytes.is_empty() {
                return Err(ApiError::BadRequest("field \"file\" is empty".into()));
            }
            return Ok(bytes.to_vec());
        }
    }
    Err(ApiError::BadRequest("missing multipart field \"file\"".into()))
}

fn multipart_error(e: axum::extract::multipart::MultipartError) -> ApiError {
    if e.status() == axum::http::StatusCode::PAYLOAD_TOO_LARGE {
        ApiError::PayloadTooLarge
    } else {
        ApiError::BadRequest(format!("invalid multipart body: {}", e.body_text()))
    }
}

/// Keeps the best score per item, then orders by score (ties keep first-seen order).
#[derive(Default)]
pub struct Ranked {
    order: Vec<i64>,
    scores: HashMap<i64, f32>,
}

impl Ranked {
    pub fn add(&mut self, item_id: i64, score: f32) {
        match self.scores.get_mut(&item_id) {
            Some(best) => *best = best.max(score),
            None => {
                self.order.push(item_id);
                self.scores.insert(item_id, score);
            }
        }
    }

    pub fn is_empty(&self) -> bool {
        self.order.is_empty()
    }

    pub fn into_sorted(self) -> Vec<(i64, f32)> {
        let mut ranked: Vec<(i64, f32)> = self.order.iter().map(|id| (*id, self.scores[id])).collect();
        ranked.sort_by(|a, b| b.1.total_cmp(&a.1));
        ranked.truncate(MAX_RESULTS);
        ranked
    }
}

/// Adds the Qdrant neighbours of `vector` to `ranked`, scored by cosine similarity.
pub async fn add_vector_hits(state: &AppState, vector: Vec<f32>, ranked: &mut Ranked) -> Result<(), ApiError> {
    let neighbours = state.vectors.search(vector, VECTOR_CANDIDATES).await?;
    let ids: Vec<_> = neighbours.iter().map(|(id, _)| *id).collect();
    let item_ids = db::item_ids_by_embedding_ids(&state.pool, &ids).await?;
    for (embedding_id, score) in neighbours {
        // Vectors whose row was rolled back or deleted are simply skipped.
        if let Some(item_id) = item_ids.get(&embedding_id) {
            ranked.add(*item_id, score);
        }
    }
    Ok(())
}

pub async fn build_search_hits(state: &AppState, ranked: &[(i64, f32)]) -> Result<Vec<SearchHit>, ApiError> {
    let ids: Vec<i64> = ranked.iter().map(|(id, _)| *id).collect();
    let scores: HashMap<i64, f32> = ranked.iter().copied().collect();
    let rows = db::fetch_hits_by_item_ids(&state.pool, &ids).await?;
    Ok(rows
        .into_iter()
        .map(|(item, sightings)| SearchHit {
            thumb_url: storage::public_url(&state.public_base_url, &item.thumb_key),
            score: scores.get(&item.id).copied().unwrap_or_default(),
            // Sightings are oldest first, so these are the earliest posts on each side.
            origin: sightings.iter().find(|s| !s.is_backup_channel).map(Link::from),
            backup: sightings.iter().find(|s| s.is_backup_channel).map(Link::from),
        })
        .collect())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn ranked_keeps_best_score_and_sorts() {
        let mut ranked = Ranked::default();
        ranked.add(1, 0.5);
        ranked.add(2, 0.9);
        ranked.add(1, 0.95);
        ranked.add(3, 0.9);
        assert_eq!(ranked.into_sorted(), vec![(1, 0.95), (2, 0.9), (3, 0.9)]);
    }

    #[test]
    fn max_distance_is_capped() {
        assert_eq!(HashParams { max_distance: None }.max_distance(), hash::DEFAULT_MAX_DISTANCE);
        assert_eq!(HashParams { max_distance: Some(64) }.max_distance(), MAX_DISTANCE_CAP);
    }
}
