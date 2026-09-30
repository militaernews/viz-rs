//! One-off migration from the old single-crate viz-rs, whose Qdrant collection stored every
//! image as base64 next to its Telegram coordinates (`chat_id`, `msg_id`, `posted_at`).
//! Re-indexes those images through the normal pipeline, so it needs no Telegram session.
//! Safe to re-run: messages that already have a sighting are skipped.

use anyhow::{Context, Result};
use base64::Engine;
use chrono::{DateTime, Utc};
use qdrant_client::Qdrant;
use qdrant_client::qdrant::value::Kind;
use qdrant_client::qdrant::{ScrollPointsBuilder, Value};
use std::collections::HashMap;
use viz_core::db;
use viz_core::types::NewSighting;

use crate::pipeline::Indexer;

const PAGE_SIZE: u32 = 64;

struct LegacyPoint {
    channel_id: i64,
    message_id: i64,
    posted_at: DateTime<Utc>,
    image: Vec<u8>,
}

pub async fn import(indexer: &Indexer, qdrant_url: &str, collection: &str) -> Result<()> {
    let client = Qdrant::from_url(qdrant_url).build().context("building Qdrant client")?;
    let (mut imported, mut skipped, mut failed) = (0usize, 0usize, 0usize);
    let mut offset = None;
    loop {
        let mut request = ScrollPointsBuilder::new(collection).limit(PAGE_SIZE).with_payload(true).with_vectors(false);
        if let Some(offset) = offset.take() {
            request = request.offset(offset);
        }
        let page = client.scroll(request).await.with_context(|| format!("scrolling legacy collection {collection}"))?;

        for point in page.result {
            let point = match parse_point(point.payload) {
                Ok(point) => point,
                Err(e) => {
                    failed += 1;
                    tracing::warn!(error = %format!("{e:#}"), "skipping malformed legacy point");
                    continue;
                }
            };
            let (channel_id, message_id) = (point.channel_id, point.message_id);
            if db::sighting_exists(&indexer.pool, channel_id, message_id).await? {
                skipped += 1;
                continue;
            }
            match import_point(indexer, point).await {
                Ok(()) => imported += 1,
                Err(e) => {
                    failed += 1;
                    tracing::warn!(channel_id, message_id, error = %format!("{e:#}"), "legacy image not imported");
                }
            }
        }

        tracing::info!(imported, skipped, failed, "legacy import progress");
        match page.next_page_offset {
            Some(next) => offset = Some(next),
            None => break,
        }
    }
    tracing::info!(imported, skipped, failed, "legacy import finished");
    Ok(())
}

async fn import_point(indexer: &Indexer, point: LegacyPoint) -> Result<()> {
    let media_item_id = indexer.index_visual(None, None, point.image).await?;
    db::insert_sightings(
        &indexer.pool,
        &[NewSighting {
            media_item_id,
            channel_id: point.channel_id,
            channel_username: None,
            message_id: point.message_id,
            is_backup_channel: false,
            posted_at: point.posted_at,
        }],
    )
    .await
}

fn parse_point(payload: HashMap<String, Value>) -> Result<LegacyPoint> {
    let integer = |name: &str| match payload.get(name).and_then(|v| v.kind.as_ref()) {
        Some(Kind::IntegerValue(n)) => Ok(*n),
        _ => anyhow::bail!("missing integer {name}"),
    };
    let string = |name: &str| match payload.get(name).and_then(|v| v.kind.as_ref()) {
        Some(Kind::StringValue(s)) => Ok(s.as_str()),
        _ => anyhow::bail!("missing string {name}"),
    };
    Ok(LegacyPoint {
        channel_id: integer("chat_id")?,
        message_id: integer("msg_id")?,
        posted_at: DateTime::parse_from_rfc3339(string("posted_at")?).context("parsing posted_at")?.to_utc(),
        image: base64::engine::general_purpose::STANDARD.decode(string("base64")?).context("decoding base64")?,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn value(kind: Kind) -> Value {
        Value { kind: Some(kind) }
    }

    #[test]
    fn parses_legacy_payload() {
        let payload = HashMap::from([
            ("chat_id".to_string(), value(Kind::IntegerValue(-1001234567890))),
            ("msg_id".to_string(), value(Kind::IntegerValue(42))),
            ("posted_at".to_string(), value(Kind::StringValue("2023-11-12T16:12:31+00:00".into()))),
            ("base64".to_string(), value(Kind::StringValue("aGk=".into()))),
        ]);
        let point = parse_point(payload).unwrap();
        assert_eq!((point.channel_id, point.message_id), (-1001234567890, 42));
        assert_eq!(point.posted_at.to_rfc3339(), "2023-11-12T16:12:31+00:00");
        assert_eq!(point.image, b"hi");
        assert!(parse_point(HashMap::new()).is_err());
    }
}
