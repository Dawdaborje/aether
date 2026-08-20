use aether_auth_bridge::{AuthBridge, BridgeError, ExternalIdentity};
use async_trait::async_trait;
use serde::Deserialize;
use url::Url;

#[derive(Debug, Clone)]
pub struct GoogleAuthBridge {
    pub client_id: String,
    pub client_secret: String,
}

impl GoogleAuthBridge {
    pub fn from_env() -> Option<Self> {
        let client_id = std::env::var("AETHER_GOOGLE_CLIENT_ID").ok()?;
        let client_secret = std::env::var("AETHER_GOOGLE_CLIENT_SECRET").ok()?;
        if client_id.is_empty() || client_secret.is_empty() {
            return None;
        }
        Some(Self {
            client_id,
            client_secret,
        })
    }

    pub fn new(client_id: impl Into<String>, client_secret: impl Into<String>) -> Self {
        Self {
            client_id: client_id.into(),
            client_secret: client_secret.into(),
        }
    }
}

#[async_trait]
impl AuthBridge for GoogleAuthBridge {
    fn name(&self) -> &'static str {
        "google"
    }

    async fn authorize_url(&self, state: &str, redirect_uri: &str) -> Result<Url, BridgeError> {
        let mut url = Url::parse("https://accounts.google.com/o/oauth2/v2/auth")
            .map_err(|e| BridgeError::OAuth(e.to_string()))?;
        {
            let mut q = url.query_pairs_mut();
            q.append_pair("client_id", &self.client_id);
            q.append_pair("redirect_uri", redirect_uri);
            q.append_pair("response_type", "code");
            q.append_pair("scope", "openid email profile");
            q.append_pair("access_type", "online");
            q.append_pair("include_granted_scopes", "true");
            q.append_pair("state", state);
            q.append_pair("prompt", "select_account");
        }
        Ok(url)
    }

    async fn exchange_code(
        &self,
        code: &str,
        redirect_uri: &str,
    ) -> Result<ExternalIdentity, BridgeError> {
        let client = reqwest::Client::new();

        #[derive(Deserialize)]
        struct TokenResponse {
            access_token: String,
            refresh_token: Option<String>,
            id_token: Option<String>,
        }

        let token: TokenResponse = client
            .post("https://oauth2.googleapis.com/token")
            .form(&[
                ("code", code),
                ("client_id", self.client_id.as_str()),
                ("client_secret", self.client_secret.as_str()),
                ("redirect_uri", redirect_uri),
                ("grant_type", "authorization_code"),
            ])
            .send()
            .await
            .map_err(|e| BridgeError::Http(e.to_string()))?
            .error_for_status()
            .map_err(|e| BridgeError::OAuth(e.to_string()))?
            .json()
            .await
            .map_err(|e| BridgeError::Http(e.to_string()))?;

        #[derive(Deserialize)]
        struct UserInfo {
            sub: String,
            email: Option<String>,
            name: Option<String>,
        }

        let info: UserInfo = client
            .get("https://openidconnect.googleapis.com/v1/userinfo")
            .bearer_auth(&token.access_token)
            .send()
            .await
            .map_err(|e| BridgeError::Http(e.to_string()))?
            .error_for_status()
            .map_err(|e| BridgeError::OAuth(e.to_string()))?
            .json()
            .await
            .map_err(|e| BridgeError::Http(e.to_string()))?;

        Ok(ExternalIdentity {
            provider: "google".into(),
            provider_user_id: info.sub,
            email: info.email,
            display_name: info.name,
            access_token: Some(token.access_token),
            refresh_token: token.refresh_token,
            raw_claims: token.id_token.map(|t| serde_json::json!({ "id_token": t })),
        })
    }
}
