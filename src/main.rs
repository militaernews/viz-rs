use axum::{
    extract::{Multipart, State},
    response::Json,
    routing::{get, post},
    Router,
};
use chrono::Utc;
use dotenvy::dotenv;
use tch::{Cuda, Device, Tensor, vision::imagenet, nn, kind, nn::Module, vision::resnet};
use pgvector::Vector;
use serde::{Deserialize, Serialize};
use sqlx::{query, PgPool, Pool, Postgres};
use std::net::SocketAddr;
use std::path::Path;
use std::fs::File;
use image::{DynamicImage, ImageReader, RgbImage};
use sqlx::postgres::PgPoolOptions;
use tokio::net::TcpListener;
use tracing::{error, info};
use uuid::Uuid;

#[derive(Clone)]
struct AppState {
    db_pool: Pool<Postgres>,
}

#[derive(Serialize, Deserialize)]
struct ImageSearchResult {
    id: Uuid,
    msg_id: String,
    chat_id: String,
    similarity: f32,
}

#[tokio::main]
async fn main()  {
    dotenv().ok();
    tracing_subscriber::fmt::init(); // Enables logging

    let database_url = std::env::var("DATABASE_URL").expect("DATABASE_URL must be set");
    let db_pool = PgPoolOptions::new()
        .max_connections(5)
        .connect(&database_url)
        .await.expect("Cannot connect to database");

    let state = AppState { db_pool };

    let app = Router::new()
        .route("/upload", post(upload_image))
        .route("/search", post(search))
        .with_state(state);

    let addr = SocketAddr::from(([127, 0, 0, 1], 3000));
    let listener = TcpListener::bind(&addr).await.expect("Can't start server");
    info!("Server running on {}", addr);
    axum::serve(listener, app).await.expect("Failed to start server");
}

















use tokio::io::AsyncWriteExt;


#[derive(Serialize)]
struct SearchResult {
    image_id: Uuid,
    distance: f32,
}



// Extract Image Features
fn extract_features(img: DynamicImage) -> Vec<f32> {
    let resized = img.resize_exact(224, 224, image::imageops::FilterType::Nearest);
    let tensor = Tensor::from_slice(&resized.to_rgb8().into_raw())
        .view([1, 3, 224, 224])
        .to_kind(tch::Kind::Float)
        / 255.0;

    let vs = nn::VarStore::new(Device::Cpu);
    let resnet50 = resnet::resnet50(&vs.root(), 1000);
    let features = resnet50.forward_t(&tensor, false);

    features.into()
}

// Handle Image Upload and Search for Similar Images
async fn search(mut multipart: Multipart, State(app_state): AppState) -> Json<Vec<SearchResult>> {


    while let Some(field) = multipart.next_field().await.unwrap() {
        if let Some(filename) = field.file_name() {
            println!("Received file: {}", filename);

            let data = field.bytes().await.unwrap();
            let img = image::load_from_memory(&data).unwrap();
            let features = extract_features(img);

            // Query DB for similar images
            let similar_images = find_similar_images(&app_state.db_pool, features, 5).await.unwrap();

            return Json(similar_images);
        }
    }
    Json(vec![]) // Return empty if no image found
}

// Find Similar Images in Database
async fn find_similar_images(
    pool: &sqlx::Pool<sqlx::Postgres>,
    query_vector: Vec<f32>,
    limit: i64,
) -> Result<Vec<SearchResult>, sqlx::Error> {
    let rows = sqlx::query!(
        "SELECT id, vector <=> $1 AS distance FROM images ORDER BY distance LIMIT $2",
        &query_vector as &[f32],
        limit
    )
        .fetch_all(pool)
        .await?;

    Ok(rows
        .iter()
        .map(|row| SearchResult {
            image_id: row.id,
            distance: row.distance.unwrap_or(0.0) as f32,
        })
        .collect())
}