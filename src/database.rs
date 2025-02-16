use crate::error::AppError::TelegramDatabaseError;
use crate::{AppError, SearchResult, UploadParams};
use anyhow::Result;
use chrono::{DateTime, Utc};
use futures::stream::StreamExt;
use qdrant_client::prelude::{Distance, PointStruct, Value};
use qdrant_client::qdrant::{
    value, CreateCollectionBuilder, ScoredPoint, SearchParamsBuilder,
    SearchPointsBuilder, UpsertPointsBuilder, VectorParamsBuilder,
};
use qdrant_client::{Payload, Qdrant};
use serde_json::json;
use sqlx::{query, PgPool};
use tch::vision::imagenet;
use uuid::Uuid;

const IMAGES_COLLECTION: &str = "images";

pub async fn insert_images(
    qdrant: &Qdrant,
    metadata: &UploadParams,
    vectors: Vec<f32>,
) -> Result<(), AppError> {
    let payload: Payload = json!(
        {
            "chat_id": metadata.chat_id,
            "msg_id": metadata.msg_id,
            "posted_at": metadata.posted_at,
        }
    )
    .try_into()?;

    //TODO avoid inserting already present msg_id/chat_id
    let id = Uuid::new_v4().to_string();

    let points = vec![PointStruct::new(id, vectors, payload)];
    qdrant
        .upsert_points(UpsertPointsBuilder::new(IMAGES_COLLECTION, points))
        .await
        .map_err(AppError::VectorDatabaseError)?;

    Ok(())
}

pub async fn search_vectors(
    qdrant: &Qdrant,
    pg_pool: &PgPool,
    query_vector: Vec<f32>,
    limit: u64,
) -> Result<Vec<SearchResult>> {
    let search_response = qdrant
        .search_points(
            SearchPointsBuilder::new(IMAGES_COLLECTION, query_vector, limit)
                .params(SearchParamsBuilder::default().hnsw_ef(256).exact(false))
                .with_payload(true)
                .with_vectors(false),
        )
        .await
        .map_err(AppError::VectorDatabaseError)?;

    let futures: Vec<_> = search_response
        .result
        .iter()
        .map(|point| extract_from_point(point, pg_pool))
        .collect();

    let converted = futures::future::try_join_all(futures).await?;

    Ok(converted)
}

async fn extract_from_point(
    point: &ScoredPoint,
    pg_pool: &PgPool,
) -> Result<SearchResult, AppError> {
    //dbg!(&point);

    let chat_id = point
        .payload
        .get("chat_id")
        .and_then(get_int64_value)
        .unwrap_or(0)

        ;

    dbg!(point);

    let row = query!(r#"Select s.username, s.bias, s.channel_name, s.display_name, s.invite from sources as s where s.channel_id = $1"#, chat_id)
       .fetch_one(pg_pool)
       .await
       .map_err(TelegramDatabaseError)?;

    Ok(SearchResult {
        msg_id: point
            .payload
            .get("msg_id")
            .and_then(get_int_value)
            .unwrap_or(0),
        chat_id,
        posted_at: point
            .payload
            .get("posted_at")
            .and_then(get_date_value)
            .unwrap_or(DateTime::default()),
        similarity: point.score,
        display_name: row.display_name.unwrap_or(row.channel_name),
        bias: row.bias,
        user_name: row.username,
        invite_hash: row.invite,
        tags: vec!["test".to_string(), "tree".to_string()],
    })
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
    eprintln!("{:?}", &value);
    dbg!(&value);

    if let Some(value::Kind::StringValue(s)) = &value.kind {
        eprintln!("{:?}", &s);
        let date: DateTime<Utc> = DateTime::from(DateTime::parse_from_rfc3339(s).unwrap());

        eprintln!("{:?}", &date);

        Some(DateTime::from(DateTime::parse_from_rfc3339(s).unwrap()))
    } else {
        None
    }
}

pub async fn set_up(qdrant: Qdrant) -> Result<()> {
    qdrant
        .create_collection(
            CreateCollectionBuilder::new(IMAGES_COLLECTION).vectors_config(
                VectorParamsBuilder::new(imagenet::CLASS_COUNT as u64, Distance::Cosine),
            ),
        )
        .await?;

    Ok(())
}
