use axum::{
    extract::State,
    http::StatusCode,
    response::{IntoResponse, Response},
    routing::{get, post},
    Json, Router,
};
use axum::extract::Multipart;
use chrono::{DateTime, Utc};
use image::ImageFormat;
use rust_bert::pipelines::image_feature_extraction::{
    ImageFeatureExtractionModel, ImageFeatureExtractionOption,
};
use dotenvy::dotenv;
use serde::{Deserialize, Serialize};
use serde_json::json;
use sqlx::{postgres::PgPoolOptions, query_as_unchecked, query_unchecked, Encode, Pool, Postgres};
use thiserror::Error;
use tokio;
use uuid::Uuid;

// Custom error types
#[derive(Error, Debug)]
enum AppError {
    #[error("Database error: {0}")]
    Database(#[from] sqlx::Error),
    #[error("Invalid vector dimensions: expected {expected}, got {actual}")]
    InvalidVectorDimension { expected: usize, actual: usize },
    #[error("Invalid request: {0}")]
    BadRequest(String),
    #[error("Image processing error: {0}")]
    ImageProcessing(String),
    #[error("Model error: {0}")]
    Model(String),
}


// Convert AppError to axum Response
impl IntoResponse for AppError {
    fn into_response(self) -> Response {
        let (status, message) = match self {
            AppError::Database(err) => (StatusCode::INTERNAL_SERVER_ERROR, err.to_string()),
            AppError::InvalidVectorDimension { expected, actual } => (
                StatusCode::BAD_REQUEST,
                format!("Invalid vector dimension: expected {expected}, got {actual}"),
            ),
            AppError::BadRequest(msg) => (StatusCode::BAD_REQUEST, msg),
        };

        (status, Json(json!({ "error": message }))).into_response()
    }
}

type Result<T> = std::result::Result<T, AppError>;

// Constants
const VECTOR_DIMENSION: usize = 1024;

#[derive(Debug, Serialize, Deserialize)]
struct ImageEntity {
    id: Uuid,
    msg_id: String,
    chat_id: String,
    vector: Vec<f32>,
    timestamp: DateTime<Utc>,
}

#[derive(Debug, Deserialize)]
struct SearchRequest {
    query_vector: Vec<f32>,
    limit: Option<i32>,
    chat_id: Option<String>,
    min_similarity: Option<f32>,
}

#[derive(Debug, Serialize)]
struct SearchResult {
    msg_id: String,
    chat_id: String,
    similarity: f32,
    timestamp: DateTime<Utc>,
}

#[derive(Clone)]
struct AppState {
    db: Pool<Postgres>,
    model: ImageFeatureExtractionModel,
}

async fn process_image(
    model: &ImageFeatureExtractionModel,
    image_data: &[u8],
) -> Result<Vec<f32>, Box<dyn std::error::Error>> {
    // Load and process image
    let img = image::load_from_memory(image_data)?;

    // Convert image to RGB format
    let rgb_img = img.to_rgb8();

    // Convert to format expected by model
    let input = ImageFeatureExtractionOption {
        image: rgb_img,
        ..Default::default()
    };

    // Generate embedding
    let embeddings = model
        .encode(&[input])
        .map_err(|e| Box::new(AppError::Model(e.to_string())) as Box<dyn std::error::Error>)?;

    Ok(embeddings[0].clone())
}

async fn search_by_image(
    State(state): State<AppState>,
    mut multipart: Multipart,
) -> Result<Json<Vec<SearchResult>>, AppError> {
    // Extract parameters and image from multipart form
    let mut image_data = None;
    let mut chat_id = None;
    let mut limit = None;
    let mut min_similarity = None;

    while let Some(field) = multipart.next_field().await.map_err(|e| AppError::BadRequest(e.to_string()))? {
        let name = field.name().unwrap_or("").to_string();
        match name.as_str() {
            "image" => {
                let data = field.bytes().await.map_err(|e| AppError::BadRequest(e.to_string()))?;
                image_data = Some(data);
            }
            "chat_id" => {
                chat_id = Some(field.text().await.map_err(|e| AppError::BadRequest(e.to_string()))?);
            }
            "limit" => {
                if let Ok(text) = field.text().await {
                    limit = Some(text.parse::<i32>().unwrap_or(10));
                }
            }
            "min_similarity" => {
                if let Ok(text) = field.text().await {
                    min_similarity = Some(text.parse::<f32>().unwrap_or(0.0));
                }
            }
            _ => {}
        }
    }

    let image_data = image_data.ok_or_else(|| AppError::BadRequest("No image provided".to_string()))?;

    // Process image and generate embedding
    let query_vector = process_image(&state.model, &image_data)
        .await
        .map_err(|e| AppError::ImageProcessing(e.to_string()))?;

    // Perform search using the generated embedding
    let limit = limit.unwrap_or(10).clamp(1, 100);
    let min_similarity = min_similarity.unwrap_or(0.0).clamp(0.0, 1.0);

    let results = match chat_id {
        Some(chat_id) => {
            query_as_unchecked!(
                SearchResult,
                r#"
                SELECT
                    msg_id,
                    chat_id,
                    timestamp,
                    1 - (vector <=> $1::vector) as similarity
                FROM images
                WHERE
                    chat_id = $2 AND
                    1 - (vector <=> $1::vector) >= $4
                ORDER BY vector <=> $1::vector
                LIMIT $3
                "#,
                &query_vector,
                chat_id,
                limit,
                min_similarity
            )
                .fetch_all(&state.db)
                .await?
        }
        None => {
            query_as_unchecked!(
                SearchResult,
                r#"
                SELECT
                    msg_id,
                    chat_id,
                    timestamp,
                    1 - (vector <=> $1::vector) as similarity
                FROM images
                WHERE 1 - (vector <=> $1::vector) >= $3
                ORDER BY vector <=> $1::vector
                LIMIT $2
                "#,
                &query_vector,
                limit,
                min_similarity
            )
                .fetch_all(&state.db)
                .await?
        }
    };

    Ok(Json(results))
}


async fn store_image(
    State(state): State<AppState>,
    Json(image): Json<ImageEntity>,
) -> Result<Json<Uuid>> {
    // Validate vector dimension
    if image.vector.len() != VECTOR_DIMENSION {
        return Err(AppError::InvalidVectorDimension {
            expected: VECTOR_DIMENSION,
            actual: image.vector.len(),
        });
    }

    // Validate input
    if image.msg_id.is_empty() || image.chat_id.is_empty() {
        return Err(AppError::BadRequest(
            "msg_id and chat_id cannot be empty".to_string(),
        ));
    }

    let id = query_unchecked!(
        r#"
        INSERT INTO images (id, msg_id, chat_id, vector, timestamp)
        VALUES ($1, $2, $3, $4, $5)
        RETURNING id
        "#,
        image.id,
        image.msg_id,
        image.chat_id,
        &image.vector,
        image.timestamp,
    )
        .fetch_one(&state.db)
        .await?
        .id;

    Ok(Json(id))
}

#[tokio::main]
async fn main() -> Result<()> {
    // Load environment variables from .env file if present
   dotenv().ok();

    // Initialize tracing
    tracing_subscriber::fmt::init();

    // Database connection
    let db_url = std::env::var("DATABASE_URL").expect("DATABASE_URL must be set");
    let pool = PgPoolOptions::new()
        .max_connections(5)
        .connect(&db_url)
        .await
        .map_err(|e| AppError::Database(e))?;

    // Initialize CLIP model
    let clip_model = Clip::new()
        .map_err(|e| AppError::ClipModel(e.to_string()))?;

    let app_state = AppState {
        db: pool,
        clip_model,
    };

    let app = Router::new()
        .route("/search", post(search_by_image))
        .route("/images", post(store_image))
        .with_state(app_state)
        .layer(tower_http::trace::TraceLayer::new_for_http())
        .layer(tower_http::cors::CorsLayer::permissive())
        .layer(tower_http::timeout::TimeoutLayer::new(std::time::Duration::from_secs(30)));

    // Start server
    let addr = std::net::SocketAddr::from(([0, 0, 0, 0], 3000));
    let listener = tokio::net::TcpListener::bind(addr).await.unwrap();
    tracing::info!("Server running on http://{}", addr);

    axum::serve(listener, app)
        .with_graceful_shutdown(shutdown_signal())
        .await
        .map_err(|e| AppError::BadRequest(e.to_string()))?;

    Ok(())
}

// Graceful shutdown handler
async fn shutdown_signal() {
    let ctrl_c = async {
        tokio::signal::ctrl_c()
            .await
            .expect("Failed to install Ctrl+C handler");
    };

    #[cfg(unix)]
    let terminate = async {
        tokio::signal::unix::signal(tokio::signal::unix::SignalKind::terminate())
            .expect("Failed to install signal handler")
            .recv()
            .await;
    };

    #[cfg(not(unix))]
    let terminate = std::future::pending::<()>();

    tokio::select! {
        _ = ctrl_c => {},
        _ = terminate => {},
    }

    tracing::info!("Shutdown signal received, starting graceful shutdown");
}