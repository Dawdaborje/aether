//! Video meetings through a Jitsi Meet server. <https://jitsi.github.io/handbook/docs/devops-guide/devops-guide-docker/#authentication>
//!
//! Nothing is called over the network: a meeting is a room on the server, and the bridge builds the link that opens it. With
//! `app_id` and `signing_key` set (the server's token authentication), the link carries a signed token (HS256) that names the
//! person, says whether they moderate, and is good for one room and a short time, so a link taken from a chat cannot open
//! another room or be used next week. Without them the link is the plain room address, which is how a public server such as
//! meet.jit.si works.

use aether_communication::{Action, ActionBridge, ConfigError, Field, SendError, Spec, Values};
use async_trait::async_trait;
use base64::{Engine, engine::general_purpose::URL_SAFE_NO_PAD};
use hmac::{Hmac, KeyInit, Mac};
use serde_json::{Value, json};
use sha2::Sha256;

pub const SPEC: Spec = Spec {
    key: "jitsi",
    label: "Jitsi Meet",
    account: &[
        Field { name: "app_id", label: "Application id of the server's token authentication (empty for a public server)", secret: false, required: false, default: None },
        Field { name: "signing_key", label: "Shared secret of the server's token authentication", secret: true, required: false, default: None },
    ],
    options: &[Field { name: "base_url", label: "Server address", secret: false, required: false, default: Some("https://meet.jit.si") }],
};

pub const ACTIONS: &[Action] = &[Action {
    name: "join_link",
    help: "{ room, name?, email?, moderator? (default false), ttl_secs? (60 to 86400, default 14400) } -> { url, room, signed }",
}];

pub struct Jitsi {
    base_url: String,
    app_id: Option<String>,
    signing_key: Option<String>,
}

impl Jitsi {
    pub fn new(values: &Values) -> Result<Self, ConfigError> {
        let get = |name: &str| values.get(name).map(|v| v.trim().to_string()).filter(|v| !v.is_empty());
        let base_url = get("base_url").unwrap_or_else(|| "https://meet.jit.si".into()).trim_end_matches('/').to_string();
        if !base_url.starts_with("https://") && !base_url.starts_with("http://") {
            return Err(ConfigError("the server address starts with https://".into()));
        }
        let (app_id, signing_key) = (get("app_id"), get("signing_key"));
        if app_id.is_some() != signing_key.is_some() {
            return Err(ConfigError("set both the application id and the shared secret, or neither".into()));
        }
        Ok(Self { base_url, app_id, signing_key })
    }

    fn host(&self) -> &str {
        self.base_url.split("://").nth(1).unwrap_or_default().split('/').next().unwrap_or_default()
    }

    fn token(&self, room: &str, name: &str, email: &str, moderator: bool, ttl: u64, now: u64) -> Option<String> {
        let (app, key) = (self.app_id.as_ref()?, self.signing_key.as_ref()?);
        let header = URL_SAFE_NO_PAD.encode(r#"{"alg":"HS256","typ":"JWT"}"#);
        let claims = json!({
            "iss": app, "aud": app, "sub": self.host(), "room": room, "nbf": now.saturating_sub(10), "exp": now + ttl,
            "context": { "user": { "name": name, "email": email, "moderator": moderator } },
            "moderator": moderator,
        });
        let body = URL_SAFE_NO_PAD.encode(claims.to_string());
        let mut mac = <Hmac<Sha256> as KeyInit>::new_from_slice(key.as_bytes()).ok()?;
        mac.update(format!("{header}.{body}").as_bytes());
        Some(format!("{header}.{body}.{}", URL_SAFE_NO_PAD.encode(mac.finalize().into_bytes())))
    }
}

fn now() -> u64 {
    std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).map_or(0, |d| d.as_secs())
}

fn room_name(params: &Value) -> Result<String, SendError> {
    let room = params["room"].as_str().map(str::trim).unwrap_or_default();
    if room.is_empty() || room.len() > 80 || !room.chars().all(|c| c.is_ascii_alphanumeric() || c == '-' || c == '_') {
        return Err(SendError::permanent("`room` is required: letters, digits, - and _, at most 80 characters"));
    }
    Ok(room.to_string())
}

fn percent(text: &str) -> String {
    text.bytes().map(|b| if b.is_ascii_alphanumeric() || b"-_.~".contains(&b) { (b as char).to_string() } else { format!("%{b:02X}") }).collect()
}

#[async_trait]
impl ActionBridge for Jitsi {
    async fn call(&self, action: &str, params: &Value) -> Result<Value, SendError> {
        if action != "join_link" {
            return Err(SendError::permanent(format!("jitsi has no action `{action}`; it offers: join_link")));
        }
        let room = room_name(params)?;
        let ttl = params["ttl_secs"].as_u64().unwrap_or(14_400);
        if !(60..=86_400).contains(&ttl) {
            return Err(SendError::permanent("`ttl_secs` is 60 to 86400"));
        }
        let text = |name: &str| params[name].as_str().unwrap_or_default().chars().take(120).collect::<String>();
        let (name, email) = (text("name"), text("email"));
        let moderator = params["moderator"].as_bool().unwrap_or(false);
        let mut url = format!("{}/{room}", self.base_url);
        let token = self.token(&room, &name, &email, moderator, ttl, now());
        if let Some(token) = &token {
            url.push_str(&format!("?jwt={token}"));
        }
        if !name.is_empty() {
            url.push_str(&format!("#userInfo.displayName=\"{}\"", percent(&name)));
        }
        Ok(json!({ "url": url, "room": room, "signed": token.is_some() }))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn bridge(pairs: &[(&str, &str)]) -> Result<Jitsi, ConfigError> {
        Jitsi::new(&pairs.iter().map(|(k, v)| (k.to_string(), v.to_string())).collect())
    }

    #[tokio::test]
    async fn a_public_server_gets_the_plain_room_address() {
        let b = bridge(&[]).unwrap_or_else(|e| panic!("{e}"));
        let out = b.call("join_link", &json!({ "room": "aether-abc", "name": "Ann B" })).await.unwrap_or_default();
        assert_eq!(out["url"], "https://meet.jit.si/aether-abc#userInfo.displayName=\"Ann%20B\"");
        assert_eq!(out["signed"], false);
    }

    #[tokio::test]
    async fn a_private_server_gets_a_signed_link_for_one_room() {
        let b = bridge(&[("base_url", "https://meet.acme.test/"), ("app_id", "aether"), ("signing_key", "s3cret")]).unwrap_or_else(|e| panic!("{e}"));
        let out = b.call("join_link", &json!({ "room": "r1", "name": "Ann", "moderator": true })).await.unwrap_or_default();
        let url = out["url"].as_str().unwrap_or_default();
        assert!(url.starts_with("https://meet.acme.test/r1?jwt="), "{url}");
        let jwt = url.split("jwt=").nth(1).unwrap_or_default().split('#').next().unwrap_or_default();
        let parts: Vec<&str> = jwt.split('.').collect();
        assert_eq!(parts.len(), 3);
        let claims: Value = serde_json::from_slice(&URL_SAFE_NO_PAD.decode(parts[1]).unwrap_or_default()).unwrap_or_default();
        assert_eq!((claims["room"].as_str(), claims["aud"].as_str(), claims["sub"].as_str(), claims["moderator"].as_bool()), (Some("r1"), Some("aether"), Some("meet.acme.test"), Some(true)));
        assert!(claims["exp"].as_u64().unwrap_or(0) > claims["nbf"].as_u64().unwrap_or(u64::MAX));
        let mut mac = <Hmac<Sha256> as KeyInit>::new_from_slice(b"s3cret").unwrap_or_else(|e| panic!("{e}"));
        mac.update(format!("{}.{}", parts[0], parts[1]).as_bytes());
        assert_eq!(parts[2], URL_SAFE_NO_PAD.encode(mac.finalize().into_bytes()), "the signature is HS256 over the secret");
    }

    #[tokio::test]
    async fn mistakes_are_refused() {
        assert!(bridge(&[("app_id", "x")]).is_err(), "half a token setup");
        assert!(bridge(&[("base_url", "ftp://x")]).is_err());
        let b = bridge(&[]).unwrap_or_else(|e| panic!("{e}"));
        for (action, params) in [("join_link", json!({})), ("join_link", json!({ "room": "a b" })), ("join_link", json!({ "room": "ok", "ttl_secs": 5 })), ("nope", json!({}))] {
            assert!(b.call(action, &params).await.err().is_some_and(|e| e.permanent), "{action} {params}");
        }
    }
}
