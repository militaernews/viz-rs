//! Runs against the local stack (`podman compose up -d`); run with `cargo test -- --ignored`.
//! Override the defaults with TEST_DATABASE_URL, TEST_QDRANT_URL and TEST_S3_ENDPOINT.
use chrono::{Duration, Utc};
use image::{DynamicImage, Rgb, RgbImage};
use uuid::Uuid;
use viz_core::db;
use viz_core::storage::{S3Settings, ThumbStore, encode_thumbnail};
use viz_core::types::{MediaKind, NewMediaItem, NewSighting, WatchedChannel};
use viz_core::vector::VectorIndex;

fn env_or(name: &str, default: &str) -> String {
    std::env::var(name).unwrap_or_else(|_| default.to_string())
}

/// Random ids keep repeated runs against the same database independent.
fn random_i64() -> i64 {
    (Uuid::new_v4().as_u128() as i64).abs()
}

fn new_item(dhash: i64, caption: &str, frame_offset_ms: Option<i64>) -> NewMediaItem {
    NewMediaItem {
        dhash,
        embedding_id: Uuid::new_v4(),
        thumb_key: ThumbStore::key_for_hash(dhash),
        caption: Some(caption.to_string()),
        ocr_text: None,
        media_kind: if frame_offset_ms.is_some() { MediaKind::VideoFrame } else { MediaKind::Photo },
        frame_offset_ms,
    }
}

#[tokio::test]
#[ignore]
async fn postgres_round_trip() {
    let pool = db::connect(&env_or("TEST_DATABASE_URL", "postgres://user:pass@localhost:5432/tgsearch"), 4)
        .await
        .unwrap();
    let dhash = random_i64();
    let word = format!("marker{}", random_i64());

    // Photos (null offset) dedup against each other thanks to NULLS NOT DISTINCT.
    let mut conn = pool.acquire().await.unwrap();
    let photo = db::insert_media_item(&mut conn, &new_item(dhash, &format!("{word} photo"), None)).await.unwrap().unwrap();
    assert_eq!(db::insert_media_item(&mut conn, &new_item(dhash, "dup", None)).await.unwrap(), None);
    let frame = db::insert_media_item(&mut conn, &new_item(dhash, "frame", Some(1500))).await.unwrap().unwrap();
    drop(conn);
    assert_eq!(db::find_item_by_dhash(&pool, dhash, None).await.unwrap(), Some(photo));
    assert_eq!(db::find_item_by_dhash(&pool, dhash, Some(1500)).await.unwrap(), Some(frame));

    let (origin_channel, backup_channel, message_id) = (-random_i64(), -random_i64(), 42);
    let now = Utc::now();
    let sighting = |channel_id, is_backup_channel, posted_at| NewSighting {
        media_item_id: photo,
        channel_id,
        channel_username: Some("chan".into()),
        message_id,
        is_backup_channel,
        posted_at,
    };
    assert!(!db::sighting_exists(&pool, origin_channel, message_id).await.unwrap());
    let sightings = [sighting(backup_channel, true, now), sighting(origin_channel, false, now - Duration::hours(1))];
    db::insert_sightings(&pool, &sightings).await.unwrap();
    db::insert_sightings(&pool, &sightings).await.unwrap();
    assert!(db::sighting_exists(&pool, origin_channel, message_id).await.unwrap());

    // Order of the requested ids is kept; sightings come oldest first.
    let hits = db::fetch_hits_by_item_ids(&pool, &[frame, photo, -1]).await.unwrap();
    assert_eq!(hits.iter().map(|(item, _)| item.id).collect::<Vec<_>>(), vec![frame, photo]);
    let photo_sightings = &hits[1].1;
    assert_eq!(photo_sightings.len(), 2);
    assert_eq!(photo_sightings[0].channel_id, origin_channel);
    assert!(photo_sightings[1].is_backup_channel);

    assert_eq!(db::search_caption_fts(&pool, &word, 10).await.unwrap(), vec![photo]);
    assert!(db::fetch_all_dhashes(&pool).await.unwrap().contains(&(photo, dhash)));

    let watched = WatchedChannel {
        channel_id: origin_channel,
        channel_username: Some("chan".into()),
        is_backup: false,
        is_private: true,
        enabled: true,
        last_message_id: 0,
    };
    db::upsert_watched_channel(&pool, &watched).await.unwrap();
    db::upsert_watched_channel_watermark(&pool, origin_channel, 100).await.unwrap();
    db::upsert_watched_channel_watermark(&pool, origin_channel, 50).await.unwrap();
    db::upsert_watched_channel(&pool, &WatchedChannel { enabled: false, ..watched.clone() }).await.unwrap();
    let stored = db::fetch_watched_channels(&pool).await.unwrap().into_iter().find(|c| c.channel_id == origin_channel).unwrap();
    assert_eq!(stored, WatchedChannel { enabled: false, last_message_id: 100, ..watched });

    db::record_failure(&pool, origin_channel, 7, "first").await.unwrap();
    db::record_failure(&pool, origin_channel, 7, "second").await.unwrap();
    let (error, retries): (String, i32) =
        sqlx::query_as("select error, retry_count from failed_items where channel_id = $1 and message_id = 7")
            .bind(origin_channel)
            .fetch_one(&pool)
            .await
            .unwrap();
    assert_eq!((error.as_str(), retries), ("second", 2));
    db::clear_failure(&pool, origin_channel, 7).await.unwrap();
}

#[tokio::test]
#[ignore]
async fn qdrant_round_trip() {
    let index = VectorIndex::connect(&env_or("TEST_QDRANT_URL", "http://localhost:6334"), &format!("test_{}", random_i64()))
        .unwrap();
    index.ensure_collection(4).await.unwrap();
    index.ensure_collection(4).await.unwrap();
    let (near, far) = (Uuid::new_v4(), Uuid::new_v4());
    index.upsert(near, vec![1.0, 0.0, 0.0, 0.0], 1).await.unwrap();
    index.upsert(far, vec![0.0, 1.0, 0.0, 0.0], 2).await.unwrap();
    let results = index.search(vec![0.9, 0.1, 0.0, 0.0], 2).await.unwrap();
    assert_eq!(results.iter().map(|(id, _)| *id).collect::<Vec<_>>(), vec![near, far]);
    assert!(results[0].1 > 0.9);
}

#[tokio::test]
#[ignore]
async fn thumbnail_upload_is_publicly_readable() {
    let endpoint = env_or("TEST_S3_ENDPOINT", "http://localhost:9000");
    let store = ThumbStore::new(&S3Settings {
        endpoint: endpoint.clone(),
        bucket: "thumbs".into(),
        region: "us-east-1".into(),
        access_key: env_or("TEST_S3_ACCESS_KEY", "rustfsadmin"),
        secret_key: env_or("TEST_S3_SECRET_KEY", "rustfsadmin"),
    })
    .await
    .unwrap();
    let img = DynamicImage::ImageRgb8(RgbImage::from_pixel(800, 600, Rgb([10, 200, 30])));
    let key = ThumbStore::key_for_hash(random_i64());
    store.upload(&key, encode_thumbnail(&img).unwrap()).await.unwrap();

    // Anonymous GET, the way browsers load thumb_url.
    let url = viz_core::storage::public_url(&format!("{endpoint}/thumbs"), &key);
    let status = std::process::Command::new("curl")
        .args(["-s", "-o", "/dev/null", "-w", "%{http_code}", &url])
        .output()
        .unwrap();
    assert_eq!(String::from_utf8_lossy(&status.stdout), "200", "GET {url}");
}
