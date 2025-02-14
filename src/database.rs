use chrono::{DateTime, Utc};
use qdrant_client::prelude::{Distance, PointStruct, Value};
use qdrant_client::{Payload, Qdrant};
use qdrant_client::qdrant::{value, CreateCollectionBuilder, SearchParamsBuilder, SearchPointsBuilder, UpsertPointsBuilder, VectorParamsBuilder};
use serde_json::json;
use tch::vision::imagenet;
use uuid::Uuid;
use crate::{AppError, SearchResult, UploadParams};

const IMAGES_COLLECTION: &str = "images";


pub async fn insert_images(qdrant: &Qdrant, metadata:  &UploadParams, vectors: Vec<f32> ) -> anyhow::Result<(), AppError> {
    let payload: Payload = json!(
    {
        "chat_id": metadata.chat_id,
        "msg_id": metadata.msg_id,
        "timestamp": metadata.posted_at,
    }
)     .try_into()?;


    let id = Uuid::new_v4().to_string();

    let points = vec![PointStruct::new(id, vectors, payload)];
    qdrant
        .upsert_points(UpsertPointsBuilder::new(IMAGES_COLLECTION, points))
        .await
        .map_err(AppError::DatabaseError)?;

    Ok(())
}

pub async fn search_vectors(qdrant: &Qdrant, query_vector: Vec<f32>, limit: u64) -> anyhow::Result<Vec<SearchResult>> {
    let search_response = qdrant
        .search_points(
            SearchPointsBuilder::new(   IMAGES_COLLECTION,  // The name of the collection
                                        query_vector,
                                        limit)
                .params(SearchParamsBuilder::default().hnsw_ef(128).exact(false))
        )
        .await.map_err(AppError::DatabaseError)?;

    let converted =  search_response.result.iter().map(|point|

        {



            SearchResult{
                msg_id: point.payload.get_key_value("msg_id").and_then(get_int_value).unwrap_or(0),
                chat_id: point.payload.get("chat_id").and_then(get_int64_value).unwrap_or(0),
                posted_at: point.payload.get("posted_at").and_then(get_date_value).unwrap_or(DateTime::default()),
                similarity: point.score,
            }
        }

    ).collect(); // convert to json?

    Ok(converted)
}


fn get_int_value(value: &Value) -> Option<i32> {
    if let Some(value::Kind::IntegerValue(s)) = &value.kind {
        Some(*s as i32)
    } else {
        None
    }
}

fn get_int64_value(value: &Value) -> Option<i64> {
    if let Some(value::Kind::IntegerValue(s)) = &value.kind {
        Some(*s)
    } else {
        None
    }
}

fn get_date_value(value: &Value) -> Option<DateTime<Utc>> {
    if let Some(value::Kind::StringValue(s)) = &value.kind {
        Some(DateTime::from(DateTime::parse_from_rfc3339(s).unwrap()))
    } else {
        None
    }
}



async fn set_up(qdrant: Qdrant)-> anyhow::Result<()> {
    qdrant
        .create_collection(
            CreateCollectionBuilder::new(IMAGES_COLLECTION)
                .vectors_config(VectorParamsBuilder::new(imagenet::CLASS_COUNT as u64, Distance::Cosine))
            ,
        )
        .await?;

    Ok(())
}