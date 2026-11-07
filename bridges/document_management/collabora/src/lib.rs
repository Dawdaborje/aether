//! Editing office files in the browser with an editor that speaks WOPI (Collabora Online, and other WOPI clients).
//! <https://learn.microsoft.com/en-us/microsoft-365/cloud-storage-partner-program/rest/>
//!
//! The editor runs on its own server. It asks **this** server for the file (`GET /api/wopi/files/<file>/contents`) and
//! sends the edited file back (`POST` to the same address); the kernel answers those calls (see `wopi` in the core).
//! What the editor presents as proof is an **access token** this bridge makes: it names one file, one person, whether they
//! may change it, and the plugin function that takes the saved file, and it is signed with `signing_key` and valid for a
//! few hours. So the editor never needs an account here and a token cannot be turned into another file or person.
//!
//! The action `editor_url` makes the address to send the person's browser to. The kernel adds `_org` to the parameters.

use aether_communication::{Action, ActionBridge, ConfigError, Field, SendError, Spec, Values};
use async_trait::async_trait;
use base64::{Engine, engine::general_purpose::URL_SAFE_NO_PAD};
use hmac::{Hmac, KeyInit, Mac};
use serde_json::{Value, json};
use sha2::Sha256;

pub const SPEC: Spec = Spec {
    key: "collabora",
    label: "Collabora Online (WOPI editor)",
    account: &[Field { name: "signing_key", label: "Secret that signs the editor's access tokens", secret: true, required: true, default: None }],
    options: &[
        Field { name: "base_url", label: "Address of the editor server", secret: false, required: true, default: None },
        Field { name: "wopi_host_url", label: "Address the editor reaches this server at", secret: false, required: true, default: None },
    ],
};

pub const ACTIONS: &[Action] = &[Action {
    name: "editor_url",
    help: "{ media (the file's media key), name, user, user_name?, can_write? (default false), plugin, save (function that takes the edited file), ref (opaque text given back to it), ttl_secs? (300 to 43200, default 14400) } -> { url, expires }",
}];

/// What an access token says. Everything the kernel needs to serve the editor, nothing it must look up.
pub const CLAIMS: &[&str] = &["org", "media", "name", "user", "user_name", "can_write", "plugin", "save", "ref", "exp"];

pub struct Collabora {
    values: Values,
}

fn address(values: &Values, name: &str, label: &str) -> Result<String, SendError> {
    let v = values.get(name).map(|v| v.trim().trim_end_matches('/').to_string()).filter(|v| !v.is_empty());
    let v = v.ok_or_else(|| SendError::permanent(format!("collabora: `{name}` is not set in the bridge settings ({label})")))?;
    if !v.starts_with("https://") && !v.starts_with("http://") {
        return Err(SendError::permanent(format!("collabora: `{name}` starts with https://")));
    }
    Ok(v)
}

impl Collabora {
    /// The settings are checked when the editor is asked for, so the bridge can be switched on before they are filled.
    pub fn new(values: &Values) -> Result<Self, ConfigError> {
        Ok(Self { values: values.clone() })
    }

    fn settings(&self) -> Result<(String, String, String), SendError> {
        let key = self.values.get("signing_key").map(|k| k.trim().to_string()).unwrap_or_default();
        if key.len() < 16 {
            return Err(SendError::permanent("collabora: `signing_key` is at least 16 characters"));
        }
        Ok((address(&self.values, "base_url", "the editor server")?, address(&self.values, "wopi_host_url", "this server, as the editor reaches it")?, key))
    }
}

fn mac(key: &str) -> Option<Hmac<Sha256>> {
    <Hmac<Sha256> as KeyInit>::new_from_slice(key.as_bytes()).ok()
}

/// `claims` as a signed token: `<claims>.<signature>`, both URL-safe base64.
pub fn sign(key: &str, claims: &Value) -> Option<String> {
    let body = URL_SAFE_NO_PAD.encode(claims.to_string());
    let mut m = mac(key)?;
    m.update(body.as_bytes());
    Some(format!("{body}.{}", URL_SAFE_NO_PAD.encode(m.finalize().into_bytes())))
}

/// The claims of a token signed with `key`, when the signature holds and it has not expired at `now` (seconds).
pub fn verify(key: &str, token: &str, now: u64) -> Option<Value> {
    let (body, signature) = token.split_once('.')?;
    let mut m = mac(key)?;
    m.update(body.as_bytes());
    m.verify_slice(&URL_SAFE_NO_PAD.decode(signature).ok()?).ok()?;
    let claims: Value = serde_json::from_slice(&URL_SAFE_NO_PAD.decode(body).ok()?).ok()?;
    (claims["exp"].as_u64()? > now).then_some(claims)
}

/// The organization of a token, read **before** the signature can be checked (the key depends on it). Trust nothing else.
pub fn organization_of(token: &str) -> Option<String> {
    let claims: Value = serde_json::from_slice(&URL_SAFE_NO_PAD.decode(token.split_once('.')?.0).ok()?).ok()?;
    claims["org"].as_str().map(str::to_string)
}

/// The file's address in WOPI URLs: its media key, URL-safe.
pub fn file_id(media: &str) -> String {
    URL_SAFE_NO_PAD.encode(media)
}

fn percent(text: &str) -> String {
    text.bytes().map(|b| if b.is_ascii_alphanumeric() || b"-_.~".contains(&b) { (b as char).to_string() } else { format!("%{b:02X}") }).collect()
}

fn now() -> u64 {
    std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).map_or(0, |d| d.as_secs())
}

fn text(params: &Value, name: &str, max: usize) -> Result<String, SendError> {
    let v = params[name].as_str().map(str::trim).unwrap_or_default();
    if v.is_empty() || v.chars().count() > max {
        return Err(SendError::permanent(format!("`{name}` is required (at most {max} characters)")));
    }
    Ok(v.to_string())
}

#[async_trait]
impl ActionBridge for Collabora {
    async fn call(&self, action: &str, params: &Value) -> Result<Value, SendError> {
        if action != "editor_url" {
            return Err(SendError::permanent(format!("collabora has no action `{action}`; it offers: editor_url")));
        }
        let (base_url, host_url, signing_key) = self.settings()?;
        let media = text(params, "media", 120)?;
        if !media.starts_with("media/") || media.contains("..") {
            return Err(SendError::permanent("`media` is a key of the media store"));
        }
        let ttl = params["ttl_secs"].as_u64().unwrap_or(14_400);
        if !(300..=43_200).contains(&ttl) {
            return Err(SendError::permanent("`ttl_secs` is 300 to 43200"));
        }
        let exp = now() + ttl;
        let claims = json!({
            "org": text(params, "_org", 120)?, "media": media, "name": text(params, "name", 200)?, "user": text(params, "user", 128)?,
            "user_name": params["user_name"].as_str().unwrap_or_default().chars().take(120).collect::<String>(),
            "can_write": params["can_write"].as_bool().unwrap_or(false), "plugin": text(params, "plugin", 80)?, "save": text(params, "save", 80)?,
            "ref": text(params, "ref", 200)?, "exp": exp,
        });
        let token = sign(&signing_key, &claims).ok_or_else(|| SendError::permanent("the signing key cannot be used"))?;
        let source = format!("{}/api/wopi/files/{}", host_url, file_id(&media));
        let url = format!("{}/browser/dist/cool.html?WOPISrc={}&access_token={}&access_token_ttl={}", base_url, percent(&source), token, exp * 1000);
        Ok(json!({ "url": url, "expires": exp }))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn bridge() -> Collabora {
        let v: Values = [("signing_key", "k".repeat(24)), ("base_url", "https://office.acme.test/".into()), ("wopi_host_url", "https://aether.acme.test".into())]
            .into_iter()
            .map(|(k, v)| (k.to_string(), v))
            .collect();
        Collabora::new(&v).unwrap_or_else(|e| panic!("{e}"))
    }

    fn params() -> Value {
        json!({ "_org": "acme", "media": "media/2026/10/abc.docx", "name": "Plan.docx", "user": "users:ann", "user_name": "Ann", "can_write": true, "plugin": "atrium_drive", "save": "new_version", "ref": "f1" })
    }

    #[tokio::test]
    async fn the_address_names_the_file_and_carries_a_token_that_only_this_key_opens() {
        let out = bridge().call("editor_url", &params()).await.unwrap_or_default();
        let url = out["url"].as_str().unwrap_or_default();
        assert!(url.starts_with("https://office.acme.test/browser/dist/cool.html?WOPISrc=https%3A%2F%2Faether.acme.test%2Fapi%2Fwopi%2Ffiles%2F"), "{url}");
        let token = url.split("access_token=").nth(1).unwrap_or_default().split('&').next().unwrap_or_default();
        let key = "k".repeat(24);
        let claims = verify(&key, token, now()).unwrap_or_default();
        assert_eq!((claims["media"].as_str(), claims["can_write"].as_bool(), claims["save"].as_str()), (Some("media/2026/10/abc.docx"), Some(true), Some("new_version")));
        assert_eq!(organization_of(token).as_deref(), Some("acme"));
        assert!(url.contains(&percent(&file_id("media/2026/10/abc.docx"))));
        assert!(verify("another-key-another-key", token, now()).is_none(), "another key does not open it");
        assert!(verify(&key, token, now() + 100_000).is_none(), "it expires");
        let mut forged = token.to_string();
        forged.insert(3, 'x');
        assert!(verify(&key, &forged, now()).is_none(), "a changed token is refused");
    }

    #[tokio::test]
    async fn mistakes_are_refused() {
        let b = bridge();
        for (action, mut p) in [("editor_url", params()), ("nope", params())] {
            p["media"] = json!("../etc/passwd");
            assert!(b.call(action, &p).await.err().is_some_and(|e| e.permanent));
        }
        let mut p = params();
        p["ttl_secs"] = json!(5);
        assert!(b.call("editor_url", &p).await.is_err());
        let mut p = params();
        p.as_object_mut().map(|o| o.remove("user"));
        assert!(b.call("editor_url", &p).await.is_err());
        let empty = Collabora::new(&Values::new()).unwrap_or_else(|e| panic!("{e}"));
        assert!(empty.call("editor_url", &params()).await.err().is_some_and(|e| e.message.contains("signing_key")));
    }
}
