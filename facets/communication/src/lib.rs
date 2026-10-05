//! What a communication bridge is.
//!
//! A plugin asks the kernel to send a message of some *type* (`email`, `sms`, …) and the kernel
//! routes it to the bridge the organization configured for that type: SMTP, Resend, Twilio,
//! Termii, Africa's Talking and so on. A bridge is a small crate under `bridges/communications/`
//! that implements one of the traits here; it knows how to talk to one provider and nothing about
//! plugins, organizations or the database. The kernel hands it the provider's settings (read from
//! the settings the administrator filled in, secrets already decrypted) and a message already
//! checked and addressed to a single recipient.

use std::collections::HashMap;

use async_trait::async_trait;

/// A bridge's settings, by field name (`api_key`, `host`, …).
pub type Values = HashMap<String, String>;

/// One setting a bridge needs, described so the kernel can show it, check it and keep it safe.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Field {
    /// Name inside the bridge; the setting key is `bridge.<bridge>.<name>`.
    pub name: &'static str,
    pub label: &'static str,
    /// Stored encrypted and never shown again.
    pub secret: bool,
    pub required: bool,
    pub default: Option<&'static str>,
}

/// What a bridge needs configured.
///
/// `account` fields identify the provider account the messages are sent from (keys, passwords,
/// the sender number registered to that account). They always come from one place together: if
/// an organization fills in any of them it uses only its own, otherwise the global ones, so an
/// organization's account is never mixed with another's credentials. `options` only change
/// behaviour and fall back one by one.
#[derive(Debug, Clone, Copy)]
pub struct Spec {
    /// The bridge's key, as in `seeds/bridges` (`twilio`, `smtp`, …).
    pub key: &'static str,
    pub label: &'static str,
    pub account: &'static [Field],
    pub options: &'static [Field],
}

impl Spec {
    pub fn fields(&self) -> impl Iterator<Item = &Field> {
        self.account.iter().chain(self.options.iter())
    }
}

/// Why a send did not happen.
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
#[error("{message}")]
pub struct SendError {
    pub message: String,
    /// Trying again cannot help (the provider rejected the message or the credentials).
    pub permanent: bool,
}

impl SendError {
    /// Worth retrying: the provider was unreachable, busy or failed.
    pub fn transient(message: impl Into<String>) -> Self {
        Self { message: message.into(), permanent: false }
    }

    /// Not worth retrying.
    pub fn permanent(message: impl Into<String>) -> Self {
        Self { message: message.into(), permanent: true }
    }

    /// From an HTTP status the provider answered with: client errors are the message's or the
    /// account's fault (permanent) except "too many requests" and "timeout"; the rest may pass.
    pub fn from_status(status: u16, detail: &str) -> Self {
        let detail: String = detail.chars().take(200).collect();
        let message = format!("the provider answered {status}: {detail}");
        match status {
            408 | 425 | 429 => Self::transient(message),
            400..=499 => Self::permanent(message),
            _ => Self::transient(message),
        }
    }
}

/// One thing a bridge can do when a plugin calls it with `bridge::call`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Action {
    pub name: &'static str,
    pub help: &'static str,
}

/// A bridge a plugin calls by action name (`bridge::call`): payments, maps, documents and the like,
/// as opposed to the messaging bridges the kernel drives itself. Like every bridge it knows one
/// provider and nothing about plugins or the database: the kernel gives it the settings it
/// described in its [`Spec`] (secrets decrypted) and the plugin's parameters, and passes the answer
/// back. A [`SendError`] says whether trying again can help.
#[async_trait]
pub trait ActionBridge: Send + Sync {
    async fn call(&self, action: &str, params: &serde_json::Value) -> Result<serde_json::Value, SendError>;
}

/// A bridge cannot be built from the settings it was given.
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
#[error("{0}")]
pub struct ConfigError(pub String);

/// An email, already checked: one recipient, a subject, and a text and/or HTML body.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Email {
    pub from_address: String,
    pub from_name: Option<String>,
    pub to: String,
    pub reply_to: Option<String>,
    pub subject: String,
    pub text: Option<String>,
    pub html: Option<String>,
}

/// A text message, already checked: one recipient in international form (`+2348012345678`).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Sms {
    pub to: String,
    pub text: String,
    /// Sender name or number the administrator chose, if the provider takes one per message.
    pub sender: Option<String>,
}

#[async_trait]
pub trait EmailBridge: Send + Sync {
    async fn send(&self, email: &Email) -> Result<(), SendError>;
}

#[async_trait]
pub trait SmsBridge: Send + Sync {
    async fn send(&self, sms: &Sms) -> Result<(), SendError>;
}

/// The value of a required field, or an error that names it.
pub fn required<'a>(values: &'a Values, bridge: &str, name: &str) -> Result<&'a str, ConfigError> {
    values
        .get(name)
        .map(String::as_str)
        .filter(|value| !value.is_empty())
        .ok_or_else(|| ConfigError(format!("{bridge}: `{name}` is not set")))
}

/// How many seconds a bridge waits for its provider.
pub const REQUEST_TIMEOUT_SECS: u64 = 30;

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn statuses_are_sorted_into_retry_and_give_up() {
        for retry in [408, 425, 429, 500, 502, 503] {
            assert!(!SendError::from_status(retry, "x").permanent, "{retry}");
        }
        for give_up in [400, 401, 403, 404, 422] {
            assert!(SendError::from_status(give_up, "x").permanent, "{give_up}");
        }
    }

    #[test]
    fn a_missing_field_is_named() {
        let mut values = Values::new();
        values.insert("empty".into(), String::new());
        values.insert("set".into(), "v".into());
        assert_eq!(required(&values, "twilio", "set"), Ok("v"));
        assert_eq!(required(&values, "twilio", "empty").unwrap_err().0, "twilio: `empty` is not set");
        assert!(required(&values, "twilio", "absent").is_err());
    }

    #[test]
    fn long_provider_messages_are_cut() {
        let error = SendError::from_status(500, &"x".repeat(1000));
        assert!(error.message.len() < 260);
    }
}

/// A one-shot fake provider for tests.
#[cfg(feature = "testing")]
pub mod testing {
    use tokio::io::{AsyncReadExt, AsyncWriteExt};

    /// Listen on a free local port, answer the first request with `status` and `body`, and
    /// send what was received (request line, headers and body) through the returned channel.
    pub async fn serve_once(status: u16, body: &str) -> (u16, tokio::sync::oneshot::Receiver<String>) {
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.expect("bind a local port");
        let port = listener.local_addr().expect("local address").port();
        let (tx, rx) = tokio::sync::oneshot::channel();
        let reply = format!(
            "HTTP/1.1 {status} X\r\ncontent-type: application/json\r\ncontent-length: {}\r\nconnection: close\r\n\r\n{body}",
            body.len()
        );
        tokio::spawn(async move {
            let Ok((mut socket, _)) = listener.accept().await else { return };
            let mut seen = Vec::new();
            let mut buffer = [0u8; 4096];
            loop {
                let Ok(read) = socket.read(&mut buffer).await else { break };
                seen.extend_from_slice(&buffer[..read]);
                let text = String::from_utf8_lossy(&seen).to_string();
                if let Some(head_end) = text.find("\r\n\r\n") {
                    let length = text
                        .lines()
                        .find_map(|line| {
                            line.to_ascii_lowercase().strip_prefix("content-length:").map(|v| v.trim().parse::<usize>().unwrap_or(0))
                        })
                        .unwrap_or(0);
                    if seen.len() >= head_end + 4 + length {
                        break;
                    }
                }
                if read == 0 {
                    break;
                }
            }
            let _ = tx.send(String::from_utf8_lossy(&seen).to_string());
            let _ = socket.write_all(reply.as_bytes()).await;
            let _ = socket.shutdown().await;
        });
        (port, rx)
    }
}
