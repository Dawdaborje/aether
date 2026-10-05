//! Paystack payments. <https://paystack.com/docs/api/>
//!
//! A plugin starts a payment with `initialize_transaction`, sends the customer to the returned
//! `authorization_url`, and later confirms the result with `verify_transaction`. The secret key
//! stays in the kernel's settings; the plugin never sees it.

use std::time::Duration;

use aether_communication::{Action, ActionBridge, ConfigError, Field, REQUEST_TIMEOUT_SECS, SendError, Spec, Values};
use async_trait::async_trait;
use serde_json::{Value, json};

pub const SPEC: Spec = Spec {
    key: "paystack",
    label: "Paystack",
    account: &[Field { name: "secret_key", label: "Secret key", secret: true, required: true, default: None }],
    options: &[Field { name: "base_url", label: "API address", secret: false, required: false, default: Some("https://api.paystack.co") }],
};

pub const ACTIONS: &[Action] = &[
    Action {
        name: "initialize_transaction",
        help: "{ email, amount (in the currency's smallest unit, e.g. kobo), currency?, reference?, callback_url?, metadata? } -> { authorization_url, access_code, reference }",
    },
    Action {
        name: "verify_transaction",
        help: "{ reference } -> { status, reference, amount, currency, paid_at, channel, customer_email, gateway_response }",
    },
];

pub struct Paystack {
    secret_key: String,
    base_url: String,
}

impl Paystack {
    pub fn new(values: &Values) -> Result<Self, ConfigError> {
        Ok(Self {
            secret_key: aether_communication::required(values, "paystack", "secret_key")?.to_string(),
            base_url: values
                .get("base_url")
                .filter(|url| !url.is_empty())
                .map_or("https://api.paystack.co", String::as_str)
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

    async fn send(&self, request: reqwest::RequestBuilder) -> Result<Value, SendError> {
        let response = request
            .bearer_auth(&self.secret_key)
            .timeout(Duration::from_secs(REQUEST_TIMEOUT_SECS))
            .send()
            .await
            .map_err(|e| SendError::transient(format!("Paystack could not be reached ({})", if e.is_timeout() { "timed out" } else { "network error" })))?;
        let status = response.status();
        let body: Value = response.json().await.unwrap_or(Value::Null);
        let message = body["message"].as_str().unwrap_or("no message").to_string();
        if !status.is_success() {
            return Err(SendError::from_status(status.as_u16(), &message));
        }
        // Paystack answers 200 with `status: false` for a request it understood and refused.
        if body["status"].as_bool() != Some(true) {
            return Err(SendError::permanent(format!("Paystack refused it: {message}")));
        }
        Ok(body["data"].clone())
    }
}

fn text<'a>(params: &'a Value, name: &str) -> Result<&'a str, SendError> {
    params[name].as_str().filter(|text| !text.is_empty()).ok_or_else(|| SendError::permanent(format!("`{name}` is required (text)")))
}

fn plain_reference(reference: &str) -> bool {
    !reference.is_empty() && reference.len() <= 100 && reference.chars().all(|c| c.is_ascii_alphanumeric() || matches!(c, '.' | '-' | '=' | '_'))
}

#[async_trait]
impl ActionBridge for Paystack {
    async fn call(&self, action: &str, params: &Value) -> Result<Value, SendError> {
        match action {
            "initialize_transaction" => {
                let email = text(params, "email")?;
                if !email.contains('@') {
                    return Err(SendError::permanent("`email` is not an email address"));
                }
                let amount = params["amount"]
                    .as_u64()
                    .filter(|amount| *amount > 0)
                    .ok_or_else(|| SendError::permanent("`amount` is required: a whole number in the currency's smallest unit"))?;
                let mut body = json!({ "email": email, "amount": amount });
                for name in ["currency", "reference", "callback_url", "metadata"] {
                    if let Some(value) = params.get(name).filter(|value| !value.is_null()) {
                        body[name] = value.clone();
                    }
                }
                if let Some(reference) = body["reference"].as_str() {
                    if !plain_reference(reference) {
                        return Err(SendError::permanent("`reference` is letters, digits, `.`, `-`, `=` and `_` (at most 100)"));
                    }
                }
                let client = reqwest::Client::new();
                let data = self.send(client.post(format!("{}/transaction/initialize", self.base_url)).json(&body)).await?;
                Ok(json!({
                    "authorization_url": data["authorization_url"],
                    "access_code": data["access_code"],
                    "reference": data["reference"],
                }))
            }
            "verify_transaction" => {
                let reference = text(params, "reference")?;
                if !plain_reference(reference) {
                    return Err(SendError::permanent("`reference` is letters, digits, `.`, `-`, `=` and `_` (at most 100)"));
                }
                let client = reqwest::Client::new();
                let data = self.send(client.get(format!("{}/transaction/verify/{reference}", self.base_url))).await?;
                Ok(json!({
                    "status": data["status"],
                    "reference": data["reference"],
                    "amount": data["amount"],
                    "currency": data["currency"],
                    "paid_at": data["paid_at"],
                    "channel": data["channel"],
                    "customer_email": data["customer"]["email"],
                    "gateway_response": data["gateway_response"],
                }))
            }
            other => Err(SendError::permanent(format!(
                "paystack has no action `{other}`; it offers: {}",
                ACTIONS.iter().map(|a| a.name).collect::<Vec<_>>().join(", ")
            ))),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use aether_communication::testing::serve_once;

    fn bridge(port: u16) -> Paystack {
        let values: Values = [("secret_key".to_string(), "sk_test_123".to_string())].into_iter().collect();
        Paystack::new(&values).unwrap_or_else(|e| panic!("{e}")).with_base_url(&format!("http://127.0.0.1:{port}"))
    }

    #[tokio::test]
    async fn a_payment_is_started_with_the_secret_key_and_the_answer_is_trimmed() {
        let reply = r#"{"status":true,"message":"ok","data":{"authorization_url":"https://pay.example/x","access_code":"ac1","reference":"r1","extra":"hidden"}}"#;
        let (port, seen) = serve_once(200, reply).await;
        let answer = bridge(port)
            .call("initialize_transaction", &json!({ "email": "ann@example.com", "amount": 50000, "currency": "NGN", "metadata": { "order": 7 } }))
            .await;
        assert_eq!(answer, Ok(json!({ "authorization_url": "https://pay.example/x", "access_code": "ac1", "reference": "r1" })));
        let request = seen.await.unwrap_or_default();
        assert!(request.starts_with("POST /transaction/initialize"), "{request}");
        assert!(request.to_ascii_lowercase().contains("authorization: bearer sk_test_123"), "{request}");
        let body: Value = serde_json::from_str(request.split("\r\n\r\n").nth(1).unwrap_or("{}")).unwrap_or_default();
        assert_eq!((body["email"].as_str(), body["amount"].as_u64(), body["currency"].as_str()), (Some("ann@example.com"), Some(50000), Some("NGN")));
        assert_eq!(body["metadata"], json!({ "order": 7 }));
    }

    #[tokio::test]
    async fn a_payment_is_verified_by_reference() {
        let reply = r#"{"status":true,"message":"ok","data":{"status":"success","reference":"r1","amount":50000,"currency":"NGN","paid_at":"2026-10-05T12:00:00Z","channel":"card","customer":{"email":"ann@example.com"},"gateway_response":"Successful","log":"big"}}"#;
        let (port, seen) = serve_once(200, reply).await;
        let answer = bridge(port).call("verify_transaction", &json!({ "reference": "r1" })).await;
        assert_eq!(
            answer,
            Ok(json!({ "status": "success", "reference": "r1", "amount": 50000, "currency": "NGN", "paid_at": "2026-10-05T12:00:00Z", "channel": "card", "customer_email": "ann@example.com", "gateway_response": "Successful" }))
        );
        assert!(seen.await.unwrap_or_default().starts_with("GET /transaction/verify/r1"));
    }

    #[tokio::test]
    async fn mistakes_are_refused_before_anything_is_sent_and_refusals_are_permanent() {
        let b = bridge(1);
        for (action, params) in [
            ("initialize_transaction", json!({ "amount": 100 })),
            ("initialize_transaction", json!({ "email": "ann@example.com", "amount": 0 })),
            ("initialize_transaction", json!({ "email": "ann@example.com", "amount": 1.5 })),
            ("initialize_transaction", json!({ "email": "not-an-email", "amount": 100 })),
            ("initialize_transaction", json!({ "email": "a@b.co", "amount": 100, "reference": "has space" })),
            ("verify_transaction", json!({})),
            ("verify_transaction", json!({ "reference": "../etc" })),
            ("refund_everything", json!({})),
        ] {
            let error = b.call(action, &params).await.err();
            assert!(error.as_ref().is_some_and(|e| e.permanent), "{action} {params}: {error:?}");
        }
        // Paystack understood and said no (200 with status false), or the key is wrong (401).
        let (port, _) = serve_once(200, r#"{"status":false,"message":"Duplicate Transaction Reference"}"#).await;
        let refused = bridge(port).call("initialize_transaction", &json!({ "email": "a@b.co", "amount": 100, "reference": "dup" })).await;
        assert!(matches!(&refused, Err(e) if e.permanent && e.message.contains("Duplicate")), "{refused:?}");
        let (port, _) = serve_once(401, r#"{"status":false,"message":"Invalid key"}"#).await;
        assert!(matches!(bridge(port).call("verify_transaction", &json!({ "reference": "r" })).await, Err(e) if e.permanent));
        let (port, _) = serve_once(503, "{}").await;
        assert!(matches!(bridge(port).call("verify_transaction", &json!({ "reference": "r" })).await, Err(e) if !e.permanent));
    }

    #[test]
    fn the_key_is_required() {
        assert!(Paystack::new(&Values::new()).is_err());
    }
}
