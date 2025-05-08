use crate::database::insert_images;
use crate::UploadParams;
use anyhow::Result;
use qdrant_client::Qdrant;

use crate::embedding::extract_features;
use grammers_client::session::Session;
use grammers_client::types::Media;
use grammers_client::{Client, Config, SignInError};
use std::env::var;
use std::{env, fs, io};
use std::io::{BufRead, Write};
use std::path::Path;
use std::time::Duration;
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

    fs::create_dir_all(format!(
        "frontend/static/img/{}/",
        chat.id()
    ))?;

    let mut image_counter = 0;

    while let Some(msg) = messages.next().await? {
       sleep(Duration::from_millis(600)).await;
        if let Some(media) = msg.media() {
            if let Media::Photo(img) = media {

                let temp_path = format!(
                    "frontend/static/img/{}/{}.jpg",
                    chat.id(), msg.id()
                );


                client
                    .download_media(&img, &Path::new(&temp_path))
                    .await?;


                let bytes = fs::read(temp_path)?;

                match extract_features(&bytes) {
                    Ok(vectors) => {
                        let metadata = UploadParams {
                            msg_id: msg.id(),
                            chat_id:chat.id(),
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

pub async fn extract_from_chat(qdrant: Qdrant) -> Result<()> {


    let api_id = var("TG_ID").expect("TG_ID invalid").parse()?;
    let api_hash = var("TG_HASH").expect("TG_HASH invalid");
    let chat_name = "nn_backup";

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
        let phone =var("PHONE").expect("PHONE invalid");
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

    // Process images
    process_chat_images(&client, &chat_name, &qdrant).await?;



    Ok(())
}

