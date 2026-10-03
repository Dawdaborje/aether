//! Builds the kernel's media backend from `[media]` in `aether.toml`.

use std::sync::Arc;

use aether_storage::{MediaBackend, StorageError};
use thiserror::Error;

use crate::config_manager::models::{MediaBackendKind, MediaConfig};

#[derive(Debug, Error)]
pub enum MediaError {
    #[error("media backend `{0}` is selected but its settings are missing")]
    MissingSettings(&'static str),

    #[error(transparent)]
    Storage(#[from] StorageError),
}

pub async fn build_media_backend(
    config: &MediaConfig,
) -> Result<Arc<dyn MediaBackend>, MediaError> {
    match config.backend {
        MediaBackendKind::Local => {
            let local = config
                .local
                .as_ref()
                .ok_or(MediaError::MissingSettings("local"))?;
            Ok(Arc::new(local_storage::build(local).await?))
        }
        MediaBackendKind::S3 => {
            let s3 = config
                .s3
                .as_ref()
                .ok_or(MediaError::MissingSettings("s3"))?;
            Ok(Arc::new(s3_storage::build(s3)?))
        }
    }
}
