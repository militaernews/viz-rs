use crate::types::{MediaItem, MediaKind, NewMediaItem, NewSighting, Sighting, WatchedChannel};
use anyhow::{Context, Result, anyhow};
use chrono::{DateTime, Utc};
use sqlx::postgres::PgPoolOptions;
use sqlx::{FromRow, PgConnection, PgPool};
use std::collections::HashMap;
use uuid::Uuid;

pub static MIGRATOR: sqlx::migrate::Migrator = sqlx::migrate!("../../migrations");

pub async fn connect(database_url: &str, max_connections: u32) -> Result<PgPool> {
    let pool = PgPoolOptions::new()
        .max_connections(max_connections)
        .connect(database_url)
        .await
        .context("connecting to Postgres")?;
    MIGRATOR.run(&pool).await.context("running migrations")?;
    Ok(pool)
}

#[derive(FromRow)]
struct MediaItemRow {
    id: i64,
    dhash: i64,
    embedding_id: Uuid,
    thumb_key: String,
    caption: Option<String>,
    ocr_text: Option<String>,
    media_kind: String,
    frame_offset_ms: Option<i64>,
    created_at: DateTime<Utc>,
}

impl TryFrom<MediaItemRow> for MediaItem {
    type Error = anyhow::Error;

    fn try_from(row: MediaItemRow) -> Result<Self> {
        Ok(MediaItem {
            id: row.id,
            dhash: row.dhash,
            embedding_id: row.embedding_id,
            thumb_key: row.thumb_key,
            caption: row.caption,
            ocr_text: row.ocr_text,
            media_kind: MediaKind::parse(&row.media_kind)
                .ok_or_else(|| anyhow!("unknown media_kind {:?} on item {}", row.media_kind, row.id))?,
            frame_offset_ms: row.frame_offset_ms,
            created_at: row.created_at,
        })
    }
}

#[derive(FromRow)]
struct SightingRow {
    media_item_id: i64,
    channel_id: i64,
    channel_username: Option<String>,
    message_id: i64,
    is_backup_channel: bool,
    posted_at: DateTime<Utc>,
}

#[derive(FromRow)]
struct WatchedChannelRow {
    channel_id: i64,
    channel_username: Option<String>,
    is_backup: bool,
    is_private: bool,
    enabled: bool,
    last_message_id: i64,
}

pub async fn find_item_by_dhash(pool: &PgPool, dhash: i64, frame_offset_ms: Option<i64>) -> Result<Option<i64>> {
    Ok(sqlx::query_scalar("select id from media_items where dhash = $1 and frame_offset_ms is not distinct from $2")
        .bind(dhash)
        .bind(frame_offset_ms)
        .fetch_optional(pool)
        .await?)
}

/// Returns the new id, or `None` if an item with the same `(dhash, frame_offset_ms)` already
/// exists. Run inside a transaction: a concurrent insert of the same key blocks until this
/// one commits or rolls back, so the vector/thumbnail writes can be part of the same unit.
pub async fn insert_media_item(conn: &mut PgConnection, item: &NewMediaItem) -> Result<Option<i64>> {
    Ok(sqlx::query_scalar(
        "insert into media_items (dhash, embedding_id, thumb_key, caption, ocr_text, media_kind, frame_offset_ms)
         values ($1, $2, $3, $4, $5, $6, $7)
         on conflict on constraint media_items_dedup_key do nothing
         returning id",
    )
    .bind(item.dhash)
    .bind(item.embedding_id)
    .bind(&item.thumb_key)
    .bind(&item.caption)
    .bind(&item.ocr_text)
    .bind(item.media_kind.as_str())
    .bind(item.frame_offset_ms)
    .fetch_optional(conn)
    .await?)
}

/// All sightings of one message are written atomically, so `sighting_exists` only reports
/// fully processed messages.
pub async fn insert_sightings(pool: &PgPool, sightings: &[NewSighting]) -> Result<()> {
    let mut tx = pool.begin().await?;
    for s in sightings {
        sqlx::query(
            "insert into media_sightings (media_item_id, channel_id, channel_username, message_id, is_backup_channel, posted_at)
             values ($1, $2, $3, $4, $5, $6)
             on conflict (channel_id, message_id, media_item_id) do nothing",
        )
        .bind(s.media_item_id)
        .bind(s.channel_id)
        .bind(&s.channel_username)
        .bind(s.message_id)
        .bind(s.is_backup_channel)
        .bind(s.posted_at)
        .execute(&mut *tx)
        .await?;
    }
    tx.commit().await?;
    Ok(())
}

pub async fn sighting_exists(pool: &PgPool, channel_id: i64, message_id: i64) -> Result<bool> {
    Ok(sqlx::query_scalar("select exists(select 1 from media_sightings where channel_id = $1 and message_id = $2)")
        .bind(channel_id)
        .bind(message_id)
        .fetch_one(pool)
        .await?)
}

/// Items with their sightings (oldest first), in the order of `item_ids`. Unknown ids are skipped.
pub async fn fetch_hits_by_item_ids(pool: &PgPool, item_ids: &[i64]) -> Result<Vec<(MediaItem, Vec<Sighting>)>> {
    if item_ids.is_empty() {
        return Ok(Vec::new());
    }
    let items: Vec<MediaItemRow> = sqlx::query_as(
        "select id, dhash, embedding_id, thumb_key, caption, ocr_text, media_kind, frame_offset_ms, created_at
         from media_items where id = any($1)",
    )
    .bind(item_ids)
    .fetch_all(pool)
    .await?;
    let sightings: Vec<SightingRow> = sqlx::query_as(
        "select media_item_id, channel_id, channel_username, message_id, is_backup_channel, posted_at
         from media_sightings where media_item_id = any($1)
         order by posted_at, id",
    )
    .bind(item_ids)
    .fetch_all(pool)
    .await?;

    let mut by_item: HashMap<i64, Vec<Sighting>> = HashMap::new();
    for s in sightings {
        by_item.entry(s.media_item_id).or_default().push(Sighting {
            channel_id: s.channel_id,
            channel_username: s.channel_username,
            message_id: s.message_id,
            is_backup_channel: s.is_backup_channel,
            posted_at: s.posted_at,
        });
    }
    let mut items: HashMap<i64, MediaItemRow> = items.into_iter().map(|row| (row.id, row)).collect();

    item_ids
        .iter()
        .filter_map(|id| items.remove(id))
        .map(|row| {
            let sightings = by_item.remove(&row.id).unwrap_or_default();
            Ok((MediaItem::try_from(row)?, sightings))
        })
        .collect()
}

/// `(item_id, dhash)` for every item, for the BK-tree rebuild.
pub async fn fetch_all_dhashes(pool: &PgPool) -> Result<Vec<(i64, i64)>> {
    Ok(sqlx::query_as("select id, dhash from media_items").fetch_all(pool).await?)
}

pub async fn item_ids_by_embedding_ids(pool: &PgPool, embedding_ids: &[Uuid]) -> Result<HashMap<Uuid, i64>> {
    if embedding_ids.is_empty() {
        return Ok(HashMap::new());
    }
    let rows: Vec<(Uuid, i64)> =
        sqlx::query_as("select embedding_id, id from media_items where embedding_id = any($1)")
            .bind(embedding_ids)
            .fetch_all(pool)
            .await?;
    Ok(rows.into_iter().collect())
}

/// Full-text search over caption + OCR text; `query` uses web-search syntax (quotes, `-word`, `or`).
pub async fn search_caption_fts(pool: &PgPool, query: &str, limit: i64) -> Result<Vec<i64>> {
    Ok(sqlx::query_scalar(
        "select id from media_items, websearch_to_tsquery('simple', $1) q
         where search_vec @@ q
         order by ts_rank(search_vec, q) desc, id desc
         limit $2",
    )
    .bind(query)
    .bind(limit)
    .fetch_all(pool)
    .await?)
}

/// Never moves the watermark backwards.
pub async fn upsert_watched_channel_watermark(pool: &PgPool, channel_id: i64, last_message_id: i64) -> Result<()> {
    sqlx::query(
        "insert into watched_channels (channel_id, last_message_id) values ($1, $2)
         on conflict (channel_id) do update
         set last_message_id = greatest(watched_channels.last_message_id, excluded.last_message_id)",
    )
    .bind(channel_id)
    .bind(last_message_id)
    .execute(pool)
    .await?;
    Ok(())
}

/// Upserts a channel's flags from the seed config, keeping its backfill watermark.
pub async fn upsert_watched_channel(pool: &PgPool, channel: &WatchedChannel) -> Result<()> {
    sqlx::query(
        "insert into watched_channels (channel_id, channel_username, is_backup, is_private, enabled)
         values ($1, $2, $3, $4, $5)
         on conflict (channel_id) do update
         set channel_username = excluded.channel_username,
             is_backup = excluded.is_backup,
             is_private = excluded.is_private,
             enabled = excluded.enabled",
    )
    .bind(channel.channel_id)
    .bind(&channel.channel_username)
    .bind(channel.is_backup)
    .bind(channel.is_private)
    .bind(channel.enabled)
    .execute(pool)
    .await?;
    Ok(())
}

pub async fn fetch_watched_channels(pool: &PgPool) -> Result<Vec<WatchedChannel>> {
    let rows: Vec<WatchedChannelRow> = sqlx::query_as(
        "select channel_id, channel_username, is_backup, is_private, enabled, last_message_id
         from watched_channels order by channel_id",
    )
    .fetch_all(pool)
    .await?;
    Ok(rows
        .into_iter()
        .map(|r| WatchedChannel {
            channel_id: r.channel_id,
            channel_username: r.channel_username,
            is_backup: r.is_backup,
            is_private: r.is_private,
            enabled: r.enabled,
            last_message_id: r.last_message_id,
        })
        .collect())
}

pub async fn record_failure(pool: &PgPool, channel_id: i64, message_id: i64, error: &str) -> Result<()> {
    sqlx::query(
        "insert into failed_items (channel_id, message_id, error) values ($1, $2, $3)
         on conflict (channel_id, message_id) do update
         set error = excluded.error,
             retry_count = failed_items.retry_count + 1,
             last_failed_at = now()",
    )
    .bind(channel_id)
    .bind(message_id)
    .bind(error)
    .execute(pool)
    .await?;
    Ok(())
}

pub async fn clear_failure(pool: &PgPool, channel_id: i64, message_id: i64) -> Result<()> {
    sqlx::query("delete from failed_items where channel_id = $1 and message_id = $2")
        .bind(channel_id)
        .bind(message_id)
        .execute(pool)
        .await?;
    Ok(())
}
