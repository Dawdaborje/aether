//! Twilio SMS. <https://www.twilio.com/docs/messaging/api/message-resource>

use std::time::Duration;

use aether_communication::{
    ConfigError, Field, REQUEST_TIMEOUT_SECS, SendError, Sms, SmsBridge, Spec, Values,
};
use async_trait::async_trait;

pub const SPEC: Spec = Spec {
    key: "twilio",
    label: "Twilio",
    account: &[
        Field { name: "account_sid", label: "Account SID", secret: false, required: true, default: None },
        Field { name: "auth_token", label: "Auth token", secret: true, required: true, default: None },
        Field { name: "from", label: "From number", secret: false, required: true, default: None },
    ],
    options: &[],
};

pub struct Twilio {
    account_sid: String,
    auth_token: String,
    from: String,
    base_url: String,
}

impl Twilio {
    pub fn new(values: &Values) -> Result<Self, ConfigError> {
        let get = |name| aether_communication::required(values, "twilio", name).map(str::to_string);
        Ok(Self {
            account_sid: get("account_sid")?,
            auth_token: get("auth_token")?,
            from: get("from")?,
            base_url: "https://api.twilio.com".into(),
        })
    }

    /// Talk to another server (tests).
    #[must_use]
    pub fn with_base_url(mut self, url: &str) -> Self {
        self.base_url = url.trim_end_matches('/').to_string();
        self
    }
}

#[async_trait]
impl SmsBridge for Twilio {
    async fn send(&self, sms: &Sms) -> Result<(), SendError> {
        let client = reqwest::Client::builder()
            .timeout(Duration::from_secs(REQUEST_TIMEOUT_SECS))
            .build()
            .map_err(|e| SendError::permanent(format!("could not prepare the request: {e}")))?;
        let url = format!("{}/2010-04-01/Accounts/{}/Messages.json", self.base_url, self.account_sid);
        let from = sms.sender.clone().unwrap_or_else(|| self.from.clone());
        let response = client
            .post(url)
            .basic_auth(&self.account_sid, Some(&self.auth_token))
            .form(&[("To", sms.to.as_str()), ("From", from.as_str()), ("Body", sms.text.as_str())])
            .send()
            .await
            .map_err(|e| SendError::transient(format!("Twilio could not be reached ({})", if e.is_timeout() { "timed out" } else { "network error" })))?;
        let status = response.status();
        if status.is_success() {
            return Ok(());
        }
        let body = response.text().await.unwrap_or_default();
        let detail = serde_json::from_str::<serde_json::Value>(&body)
            .ok()
            .and_then(|v| v.get("message").and_then(|m| m.as_str()).map(str::to_string))
            .unwrap_or(body);
        Err(SendError::from_status(status.as_u16(), &detail))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use aether_communication::testing::serve_once;

    fn values() -> Values {
        [("account_sid", "AC123"), ("auth_token", "tok"), ("from", "+15550001")]
            .into_iter()
            .map(|(k, v)| (k.to_string(), v.to_string()))
            .collect()
    }

    fn sms() -> Sms {
        Sms { to: "+2348012345678".into(), text: "hi & bye".into(), sender: None }
    }

    #[tokio::test]
    async fn posts_the_message_with_basic_auth() {
        let (port, seen) = serve_once(201, "{\"sid\":\"SM1\"}").await;
        let bridge = Twilio::new(&values()).unwrap().with_base_url(&format!("http://127.0.0.1:{port}"));
        bridge.send(&sms()).await.unwrap();
        let request = seen.await.unwrap();
        assert!(request.starts_with("POST /2010-04-01/Accounts/AC123/Messages.json"), "{request}");
        assert!(request.to_ascii_lowercase().contains("authorization: basic "), "{request}");
        assert!(request.contains("To=%2B2348012345678") && request.contains("From=%2B15550001"), "{request}");
        assert!(request.contains("Body=hi+%26+bye"), "{request}");
    }

    #[tokio::test]
    async fn bad_credentials_are_permanent_and_outages_are_not() {
        let (port, _) = serve_once(401, "{\"message\":\"Authenticate\"}").await;
        let bridge = Twilio::new(&values()).unwrap().with_base_url(&format!("http://127.0.0.1:{port}"));
        let error = bridge.send(&sms()).await.unwrap_err();
        assert!(error.permanent && error.message.contains("Authenticate"), "{error:?}");

        let (port, _) = serve_once(503, "down").await;
        let bridge = Twilio::new(&values()).unwrap().with_base_url(&format!("http://127.0.0.1:{port}"));
        assert!(!bridge.send(&sms()).await.unwrap_err().permanent);
    }

    #[test]
    fn missing_settings_are_named() {
        let mut values = values();
        values.remove("auth_token");
        assert!(Twilio::new(&values).err().is_some_and(|e| e.0.contains("auth_token")));
    }
}
