//! Africa's Talking SMS. <https://developers.africastalking.com/docs/sms/sending/bulk>

use std::time::Duration;

use aether_communication::{ConfigError, Field, REQUEST_TIMEOUT_SECS, SendError, Sms, SmsBridge, Spec, Values};
use async_trait::async_trait;

pub const SPEC: Spec = Spec {
    key: "africas_talking",
    label: "Africa's Talking",
    account: &[
        Field { name: "username", label: "Username", secret: false, required: true, default: None },
        Field { name: "api_key", label: "API key", secret: true, required: true, default: None },
        Field { name: "sender", label: "Sender ID (optional)", secret: false, required: false, default: None },
    ],
    options: &[Field {
        name: "base_url",
        label: "API address (use https://api.sandbox.africastalking.com to test)",
        secret: false,
        required: false,
        default: Some("https://api.africastalking.com"),
    }],
};

pub struct AfricasTalking {
    username: String,
    api_key: String,
    sender: Option<String>,
    base_url: String,
}

impl AfricasTalking {
    pub fn new(values: &Values) -> Result<Self, ConfigError> {
        Ok(Self {
            username: aether_communication::required(values, "africas_talking", "username")?.to_string(),
            api_key: aether_communication::required(values, "africas_talking", "api_key")?.to_string(),
            sender: values.get("sender").filter(|s| !s.is_empty()).cloned(),
            base_url: values
                .get("base_url")
                .filter(|u| !u.is_empty())
                .map_or("https://api.africastalking.com", String::as_str)
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
impl SmsBridge for AfricasTalking {
    async fn send(&self, sms: &Sms) -> Result<(), SendError> {
        let client = reqwest::Client::builder()
            .timeout(Duration::from_secs(REQUEST_TIMEOUT_SECS))
            .build()
            .map_err(|e| SendError::permanent(format!("could not prepare the request: {e}")))?;
        let mut form = vec![("username", self.username.clone()), ("to", sms.to.clone()), ("message", sms.text.clone())];
        if let Some(sender) = sms.sender.clone().or_else(|| self.sender.clone()) {
            form.push(("from", sender));
        }
        let response = client
            .post(format!("{}/version1/messaging", self.base_url))
            .header("apiKey", &self.api_key)
            .header("Accept", "application/json")
            .form(&form)
            .send()
            .await
            .map_err(|e| SendError::transient(format!("Africa's Talking could not be reached ({})", if e.is_timeout() { "timed out" } else { "network error" })))?;
        let status = response.status();
        let body = response.text().await.unwrap_or_default();
        if !status.is_success() {
            return Err(SendError::from_status(status.as_u16(), &body));
        }
        // A 201 can still carry a refusal for the recipient.
        let parsed: serde_json::Value = serde_json::from_str(&body).unwrap_or_default();
        let recipient = parsed["SMSMessageData"]["Recipients"].get(0);
        match recipient.and_then(|r| r["status"].as_str()) {
            Some("Success") => Ok(()),
            Some(other) => Err(SendError::permanent(format!("Africa's Talking did not send it: {other}"))),
            None => Err(SendError::permanent(format!(
                "Africa's Talking did not accept it: {}",
                parsed["SMSMessageData"]["Message"].as_str().unwrap_or("no recipient in the answer")
            ))),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use aether_communication::testing::serve_once;

    fn values() -> Values {
        [("username", "acme"), ("api_key", "k1"), ("sender", "ACME")].into_iter().map(|(k, v)| (k.to_string(), v.to_string())).collect()
    }

    fn sms() -> Sms {
        Sms { to: "+254711000111".into(), text: "hello".into(), sender: None }
    }

    #[tokio::test]
    async fn posts_a_form_with_the_api_key_header() {
        let ok = r#"{"SMSMessageData":{"Message":"Sent to 1/1","Recipients":[{"status":"Success","statusCode":101}]}}"#;
        let (port, seen) = serve_once(201, ok).await;
        let bridge = AfricasTalking::new(&values()).unwrap().with_base_url(&format!("http://127.0.0.1:{port}"));
        bridge.send(&sms()).await.unwrap();
        let request = seen.await.unwrap();
        assert!(request.starts_with("POST /version1/messaging"), "{request}");
        assert!(request.to_ascii_lowercase().contains("apikey: k1"), "{request}");
        assert!(request.contains("username=acme") && request.contains("to=%2B254711000111") && request.contains("from=ACME"), "{request}");
    }

    #[tokio::test]
    async fn a_refused_recipient_is_an_error_even_on_a_201() {
        let refused = r#"{"SMSMessageData":{"Message":"Sent to 0/1","Recipients":[{"status":"InvalidPhoneNumber","statusCode":403}]}}"#;
        let (port, _) = serve_once(201, refused).await;
        let bridge = AfricasTalking::new(&values()).unwrap().with_base_url(&format!("http://127.0.0.1:{port}"));
        let error = bridge.send(&sms()).await.unwrap_err();
        assert!(error.permanent && error.message.contains("InvalidPhoneNumber"), "{error:?}");
    }
}
