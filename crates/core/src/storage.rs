use anyhow::{Result, anyhow};
use aws_config::{BehaviorVersion, Region};
use aws_sdk_s3::Client as S3Client;
use aws_sdk_s3::config::Credentials;
use image::DynamicImage;
use image::imageops::FilterType;

use crate::config::{optional, required};

const THUMB_MAX_EDGE: u32 = 400;
const THUMB_WEBP_QUALITY: f32 = 75.0;

pub struct ThumbStore {
    client: S3Client,
    bucket: String,
}

pub struct S3Settings {
    pub endpoint: String,
    pub bucket: String,
    pub region: String,
    pub access_key: String,
    pub secret_key: String,
}

impl S3Settings {
    pub fn from_env() -> Result<Self> {
        Ok(Self {
            endpoint: required("S3_ENDPOINT")?,
            bucket: required("S3_BUCKET")?,
            region: optional("S3_REGION").unwrap_or_else(|| "us-east-1".into()),
            access_key: required("S3_ACCESS_KEY")?,
            secret_key: required("S3_SECRET_KEY")?,
        })
    }
}

impl ThumbStore {
    pub async fn new(settings: &S3Settings) -> Result<Self> {
        let shared = aws_config::defaults(BehaviorVersion::latest())
            .endpoint_url(&settings.endpoint)
            .region(Region::new(settings.region.clone()))
            .credentials_provider(Credentials::new(&settings.access_key, &settings.secret_key, None, None, "viz-env"))
            .load()
            .await;
        // Path-style addressing works for MinIO, R2 and S3 alike.
        let config = aws_sdk_s3::config::Builder::from(&shared).force_path_style(true).build();
        Ok(Self { client: S3Client::from_conf(config), bucket: settings.bucket.clone() })
    }

    pub async fn upload(&self, key: &str, webp_bytes: Vec<u8>) -> Result<()> {
        self.client
            .put_object()
            .bucket(&self.bucket)
            .key(key)
            .body(webp_bytes.into())
            .content_type("image/webp")
            .cache_control("public, max-age=31536000, immutable")
            .send()
            .await
            .map_err(|e| {
                anyhow!("uploading {key} to bucket {}: {}", self.bucket, aws_sdk_s3::error::DisplayErrorContext(e))
            })?;
        Ok(())
    }

    /// Creates the bucket if it's missing (private; see `API_SERVE_THUMBS` for serving it).
    pub async fn ensure_bucket(&self) -> Result<()> {
        if self.client.head_bucket().bucket(&self.bucket).send().await.is_ok() {
            return Ok(());
        }
        match self.client.create_bucket().bucket(&self.bucket).send().await {
            Ok(_) => {
                tracing::info!(bucket = %self.bucket, "created thumbnail bucket");
                Ok(())
            }
            Err(e) if e.as_service_error().is_some_and(|e| e.is_bucket_already_owned_by_you()) => Ok(()),
            Err(e) => Err(anyhow!("creating bucket {}: {}", self.bucket, aws_sdk_s3::error::DisplayErrorContext(e))),
        }
    }

    /// `None` if the object doesn't exist.
    pub async fn get(&self, key: &str) -> Result<Option<Vec<u8>>> {
        let object = match self.client.get_object().bucket(&self.bucket).key(key).send().await {
            Ok(object) => object,
            Err(e) if e.as_service_error().is_some_and(|e| e.is_no_such_key()) => return Ok(None),
            Err(e) => {
                return Err(anyhow!(
                    "fetching {key} from bucket {}: {}",
                    self.bucket,
                    aws_sdk_s3::error::DisplayErrorContext(e)
                ));
            }
        };
        let bytes = object.body.collect().await.map_err(|e| anyhow!("reading {key}: {e}"))?;
        Ok(Some(bytes.into_bytes().to_vec()))
    }

    /// Whether `key` has the shape `key_for_hash` produces, so arbitrary keys never reach S3.
    pub fn is_thumb_key(key: &str) -> bool {
        key.strip_suffix(".webp")
            .is_some_and(|hex| hex.len() == 16 && hex.bytes().all(|b| matches!(b, b'0'..=b'9' | b'a'..=b'f')))
    }

    /// Content-addressed, so re-uploading the same visual is idempotent.
    pub fn key_for_hash(dhash: i64) -> String {
        format!("{:016x}.webp", dhash as u64)
    }
}

pub fn public_url(base_url: &str, key: &str) -> String {
    format!("{}/{}", base_url.trim_end_matches('/'), key)
}

/// Longest edge at most 400px, lossy WebP at quality 75.
pub fn encode_thumbnail(img: &DynamicImage) -> Result<Vec<u8>> {
    let thumb = if img.width().max(img.height()) > THUMB_MAX_EDGE {
        img.resize(THUMB_MAX_EDGE, THUMB_MAX_EDGE, FilterType::Triangle)
    } else {
        img.clone()
    };
    let rgb = DynamicImage::ImageRgb8(thumb.to_rgb8());
    let encoder = webp::Encoder::from_image(&rgb).map_err(|e| anyhow!("webp encoder: {e}"))?;
    Ok(encoder.encode(THUMB_WEBP_QUALITY).to_vec())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn thumbnail_is_bounded_webp() {
        let img = DynamicImage::new_rgb8(1600, 900);
        let webp_bytes = encode_thumbnail(&img).unwrap();
        assert!(crate::media::is_image(&webp_bytes));
        let decoded = image::load_from_memory(&webp_bytes).unwrap();
        assert_eq!((decoded.width(), decoded.height()), (400, 225));
    }

    #[test]
    fn keys_and_urls() {
        assert_eq!(ThumbStore::key_for_hash(-1), "ffffffffffffffff.webp");
        assert_eq!(ThumbStore::key_for_hash(255), "00000000000000ff.webp");
        assert!(ThumbStore::is_thumb_key("00000000000000ff.webp"));
        assert!(!ThumbStore::is_thumb_key("00000000000000FF.webp"));
        assert!(!ThumbStore::is_thumb_key("../../secret.webp"));
        assert!(!ThumbStore::is_thumb_key("00000000000000ff.png"));
        assert_eq!(public_url("https://cdn.example/", "thumbs/a.webp"), "https://cdn.example/thumbs/a.webp");
    }
}
