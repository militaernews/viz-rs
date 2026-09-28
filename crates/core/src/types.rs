use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use uuid::Uuid;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MediaItem {
    pub id: i64,
    pub dhash: i64,
    pub embedding_id: Uuid,
    pub thumb_key: String,
    pub caption: Option<String>,
    pub ocr_text: Option<String>,
    pub media_kind: MediaKind,
    pub frame_offset_ms: Option<i64>,
    pub created_at: DateTime<Utc>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum MediaKind {
    Photo,
    VideoFrame,
}

impl MediaKind {
    pub fn as_str(self) -> &'static str {
        match self {
            MediaKind::Photo => "photo",
            MediaKind::VideoFrame => "video_frame",
        }
    }

    pub fn parse(s: &str) -> Option<Self> {
        match s {
            "photo" => Some(MediaKind::Photo),
            "video_frame" => Some(MediaKind::VideoFrame),
            _ => None,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Sighting {
    pub channel_id: i64,
    pub channel_username: Option<String>,
    pub message_id: i64,
    pub is_backup_channel: bool,
    pub posted_at: DateTime<Utc>,
}

#[derive(Debug, Clone)]
pub struct NewMediaItem {
    pub dhash: i64,
    pub embedding_id: Uuid,
    pub thumb_key: String,
    pub caption: Option<String>,
    pub ocr_text: Option<String>,
    pub media_kind: MediaKind,
    pub frame_offset_ms: Option<i64>,
}

#[derive(Debug, Clone)]
pub struct NewSighting {
    pub media_item_id: i64,
    pub channel_id: i64,
    pub channel_username: Option<String>,
    pub message_id: i64,
    pub is_backup_channel: bool,
    pub posted_at: DateTime<Utc>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct WatchedChannel {
    pub channel_id: i64,
    pub channel_username: Option<String>,
    pub is_backup: bool,
    pub is_private: bool,
    pub enabled: bool,
    pub last_message_id: i64,
}

/// What the API returns per search result.
#[derive(Debug, Clone, Serialize)]
pub struct SearchHit {
    pub thumb_url: String,
    /// 1.0 for an exact hash or caption match, otherwise hash similarity or cosine similarity.
    pub score: f32,
    pub origin: Option<Link>,
    pub backup: Option<Link>,
}

#[derive(Debug, Clone, Serialize)]
pub struct Link {
    pub channel_id: i64,
    pub channel_username: Option<String>,
    pub message_id: i64,
    pub posted_at: DateTime<Utc>,
}

impl From<&Sighting> for Link {
    fn from(s: &Sighting) -> Self {
        Link {
            channel_id: s.channel_id,
            channel_username: s.channel_username.clone(),
            message_id: s.message_id,
            posted_at: s.posted_at,
        }
    }
}
