//! Resend email. <https://resend.com/docs/api-reference/emails/send-email>

use std::time::Duration;

use aether_communication::{ConfigError, Email, EmailBridge, Field, REQUEST_TIMEOUT_SECS, SendError, Spec, Values};
use async_trait::async_trait;

pub const SPEC: Spec = Spec {
    key: "resend",
    label: "Resend",
    account: &[Field { name: "api_key", label: "API key", secret: true, required: true, default: None }],
    options: &[],
};

pub struct Resend {
    api_key: String,
    base_url: String,
}

impl Resend {
    pub fn new(values: &Values) -> Result<Self, ConfigError> {
        Ok(Self {
            api_key: aether_communication::required(values, "resend", "api_key")?.to_string(),
            base_url: "https://api.resend.com".into(),
        })
    }

    /// Talk to another server (tests).
    #[must_use]
    pub fn with_base_url(mut self, url: &str) -> Self {
        self.base_url = url.trim_end_matches('/').to_string();
        self
    }
}

fn from_header(email: &Email) -> String {
    match &email.from_name {
        // Quotes and angle brackets would end the display name early.
        Some(name) => format!("{} <{}>", name.replace(['"', '<', '>'], ""), email.from_address),
        None => email.from_address.clone(),
    }
}

#[async_trait]
impl EmailBridge for Resend {
    async fn send(&self, email: &Email) -> Result<(), SendError> {
        let client = reqwest::Client::builder()
            .timeout(Duration::from_secs(REQUEST_TIMEOUT_SECS))
            .build()
            .map_err(|e| SendError::permanent(format!("could not prepare the request: {e}")))?;
        let mut body = serde_json::json!({
            "from": from_header(email),
            "to": [email.to],
            "subject": email.subject,
        });
        if let Some(text) = &email.text {
            body["text"] = text.clone().into();
        }
        if let Some(html) = &email.html {
            body["html"] = html.clone().into();
        }
        if let Some(reply_to) = &email.reply_to {
            body["reply_to"] = reply_to.clone().into();
        }
        let response = client
            .post(format!("{}/emails", self.base_url))
            .bearer_auth(&self.api_key)
            .json(&body)
            .send()
            .await
            .map_err(|e| SendError::transient(format!("Resend could not be reached ({})", if e.is_timeout() { "timed out" } else { "network error" })))?;
        let status = response.status();
        if status.is_success() {
            return Ok(());
        }
        let text = response.text().await.unwrap_or_default();
        let detail = serde_json::from_str::<serde_json::Value>(&text)
            .ok()
            .and_then(|v| v.get("message").and_then(|m| m.as_str()).map(str::to_string))
            .unwrap_or(text);
        Err(SendError::from_status(status.as_u16(), &detail))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use aether_communication::testing::serve_once;

    fn email() -> Email {
        Email {
            from_address: "billing@acme.test".into(),
            from_name: Some("Acme \"Billing\"".into()),
            to: "ann@example.com".into(),
            reply_to: Some("help@acme.test".into()),
            subject: "Your invoice".into(),
            text: Some("Total: 500".into()),
            html: None,
        }
    }

    fn bridge(port: u16) -> Resend {
        let values: Values = [("api_key".to_string(), "re_123".to_string())].into_iter().collect();
        Resend::new(&values).unwrap().with_base_url(&format!("http://127.0.0.1:{port}"))
    }

    #[tokio::test]
    async fn sends_json_with_a_bearer_key() {
        let (port, seen) = serve_once(200, "{\"id\":\"abc\"}").await;
        bridge(port).send(&email()).await.unwrap();
        let request = seen.await.unwrap();
        assert!(request.starts_with("POST /emails"), "{request}");
        assert!(request.to_ascii_lowercase().contains("authorization: bearer re_123"), "{request}");
        let body: serde_json::Value = serde_json::from_str(request.split("\r\n\r\n").nth(1).unwrap()).unwrap();
        assert_eq!(body["from"], "Acme Billing <billing@acme.test>");
        assert_eq!(body["to"], serde_json::json!(["ann@example.com"]));
        assert_eq!(body["text"], "Total: 500");
        assert_eq!(body["reply_to"], "help@acme.test");
        assert!(body.get("html").is_none());
    }

    #[tokio::test]
    async fn a_validation_error_is_permanent_and_a_rate_limit_is_not() {
        let (port, _) = serve_once(422, "{\"message\":\"domain not verified\"}").await;
        let error = bridge(port).send(&email()).await.unwrap_err();
        assert!(error.permanent && error.message.contains("domain not verified"), "{error:?}");
        let (port, _) = serve_once(429, "{}").await;
        assert!(!bridge(port).send(&email()).await.unwrap_err().permanent);
    }
}
