use crate::database::insert_images;
use crate::UploadParams;
use anyhow::Result;
use qdrant_client::Qdrant;

use crate::embedding::extract_features;
use base64::engine::general_purpose::STANDARD;
use base64::Engine;
use grammers_client::session::Session;
use grammers_client::types::Media;
use grammers_client::{Client, Config, SignInError, Update};
use std::env::var;
use std::io::{BufRead, Write};
use std::path::Path;
use std::time::Duration;
use std::{fs, io};
use tokio::time::sleep;

const SESSION_FILE: &str = "image_downloader.session";


async fn process_chat_images(
    client: &Client,
    chat_name: &str,
    qdrant: &Qdrant,
) -> Result<()> {
    let maybe_chat = client.resolve_username(chat_name).await?;
    let chat = maybe_chat.unwrap_or_else(|| panic!("Chat {chat_name} could not be found"));

    let mut messages = client.iter_messages(&chat);
    println!(
        "Processing images from chat {}. Total messages: {}",
        chat_name,
        messages.total().await.unwrap_or(0)
    );

    let mut chat_id = chat.id();
  /*    if (chat.) {  
    

        chat_id= format!("-100{}", chat.id()).parse::<i64>()?;
      
    }*/
    
    

    
    

    fs::create_dir_all(format!(
        "frontend/static/img/{chat_id}/",

    ))?;

    let mut image_counter = 0;

    while let Some(msg) = messages.next().await? {
       sleep(Duration::from_millis(600)).await;
        if let Some(media) = msg.media() {
            if let Media::Photo(img) = media {

                let temp_path = format!(
                    "frontend/static/img/{chat_id}/{}.jpg"
                    , msg.id()
                );


                client
                    .download_media(&img, &Path::new(&temp_path))
                    .await?;


                let bytes = fs::read(temp_path)?;



                match extract_features(&bytes) {
                    Ok(vectors) => {
                        let metadata = UploadParams {
                            msg_id: msg.id(),
                            chat_id: chat_id,
                            posted_at: msg.date(),
                            collection:"ukraine".to_string(),
                        };



                        // Insert into Qdrant
                        insert_images(qdrant, &metadata, vectors,STANDARD.encode(&bytes) ).await?;

                        image_counter += 1;
                        if image_counter % 10 == 0 {
                            println!("Processed {image_counter} images" );
                        }
                    }
                    Err(e) => {
                        eprintln!("Failed to extract features from message {}: {e:?}", msg.id() );
                        continue;
                    }
                }
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
    chat_name: &str,
    collection: &str,
    qdrant: &Qdrant,
) -> Result<()> {
    let maybe_chat = client.resolve_username(chat_name).await?;
    let chat = maybe_chat.unwrap_or_else(|| panic!("Chat {chat_name} could not be found"));
    let target_id = chat.id();

    fs::create_dir_all(format!("frontend/static/img/{target_id}/"))?;

    println!("Watching {chat_name} (chat_id {target_id}) for new posts...");

    loop {
        let update = match client.next_update().await {
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

        if msg.chat().id() != target_id {
            continue;
        }

        let Some(Media::Photo(img)) = msg.media() else {
            continue;
        };

        let temp_path = format!("frontend/static/img/{target_id}/{}.jpg", msg.id());

        if let Err(e) = client.download_media(&img, Path::new(&temp_path)).await {
            eprintln!("Failed to download media for message {}: {e:?}", msg.id());
            continue;
        }

        let bytes = match fs::read(&temp_path) {
            Ok(bytes) => bytes,
            Err(e) => {
                eprintln!("Failed to read downloaded media for message {}: {e:?}", msg.id());
                continue;
            }
        };

        match extract_features(&bytes) {
            Ok(vectors) => {
                let metadata = UploadParams {
                    msg_id: msg.id(),
                    chat_id: target_id,
                    posted_at: msg.date(),
                    collection: collection.to_string(),
                };

                match insert_images(qdrant, &metadata, vectors, STANDARD.encode(&bytes)).await {
                    Ok(_) => println!("Ingested new post {} from {chat_name}", msg.id()),
                    Err(e) => eprintln!("Failed to insert message {}: {e:?}", msg.id()),
                }
            }
            Err(e) => eprintln!("Failed to extract features from message {}: {e:?}", msg.id()),
        }
    }
}

fn prompt(message: &str) -> Result<String> {
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
async fn connect_and_authenticate() -> Result<Client> {
    let api_id = var("TG_ID").expect("TG_ID invalid").parse()?;
    let api_hash = var("TG_HASH").expect("TG_HASH invalid");

    println!("Connecting to Telegram...");
    let client = Client::connect(Config {
        session: Session::load_file_or_create(SESSION_FILE)?,
        api_id,
        api_hash: api_hash.clone(),
        params: Default::default(),
    })
        .await?;
    println!("Connected!");

    if !client.is_authorized().await? {
        println!("Signing in...");
        let phone = var("PHONE").expect("PHONE invalid");
        let token = client.request_login_code(&phone).await?;
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
        match client.session().save_to_file(SESSION_FILE) {
            Ok(_) => {}
            Err(e) => {
                println!("NOTE: failed to save the session, will sign out when done: {e}");
            }
        }
    }

    Ok(client)
}

pub async fn extract_from_chat(qdrant: Qdrant) -> Result<()> {
    let client = connect_and_authenticate().await?;
    process_chat_images(&client, "nn_backup", &qdrant).await?;
    Ok(())
}

/// Long-running: ingests new photo posts from `chat_name` as they arrive,
/// instead of walking existing history. Never returns under normal operation.
pub async fn watch_new_posts(qdrant: Qdrant, chat_name: &str, collection: &str) -> Result<()> {
    let client = connect_and_authenticate().await?;
    watch_chat_for_new_posts(&client, chat_name, collection, &qdrant).await?;
    Ok(())
}

