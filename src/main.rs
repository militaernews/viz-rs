mod telegram;
mod database;
mod entity;
mod route;
mod error;
mod embedding;

use anyhow::Result;
use axum::response::IntoResponse;
use serde::{Deserialize, Serialize};

use crate::entity::{SearchResult, UploadParams};
use crate::error::AppError;
use crate::telegram::extract_from_chat;
use axum::extract::FromRef;
use dotenvy::dotenv;
use qdrant_client::Qdrant;
use tch::nn::ModuleT;
use tch::nn::Module;

// Set Up Axum Server
#[tokio::main]
async fn main() -> Result<(), AppError> {
    dotenv().ok();


    let qdrant = Qdrant::new(Default::default())?;


  //extract_from_chat(qdrant).await?;


    route::serve(qdrant).await?;

    Ok(())
}



