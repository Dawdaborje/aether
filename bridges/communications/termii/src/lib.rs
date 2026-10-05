//! Termii SMS. <https://developers.termii.com/messaging-api>

use std::time::Duration;

use aether_communication::{ConfigError, Field, REQUEST_TIMEOUT_SECS, SendError, Sms, SmsBridge, Spec, Values};
use async_trait::async_trait;

pub const SPEC: Spec = Spec {
    key: "termii",
    label: "Termii",
    account: &[
        Field { name: "api_key", label: "API key", secret: true, required: true, default: None },
        Field { name: "sender_id", label: "Sender ID", secret: false, required: true, default: None },
    ],
    options: &[
        Field { name: "channel", label: "Channel (generic, dnd or whatsapp)", secret: false, required: false, default: Some("generic") },
        Field { name: "base_url", label: "API address", secret: false, required: false, default: Some("https://api.ng.termii.com") },
    ],
};

pub struct Termii {
    api_key: String,
    sender_id: String,
    channel: String,
    base_url: String,
}

impl Termii {
    pub fn new(values: &Values) -> Result<Self, ConfigError> {
        let channel = values.get("channel").filter(|c| !c.is_empty()).cloned().unwrap_or_else(|| "generic".into());
        if !["generic", "dnd", "whatsapp"].contains(&channel.as_str()) {
            return Err(ConfigError(format!("termii: channel `{channel}` is not generic, dnd or whatsapp")));
        }
        Ok(Self {
            api_key: aether_communication::required(values, "termii", "api_key")?.to_string(),
            sender_id: aether_communication::required(values, "termii", "sender_id")?.to_string(),
            channel,
            base_url: values
                .get("base_url")
                .filter(|u| !u.is_empty())
                .map_or("https://api.ng.termii.com", String::as_str)
                .trim_end_matches('/')
                .to_string(),
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
impl SmsBridge for Termii {
    async fn send(&self, sms: &Sms) -> Result<(), SendError> {
        let client = reqwest::Client::builder()
            .timeout(Duration::from_secs(REQUEST_TIMEOUT_SECS))
            .build()
            .map_err(|e| SendError::permanent(format!("could not prepare the request: {e}")))?;
        // Termii wants the number without the leading plus.
        let to = sms.to.trim_start_matches('+');
        let response = client
            .post(format!("{}/api/sms/send", self.base_url))
            .json(&serde_json::json!({
                "to": to,
                "from": sms.sender.clone().unwrap_or_else(|| self.sender_id.clone()),
                "sms": sms.text,
                "type": "plain",
                "channel": self.channel,
                "api_key": self.api_key,
            }))
            .send()
            .await
            .map_err(|e| SendError::transient(format!("Termii could not be reached ({})", if e.is_timeout() { "timed out" } else { "network error" })))?;
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
        [("api_key", "key1"), ("sender_id", "Acme")].into_iter().map(|(k, v)| (k.to_string(), v.to_string())).collect()
    }

    #[tokio::test]
    async fn sends_json_with_the_number_unprefixed() {
        let (port, seen) = serve_once(200, "{\"message\":\"Successfully Sent\"}").await;
        let bridge = Termii::new(&values()).unwrap().with_base_url(&format!("http://127.0.0.1:{port}"));
        bridge.send(&Sms { to: "+2348012345678".into(), text: "hello".into(), sender: None }).await.unwrap();
        let request = seen.await.unwrap();
        assert!(request.starts_with("POST /api/sms/send"), "{request}");
        let body: serde_json::Value = serde_json::from_str(request.split("\r\n\r\n").nth(1).unwrap()).unwrap();
        assert_eq!(body["to"], "2348012345678");
        assert_eq!(body["from"], "Acme");
        assert_eq!(body["sms"], "hello");
        assert_eq!(body["channel"], "generic");
        assert_eq!(body["api_key"], "key1");
    }

    #[tokio::test]
    async fn a_rejection_is_permanent() {
        let (port, _) = serve_once(400, "{\"message\":\"Invalid number\"}").await;
        let bridge = Termii::new(&values()).unwrap().with_base_url(&format!("http://127.0.0.1:{port}"));
        let error = bridge.send(&Sms { to: "+1".into(), text: "x".into(), sender: None }).await.unwrap_err();
        assert!(error.permanent && error.message.contains("Invalid number"), "{error:?}");
    }

    #[test]
    fn the_channel_is_checked() {
        let mut values = values();
        values.insert("channel".into(), "pigeon".into());
        assert!(Termii::new(&values).is_err());
    }
}
