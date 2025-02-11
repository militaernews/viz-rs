use axum::{
    extract::{Multipart, Query, State},
    routing::post,
    response::Json,
    Router,
};
use anyhow::Result;

use image::DynamicImage;
use pgvector::Vector;
use serde::{Deserialize, Serialize};
use sqlx::postgres::PgPoolOptions;
use sqlx::Pool;
use sqlx::Postgres;
use std::net::SocketAddr;
use tch::{nn, nn::Module, vision::resnet, Device, Kind, Tensor};
use chrono::{DateTime, Utc};
use tch::nn::{ModuleT, VarStore};
use tch::vision::imagenet;
use tch::vision::resnet::resnet18;
use tokio::net::TcpListener;
use tracing::log::info;

type DbPool = Pool<Postgres>;

#[derive(Clone)]
struct AppState {
    db_pool: DbPool,
}

#[derive(Deserialize)]
struct UploadParams {
    msg_id: String,
    chat_id: String,
    timestamp: Option<DateTime<Utc>>,
}

#[derive(Serialize)]
struct UploadResponse {
    msg_id: String,
    chat_id: String,
    timestamp: DateTime<Utc>,
}

#[derive(Serialize)]
struct SearchResult {
    msg_id: String,
    chat_id: String,
    distance: f32,
}

// Extract Image Features
fn extract_features(img: DynamicImage) -> Result<Vec<f32>> {
    // Resize and convert image to tensor



    let mut vs = VarStore::new(Device::cuda_if_available());
    let model = resnet18(&vs.root(), 1000);

    vs.load("D:\\dev\\tools\\resnet50.safetensors")?;

    let image = imagenet::load_image_from_memory(img)?
        .to_device(vs.device());

    let output = image
        .unsqueeze(0)
        .apply_t(&model, false)
        .softmax(-1, Kind::Float);


}


// Upload or Update Image in Database
async fn upload_image(
    State(state): State<AppState>,
    mut multipart: Multipart,
    Query(params): Query<UploadParams>,
) -> Json<UploadResponse> {
    let timestamp = params.timestamp.unwrap_or(Utc::now());

    while let Some(field) = multipart.next_field().await.unwrap() {
        if let Some(filename) = field.file_name() {
            println!("Received file: {}", filename);

            let data = field.bytes().await.unwrap();
            let img = image::load_from_memory(&data).unwrap();
            let features = extract_features(img);

            // Insert or Update Image in DB
            sqlx::query!(
                "INSERT INTO images (msg_id, chat_id, vector, timestamp)
                 VALUES ($1, $2, $3, $4)
                 ON CONFLICT (msg_id, chat_id)
                 DO UPDATE SET vector = EXCLUDED.vector, timestamp = EXCLUDED.timestamp",
                params.msg_id,
                params.chat_id,
                &features as &[f32],
                timestamp
            )
                .execute(&state.db_pool)
                .await
                .unwrap();

            return Json(UploadResponse {
                msg_id: params.msg_id,
                chat_id: params.chat_id,
                timestamp,
            });
        }
    }

    panic!("No image uploaded.");
}

// Search for Similar Images
async fn search_similar_images(
    State(state): State<AppState>,
    mut multipart: Multipart,
) -> Json<Vec<SearchResult>> {
    while let Some(field) = multipart.next_field().await.unwrap() {
        if let Some(filename) = field.file_name() {
            println!("Searching for similar to: {}", filename);

            let data = field.bytes().await.unwrap();
            let img = image::load_from_memory(&data).unwrap();
            let features = extract_features(img);

            // Query DB for similar images
            let similar_images = find_similar_images(&state.db_pool, features, 5).await.unwrap();

            return Json(similar_images);
        }
    }

    Json(vec![]) // Return empty if no image found
}

// Find Similar Images in Database
async fn find_similar_images(
    pool: &DbPool,
    query_vector: Vec<f32>,
    limit: i64,
) -> Result<Vec<SearchResult>, sqlx::Error> {
    let rows = sqlx::query!(
        "SELECT msg_id, chat_id, vector <=> $1 AS distance FROM images ORDER BY distance LIMIT $2",
        &query_vector as &[f32],
        limit
    )
        .fetch_all(pool)
        .await?;

    Ok(rows
        .iter()
        .map(|row| SearchResult {
            msg_id: row.msg_id.clone(),
            chat_id: row.chat_id.clone(),
            distance: row.distance.unwrap_or(0.0) as f32,
        })
        .collect())
}

// Set Up Axum Server
#[tokio::main]
async fn main() ->Result<()>{
    // Initialize Database Pool
    let db_pool = PgPoolOptions::new()
        .max_connections(5)
        .connect("postgres://user:password@localhost/db_name")
        .await
        .expect("Failed to connect to database");

    let state = AppState { db_pool };

    let app = Router::new()
        .route("/upload", post(upload_image)) // Store or Update Image
        .route("/search", post(search_similar_images)) // Search Images
        .with_state(state);

    let addr = SocketAddr::from(([127, 0, 0, 1], 3000));
    println!("Server running on {}", addr);
    let addr = SocketAddr::from(([127, 0, 0, 1], 3000));
    let listener = TcpListener::bind(&addr).await.expect("Can't start server");
    info!("Server running on {}", addr);
    axum::serve(listener, app).await?;

    Ok(())
}
