mod telegram;
mod database;
mod entity;
mod route;
mod error;
mod embedding;

use std::env::var;
use anyhow::{anyhow, Result};
use axum::response::IntoResponse;
use serde::{Deserialize, Serialize};

use crate::entity::{SearchResult, UploadParams};
use crate::error::AppError;
use crate::telegram::extract_from_chat;
use axum::extract::FromRef;
use dotenvy::dotenv;
use log::info;

use qdrant_client::config::QdrantConfig;
use qdrant_client::Qdrant;
use sqlx::postgres::PgPoolOptions;
use tch::nn::ModuleT;
use tch::nn::Module;
use tracing_subscriber::EnvFilter;
use tracing_subscriber::layer::SubscriberExt;
use tracing_subscriber::util::SubscriberInitExt;
use crate::database::set_up;
use crate::route::serve;

// Set Up Axum Server
#[tokio::main]
async fn main() -> Result<(), AppError> {
    dotenv().ok();

    tracing_subscriber::registry()
        .with(EnvFilter::new(var("RUST_LOG").unwrap_or_else(
            |_| "axum_login=debug,tower_sessions=debug,sqlx=warn,tower_http=debug".into(),
        )))
        .with(tracing_subscriber::fmt::layer())
        .try_init().expect("Failed to initialise logging");

    info!("Env: log");
    println!("Env: print");
    for (key, value) in std::env::vars() {
        println!("{key}: {value}");
    }


    let qdrant = Qdrant::new(QdrantConfig {
        uri: "http://localhost:6334".to_string(),
        check_compatibility: false,  // Skip version compatibility check
        ..Default::default()
    })?;
//set_up(qdrant).await?;

    let pg_pool = PgPoolOptions::new()
        .max_connections(5)
        .connect(var("DATABASE_URL").map_err(|e| anyhow!("Failed to get DATABASE_URL: {}", e))?.as_str())
        .await
        .map_err(|e| anyhow!("DB connection failed: {}", e))?;


 // extract_from_chat(qdrant).await?;


  serve(qdrant, pg_pool).await?;

    Ok(())
}



