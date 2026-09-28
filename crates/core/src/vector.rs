use anyhow::{Context, Result};
use qdrant_client::Qdrant;
use qdrant_client::qdrant::point_id::PointIdOptions;
use qdrant_client::qdrant::{CreateCollectionBuilder, Distance, PointStruct, QueryPointsBuilder, UpsertPointsBuilder, VectorParamsBuilder};
use uuid::Uuid;

/// CLIP embeddings in Qdrant. Point ids are `media_items.embedding_id`, so results are
/// joined back to Postgres by that UUID.
pub struct VectorIndex {
    client: Qdrant,
    collection: String,
}

impl VectorIndex {
    pub fn connect(url: &str, collection: &str) -> Result<Self> {
        let client = Qdrant::from_url(url).build().context("building Qdrant client")?;
        Ok(Self { client, collection: collection.to_string() })
    }

    pub async fn ensure_collection(&self, dim: usize) -> Result<()> {
        if self.client.collection_exists(&self.collection).await? {
            return Ok(());
        }
        self.client
            .create_collection(
                CreateCollectionBuilder::new(&self.collection)
                    .vectors_config(VectorParamsBuilder::new(dim as u64, Distance::Cosine)),
            )
            .await
            .with_context(|| format!("creating Qdrant collection {}", self.collection))?;
        tracing::info!(collection = %self.collection, dim, "created Qdrant collection");
        Ok(())
    }

    pub async fn upsert(&self, embedding_id: Uuid, vector: Vec<f32>, dhash: i64) -> Result<()> {
        let payload = qdrant_client::Payload::from([("dhash", qdrant_client::qdrant::Value::from(dhash))]);
        let point = PointStruct::new(embedding_id.to_string(), vector, payload);
        self.client
            .upsert_points(UpsertPointsBuilder::new(&self.collection, vec![point]).wait(true))
            .await?;
        Ok(())
    }

    /// Returns `(embedding_id, cosine similarity)`, best first.
    pub async fn search(&self, vector: Vec<f32>, limit: u64) -> Result<Vec<(Uuid, f32)>> {
        let response = self
            .client
            .query(QueryPointsBuilder::new(&self.collection).query(vector).limit(limit))
            .await?;
        Ok(response
            .result
            .into_iter()
            .filter_map(|point| match point.id?.point_id_options? {
                PointIdOptions::Uuid(id) => Some((Uuid::parse_str(&id).ok()?, point.score)),
                PointIdOptions::Num(_) => None,
            })
            .collect())
    }
}
