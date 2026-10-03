use std::fmt;

use serde::{Deserialize, Serialize};

/// `[media.s3]` in `aether.toml`.
#[derive(Clone, Deserialize, Serialize)]
pub struct S3StorageConfig {
    pub bucket: String,
    pub region: Option<String>,
    /// Custom endpoint for S3-compatible services such as Garage or MinIO.
    pub endpoint: Option<String>,
    /// Falls back to `AWS_ACCESS_KEY_ID` when omitted.
    pub access_key_id: Option<String>,
    /// Falls back to `AWS_SECRET_ACCESS_KEY` when omitted.
    #[serde(skip_serializing)]
    pub secret_access_key: Option<String>,
    /// Permit plain-HTTP endpoints (local development only).
    #[serde(default)]
    pub allow_http: bool,
    /// Use `bucket.host` addressing instead of `host/bucket` path style.
    #[serde(default)]
    pub virtual_hosted_style: bool,
    /// Key prefix every object is stored under.
    pub prefix: Option<String>,
}

impl fmt::Debug for S3StorageConfig {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("S3StorageConfig")
            .field("bucket", &self.bucket)
            .field("region", &self.region)
            .field("endpoint", &self.endpoint)
            .field("access_key_id", &self.access_key_id)
            .field(
                "secret_access_key",
                &self.secret_access_key.as_ref().map(|_| "<redacted>"),
            )
            .field("allow_http", &self.allow_http)
            .field("virtual_hosted_style", &self.virtual_hosted_style)
            .field("prefix", &self.prefix)
            .finish()
    }
}
