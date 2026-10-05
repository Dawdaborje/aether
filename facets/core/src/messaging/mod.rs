//! Sending messages through the bridges the administrator configured.
//!
//! A plugin never names a provider. It sends a message of some type (`communication::send`) and
//! the kernel finds the bridge for that type in the settings:
//!
//! | Setting | Meaning |
//! |---|---|
//! | `communications.email.provider` | `smtp` or `resend` |
//! | `communications.sms.provider` | `twilio`, `termii` or `africas_talking` |
//! | `bridge.<provider>.<field>` | that provider's credentials and options |
//!
//! All of it lives in the database settings, at two levels: global, and per organization.
//! **Credentials are resolved as a unit.** If an organization filled in any account field of the
//! provider (a key, a password, the sender number registered to the account), all of its account
//! fields come from the organization and a missing one is an error: an organization's account is
//! never mixed with the global one's. If it filled none, all of them come from the global settings.
//! Plain options and the provider choice itself fall back one by one.

use std::collections::HashMap;

use aether_communication::{ConfigError, EmailBridge, SendError, SmsBridge, Spec, Values};
use serde_json::Value;

use crate::application::settings::{Scope, get_setting_plain_in};
use crate::state::AppState;
use crate::tenancy::OrgRef;

pub mod message;
pub use message::{MAX_RECIPIENTS, Message, MessageError, TYPES};

/// The bridges that can send email.
pub const EMAIL_BRIDGES: &[&Spec] = &[&smtp::SPEC, &resend::SPEC];
/// The bridges that can send text messages.
pub const SMS_BRIDGES: &[&Spec] = &[&twilio::SPEC, &termii::SPEC, &africas_talking::SPEC];

/// The setting that names the bridge for a message type.
pub fn provider_setting(kind: &str) -> String {
    format!("communications.{kind}.provider")
}

/// The setting key of a bridge's field.
pub fn bridge_setting(bridge: &str, field: &str) -> String {
    format!("bridge.{bridge}.{field}")
}

fn find_spec(specs: &[&'static Spec], key: &str) -> Option<&'static Spec> {
    specs.iter().copied().find(|spec| spec.key == key)
}

fn text(value: Option<Value>) -> Option<String> {
    value.and_then(|value| value.as_str().map(str::trim).map(str::to_string)).filter(|text| !text.is_empty())
}

fn org_ref(org_db: &str) -> OrgRef {
    OrgRef { slug: org_db.to_string(), db_name: org_db.to_string() }
}

fn unreadable(error: impl std::fmt::Display) -> SendError {
    SendError::transient(format!("settings could not be read: {error}"))
}

/// A setting's value for an organization: its own if filled in, else the global one.
async fn setting(state: &AppState, org: &OrgRef, key: &str) -> Result<Option<String>, SendError> {
    if let Some(own) = text(get_setting_plain_in(state, key, Scope::Org(org)).await.map_err(unreadable)?) {
        return Ok(Some(own));
    }
    Ok(text(get_setting_plain_in(state, key, Scope::Global).await.map_err(unreadable)?))
}

/// The settings of one bridge for one organization, following the rules in the module docs.
pub async fn resolve_values(state: &AppState, org: &OrgRef, spec: &Spec) -> Result<Values, SendError> {
    let mut own = HashMap::new();
    for field in spec.account {
        let value = text(get_setting_plain_in(state, &bridge_setting(spec.key, field.name), Scope::Org(org)).await.map_err(unreadable)?);
        if let Some(value) = value {
            own.insert(field.name, value);
        }
    }
    let uses_own = !own.is_empty();
    let mut values = Values::new();
    let mut missing = Vec::new();
    for field in spec.account {
        let value = if uses_own {
            own.get(field.name).cloned()
        } else {
            text(get_setting_plain_in(state, &bridge_setting(spec.key, field.name), Scope::Global).await.map_err(unreadable)?)
        };
        match value {
            Some(value) => {
                values.insert(field.name.to_string(), value);
            }
            None if field.required => missing.push(field.label),
            None => {}
        }
    }
    if !missing.is_empty() {
        let whose = if uses_own { "this organization's own" } else { "the global" };
        return Err(SendError::transient(format!(
            "{} settings are incomplete: {} {} not set in {whose} settings",
            spec.label,
            missing.join(", "),
            if missing.len() == 1 { "is" } else { "are" }
        )));
    }
    for field in spec.options {
        let value = setting(state, org, &bridge_setting(spec.key, field.name)).await?.or_else(|| field.default.map(str::to_string));
        if let Some(value) = value {
            values.insert(field.name.to_string(), value);
        }
    }
    Ok(values)
}

fn config(error: ConfigError) -> SendError {
    SendError::transient(error.to_string())
}

fn build_email(key: &str, values: &Values) -> Result<Box<dyn EmailBridge>, SendError> {
    Ok(match key {
        "smtp" => Box::new(smtp::Smtp::new(values).map_err(config)?),
        "resend" => Box::new(resend::Resend::new(values).map_err(config)?),
        other => return Err(SendError::transient(format!("`{other}` is not an email provider"))),
    })
}

fn build_sms(key: &str, values: &Values) -> Result<Box<dyn SmsBridge>, SendError> {
    Ok(match key {
        "twilio" => Box::new(twilio::Twilio::new(values).map_err(config)?),
        "termii" => Box::new(termii::Termii::new(values).map_err(config)?),
        "africas_talking" => Box::new(africas_talking::AfricasTalking::new(values).map_err(config)?),
        other => return Err(SendError::transient(format!("`{other}` is not an SMS provider"))),
    })
}

/// Why a type of message cannot be sent, if it cannot: no provider chosen, or the provider's
/// settings incomplete. Used to refuse early with a clear message.
pub async fn check_configured(state: &AppState, org_db: &str, kind: &str) -> Result<(), SendError> {
    let org = org_ref(org_db);
    let specs = if kind == "email" { EMAIL_BRIDGES } else { SMS_BRIDGES };
    let provider = setting(state, &org, &provider_setting(kind))
        .await?
        .ok_or_else(|| SendError::permanent(format!("{kind} is not set up: no provider is chosen in the settings")))?;
    let spec = find_spec(specs, &provider)
        .ok_or_else(|| SendError::permanent(format!("`{provider}` is not a {kind} provider (use {})", names(specs))))?;
    resolve_values(state, &org, spec).await.map(|_| ())
}

fn names(specs: &[&Spec]) -> String {
    specs.iter().map(|spec| spec.key).collect::<Vec<_>>().join(", ")
}

/// Deliver a queued message through the configured bridge. The error says whether trying again
/// can help.
pub async fn deliver(state: &AppState, org_db: &str, payload: &Value) -> Result<(), SendError> {
    let message = Message::from_job(payload).map_err(|e| SendError::permanent(e.to_string()))?;
    let org = org_ref(org_db);
    let kind = message.kind();
    let specs = if kind == "email" { EMAIL_BRIDGES } else { SMS_BRIDGES };
    let provider = setting(state, &org, &provider_setting(kind))
        .await?
        .ok_or_else(|| SendError::transient(format!("{kind} is not set up: no provider is chosen in the settings")))?;
    let spec = find_spec(specs, &provider)
        .ok_or_else(|| SendError::transient(format!("`{provider}` is not a {kind} provider (use {})", names(specs))))?;
    let values = resolve_values(state, &org, spec).await?;

    match &message {
        Message::Email { .. } => {
            let from_address = setting(state, &org, "communications.email.from_address")
                .await?
                .ok_or_else(|| SendError::transient("no sender address: set `From address` under Email in the settings"))?;
            let from_name = setting(state, &org, "communications.email.from_name").await?;
            let email = message::email_for_bridge(&message, from_address, from_name)
                .ok_or_else(|| SendError::permanent("not an email"))?;
            build_email(spec.key, &values)?.send(&email).await
        }
        Message::Sms { .. } => {
            let sender = setting(state, &org, "communications.sms.sender").await?;
            let sms = message::sms_for_bridge(&message, sender).ok_or_else(|| SendError::permanent("not an SMS"))?;
            build_sms(spec.key, &values)?.send(&sms).await
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The seeded settings and the bridges' own descriptions must agree, or the settings screen
    /// would offer fields the bridge ignores (or miss ones it needs).
    #[test]
    fn the_seeded_settings_match_what_each_bridge_asks_for() {
        let seed: Value = serde_json::from_str(include_str!("../../../../seeds/settings/communications.json")).unwrap();
        let mut keys: HashMap<String, (bool, Value)> = HashMap::new();
        for group in seed["groups"].as_array().unwrap() {
            for item in group["items"].as_array().unwrap() {
                keys.insert(
                    item["s_key"].as_str().unwrap().to_string(),
                    (item["secret"].as_bool().unwrap_or(false), item["s_value"].clone()),
                );
            }
        }
        for spec in EMAIL_BRIDGES.iter().chain(SMS_BRIDGES) {
            for field in spec.fields() {
                let key = bridge_setting(spec.key, field.name);
                let (secret, default) = keys.get(&key).unwrap_or_else(|| panic!("`{key}` is not seeded"));
                assert_eq!(*secret, field.secret, "{key}: secret flag");
                if let Some(expected) = field.default {
                    assert_eq!(default, expected, "{key}: default");
                }
            }
        }
        for kind in TYPES {
            assert!(keys.contains_key(&provider_setting(kind)), "{kind} provider setting");
        }
        for key in ["communications.email.from_address", "communications.email.from_name", "communications.sms.sender"] {
            assert!(keys.contains_key(key), "{key}");
        }
    }

    /// Setting labels and keys are unique across the whole catalog (a database index enforces it),
    /// so a seed with a repeated one fails to apply and leaves the catalog half made.
    #[test]
    fn seeded_labels_and_keys_are_unique_across_every_settings_file() {
        let files = [
            include_str!("../../../../seeds/settings/communications.json"),
            include_str!("../../../../seeds/settings/global_settings.json"),
            include_str!("../../../../seeds/settings/chatter.json"),
            include_str!("../../../../seeds/settings/bridges.json"),
        ];
        let (mut labels, mut keys, mut groups) = (std::collections::HashSet::new(), std::collections::HashSet::new(), std::collections::HashSet::new());
        for file in files {
            let seed: Value = serde_json::from_str(file).unwrap();
            for group in seed["groups"].as_array().unwrap() {
                // A later file may add to an existing group, so only labels of items must differ.
                groups.insert(group["label"].as_str().unwrap().to_string());
                for item in group["items"].as_array().unwrap() {
                    let label = item["label"].as_str().unwrap().to_string();
                    let key = item["s_key"].as_str().unwrap().to_string();
                    assert!(labels.insert(label.clone()), "label `{label}` is used twice");
                    assert!(keys.insert(key.clone()), "key `{key}` is used twice");
                }
            }
        }
    }

    #[test]
    fn every_routed_bridge_can_be_built_from_its_own_fields() {
        // A bridge listed in a spec list must be buildable, or a configured provider would fail.
        let full = |spec: &Spec| -> Values {
            spec.account
                .iter()
                .filter(|f| f.required)
                .map(|f| (f.name.to_string(), "x".to_string()))
                .chain(spec.options.iter().filter_map(|f| f.default.map(|d| (f.name.to_string(), d.to_string()))))
                .collect()
        };
        for spec in EMAIL_BRIDGES {
            assert!(build_email(spec.key, &full(spec)).is_ok(), "{}", spec.key);
        }
        for spec in SMS_BRIDGES {
            assert!(build_sms(spec.key, &full(spec)).is_ok(), "{}", spec.key);
        }
        assert!(build_email("twilio", &Values::new()).is_err());
        assert!(build_sms("smtp", &Values::new()).is_err());
    }

    #[test]
    fn the_secret_flag_marks_exactly_the_credentials() {
        for spec in EMAIL_BRIDGES.iter().chain(SMS_BRIDGES) {
            for field in spec.fields() {
                let looks_secret = ["key", "token", "password"].iter().any(|word| field.name.contains(word));
                assert_eq!(field.secret, looks_secret, "{}.{}", spec.key, field.name);
            }
        }
    }
}
