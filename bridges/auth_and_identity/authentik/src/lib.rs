use aether_auth_bridge::{AuthBridge, BridgeError, ExternalIdentity};
use async_trait::async_trait;
use url::Url;

#[derive(Debug, Default, Clone)]
pub struct AuthentikBridge;

#[async_trait]
impl AuthBridge for AuthentikBridge {
    fn name(&self) -> &'static str {
        "authentik"
    }

    async fn authorize_url(&self, _state: &str, _redirect_uri: &str) -> Result<Url, BridgeError> {
        Err(BridgeError::NotConfigured("authentik".into()))
    }

    async fn exchange_code(
        &self,
        _code: &str,
        _redirect_uri: &str,
    ) -> Result<ExternalIdentity, BridgeError> {
        Err(BridgeError::NotConfigured("authentik".into()))
    }
}
