mod telegram;
mod database;
mod entity;
mod route;
mod error;
mod embedding;

use anyhow::{anyhow, Result};
use axum::response::IntoResponse;
use serde::{Deserialize, Serialize};
use std::env::var;

use crate::entity::{ImageSearchParams, SearchResult, TextSearchParams, UploadParams};
use crate::error::AppError;
use crate::telegram::extract_from_chat;
use axum::extract::FromRef;
use dotenvy::dotenv;
use log::info;

use crate::database::set_up;
use crate::route::serve;
use qdrant_client::config::QdrantConfig;
use qdrant_client::Qdrant;
use sqlx::postgres::PgPoolOptions;
use tch::nn::Module;
use tch::nn::ModuleT;
use tracing_subscriber::layer::SubscriberExt;
use tracing_subscriber::util::SubscriberInitExt;
use tracing_subscriber::EnvFilter;

// Set Up Axum Server
#[tokio::main]
async fn main() -> Result<(), AppError> {
    dotenv().ok();

    tracing_subscriber::registry()
        .with(EnvFilter::new(var("RUST_LOG").unwrap_or_else(
            |_| "axum_login=debug,tower_sessions=debug,sqlx=warn,tower_http=debug,info,viz_rs=debug,info".into(),
        )))
        .with(tracing_subscriber::fmt::layer())
        .try_init().expect("Failed to initialise logging");


    info!("Starting application with logging enabled");
    info!("Environment variables loaded");

    for (key, value) in std::env::vars() {
        if key.contains("DATABASE") || key.contains("QDRANT") {
            info!("Config: {}: {}", key, value);
        }
    }

    // Initialize Qdrant
    let qdrant = Qdrant::new(QdrantConfig {
        uri: "http://localhost:6334".to_string(),

        ..Default::default()
    })?;

    info!("Connected to Qdrant at: {}", qdrant.config.uri);

    // Uncomment to set up collections
    // set_up(&qdrant).await?;

    let database_url = var("DATABASE_URL")
        .map_err(|e| anyhow!("Failed to get DATABASE_URL: {}", e))?;

    info!("Connecting to PostgreSQL database");

    let pg_pool = PgPoolOptions::new()
        .max_connections(5)
        .connect(&database_url)
        .await
        .map_err(|e| anyhow!("DB connection failed: {}", e))?;

    info!("Successfully connected to PostgreSQL");

    // Uncomment to extract from Telegram chat
    // extract_from_chat(qdrant).await?;

    info!("Starting web server");
    serve(qdrant, pg_pool).await.map_err(|e| {
        log::error!("Server error: {:?}", e);
        AppError::Unknown
    })?;

    Ok(())
}