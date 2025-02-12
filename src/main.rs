use anyhow::{anyhow, Result};
use axum::{
    extract::{Multipart, Query, State},
    response::{IntoResponse, Json},
    routing::post,
    Router,
};
use image::DynamicImage;
use serde::{Deserialize, Serialize};
use std::env::var;
use uuid::Uuid;

use axum::extract::FromRef;
use chrono::{DateTime, Utc};
use dotenvy::dotenv;
use qdrant_client::config::QdrantConfig;
use qdrant_client::prelude::point_id::PointIdOptions;
use qdrant_client::qdrant::RecommendExample::PointId;
use qdrant_client::qdrant::{value, CreateCollection, CreateCollectionBuilder, Distance, PointStruct, ScoredPoint, SearchParams, SearchParamsBuilder, SearchPointsBuilder, UpsertPointsBuilder, Value, VectorParams, VectorParamsBuilder, VectorsConfig};
use qdrant_client::{Payload, Qdrant, QdrantError};
use std::net::SocketAddr;
use std::sync::Arc;
use tch::nn::{ModuleT, VarStore};
use tch::vision::imagenet;
use tch::vision::resnet::{resnet18, resnet34, resnet50};
use tch::{nn, nn::Module, vision::resnet, Device, Kind, Tensor};
use thiserror::Error;
use tokio::net::TcpListener;
use tracing::{error, log::info};

#[derive(Error, Debug)]
pub enum AppError {
    #[error("Database error: {0}")]
    DatabaseError(#[from] QdrantError),

    #[error("Image processing error: {0}")]
    ImageProcessingError(#[from] image::ImageError),

    #[error("Model loading error: {0}")]
    ModelLoadingError(#[from] tch::TchError),

    #[error("Nested error: {0}")]
    NestedError(#[from] anyhow::Error),

    #[error("Multipart field error: {0}")]
    MultipartError(#[from] axum::extract::multipart::MultipartError),

    #[error("Incorrect Metadata provided: {0}")]
    MetadataError(#[from] serde_json::Error),


    #[error("No image uploaded")]
    NoImageUploaded,

    #[error("Unknown error occurred")]
    Unknown,
}

impl IntoResponse for AppError {
    fn into_response(self) -> axum::response::Response {
        let status_code = match &self {
            AppError::DatabaseError(_) => http::StatusCode::INTERNAL_SERVER_ERROR,
            AppError::ImageProcessingError(_) => http::StatusCode::BAD_REQUEST,
            AppError::ModelLoadingError(_) => http::StatusCode::INTERNAL_SERVER_ERROR,
            AppError::MultipartError(_) => http::StatusCode::BAD_REQUEST,
            AppError::NoImageUploaded => http::StatusCode::BAD_REQUEST,
            AppError::Unknown => http::StatusCode::INTERNAL_SERVER_ERROR,
            _ => http::StatusCode::INTERNAL_SERVER_ERROR
        };
        (status_code, Json(serde_json::json!({ "error": self.to_string() }))).into_response()
    }
}

#[derive(Clone, FromRef)]
pub struct AppState {
    pub qdrant: Arc<Qdrant>,
}

#[derive(Deserialize, Debug)]
struct UploadParams {
    msg_id: i32,
    chat_id: i32,
    posted_at: DateTime<Utc>,
}

#[derive(Serialize)]
struct UploadResponse {
    msg_id: i32,
    chat_id: i32,
    posted_at: DateTime<Utc>,
}

#[derive(Serialize)]
struct SearchResult {
    msg_id: i32,
    chat_id: i32,
//    posted_at: DateTime<Utc>,
    similarity: f32,
}


const IMAGES_COLLECTION: &str = "images";

// Extract Image Features
fn extract_features(img: &[u8]) -> Result<Vec<f32>, AppError> {
  //  let mut vs = VarStore::new(Device::cuda_if_available());
   // let model = resnet50(&vs.root(), 1000);

    // Attempt to load the model
//  vs.load("D:\\dev\\tools\\resnet50.safetensors").map_err(AppError::ModelLoadingError)?;


    let image = imagenet::load_image_and_resize_from_memory(img,224,224)?;

    // A variable store is created to hold the model parameters.
    let mut vs = VarStore::new(Device::Cpu);

    // Then the model is built on this variable store, and the weights are loaded.
    let resnet18 = resnet34(&vs.root(), imagenet::CLASS_COUNT);
    vs.load("D:\\dev\\tools\\resnet34.ot")?;


    let output = resnet18
        .forward_t(&image.unsqueeze(0), /*train=*/ false)
        .softmax(-1,Kind::Float);




    // Finally print the top 5 categories and their associated probabilities.
    for (probability, class) in imagenet::top(&output, 7).iter() {
        println!("{:50} {:5.2}%", class, 100.0 * probability)
    }

  /*  let img = img.resize_exact(224, 224, image::imageops::FilterType::CatmullRom);
    let rgb = img.to_rgb8();
    let (width, height) = rgb.dimensions();

    // Convert image to tensor
    let img_tensor = Tensor::from_data_size(
        &rgb.as_flat_samples().samples,
        &[1, 3, height as i64, width as i64],
        Kind::Uint8,
    )
        .to_kind(Kind::Double )/ 255.0;

    // Normalize the tensor (ImageNet normalization)
    let mean = Tensor::from_slice(&[0.485, 0.456, 0.406]).view([1, 3, 1, 1]);
    let std = Tensor::from_slice(&[0.229, 0.224, 0.225]).view([1, 3, 1, 1]);
    let img_tensor = (img_tensor - mean) / std;

    // Apply the forward pass of the model to get the output
    let output = img_tensor.apply_t(&model, false);

    // Here we extract the features from the second-to-last layer, before the final classification head
    let features = output.flatten(1, 1); // Flatten the output into a 1D feature vector

    let mut vec_f32: Vec<f32> = vec![0.0; features.numel()];
    features.copy_data(&mut vec_f32, features.numel());

    Ok(vec_f32)

*/




    let features = output.flatten(1, 1); // Flatten the output into a 1D feature vector

    let mut vec_f32: Vec<f32> = vec![0.0; features.numel()];
    features.copy_data(&mut vec_f32, features.numel());

    dbg!(&vec_f32);

    Ok(vec_f32)


/*
    let tensor = Tensor::try_from(&img)?;
    let vs = nn::VarStore::new(Device::cuda_if_available());
    let resnet = resnet::resnet50_no_final_layer(&vs.root());
    vs.load("path/to/resnet50_weights.ot")?;

    // Preprocess the image tensor
    let image_tensor = tensor.unsqueeze(0).to_device(vs.device()); // Add batch dimension and move to device

    // Extract features
    let features = resnet.forward_t(&image_tensor, false);
*/


}

// Upload or Update Image in Database
async fn upload_image(
    State(state): State<AppState>,
    mut multipart: Multipart,
) -> Result<Json<UploadResponse>, AppError> {
    let mut metadata: Option<UploadParams> = None;
    let mut vectors: Option<Vec<f32>> = None;

    while let Some(field) = multipart.next_field().await.map_err(AppError::MultipartError)? {
        let name = field.name().unwrap().to_string();
        println!("Received file: {}", name);
        let data = field.bytes().await.map_err(AppError::MultipartError)?;

        if name == "meta" {
            metadata = serde_json::from_slice(&data).map_err(AppError::MetadataError)?;
            println!("Done receiving: {:?} - data: {:?}", metadata, data);
        } else if name == "image" {
            vectors = Some(extract_features(&*data)?);
        }
    }

    println!("Done receiving: {:?}", metadata);

    if let (Some(metadata), Some(vectors)) = (metadata, vectors) {


        let payload: Payload = serde_json::json!(
    {
        "chat_id": metadata.chat_id,
        "msg_id": metadata.msg_id,
        "timestamp": metadata.posted_at,
    }
)     .try_into()?;


        let id = Uuid::new_v4().to_string();

        let points = vec![PointStruct::new(id, vectors, payload)];
        state.qdrant
            .upsert_points(UpsertPointsBuilder::new(IMAGES_COLLECTION, points))
            .await
            .map_err(AppError::DatabaseError)?;

        Ok(Json(UploadResponse {
            msg_id: metadata.msg_id,
            chat_id: metadata.chat_id,
            posted_at: metadata.posted_at,
        }))

}else{
        Err(AppError::NoImageUploaded)
    }


}

// Search for Similar Images
async fn search_similar_images(
    State(state): State<AppState>,
    mut multipart: Multipart,
) -> Result<Json<Vec<SearchResult>>, AppError> {

    while let Some(mut field) = multipart.next_field().await.map_err(AppError::MultipartError)? {
        let name = field.name().unwrap().to_string();
        let data = field.bytes().await.map_err(AppError::MultipartError)?;

        println!("Length of `{}` is {} bytes", name, data.len());


        let features = extract_features(&*data)?;

        // Query DB for similar images
        let similar_images = search_vectors(&state.qdrant, features, 16).await?;

        return Ok(Json(similar_images));
    }

    Ok(Json(vec![])) // Return empty if no image found
}



// Set Up Axum Server
#[tokio::main]
async fn main() -> Result<(), AppError> {

    dotenv().ok();



    let qdrant =Arc::from( Qdrant::new(Default::default())?);

















    let state = AppState { qdrant };

    let app = Router::new()
        .route("/search", post(search_similar_images)) // Search Images
        .route("/upload", post(upload_image)) // Search Images
        .with_state(state);

    let addr = SocketAddr::from(([127, 0, 0, 1], 3000));
    let listener = TcpListener::bind(&addr)
        .await
        .map_err(|e| AppError::Unknown)?;

    info!("Server running on {}", addr);
    axum::serve(listener, app).await.map_err(|e| AppError::Unknown)?;

    Ok(())
}



async fn search_vectors(qdrant: &Qdrant, query_vector: Vec<f32>, limit: u64) -> Result<Vec<SearchResult>> {
    let search_response = qdrant
        .search_points(
            SearchPointsBuilder::new(   IMAGES_COLLECTION,  // The name of the collection
            query_vector,
            limit)
                .params(SearchParamsBuilder::default().hnsw_ef(128).exact(false))
        )
        .await.map_err(AppError::DatabaseError)?;

  let converted =  search_response.result.iter().map(|point| SearchResult{
        msg_id: point.payload.get("msg_id").and_then(get_string_value).unwrap_or(0),
        chat_id: point.payload.get("chat_id").and_then(get_string_value).unwrap_or(0),
      similarity: point.score,
    }).collect(); // convert to json?

    Ok(converted)
}


fn get_string_value(value: &Value) -> Option<i32> {
    if let Some(value::Kind::IntegerValue(s)) = &value.kind {
        Some(*s as i32)
    } else {
        None
    }
}


async fn set_up(qdrant: Qdrant)->Result<()>{
    qdrant
        .create_collection(
            CreateCollectionBuilder::new(IMAGES_COLLECTION)
                .vectors_config(VectorParamsBuilder::new(imagenet::CLASS_COUNT as u64, Distance::Cosine))
            ,
        )
        .await?;

    Ok(())
}