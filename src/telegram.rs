use crate::database::insert_images;
use crate::UploadParams;
use anyhow::Result;
use qdrant_client::Qdrant;

use crate::embedding::extract_features;
use base64::engine::general_purpose::STANDARD;
use base64::Engine;
use chrono::DateTime;
use grammers_client::media::Media;
use grammers_client::update::Update;
use grammers_client::{Client, SignInError};
use grammers_mtsender::{SenderPool, UpdatesConfiguration};
use grammers_session::storages::SqliteSession;
use grammers_session::types::PeerId;
use grammers_session::updates::UpdatesLike;
use std::env::var;
use std::io::{BufRead, Write};
use std::sync::Arc;
use std::time::Duration;
use std::{fs, io};
use tokio::sync::mpsc;
use tokio::time::sleep;

const SESSION_FILE: &str = "image_downloader.session";

/// `msg.date()` returns a `jiff::Timestamp`; converting via unix seconds/nanos
/// means the type never has to be named, so `jiff` doesn't need to be a direct
/// dependency of ours despite grammers depending on it internally.
async fn download_photo(
    msg: &grammers_client::message::Message,
    temp_path: &str,
) -> anyhow::Result<Option<Vec<u8>>> {
    if !matches!(msg.media(), Some(Media::Photo(_))) {
        return Ok(None);
    }

    if let Err(e) = msg.download_media(temp_path).await {
        eprintln!("Failed to download media for message {}: {e:?}", msg.id());
        return Ok(None);
    }

    match fs::read(temp_path) {
        Ok(bytes) => Ok(Some(bytes)),
        Err(e) => {
            eprintln!(
                "Failed to read downloaded media for message {}: {e:?}",
                msg.id()
            );
            Ok(None)
        }
    }
}

async fn process_chat_images(
    client: &Client,
    chat_name: &str,
    collection: &str,
    qdrant: &Qdrant,
) -> Result<()> {
    let maybe_peer = client.resolve_username(chat_name).await?;
    let peer = maybe_peer.unwrap_or_else(|| panic!("Chat {chat_name} could not be found"));

    // grammers's PeerId bitpacks using the same "Bot API Dialog ID" convention as
    // Telegram's Bot API / ptb-nn's Postgres `sources.channel_id` (the old "-100" prefix
    // trick), so no manual conversion is needed here anymore.
    let chat_id = peer
        .id()
        .bot_api_dialog_id()
        .expect("channel/group should have a concrete id");

    let peer_ref = peer
        .to_ref()
        .await?
        .unwrap_or_else(|| panic!("no access to {chat_name}"));

    fs::create_dir_all(format!("frontend/static/img/{chat_id}/"))?;

    let mut messages = client.iter_messages(peer_ref);
    println!(
        "Processing images from chat {}. Total messages: {}",
        chat_name,
        messages.total().await.unwrap_or(0)
    );

    let mut image_counter = 0;

    while let Some(msg) = messages.next().await? {
        sleep(Duration::from_millis(600)).await;

        let temp_path = format!("frontend/static/img/{chat_id}/{}.jpg", msg.id());
        let Some(bytes) = download_photo(&msg, &temp_path).await? else {
            continue;
        };

        match extract_features(&bytes) {
            Ok(vectors) => {
                let ts = msg.date();
                let metadata = UploadParams {
                    msg_id: msg.id(),
                    chat_id,
                    posted_at: DateTime::from_timestamp(ts.as_second(), ts.subsec_nanosecond() as u32)
                        .unwrap_or_default(),
                    collection: collection.to_string(),
                };

                insert_images(qdrant, &metadata, vectors, STANDARD.encode(&bytes)).await?;

                image_counter += 1;
                if image_counter % 10 == 0 {
                    println!("Processed {image_counter} images");
                }
            }
            Err(e) => {
                eprintln!(
                    "Failed to extract features from message {}: {e:?}",
                    msg.id()
                );
                continue;
            }
        }
    }

    println!("Finished processing {} images", image_counter);
    Ok(())
}

/// Runs forever, ingesting new photo posts as they arrive rather than walking
/// existing history (see `process_chat_images` for a one-shot backfill).
async fn watch_chat_for_new_posts(
    client: &Client,
    updates_rx: mpsc::Receiver<UpdatesLike>,
    chat_name: &str,
    collection: &str,
    qdrant: &Qdrant,
) -> Result<()> {
    let maybe_peer = client.resolve_username(chat_name).await?;
    let peer = maybe_peer.unwrap_or_else(|| panic!("Chat {chat_name} could not be found"));
    let target_id: PeerId = peer.id();
    let bot_api_id = target_id
        .bot_api_dialog_id()
        .expect("channel/group should have a concrete id");

    fs::create_dir_all(format!("frontend/static/img/{bot_api_id}/"))?;

    println!("Watching {chat_name} (chat_id {bot_api_id}) for new posts...");

    let mut stream = client
        .stream_updates(updates_rx, UpdatesConfiguration { catch_up: true })
        .await
        .map_err(|e| anyhow::anyhow!("failed to start update stream: {e}"))?;

    loop {
        let update = match stream.next().await {
            Ok(update) => update,
            Err(e) => {
                eprintln!("Update stream error, retrying in 5s: {e:?}");
                sleep(Duration::from_secs(5)).await;
                continue;
            }
        };

        let Update::NewMessage(msg) = update else {
            continue;
        };

        if msg.peer_id() != target_id {
            continue;
        }

        let temp_path = format!("frontend/static/img/{bot_api_id}/{}.jpg", msg.id());
        let Some(bytes) = download_photo(&msg, &temp_path).await? else {
            continue;
        };

        match extract_features(&bytes) {
            Ok(vectors) => {
                let ts = msg.date();
                let metadata = UploadParams {
                    msg_id: msg.id(),
                    chat_id: bot_api_id,
                    posted_at: DateTime::from_timestamp(ts.as_second(), ts.subsec_nanosecond() as u32)
                        .unwrap_or_default(),
                    collection: collection.to_string(),
                };

                match insert_images(qdrant, &metadata, vectors, STANDARD.encode(&bytes)).await {
                    Ok(_) => println!("Ingested new post {} from {chat_name}", msg.id()),
                    Err(e) => eprintln!("Failed to insert message {}: {e:?}", msg.id()),
                }
            }
            Err(e) => eprintln!(
                "Failed to extract features from message {}: {e:?}",
                msg.id()
            ),
        }
    }
}

/// Reads a one-time login code/password. If TG_CODE_FILE is set, polls that
/// path instead of blocking on stdin - useful when this runs detached (e.g.
/// under `podman exec`/systemd) and the human relaying the code from their
/// phone doesn't have a TTY into that process. Whoever has that access just
/// writes the value to the file once Telegram sends it.
fn prompt(message: &str) -> Result<String> {
    if let Ok(code_file) = var("TG_CODE_FILE") {
        println!("{message}");
        println!("(waiting for {code_file} to contain the code...)");
        loop {
            if let Ok(contents) = fs::read_to_string(&code_file) {
                let trimmed = contents.trim();
                if !trimmed.is_empty() {
                    let code = trimmed.to_string();
                    fs::remove_file(&code_file).ok();
                    return Ok(code);
                }
            }
            std::thread::sleep(Duration::from_secs(2));
        }
    }

    let stdout = io::stdout();
    let mut stdout = stdout.lock();
    stdout.write_all(message.as_bytes())?;
    stdout.flush()?;

    let stdin = io::stdin();
    let mut stdin = stdin.lock();

    let mut line = String::new();
    stdin.read_line(&mut line)?;
    Ok(line)
}

/// Connects (or resumes a saved session) and runs the interactive login-code
/// flow if needed. On first run this blocks on stdin waiting for the code
/// Telegram sends to PHONE - run this from a real terminal, not headlessly.
///
/// Returns the connected `Client` plus the raw updates channel from the
/// `SenderPool` - only `watch_new_posts` needs it (to build an `UpdateStream`),
/// but it's only produced once per pool so it has to be threaded out of here.
async fn connect_and_authenticate() -> Result<(Client, mpsc::Receiver<UpdatesLike>)> {
    let api_id: i32 = var("TG_ID").expect("TG_ID invalid").parse()?;
    let api_hash = var("TG_HASH").expect("TG_HASH invalid");

    println!("Connecting to Telegram...");
    let session = Arc::new(SqliteSession::open(SESSION_FILE).await?);
    let SenderPool {
        runner,
        updates,
        handle,
    } = SenderPool::new(Arc::clone(&session), api_id);
    let client = Client::new(handle);
    // Detached: the pool keeps running independently of this JoinHandle.
    let _ = tokio::spawn(runner.run());
    println!("Connected!");

    if !client.is_authorized().await? {
        println!("Signing in...");
        let phone = var("PHONE").expect("PHONE invalid");
        let token = client.request_login_code(&phone, &api_hash).await?;
        let code = prompt("Enter the code you received: ")?;
        let signed_in = client.sign_in(&token, &code).await;
        match signed_in {
            Err(SignInError::PasswordRequired(password_token)) => {
                // Note: this `prompt` method will echo the password in the console.
                //       Real code might want to use a better way to handle this.
                let hint = password_token.hint().unwrap_or("None");
                let prompt_message = format!("Enter the password (hint {}): ", &hint);
                let password = prompt(prompt_message.as_str())?;

                client
                    .check_password(password_token, password.trim())
                    .await?;
            }
            Ok(_) => (),
            Err(e) => panic!("{}", e),
        };
        println!("Signed in!");
        // SqliteSession persists to disk as it goes - no explicit save step needed.
    }

    Ok((client, updates))
}

pub async fn extract_from_chat(qdrant: Qdrant) -> Result<()> {
    let (client, _updates) = connect_and_authenticate().await?;
    process_chat_images(&client, "nn_backup", "ukraine", &qdrant).await?;
    Ok(())
}

/// One-shot: walks `chat_name`'s entire existing history into `collection`.
/// The target collection must already exist in Qdrant (see `create_collection`
/// in database.rs / the `/collection` endpoint) before calling this.
pub async fn backfill_chat(qdrant: Qdrant, chat_name: &str, collection: &str) -> Result<()> {
    let (client, _updates) = connect_and_authenticate().await?;
    process_chat_images(&client, chat_name, collection, &qdrant).await?;
    Ok(())
}

/// Long-running: ingests new photo posts from `chat_name` as they arrive,
/// instead of walking existing history. Never returns under normal operation.
pub async fn watch_new_posts(qdrant: Qdrant, chat_name: &str, collection: &str) -> Result<()> {
    let (client, updates) = connect_and_authenticate().await?;
    watch_chat_for_new_posts(&client, updates, chat_name, collection, &qdrant).await?;
    Ok(())
}
