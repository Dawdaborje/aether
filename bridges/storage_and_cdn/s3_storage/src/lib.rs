//! S3-compatible media storage bridge (AWS S3, Garage, MinIO, …).

pub mod model;

use std::sync::Arc;

use aether_storage::{ObjectStoreBackend, StorageError};
use object_store::{ObjectStore, aws::AmazonS3Builder, prefix::PrefixStore};

pub use model::S3StorageConfig;

/// Build an S3 media backend. Credentials missing from the config fall back to
/// the standard `AWS_*` environment variables.
pub fn build(config: &S3StorageConfig) -> Result<ObjectStoreBackend, StorageError> {
    if config.bucket.trim().is_empty() {
        return Err(StorageError::Config("media.s3.bucket is empty".into()));
    }

    let mut builder = AmazonS3Builder::from_env()
        .with_bucket_name(&config.bucket)
        .with_allow_http(config.allow_http)
        .with_virtual_hosted_style_request(config.virtual_hosted_style);
    if let Some(region) = &config.region {
        builder = builder.with_region(region);
    }
    if let Some(endpoint) = &config.endpoint {
        builder = builder.with_endpoint(endpoint);
    }
    if let Some(access_key_id) = &config.access_key_id {
        builder = builder.with_access_key_id(access_key_id);
    }
    if let Some(secret_access_key) = &config.secret_access_key {
        builder = builder.with_secret_access_key(secret_access_key);
    }
    let s3 = builder
        .build()
        .map_err(|error| StorageError::Backend(Box::new(error)))?;

    let store: Arc<dyn ObjectStore> = match config.prefix.as_deref().filter(|p| !p.is_empty()) {
        Some(prefix) => Arc::new(PrefixStore::new(s3, prefix)),
        None => Arc::new(s3),
    };
    Ok(ObjectStoreBackend::new(store))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn config() -> S3StorageConfig {
        S3StorageConfig {
            bucket: "aether-media".into(),
            region: Some("us-east-1".into()),
            endpoint: Some("http://127.0.0.1:3900".into()),
            access_key_id: Some("key".into()),
            secret_access_key: Some("secret".into()),
            allow_http: true,
            virtual_hosted_style: false,
            prefix: Some("tenants".into()),
        }
    }

    #[test]
    fn builds_without_contacting_the_network() {
        assert!(build(&config()).is_ok());
    }

    #[test]
    fn rejects_an_empty_bucket() {
        let config = S3StorageConfig {
            bucket: String::new(),
            ..config()
        };
        assert!(matches!(build(&config), Err(StorageError::Config(_))));
    }

    #[test]
    fn debug_output_hides_the_secret() {
        assert!(!format!("{:?}", config()).contains("secret\""));
    }
}
