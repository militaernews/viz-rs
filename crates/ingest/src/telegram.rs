use anyhow::{Context, Result, anyhow, bail};
use chrono::{DateTime, Utc};
use futures::StreamExt;
use grammers_client::client::UpdatesConfiguration;
use grammers_client::media::Media;
use grammers_client::message::Message;
use grammers_client::session::storages::SqliteSession;
use grammers_client::session::types::{PeerId, PeerRef};
use grammers_client::session::updates::UpdatesLike;
use grammers_client::update::Update;
use grammers_client::{Client, SenderPool, SignInError};
use std::collections::HashMap;
use std::io::{BufRead, Write};
use std::path::Path;
use std::sync::Arc;
use std::time::Duration;
use tokio::sync::mpsc;

use crate::config::normalize_username;
use crate::pipeline::{self, PipelineCtx};

const BACKFILL_BATCH: usize = 100;
const UPDATE_STATE_SYNC_INTERVAL: Duration = Duration::from_secs(30);

pub struct Connection {
    pub client: Client,
    pub updates: mpsc::UnboundedReceiver<UpdatesLike>,
}

/// Opens (or creates) the SQLite session and starts the MTProto sender pool.
pub async fn connect(api_id: i32, session_path: &str) -> Result<Connection> {
    if let Some(parent) = Path::new(session_path).parent().filter(|p| !p.as_os_str().is_empty()) {
        std::fs::create_dir_all(parent).with_context(|| format!("creating {}", parent.display()))?;
    }
    let session = Arc::new(
        SqliteSession::open(session_path)
            .await
            .with_context(|| format!("opening Telegram session {session_path}"))?,
    );
    let SenderPool { runner, updates, handle } = SenderPool::new(Arc::clone(&session), api_id);
    let client = Client::new(handle);
    tokio::spawn(runner.run());
    Ok(Connection { client, updates })
}

pub async fn ensure_authorized(client: &Client) -> Result<()> {
    if !client.is_authorized().await? {
        bail!("Telegram session is not authorized; run `ingest --login` once first");
    }
    Ok(())
}

/// One-time interactive login; the SQLite session persists as it goes.
///
/// If the phone never receives a code, the login is most likely coming from a datacenter/VPS
/// IP: Telegram's `auth.sendCode` then often reports success but silently withholds delivery.
/// Log in once from a residential connection and copy the session file to the server instead.
pub async fn login(client: &Client, phone: &str, api_hash: &str) -> Result<()> {
    if client.is_authorized().await? {
        tracing::info!("session is already authorized");
        return Ok(());
    }
    let token = client.request_login_code(phone, api_hash).await?;
    let code = prompt("Enter the code you received: ")?;
    match client.sign_in(&token, code.trim()).await {
        Ok(_) => {}
        Err(SignInError::PasswordRequired(password_token)) => {
            let hint = password_token.hint().unwrap_or("none").to_string();
            let password = prompt(&format!("Enter the 2FA password (hint: {hint}): "))?;
            client.check_password(password_token, password.trim()).await?;
        }
        Err(e) => bail!("sign-in failed: {e}"),
    }
    // Caches the peers needed for update gap resolution and private channel lookup.
    DialogIndex::load(client).await?;
    Ok(())
}

/// Reads a login code/password from stdin, or, if TG_CODE_FILE is set, polls that file
/// instead so the login also works when the process has no TTY (podman exec, systemd).
fn prompt(message: &str) -> Result<String> {
    if let Ok(code_file) = std::env::var("TG_CODE_FILE") {
        println!("{message}(waiting for {code_file} to contain the value...)");
        loop {
            if let Ok(contents) = std::fs::read_to_string(&code_file) {
                let value = contents.trim().to_string();
                if !value.is_empty() {
                    std::fs::remove_file(&code_file).ok();
                    return Ok(value);
                }
            }
            std::thread::sleep(Duration::from_secs(2));
        }
    }
    let mut stdout = std::io::stdout().lock();
    stdout.write_all(message.as_bytes())?;
    stdout.flush()?;
    let mut line = String::new();
    std::io::stdin().lock().read_line(&mut line)?;
    Ok(line)
}

#[derive(Debug, Clone)]
pub struct Channel {
    pub peer_id: PeerId,
    pub peer_ref: PeerRef,
    /// Bot API style id (-100…), the `channel_id` stored in Postgres.
    pub channel_id: i64,
    pub username: Option<String>,
    pub is_backup: bool,
    /// Only joined channels deliver live updates.
    pub joined: bool,
    pub last_message_id: i64,
}

/// The account's dialogs, which is the only way to reach private channels (no username).
pub struct DialogIndex {
    by_id: HashMap<PeerId, (PeerRef, Option<String>)>,
    by_username: HashMap<String, PeerId>,
}

impl DialogIndex {
    pub async fn load(client: &Client) -> Result<Self> {
        let mut index = Self { by_id: HashMap::new(), by_username: HashMap::new() };
        let mut dialogs = client.iter_dialogs();
        while let Some(dialog) = dialogs.next().await? {
            let username = dialog.peer().username().map(normalize_username);
            if let Some(name) = &username {
                index.by_username.insert(name.clone(), dialog.peer_id());
            }
            index.by_id.insert(dialog.peer_id(), (dialog.peer_ref(), username));
        }
        tracing::info!(dialogs = index.by_id.len(), "loaded Telegram dialogs");
        Ok(index)
    }
}

/// Finds a channel by Bot API id or username, preferring the account's own dialogs.
pub async fn resolve_channel(
    client: &Client,
    dialogs: &DialogIndex,
    channel_id: Option<i64>,
    username: Option<&str>,
) -> Result<Option<Channel>> {
    let joined_peer = channel_id
        .and_then(PeerId::from_bot_api_dialog_id)
        .filter(|id| dialogs.by_id.contains_key(id))
        .or_else(|| username.and_then(|name| dialogs.by_username.get(name).copied()));
    if let Some(peer_id) = joined_peer {
        let (peer_ref, username) = dialogs.by_id[&peer_id].clone();
        return Ok(Some(channel(peer_id, peer_ref, username, true)?));
    }

    let Some(name) = username else {
        return Ok(None);
    };
    let peer = match client.resolve_username(name).await {
        Ok(Some(peer)) => peer,
        Ok(None) => return Ok(None),
        Err(e) => {
            tracing::warn!(username = name, error = %e, "resolving username failed");
            return Ok(None);
        }
    };
    let peer_ref = peer
        .to_ref()
        .await
        .map_err(|e| anyhow!("no usable reference for @{name}: {e}"))?
        .with_context(|| format!("no access hash for @{name}"))?;
    Ok(Some(channel(peer.id(), peer_ref, peer.username().map(normalize_username), false)?))
}

fn channel(peer_id: PeerId, peer_ref: PeerRef, username: Option<String>, joined: bool) -> Result<Channel> {
    Ok(Channel {
        peer_id,
        peer_ref,
        channel_id: peer_id.bot_api_dialog_id().context("peer has no Bot API id")?,
        username,
        is_backup: false,
        joined,
        last_message_id: 0,
    })
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum MediaClass {
    Photo,
    Video,
}

pub fn classify(media: &Media) -> Option<MediaClass> {
    match media {
        Media::Photo(_) => Some(MediaClass::Photo),
        Media::Document(doc) => match doc.mime_type() {
            Some(mime) if mime.starts_with("video/") => Some(MediaClass::Video),
            Some(mime) if mime.starts_with("image/") => Some(MediaClass::Photo),
            _ => None,
        },
        _ => None,
    }
}

pub struct PendingMedia {
    pub channel_id: i64,
    pub channel_username: Option<String>,
    pub message_id: i64,
    pub caption: Option<String>,
    pub posted_at: DateTime<Utc>,
    pub is_backup: bool,
    pub class: MediaClass,
    pub media: Media,
}

impl PendingMedia {
    pub fn from_message(msg: &Message, channel: &Channel) -> Option<Self> {
        let media = msg.media()?;
        let class = classify(&media)?;
        let caption = msg.text().trim();
        Some(Self {
            channel_id: channel.channel_id,
            channel_username: channel.username.clone(),
            message_id: i64::from(msg.id()),
            caption: (!caption.is_empty()).then(|| caption.to_string()),
            posted_at: msg.date(),
            is_backup: channel.is_backup,
            class,
            media,
        })
    }
}

/// Forwards new media posts from watched channels to the worker queue. Never processes
/// inline: one slow video must not stall the update stream.
pub async fn event_loop(
    client: &Client,
    updates: mpsc::UnboundedReceiver<UpdatesLike>,
    channels: &[Channel],
    tx: mpsc::Sender<PendingMedia>,
) -> Result<()> {
    let by_peer: HashMap<PeerId, &Channel> = channels.iter().map(|c| (c.peer_id, c)).collect();
    let config = UpdatesConfiguration {
        catch_up: true,
        // Workers apply backpressure through `tx`; don't drop updates while they catch up.
        update_queue_limit: Some(10_000),
    };
    let mut stream = client
        .stream_updates(updates, config)
        .await
        .map_err(|e| anyhow!("starting update stream: {e}"))?;
    let mut sync = tokio::time::interval(UPDATE_STATE_SYNC_INTERVAL);
    tracing::info!(channels = by_peer.len(), "watching for new posts");

    loop {
        let update = tokio::select! {
            update = stream.next() => update,
            _ = sync.tick() => {
                // Persisting the update state is what lets `catch_up` replay missed posts after a restart.
                if let Err(e) = stream.sync_update_state().await {
                    tracing::warn!(error = %e, "saving update state failed");
                }
                continue;
            }
        };
        let update = match update {
            Ok(update) => update,
            Err(e) => {
                tracing::warn!(error = %e, "update stream error, backing off");
                tokio::time::sleep(Duration::from_secs(5)).await;
                continue;
            }
        };
        let Update::NewMessage(msg) = update else {
            continue;
        };
        if msg.outgoing() {
            continue;
        }
        let Some(channel) = by_peer.get(&msg.peer_id()) else {
            continue;
        };
        let Some(pending) = PendingMedia::from_message(&msg, channel) else {
            continue;
        };
        if tx.send(pending).await.is_err() {
            tracing::error!("processing queue closed, stopping ingest loop");
            break;
        }
    }
    stream.sync_update_state().await.map_err(|e| anyhow!("saving update state: {e}"))?;
    Ok(())
}

/// Walks a channel's history oldest-first from its watermark. Each batch is fully processed
/// before the watermark advances, so a restart resumes instead of rescanning or skipping.
pub async fn backfill(ctx: &PipelineCtx, channel: &Channel, concurrency: usize) -> Result<()> {
    let start = i32::try_from(channel.last_message_id).context("watermark out of range")?;
    tracing::info!(channel_id = channel.channel_id, from_message_id = start, "starting backfill");
    let mut messages = ctx.client.iter_messages(channel.peer_ref).reverse(true).offset_id(start);
    let mut scanned = 0usize;

    loop {
        let mut batch = Vec::new();
        let mut last_id = None;
        for _ in 0..BACKFILL_BATCH {
            let Some(msg) = messages.next().await? else { break };
            last_id = Some(i64::from(msg.id()));
            batch.extend(PendingMedia::from_message(&msg, channel));
        }
        let Some(last_id) = last_id else { break };

        let media_count = batch.len();
        futures::stream::iter(batch)
            .for_each_concurrent(concurrency, |item| pipeline::handle(ctx, item))
            .await;
        viz_core::db::upsert_watched_channel_watermark(&ctx.pool, channel.channel_id, last_id).await?;
        scanned += BACKFILL_BATCH;
        tracing::info!(channel_id = channel.channel_id, last_message_id = last_id, media_count, scanned, "backfill batch done");
    }
    tracing::info!(channel_id = channel.channel_id, "backfill complete");
    Ok(())
}
