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

use aether_communication::{Action, ActionBridge, ConfigError, SendError, Spec, Values};
use serde_json::Value;

use crate::{kernel::host::context::BridgeHandle, state::AppState, tenancy::OrgRef};

/// The bridges plugins can call, with what each offers.
pub fn all() -> [(&'static Spec, &'static [Action]); 2] {
    [(&aether_paystack_integration::SPEC, aether_paystack_integration::ACTIONS), (&open_street_map::SPEC, open_street_map::ACTIONS)]
}

pub fn find(key: &str) -> Option<(&'static Spec, &'static [Action])> {
    all().into_iter().find(|(spec, _)| spec.key == key)
}

/// The names of the bridges plugins can call, for messages.
pub fn names() -> String {
    all().iter().map(|(spec, _)| spec.key).collect::<Vec<_>>().join(", ")
}

fn config(error: ConfigError) -> SendError {
    SendError::transient(error.to_string())
}

fn build(key: &str, values: &Values) -> Result<Box<dyn ActionBridge>, SendError> {
    Ok(match key {
        "paystack" => Box::new(aether_paystack_integration::Paystack::new(values).map_err(config)?),
        "open_street_map" => Box::new(open_street_map::OpenStreetMap::new(values).map_err(config)?),
        other => return Err(SendError::permanent(format!("`{other}` is not a bridge plugins can call (use one of: {})", names()))),
    })
}

/// Call `action` of bridge `key` for the organization `org_db`.
pub async fn call(state: &AppState, org_db: &str, key: &str, action: &str, params: &Value) -> Result<Value, SendError> {
    let (spec, actions) = find(key).ok_or_else(|| SendError::permanent(format!("`{key}` is not a bridge plugins can call (use one of: {})", names())))?;
    if !actions.iter().any(|offered| offered.name == action) {
        return Err(SendError::permanent(format!(
            "bridge `{key}` has no action `{action}`; it offers: {}",
            actions.iter().map(|offered| offered.name).collect::<Vec<_>>().join(", ")
        )));
    }
    let org = OrgRef { slug: org_db.to_string(), db_name: org_db.to_string() };
    let values = crate::messaging::resolve_values(state, &org, spec).await?;
    let bridge = build(key, &values)?;
    match tokio::time::timeout(Duration::from_secs(45), bridge.call(action, params)).await {
        Ok(answer) => answer,
        Err(_) => Err(SendError::transient(format!("bridge `{key}` did not answer within 45 seconds"))),
    }
}

/// What `bridge::call` uses to reach the bridges and the settings.
pub struct AppBridges {
    pub state: AppState,
}

#[async_trait::async_trait]
impl BridgeHandle for AppBridges {
    async fn call(&self, org: &str, bridge: &str, action: &str, params: Value) -> Result<Value, SendError> {
        call(&self.state, org, bridge, action, &params).await
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn every_callable_bridge_can_be_built_from_its_own_fields() {
        for (spec, actions) in all() {
            let values: Values = spec
                .account
                .iter()
                .filter(|f| f.required)
                .map(|f| (f.name.to_string(), "x".to_string()))
                .chain(spec.options.iter().filter_map(|f| f.default.map(|d| (f.name.to_string(), d.to_string()))))
                .collect();
            assert!(build(spec.key, &values).is_ok(), "{}", spec.key);
            assert!(!actions.is_empty(), "{} offers no action", spec.key);
            // The secret flag marks exactly the credentials.
            for field in spec.fields() {
                let looks_secret = ["key", "token", "password"].iter().any(|word| field.name.contains(word));
                assert_eq!(field.secret, looks_secret, "{}.{}", spec.key, field.name);
            }
        }
        assert!(build("stripe", &Values::new()).is_err());
        assert!(find("paystack").is_some() && find("nope").is_none());
    }

    #[test]
    fn the_seeded_settings_match_what_each_bridge_asks_for() {
        let seed: Value = serde_json::from_str(include_str!("../../../seeds/settings/bridges.json")).unwrap_or(Value::Null);
        let mut keys = std::collections::HashMap::new();
        for group in seed["groups"].as_array().into_iter().flatten() {
            for item in group["items"].as_array().into_iter().flatten() {
                keys.insert(
                    item["s_key"].as_str().unwrap_or_default().to_string(),
                    (item["secret"].as_bool().unwrap_or(false), item["s_value"].clone()),
                );
            }
        }
        assert!(!keys.is_empty(), "seeds/settings/bridges.json is missing or empty");
        for (spec, _) in all() {
            for field in spec.fields() {
                let key = crate::messaging::bridge_setting(spec.key, field.name);
                let (secret, default) = keys.get(&key).unwrap_or_else(|| panic!("`{key}` is not seeded"));
                assert_eq!(*secret, field.secret, "{key}: secret flag");
                if let Some(expected) = field.default {
                    assert_eq!(default, expected, "{key}: default");
                }
            }
        }
    }
}
