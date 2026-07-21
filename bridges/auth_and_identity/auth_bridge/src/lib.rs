use async_trait::async_trait;
use serde::{Deserialize, Serialize};
use serde_json::Value as JsonValue;
use thiserror::Error;
use url::Url;

#[derive(Debug, Error)]
pub enum BridgeError {
    #[error("bridge `{0}` is not configured")]
    NotConfigured(String),
    #[error("oauth error: {0}")]
    OAuth(String),
    #[error("http error: {0}")]
    Http(String),
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ExternalIdentity {
    pub provider: String,
    pub provider_user_id: String,
    pub email: Option<String>,
    pub display_name: Option<String>,
    pub access_token: Option<String>,
    pub refresh_token: Option<String>,
    pub raw_claims: Option<JsonValue>,
}

#[async_trait]
pub trait AuthBridge: Send + Sync {
    fn name(&self) -> &'static str;

    async fn authorize_url(&self, state: &str, redirect_uri: &str) -> Result<Url, BridgeError>;

    async fn exchange_code(
        &self,
        code: &str,
        redirect_uri: &str,
    ) -> Result<ExternalIdentity, BridgeError>;
}
