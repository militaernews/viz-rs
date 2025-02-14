use crate::database::insert_images;
use crate::UploadParams;
use anyhow::Result;
use qdrant_client::Qdrant;

use crate::embedding::extract_features;
use grammers_client::session::Session;
use grammers_client::types::Media;
use grammers_client::{Client, Config};
use simple_logger::SimpleLogger;
use std::env;
use std::path::Path;

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

    let mut image_counter = 0;

    while let Some(msg) = messages.next().await? {
        if let Some(media) = msg.media() {


            // Only process photos
            if let Media::Photo(img) = media {
                let temp_path = format!(
                    "target/temp-image-{}.jpg",
                    msg.id()
                );

                client
                    .download_media(&media, &Path::new(&temp_path))
                    .await?;


                let bytes = std::fs::read(temp_path)?;

                match extract_features(&bytes) {
                    Ok(vectors) => {
                        let metadata = UploadParams {
                            msg_id: msg.id(),
                            chat_id: chat.id(),
                            posted_at: msg.date(),
                        };

                        // Insert into Qdrant
                        insert_images(qdrant, &metadata, vectors).await?;

                        image_counter += 1;
                        if image_counter % 10 == 0 {
                            println!("Processed {} images", image_counter);
                        }
                    }
                    Err(e) => {
                        eprintln!("Failed to extract features from message {}: {}", msg.id(), e);
                        continue;
                    }
                }
            }
        }
    }

    println!("Finished processing {} images", image_counter);
    Ok(())
}

pub async fn extract_from_chat(qdrant: Qdrant) -> Result<()> {
    SimpleLogger::new()
        .with_level(log::LevelFilter::Info)
        .init()?;

    let api_id = env::var("TG_ID").expect("TG_ID invalid").parse()?;
    let api_hash = env::var("TG_HASH").expect("TG_HASH invalid");
    let chat_name = "reverse_psyop";

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
        println!("Please run the auth_setup example first to create a valid session file");
        return Ok(());
    }

    // Process images
    process_chat_images(&client, &chat_name, &qdrant).await?;

    Ok(())
}

