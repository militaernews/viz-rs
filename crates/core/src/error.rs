#[derive(Debug, thiserror::Error)]
pub enum CoreError {
    #[error("database error: {0}")]
    Db(#[from] sqlx::Error),
    #[error("storage error: {0}")]
    Storage(anyhow::Error),
    #[error("invalid media: {0}")]
    InvalidMedia(String),
}

pub fn is_invalid_media(err: &anyhow::Error) -> bool {
    matches!(err.downcast_ref::<CoreError>(), Some(CoreError::InvalidMedia(_)))
}
