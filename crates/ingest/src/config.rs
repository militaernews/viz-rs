use anyhow::{Context, Result, bail};
use serde::Deserialize;
use sqlx::PgPool;
use viz_core::config::{optional, parsed, parsed_or, required};
use viz_core::storage::S3Settings;
use viz_core::types::WatchedChannel;

use crate::telegram::{self, Channel, DialogIndex};
use grammers_client::Client;

pub struct TelegramConfig {
    pub api_id: i32,
    pub api_hash: String,
    pub session_path: String,
}

impl TelegramConfig {
    pub fn from_env() -> Result<Self> {
        Ok(Self {
            api_id: parsed("TG_API_ID")?,
            api_hash: required("TG_API_HASH")?,
            session_path: optional("TG_SESSION_PATH").unwrap_or_else(|| "./data/ingest.session".into()),
        })
    }
}

pub struct Config {
    pub database_url: String,
    pub qdrant_url: String,
    pub qdrant_collection: String,
    pub s3: S3Settings,
    pub clip_vision_model_path: String,
    pub ffmpeg_bin: String,
    pub frame_extract_timeout_secs: u64,
    pub channels_file: Option<String>,
    pub workers: usize,
    pub max_media_bytes: usize,
    pub tesseract_bin: Option<String>,
    pub tesseract_langs: String,
}

impl Config {
    pub fn from_env() -> Result<Self> {
        let cores = std::thread::available_parallelism().map(|n| n.get()).unwrap_or(2);
        Ok(Self {
            database_url: required("DATABASE_URL")?,
            qdrant_url: required("QDRANT_URL")?,
            qdrant_collection: required("QDRANT_COLLECTION")?,
            s3: S3Settings {
                endpoint: required("S3_ENDPOINT")?,
                bucket: required("S3_BUCKET")?,
                region: optional("S3_REGION").unwrap_or_else(|| "us-east-1".into()),
                access_key: required("S3_ACCESS_KEY")?,
                secret_key: required("S3_SECRET_KEY")?,
            },
            clip_vision_model_path: required("CLIP_VISION_MODEL_PATH")?,
            ffmpeg_bin: optional("FFMPEG_BIN").unwrap_or_else(|| "ffmpeg".into()),
            frame_extract_timeout_secs: parsed_or("FRAME_EXTRACT_TIMEOUT_SECS", 60)?,
            channels_file: optional("CHANNELS_FILE"),
            workers: parsed_or("INGEST_WORKERS", cores)?.max(1),
            max_media_bytes: parsed_or("INGEST_MAX_MEDIA_BYTES", 200 * 1024 * 1024)?,
            tesseract_bin: optional("TESSERACT_BIN"),
            tesseract_langs: optional("TESSERACT_LANGS").unwrap_or_else(|| "eng".into()),
        })
    }
}

#[derive(Debug, Deserialize)]
struct ChannelsFile {
    #[serde(default)]
    channels: Vec<ChannelSeed>,
}

/// One `[[channels]]` entry of the seed file. Needs `id` (Bot API style, e.g.
/// -1001234567890) or `username`; private channels without a username need the id.
#[derive(Debug, Clone, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ChannelSeed {
    pub id: Option<i64>,
    pub username: Option<String>,
    #[serde(default)]
    pub is_backup: bool,
    #[serde(default)]
    pub is_private: bool,
    #[serde(default = "enabled_by_default")]
    pub enabled: bool,
}

fn enabled_by_default() -> bool {
    true
}

fn load_seed_file(path: &str) -> Result<Vec<ChannelSeed>> {
    let raw = std::fs::read_to_string(path).with_context(|| format!("reading channel seed file {path}"))?;
    let file: ChannelsFile = toml::from_str(&raw).with_context(|| format!("parsing channel seed file {path}"))?;
    for seed in &file.channels {
        if seed.id.is_none() && seed.username.is_none() {
            bail!("{path}: every [[channels]] entry needs an `id` or a `username`");
        }
    }
    Ok(file.channels)
}

pub fn normalize_username(name: &str) -> String {
    let name = name.trim();
    let name = name
        .strip_prefix("https://t.me/")
        .or_else(|| name.strip_prefix("t.me/"))
        .unwrap_or(name);
    name.trim_start_matches('@').trim_end_matches('/').to_ascii_lowercase()
}

/// Upserts the seed file into `watched_channels`, then resolves every enabled channel in
/// the table to a Telegram peer. Channels that can't be resolved are logged and skipped.
pub async fn sync_channels(client: &Client, pool: &PgPool, seed_file: Option<&str>) -> Result<Vec<Channel>> {
    let dialogs = DialogIndex::load(client).await?;

    if let Some(path) = seed_file {
        for seed in load_seed_file(path)? {
            let username = seed.username.as_deref().map(normalize_username);
            let Some(resolved) = telegram::resolve_channel(client, &dialogs, seed.id, username.as_deref()).await? else {
                tracing::warn!(?seed, "seed channel could not be resolved; skipping");
                continue;
            };
            viz_core::db::upsert_watched_channel(
                pool,
                &WatchedChannel {
                    channel_id: resolved.channel_id,
                    channel_username: resolved.username.clone().or(username),
                    is_backup: seed.is_backup,
                    is_private: seed.is_private,
                    enabled: seed.enabled,
                    last_message_id: 0,
                },
            )
            .await?;
        }
    }

    let mut channels = Vec::new();
    for watched in viz_core::db::fetch_watched_channels(pool).await? {
        if !watched.enabled {
            continue;
        }
        let username = watched.channel_username.as_deref().map(normalize_username);
        match telegram::resolve_channel(client, &dialogs, Some(watched.channel_id), username.as_deref()).await? {
            Some(mut channel) => {
                channel.is_backup = watched.is_backup;
                channel.last_message_id = watched.last_message_id;
                if !channel.joined {
                    tracing::warn!(
                        channel_id = channel.channel_id,
                        "not a member of this channel: backfill works, but live updates won't arrive until the account joins"
                    );
                }
                channels.push(channel);
            }
            None => tracing::warn!(channel_id = watched.channel_id, "watched channel could not be resolved; skipping"),
        }
    }
    Ok(channels)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_seed_file() {
        let file: ChannelsFile = toml::from_str(
            r#"
            [[channels]]
            username = "@nn_backup"
            is_backup = true

            [[channels]]
            id = -1001234567890
            is_private = true
            enabled = false
            "#,
        )
        .unwrap();
        assert_eq!(file.channels.len(), 2);
        assert!(file.channels[0].is_backup && file.channels[0].enabled);
        assert_eq!(file.channels[1].id, Some(-1001234567890));
        assert!(!file.channels[1].enabled);
        assert!(toml::from_str::<ChannelsFile>("[[channels]]\nusername = \"x\"\nbackup = true").is_err());
    }

    #[test]
    fn normalizes_usernames() {
        assert_eq!(normalize_username("@NN_Backup"), "nn_backup");
        assert_eq!(normalize_username("https://t.me/nn_backup/"), "nn_backup");
        assert_eq!(normalize_username(" t.me/Foo "), "foo");
    }
}
