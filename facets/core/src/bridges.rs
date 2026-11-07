//! Bridges a plugin calls by name: `bridge::call`.
//!
//! A *bridge* is a first-party integration with an outside service. The messaging bridges (SMTP,
//! Twilio, …) are driven by the kernel itself behind `communication::send`. The rest are called by
//! plugins:
//!
//! ```json
//! { "bridge": "paystack", "action": "verify_transaction", "params": { "reference": "r1" } }
//! ```
//!
//! The plugin never holds a key. The bridge's settings (`bridge.<name>.<field>`) are read from the
//! database for the organization the call is for, with the same rules as every bridge: an
//! organization's own account or the global one, never a mix, and secrets decrypted only here (see
//! [`crate::messaging`]). A plugin may call only the bridges it lists under `bridges` in
//! `plugin.toml`, in addition to holding the `bridge::call` capability.
//!
//! Unlike sending a message, the call is **synchronous**: the plugin waits for the answer (at most 45
//! seconds). Retrying is the plugin's decision; the error says whether trying again can help.

use std::time::Duration;

use aether_communication::{ActionBridge, SendError};
use serde_json::{Value, json};

use crate::{kernel::host::context::BridgeHandle, state::AppState, tenancy::OrgRef};

pub use aether_kernel::bridges::{PER_USER, all, build, config, find, internal, names};


/// Build an internal bridge for an organization, checking it offers `action`.
async fn internal_bridge(state: &AppState, org_db: &str, key: &str, action: &str) -> Result<Box<dyn ActionBridge>, SendError> {
    let (spec, actions) = internal()
        .into_iter()
        .find(|(spec, _)| spec.key == key)
        .ok_or_else(|| SendError::permanent(format!("`{key}` is not an engine the kernel knows")))?;
    if !actions.iter().any(|offered| offered.name == action) {
        return Err(SendError::permanent(format!("`{key}` has no action `{action}`")));
    }
    let org = OrgRef { slug: org_db.to_string(), db_name: org_db.to_string() };
    let values = crate::messaging::resolve_values(state, &org, spec).await?;
    Ok(match key {
        "liteparse" => Box::new(liteparse::LiteParse::new(&values).map_err(config)?),
        "ollama" => Box::new(ollama::Ollama::new(&values).map_err(config)?),
        "openai_compatible" => Box::new(openai_compatible::OpenAiCompatible::new(&values).map_err(config)?),
        "anthropic" => Box::new(anthropic::Anthropic::new(&values).map_err(config)?),
        other => return Err(SendError::permanent(format!("`{other}` cannot be built"))),
    })
}

/// Call `action` of an internal bridge. Same settings rules and time limit as [`call`].
pub async fn call_internal(
    state: &AppState,
    org_db: &str,
    key: &str,
    action: &str,
    params: &Value,
) -> Result<Value, SendError> {
    let bridge = internal_bridge(state, org_db, key, action).await?;
    // The engine keeps its own, shorter, limit; this one only stops a stuck call.
    match tokio::time::timeout(Duration::from_secs(660), bridge.call(action, params)).await {
        Ok(answer) => answer,
        Err(_) => Err(SendError::transient(format!("`{key}` did not answer in time"))),
    }
}

/// Like [`call_internal`], handing `sink` each piece of text as the provider produces it.
pub async fn call_internal_stream(
    state: &AppState,
    org_db: &str,
    key: &str,
    action: &str,
    params: &Value,
    sink: &dyn aether_communication::TextSink,
) -> Result<Value, SendError> {
    let bridge = internal_bridge(state, org_db, key, action).await?;
    match tokio::time::timeout(Duration::from_secs(660), bridge.stream(action, params, sink)).await {
        Ok(answer) => answer,
        Err(_) => Err(SendError::transient(format!("`{key}` did not answer in time"))),
    }
}

/// Call `action` of bridge `key` for the organization `org_db`.
pub async fn call(
    state: &AppState,
    org_db: &str,
    key: &str,
    action: &str,
    params: &Value,
) -> Result<Value, SendError> {
    let (spec, actions) = find(key).ok_or_else(|| {
        SendError::permanent(format!(
            "`{key}` is not a bridge plugins can call (use one of: {})",
            names()
        ))
    })?;
    if !actions.iter().any(|offered| offered.name == action) {
        return Err(SendError::permanent(format!(
            "bridge `{key}` has no action `{action}`; it offers: {}",
            actions
                .iter()
                .map(|offered| offered.name)
                .collect::<Vec<_>>()
                .join(", ")
        )));
    }
    let org = OrgRef {
        slug: org_db.to_string(),
        db_name: org_db.to_string(),
    };
    let values = crate::messaging::resolve_values(state, &org, spec).await?;
    let bridge = build(key, &values)?;
    // The bridge learns which organization it works for from the kernel, never from the plugin.
    let mut params = params.clone();
    // `_user` is put there by the kernel for the call it serves; a bridge that works for a person gets their login too.
    let user = params.as_object_mut().and_then(|o| o.remove("_user")).and_then(|u| u.as_str().map(str::to_string));
    let override_login = params.as_object_mut().and_then(|o| o.remove("_credential_override"));
    if let Some(object) = params.as_object_mut() {
        object.remove("_credential");
        object.insert("_org".into(), Value::String(org_db.to_string()));
    }
    if PER_USER.contains(&key) {
        let login = match (override_login, &user) {
            (Some(login), Some(_)) => Some((login["username"].as_str().unwrap_or_default().to_string(), login["secret"].as_str().unwrap_or_default().to_string())),
            (_, Some(user)) => crate::bridge_credentials::load(state, org_db, user, key).await.map_err(|e| {
                log::error!("bridge credentials: {e}");
                SendError::transient("your login could not be read")
            })?,
            _ => return Err(SendError::permanent("this needs a signed-in person")),
        };
        let Some((username, secret)) = login else {
            return Err(SendError::permanent("you have not connected your account yet: connect it first, then try again"));
        };
        if let Some(object) = params.as_object_mut() {
            object.insert("_credential".into(), json!({ "username": username, "secret": secret }));
        }
    }
    match tokio::time::timeout(Duration::from_secs(45), bridge.call(action, &params)).await {
        Ok(answer) => answer,
        Err(_) => Err(SendError::transient(format!(
            "bridge `{key}` did not answer within 45 seconds"
        ))),
    }
}

/// What `bridge::call` uses to reach the bridges and the settings.
pub struct AppBridges {
    pub state: AppState,
}

#[async_trait::async_trait]
impl BridgeHandle for AppBridges {
    async fn call(
        &self,
        org: &str,
        bridge: &str,
        action: &str,
        params: Value,
    ) -> Result<Value, SendError> {
        call(&self.state, org, bridge, action, &params).await
    }

    async fn call_internal(
        &self,
        org: &str,
        bridge: &str,
        action: &str,
        params: Value,
    ) -> Result<Value, SendError> {
        call_internal(&self.state, org, bridge, action, &params).await
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use aether_communication::Values;

    #[test]
    fn every_callable_bridge_can_be_built_from_its_own_fields() {
        for (spec, actions) in all() {
            let values: Values = spec
                .account
                .iter()
                .filter(|f| f.required)
                .map(|f| (f.name.to_string(), "x".to_string()))
                .chain(
                    spec.options
                        .iter()
                        .filter_map(|f| f.default.map(|d| (f.name.to_string(), d.to_string()))),
                )
                .collect();
            assert!(build(spec.key, &values).is_ok(), "{}", spec.key);
            assert!(!actions.is_empty(), "{} offers no action", spec.key);
            // The secret flag marks exactly the credentials.
            for field in spec.fields() {
                let looks_secret = ["key", "token", "password"]
                    .iter()
                    .any(|word| field.name.contains(word));
                assert_eq!(field.secret, looks_secret, "{}.{}", spec.key, field.name);
            }
        }
        assert!(build("stripe", &Values::new()).is_err());
        assert!(find("paystack").is_some() && find("nope").is_none());
    }

    #[test]
    fn engines_are_internal_and_never_listed_for_plugins() {
        for (spec, actions) in internal() {
            assert!(find(spec.key).is_none(), "{} must not be callable by a plugin", spec.key);
            assert!(!actions.is_empty());
            for field in spec.fields() {
                let looks_secret = ["key", "token", "password"].iter().any(|w| field.name.contains(w));
                assert_eq!(field.secret, looks_secret, "{}.{}", spec.key, field.name);
            }
        }
    }

    #[test]
    fn the_seeded_settings_match_what_each_bridge_asks_for() {
        let seed: Value =
            serde_json::from_str(include_str!("../../../seeds/settings/bridges.json"))
                .unwrap_or(Value::Null);
        let mut keys = std::collections::HashMap::new();
        for group in seed["groups"].as_array().into_iter().flatten() {
            for item in group["items"].as_array().into_iter().flatten() {
                keys.insert(
                    item["s_key"].as_str().unwrap_or_default().to_string(),
                    (
                        item["secret"].as_bool().unwrap_or(false),
                        item["s_value"].clone(),
                    ),
                );
            }
        }
        assert!(
            !keys.is_empty(),
            "seeds/settings/bridges.json is missing or empty"
        );
        for (spec, _) in all().into_iter().chain(internal()) {
            for field in spec.fields() {
                let key = crate::messaging::bridge_setting(spec.key, field.name);
                let (secret, default) = keys
                    .get(&key)
                    .unwrap_or_else(|| panic!("`{key}` is not seeded"));
                assert_eq!(*secret, field.secret, "{key}: secret flag");
                if let Some(expected) = field.default {
                    assert_eq!(default, expected, "{key}: default");
                }
            }
        }
    }
}
