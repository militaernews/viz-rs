mod config;
mod pipeline;
mod telegram;

use anyhow::{Context, Result, bail};
use std::sync::Arc;
use tokio::sync::mpsc;
use tracing_subscriber::EnvFilter;
use viz_core::embed::ClipVisionEmbedder;
use viz_core::ocr::Ocr;
use viz_core::storage::ThumbStore;
use viz_core::vector::VectorIndex;

use crate::config::{Config, TelegramConfig, normalize_username};
use crate::pipeline::PipelineCtx;

const QUEUE_CAPACITY: usize = 256;

const USAGE: &str = "usage: ingest              watch all enabled channels for new posts
       ingest --login      interactive one-time Telegram login (needs TG_PHONE)
       ingest --backfill <username|channel_id>";

enum Mode {
    Live,
    Login,
    Backfill(String),
}

fn parse_args() -> Result<Mode> {
    let args: Vec<String> = std::env::args().skip(1).collect();
    match args.iter().map(String::as_str).collect::<Vec<_>>().as_slice() {
        [] => Ok(Mode::Live),
        ["--login"] => Ok(Mode::Login),
        ["--backfill", channel] => Ok(Mode::Backfill(channel.to_string())),
        _ => bail!("{USAGE}"),
    }
}

#[tokio::main]
async fn main() -> Result<()> {
    dotenvy::dotenv().ok();
    tracing_subscriber::fmt()
        .with_env_filter(EnvFilter::try_from_default_env().unwrap_or_else(|_| EnvFilter::new("info")))
        .json()
        .init();

    let mode = parse_args()?;
    let tg = TelegramConfig::from_env()?;
    let telegram::Connection { client, updates } = telegram::connect(tg.api_id, &tg.session_path).await?;

    if let Mode::Login = mode {
        let phone = viz_core::config::required("TG_PHONE")?;
        telegram::login(&client, &phone, &tg.api_hash).await?;
        tracing::info!(session = %tg.session_path, "logged in, session saved");
        return Ok(());
    }
    telegram::ensure_authorized(&client).await?;

    let cfg = Config::from_env()?;
    // Separate pool from the API, sized to the workers so search traffic can't starve writes.
    let pool = viz_core::db::connect(&cfg.database_url, cfg.workers as u32 + 2).await?;
    let channels = config::sync_channels(&client, &pool, cfg.channels_file.as_deref()).await?;
    if channels.is_empty() {
        bail!("no enabled channels resolved; add some via CHANNELS_FILE (see channels.example.toml)");
    }

    let model_path = cfg.clip_vision_model_path.clone();
    let vision = Arc::new(tokio::task::spawn_blocking(move || ClipVisionEmbedder::load(&model_path)).await??);
    let vectors = VectorIndex::connect(&cfg.qdrant_url, &cfg.qdrant_collection)?;
    vectors.ensure_collection(vision.dim()).await?;
    let ocr = Ocr::new(cfg.tesseract_bin.clone(), cfg.tesseract_langs.clone());
    if !ocr.is_enabled() {
        tracing::info!("TESSERACT_BIN not set, OCR disabled");
    }

    let ctx = Arc::new(PipelineCtx {
        pool,
        client: client.clone(),
        vision,
        vectors,
        thumbs: ThumbStore::new(&cfg.s3).await?,
        ocr,
        ffmpeg_bin: cfg.ffmpeg_bin.clone(),
        frame_timeout_secs: cfg.frame_extract_timeout_secs,
        max_media_bytes: cfg.max_media_bytes,
    });

    match mode {
        Mode::Backfill(target) => {
            let wanted_id = target.parse::<i64>().ok();
            let wanted_name = normalize_username(&target);
            let channel = channels
                .iter()
                .find(|c| Some(c.channel_id) == wanted_id || c.username.as_deref() == Some(wanted_name.as_str()))
                .with_context(|| format!("{target} is not an enabled watched channel (add it to CHANNELS_FILE first)"))?;
            telegram::backfill(&ctx, channel, cfg.workers).await
        }
        Mode::Live => {
            let (tx, rx) = mpsc::channel(QUEUE_CAPACITY);
            let _workers = pipeline::spawn_workers(Arc::clone(&ctx), rx, cfg.workers);
            tokio::select! {
                result = telegram::event_loop(&client, updates, &channels, tx) => result,
                _ = tokio::signal::ctrl_c() => {
                    tracing::info!("shutting down");
                    Ok(())
                }
            }
        }
        Mode::Login => unreachable!("handled above"),
    }
}
