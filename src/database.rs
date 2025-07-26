use crate::error::AppError::TelegramDatabaseError;
use crate::{AppError, SearchResult, UploadParams};
use anyhow::Result;
use chrono::{DateTime, Utc};


use qdrant_client::qdrant::{value, CreateCollectionBuilder, ScoredPoint, SearchParamsBuilder, SearchPointsBuilder, UpsertPointsBuilder, VectorParamsBuilder, Filter, Condition, FieldCondition, Range, Distance, Value, PointStruct};
use qdrant_client::{Payload, Qdrant};
use serde_json::json;
use sqlx::{query, PgPool};
use tch::vision::imagenet;
use uuid::Uuid;
use log::{info, debug, warn, error};
use tch::vision::imagenet::CLASSES;

pub async fn insert_images(
    qdrant: &Qdrant,
    metadata: &UploadParams,
    vectors: Vec<f32>,
    base64str: String
) -> Result<String, AppError> {
    let collection_name = &metadata.collection;

    // Check if collection exists
    if !collection_exists(qdrant, collection_name).await? {
        error!("Collection '{}' does not exist", collection_name);
        return Err(AppError::CollectionNotFound(collection_name.to_string()));
    }

    let payload: Payload = json!(
        {
            "chat_id": metadata.chat_id,
            "msg_id": metadata.msg_id,
            "posted_at": metadata.posted_at.to_rfc3339(),
            "base64": base64str
        }
    )
        .try_into()?;

    let id = Uuid::new_v4().to_string();
    debug!("Inserting image with ID: {}, chat_id: {}, msg_id: {} into collection: {}",
           id, metadata.chat_id, metadata.msg_id, collection_name);

    let points = vec![PointStruct::new(id, vectors, payload)];
    qdrant
        .upsert_points(UpsertPointsBuilder::new(collection_name, points))
        .await
        .map_err(AppError::VectorDatabaseError)?;

    info!("Successfully inserted image for chat_id: {}, msg_id: {} into collection: {}",
          metadata.chat_id, metadata.msg_id, collection_name);

    Ok(collection_name.to_string())
}

pub async fn search_vectors(
    qdrant: &Qdrant,
    pg_pool: &PgPool,
    query_vector: Vec<f32>,
    limit: u64,
    start_from: Option<DateTime<Utc>>,
    start_after: Option<DateTime<Utc>>,
    collection_name: String
) -> Result<Vec<SearchResult>,AppError> {

    // Check if collection exists
    if !collection_exists(qdrant, &collection_name).await? {
        error!("Collection '{}' does not exist", collection_name);
        return Err(AppError::CollectionNotFound(collection_name.to_string()));
    }

    debug!("Searching vectors in collection '{}' with limit: {}, date filters: from={:?}, after={:?}",
           collection_name, limit, start_from, start_after);

    let mut search_builder = SearchPointsBuilder::new(&collection_name, query_vector, limit)
        .params(SearchParamsBuilder::default().hnsw_ef(256).exact(false))
        .with_payload(true)
        .with_vectors(false);

    // Add date filters if provided
    if let Some(filter) = build_date_filter(start_from, start_after) {
        search_builder = search_builder.filter(filter);
    }

    let search_response = qdrant
        .search_points(search_builder)
        .await
        .map_err(AppError::VectorDatabaseError)?;

    debug!("Vector search in '{}' returned {} points", collection_name, search_response.result.len());

    let futures: Vec<_> = search_response
        .result
        .iter()
        .map(|point| extract_from_point(point, pg_pool))
        .collect();

    let converted = futures::future::try_join_all(futures).await?;
    info!("Successfully processed {} search results from collection '{}'", converted.len(), collection_name);

    Ok(converted)
}

pub async fn search_by_tags(
    qdrant: &Qdrant,
    pg_pool: &PgPool,
    tags: Vec<String>,
    limit: u64,
    start_from: Option<DateTime<Utc>>,
    start_after: Option<DateTime<Utc>>,
    collection_name: String,
) -> Result<Vec<SearchResult>, AppError> {

    // Check if collection exists
    if !collection_exists(qdrant, &*collection_name).await? {
        error!("Collection '{}' does not exist", collection_name);
        return Err(AppError::CollectionNotFound(collection_name.to_string()));
    }

    info!("Searching by tags: {:?} in collection '{}'", tags, collection_name);

    // Convert tags to a query vector using ImageNet class matching
    let query_vector = tags_to_vector(&tags)?;

    debug!("Generated query vector from {} tags for collection '{}'", tags.len(), collection_name);

    let mut search_builder = SearchPointsBuilder::new(&collection_name, query_vector, limit)
        .params(SearchParamsBuilder::default().hnsw_ef(256).exact(false))
        .with_payload(true)
        .with_vectors(false);

    // Add date filters if provided
    if let Some(filter) = build_date_filter(start_from, start_after) {
        search_builder = search_builder.filter(filter);
    }

    let search_response = qdrant
        .search_points(search_builder)
        .await
        .map_err(AppError::VectorDatabaseError)?;

    debug!("Tag search in '{}' returned {} points", collection_name, search_response.result.len());

    let futures: Vec<_> = search_response
        .result
        .iter()
        .map(|point| extract_from_point(point, pg_pool))
        .collect();

    let converted = futures::future::try_join_all(futures).await
        .map_err(|e| {
            error!("Failed to process search results from collection '{}': {:?}", collection_name, e);
            AppError::Unknown
        })?;

    info!("Successfully processed {} tag search results from collection '{}'", converted.len(), collection_name);
    Ok(converted)
}

fn build_date_filter(
    start_from: Option<DateTime<Utc>>,
    start_after: Option<DateTime<Utc>>,
) -> Option<Filter> {
    if start_from.is_none() && start_after.is_none() {
        return None;
    }

    let mut conditions = Vec::new();

    if let Some(from_date) = start_from {
        debug!("Adding start_from filter: {}", from_date);
        conditions.push(Condition {
            condition_one_of: Some(
                qdrant_client::qdrant::condition::ConditionOneOf::Field(
                    FieldCondition {
                        key: "posted_at".to_string(),
                        r#match: None,
                        range: Some(Range {
                            gte: Some(from_date.to_rfc3339().parse().unwrap()),
                            gt: None,
                            lte: None,
                            lt: None,
                        }),
                        geo_bounding_box: None,
                        geo_radius: None,
                        values_count: None,
                        geo_polygon: None,
                        datetime_range: None,
                        is_empty: None,
                        is_null: None,
                    }
                )
            )
        });
    }

    if let Some(after_date) = start_after {
        debug!("Adding start_after filter: {}", after_date);
        conditions.push(Condition {
            condition_one_of: Some(
                qdrant_client::qdrant::condition::ConditionOneOf::Field(
                    FieldCondition {
                        key: "posted_at".to_string(),
                        r#match: None,
                        range: Some(Range {
                            gte: None,
                            gt: Some(after_date.to_rfc3339().parse().unwrap()),
                            lte: None,
                            lt: None,
                        }),
                        geo_bounding_box: None,
                        geo_radius: None,
                        values_count: None,
                        geo_polygon: None,
                        datetime_range: None,
                        is_empty: None,
                        is_null: None,
                    }
                )
            )
        });
    }

    if conditions.is_empty() {
        None
    } else {
        Some(Filter {
            should: vec![],
            min_should: None,
            must: conditions,
            must_not: vec![],
        })
    }
}

fn tags_to_vector(tags: &[String]) -> Result<Vec<f32>, AppError> {
    // Create a vector with the same dimensionality as ImageNet classes (1000)
    let mut query_vector = vec![0.0f32; imagenet::CLASS_COUNT as usize];

    // Get all ImageNet class names from the tch crate
    let imagenet_classes = get_all_imagenet_classes();

    let mut matches_found = 0;

    // For each input tag, find matching ImageNet classes
    for tag in tags {
        let tag_lower = tag.to_lowercase();
        debug!("Processing tag: {}", tag);

        // Search through all ImageNet classes for matches
        for (idx, class_name) in imagenet_classes.iter().enumerate() {
            let class_lower = class_name.to_lowercase();

            // Check for exact match or partial match
            if class_lower == tag_lower ||
                class_lower.contains(&tag_lower) ||
                tag_lower.contains(&class_lower) ||
                // Handle compound class names (e.g., "German shepherd" matches "dog")
                class_lower.split_whitespace().any(|word| word == tag_lower || tag_lower.contains(word)) {
                query_vector[idx] = 1.0;
                matches_found += 1;
                debug!("Match for tag '{}' with ImageNet class '{}' at index {}", tag, class_name, idx);
            }
        }
    }

    if matches_found == 0 {
        warn!("No matches found for tags: {:?}. Using uniform distribution.", tags);
        // If no matches found, create a small uniform distribution
        let uniform_weight = 1.0 / (imagenet::CLASS_COUNT as f32);
        query_vector.fill(uniform_weight);
    } else {
        // Normalize the vector
        let sum: f32 = query_vector.iter().sum();
        if sum > 0.0 {
            for val in query_vector.iter_mut() {
                *val /= sum;
            }
        }
    }

    info!("Found {} matches for {} input tags", matches_found, tags.len());
    Ok(query_vector)
}

fn get_all_imagenet_classes() -> [&'static str; 1000] {
    // Use the actual ImageNet class names from the tch crate
    // The imagenet module provides access to all 1000 class names
    CLASSES
}

async fn extract_from_point(
    point: &ScoredPoint,
    pg_pool: &PgPool,
) -> Result<SearchResult, AppError> {
    let chat_id = point
        .payload
        .get("chat_id")
        .and_then(get_int64_value)
        .unwrap_or(0);

    debug!("Extracting data for chat_id: {}", chat_id);

    let row = query!(
        r#"SELECT s.username, s.bias, s.channel_name, s.display_name, s.invite
           FROM sources as s WHERE s.channel_id = $1"#,
        chat_id
    )
        .fetch_one(pg_pool)
        .await
        .map_err(|e| {
            error!("Query failed for chat_id {}: {:?}", chat_id, e);
            TelegramDatabaseError(e)
        })?;

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
            .unwrap_or_default(),
        similarity: point.score,
        display_name: row.display_name.unwrap_or(row.channel_name),
        bias: row.bias,
        user_name: row.username,
        invite_hash: row.invite,
        tags: vec!["test".to_string(), "tree".to_string()],
        img: point
            .payload
            .get("base64")
            .and_then(get_string_value)
            .unwrap_or_default()
    })
}

fn get_string_value(value: &Value) -> Option<String> {
    if let Some(value::Kind::StringValue(s)) = &value.kind {
        Some(s.to_string())
    } else {
        None
    }
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
        DateTime::parse_from_rfc3339(s)
            .map(|dt| dt.with_timezone(&Utc))
            .map_err(|e| {
                warn!("Failed to parse date string '{}': {:?}", s, e);
                e
            })
            .ok()
    } else {
        None
    }
}

async fn collection_exists(qdrant: &Qdrant, collection_name: &str) -> Result<bool, AppError> {
    match qdrant.collection_info(collection_name).await {
        Ok(_) => {
            debug!("Collection '{}' exists", collection_name);
            Ok(true)
        },
        Err(e) => {
            debug!("Collection '{}' does not exist: {:?}", collection_name, e);
            Ok(false)
        }
    }
}

pub async fn set_up(qdrant: &Qdrant) -> Result<()> {
  /*  info!("Setting up Qdrant collection: {}", IMAGES_COLLECTION);

    qdrant
        .create_collection(
            CreateCollectionBuilder::new(IMAGES_COLLECTION).vectors_config(
                VectorParamsBuilder::new(imagenet::CLASS_COUNT as u64, Distance::Cosine),
            ),
        )
        .await?;

    info!("Successfully created collection: {}", IMAGES_COLLECTION); */
    Ok(())
}

pub async fn create_collection(qdrant: &Qdrant, collection_name: &str) -> Result<(), AppError> {
    info!("Creating new collection: {}", collection_name);

    qdrant
        .create_collection(
            CreateCollectionBuilder::new(collection_name).vectors_config(
                VectorParamsBuilder::new(imagenet::CLASS_COUNT as u64, Distance::Cosine),
            ),
        )
        .await
        .map_err(AppError::VectorDatabaseError)?;

    info!("Successfully created collection: {}", collection_name);
    Ok(())
}