use std::sync::Arc;
use axum::{
    routing::post,
    Router,
    response::Json,
    http::StatusCode,
};
use serde::{Deserialize, Serialize};
use tokio::fs::File;
use uuid::Uuid;
use anyhow::{Result, Context};
use ort::{Environment, SessionBuilder, Value};
use image::{self, ImageBuffer, Rgb};
use ndarray::{Array4, Array1};
use qdrant_client::qdrant::{vectors_config, Condition, CreateCollection, CreateCollectionBuilder, Distance, Filter, OptimizersConfigDiff, PointStruct, ScalarQuantizationBuilder, SearchParamsBuilder, SearchPoints, SearchPointsBuilder, UpsertPointsBuilder, VectorParams, VectorParamsBuilder, VectorsConfig};
use qdrant_client::{Payload, Qdrant, QdrantError};
use tower::limit::RateLimit;
use tower_governor::{
    governor::GovernorConfigBuilder,
    key_extractor::SmartIpKeyExtractor,
    GovernorLayer,
};
use std::time::Duration;
use axum::extract::{FromRef, Multipart, State};
use tokio::sync::Semaphore;
use metrics::{counter, gauge};

use serde_json::json;

use tokio::io::AsyncWriteExt;
use tracing::{info, error, warn};

#[derive(Serialize)]
struct SimilarityMatch {
    url: String,
    similarity: f32,
    metadata: Option<serde_json::Value>,
}

#[derive(Serialize)]
struct SimilarityResponse {
    matches: Vec<SimilarityMatch>,
    processing_time_ms: u64,
}

#[derive(Debug, thiserror::Error)]
enum AppError {
    #[error("Invalid input: {0}")]
    InvalidInput(String),

  //  #[error("Database error: {0}")]
  //  Database(#[from] qdrant_client::Error),

    #[error("Image processing error: {0}")]
    ImageProcessing(#[from] image::ImageError),

 //   #[error("Model inference error: {0}")]
//    ModelInference(#[from] ort::Error),

    #[error("Internal server error: {0}")]
    Internal(String),
}

impl axum::response::IntoResponse for AppError {
    fn into_response(self) -> axum::response::Response {
        let (status, message) = match self {
            AppError::InvalidInput(_) => (StatusCode::BAD_REQUEST, self.to_string()),
         //   AppError::Database(_) => (StatusCode::SERVICE_UNAVAILABLE, "Database error".to_string()),
            AppError::ImageProcessing(_) => (StatusCode::BAD_REQUEST, "Invalid image format".to_string()),
          //  AppError::ModelInference(_) => (StatusCode::INTERNAL_SERVER_ERROR, "Processing error".to_string()),
            AppError::Internal(_) => (StatusCode::INTERNAL_SERVER_ERROR, "Internal server error".to_string()),
        };

        error!(?self, "Request failed");
        (status, Json(json!({ "error": message }))).into_response()
    }
}

#[derive(Clone, FromRef)]
pub struct AppState {
   pub qdrant: Arc<Qdrant>,
    pub   model_env:      Arc<Environment>,
   pub concurrent_searches: Arc<Semaphore>,
}

const COLLECTION_NAME: &str = "images";
const VECTOR_SIZE: usize = 512;
const MAX_CONCURRENT_SEARCHES: usize = 10;

async fn initialize_qdrant(client: &Qdrant) -> Result<()> {
    // Create collection if it doesn't exist
    let create_collection = CreateCollection {
        collection_name: COLLECTION_NAME.to_string(),
        vectors_config: Some(VectorsConfig {
            config: Some(vectors_config::Config::Params(VectorParams {
                size: VECTOR_SIZE as u64,
                distance: Distance::Cosine.into(),
                ..Default::default()
            })),
        }),
        optimizers_config: Some(OptimizersConfigDiff {
            indexing_threshold: Some(20000), // Start indexing after 20k points
            ..Default::default()
        }),
        ..Default::default()
    };

    client
        .create_collection(&create_collection)
        .await
        .or_else(|e| {
            if e.to_string().contains("already exists") {
                Ok(())
            } else {
                Err(e)
            }
        })?;

    Ok(())
}

async fn process_image(
    img_path: &str,
    model_env: Arc<Environment>
) -> Result<Array1<f32>, AppError> {
    let img = image::open(img_path)
        .context("Failed to open image")?;

    let resized = img.resize_exact(224, 224, image::imageops::FilterType::Lanczos3);
    let rgb_img: ImageBuffer<Rgb<f32>, Vec<f32>> = resized.to_rgb32f();

    let mut normalized = Array4::zeros((1, 3, 224, 224));
    for (x, y, pixel) in rgb_img.enumerate_pixels() {
        normalized[[0, 0, y as usize, x as usize]] = (pixel[0] - 0.485) / 0.229;
        normalized[[0, 1, y as usize, x as usize]] = (pixel[1] - 0.456) / 0.224;
        normalized[[0, 2, y as usize, x as usize]] = (pixel[2] - 0.406) / 0.225;
    }

    let session = SessionBuilder::new(&model_env)
        .context("Failed to create session")?
        .with_model_from_file("resnet50.onnx")
        .context("Failed to load model")?;

    let input_tensor = Value::from_array(normalized)
        .context("Failed to create input tensor")?;

    let outputs = session.run(vec![input_tensor])
        .context("Model inference failed")?;

    let embedding = outputs[0].try_extract::<f32>()
        .context("Failed to extract output")?;

    Ok(Array1::from_vec(embedding.view().to_vec()))
}

async fn find_similar_images(
    client: &Qdrant,
    embedding: Array1<f32>,
    limit: u64
) -> Result<Vec<SimilarityMatch>, AppError> {
    let search_result = client
        .search_points(&SearchPoints {
            collection_name: COLLECTION_NAME.to_string(),
            vector: embedding.to_vec(),
            limit,
            with_payload: Some(true.into()),
            ..Default::default()
        })
        .await?;

    Ok(search_result
        .into_iter()
        .map(|point| SimilarityMatch {
            url: point.payload["url"].as_str().unwrap_or_default().to_string(),
            similarity: point.score,
            metadata: point.payload.get("metadata").cloned(),
        })
        .collect())
}

async fn upload_handler(
    mut multipart: Multipart,
   State( state): State<AppState>,
) -> Result<Json<SimilarityResponse>, AppError> {
    let start_time = std::time::Instant::now();

    // Get semaphore permit for concurrent search limiting
    let _permit = state.concurrent_searches
        .acquire()
        .await
        .map_err(|_| AppError::Internal("Server too busy".to_string()))?;

    // Extract image file
    let field = multipart
        .next_field()
        .await
        .map_err(|e| AppError::InvalidInput(e.to_string()))?
        .ok_or_else(|| AppError::InvalidInput("No file uploaded".to_string()))?;

    // Save temporarily
    let temp_path = format!("/tmp/{}.jpg", Uuid::new_v4());
    let mut file = File::create(&temp_path)
        .await
        .map_err(|e| AppError::Internal(e.to_string()))?;

    file.write_all(&field.bytes().await.map_err(|e| AppError::InvalidInput(e.to_string()))?)
        .await
        .map_err(|e| AppError::Internal(e.to_string()))?;

    // Process image and find matches
    let embedding = process_image(&temp_path, &state.model_env).await?;
    let matches = find_similar_images(&state.qdrant, embedding, 10).await?;

    // Cleanup
    if let Err(e) = tokio::fs::remove_file(&temp_path).await {
        warn!("Failed to remove temporary file: {}", e);
    }

    let processing_time = start_time.elapsed().as_millis() as u64;
   // gauge!("image_processing_time_ms", processing_time as f64);
    // counter!("processed_images_total", 1);

    Ok(Json(SimilarityResponse {
        matches,
        processing_time_ms: processing_time,
    }))
}

#[tokio::main]
async fn main() -> Result<()> {
    // Initialize tracing
    tracing_subscriber::fmt::init();

    // Initialize metrics
    metrics_exporter_prometheus::PrometheusBuilder::new()
        .with_http_listener(([0, 0, 0, 0], 9000))
        .install()?;

    // Initialize Qdrant client
    let qdrant =Qdrant::from_url("http://localhost:6334").build()?;

    initialize_qdrant(&qdrant).await?;

    // Initialize ONNX Runtime
    let model_env = Environment::builder()
        .with_name("image_similarity")
        .build()?;

    // Configure rate limiting
    let governor_conf = GovernorConfigBuilder::default()
        .per_second(2)
        .burst_size(5)
        .finish()
        .unwrap();

    let app_state = AppState {
        qdrant,
        model_env,
        concurrent_searches: Arc::from(Semaphore::new(MAX_CONCURRENT_SEARCHES)),
    };

    // Create router with rate limiting
    let app = Router::new()
        .route("/similarity", post(upload_handler))
     //   .layer(GovernorLayer::new(governor_conf))
        .with_state(app_state);

    // Start server
    let listener = tokio::net::TcpListener::bind("0.0.0.0:3000").await?;
    info!("Server running on http://0.0.0.0:3000");
    axum::serve(listener, app).await?;

    Ok(())
}