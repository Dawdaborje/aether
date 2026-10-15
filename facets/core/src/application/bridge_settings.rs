//! Bridge settings rows are only materialised for bridges that are enabled globally.
//!
//! Catalog JSON under `seeds/settings/` still *describes* every bridge's fields (tests check
//! Spec ↔ seed agreement). Seeding and the settings API skip `bridge.<name>.*` keys until that
//! bridge is turned on; enabling a bridge upserts its rows from those seeds.

use std::collections::HashSet;

use serde::Deserialize;
use serde_json::Value as JsonValue;
use surrealdb::{Surreal, engine::remote::ws::Client};

use crate::messaging::bridge_name_from_setting_key;
use crate::state::AppState;

use super::settings::{CatalogGroup, SettingsError};

const BRIDGES_SETTINGS_SEED: &str = include_str!("../../../../seeds/settings/bridges.json");
const COMMUNICATIONS_SETTINGS_SEED: &str =
    include_str!("../../../../seeds/settings/communications.json");

#[derive(Debug, Deserialize)]
struct SettingsSeedFile {
    groups: Vec<SettingsGroupSeed>,
}

#[derive(Debug, Deserialize)]
struct SettingsGroupSeed {
    id: String,
    label: String,
    color: Option<String>,
    icon: Option<String>,
    icon_type: Option<String>,
    items: Vec<SettingsItemSeed>,
}

#[derive(Debug, Deserialize)]
struct SettingsItemSeed {
    id: String,
    label: String,
    s_key: String,
    s_value: JsonValue,
    description: Option<String>,
    long_description: Option<String>,
    #[serde(default)]
    secret: bool,
}

/// Feature keys of bridges with `enabled_globally = true`.
pub async fn enabled_bridge_keys(db: &Surreal<Client>) -> Result<HashSet<String>, SettingsError> {
    let mut response = db
        .query("SELECT VALUE feature_key FROM bridges WHERE enabled_globally = true;")
        .await?
        .check()?;
    Ok(response.take::<Vec<String>>(0)?.into_iter().collect())
}

/// Drop catalog entries for bridges that are not enabled (and empty groups left behind).
pub fn retain_enabled_bridge_settings(catalog: &mut Vec<CatalogGroup>, enabled: &HashSet<String>) {
    for group in catalog.iter_mut() {
        group
            .items
            .retain(|item| match bridge_name_from_setting_key(&item.key) {
                Some(name) => enabled.contains(name),
                None => true,
            });
    }
    catalog.retain(|group| !group.items.is_empty());
}

/// Upsert settings rows for one bridge from the seed JSON (no-op when the seed has none).
pub async fn ensure_bridge_settings(
    db: &Surreal<Client>,
    feature_key: &str,
) -> Result<(), SettingsError> {
    let key = feature_key.trim();
    if key.is_empty() {
        return Ok(());
    }
    for file in [BRIDGES_SETTINGS_SEED, COMMUNICATIONS_SETTINGS_SEED] {
        let seed: SettingsSeedFile = serde_json::from_str(file)
            .map_err(|err| SettingsError::Invalid(format!("bridge settings seed: {err}")))?;
        for group in seed.groups {
            let items: Vec<_> = group
                .items
                .into_iter()
                .filter(|item| bridge_name_from_setting_key(&item.s_key) == Some(key))
                .collect();
            if items.is_empty() {
                continue;
            }
            upsert_group(
                db,
                &group.id,
                &group.label,
                &group.color,
                &group.icon,
                &group.icon_type,
            )
            .await?;
            for item in items {
                upsert_item(db, &group.id, &item).await?;
            }
        }
    }
    Ok(())
}

/// Forget cached settings catalogs after bridge enablement changes.
pub async fn invalidate_settings_catalog(state: &AppState) {
    let key = crate::cache::Cache::warm_settings_catalog_key();
    let _ = state.cache.kernel_invalidate(key);
    if let Ok(orgs) = crate::access::org_databases::list_cached(state).await {
        for org in orgs {
            let _ = state.cache.kernel_org_invalidate(&org, key);
        }
    }
}

async fn upsert_group(
    db: &Surreal<Client>,
    id: &str,
    label: &str,
    color: &Option<String>,
    icon: &Option<String>,
    icon_type: &Option<String>,
) -> Result<(), SettingsError> {
    db.query(
        r#"
UPSERT type::record("gl_settings_groups", $id) CONTENT {
    label: $label,
    color: $color,
    icon: $icon,
    icon_type: $icon_type,
    date_created: time::now(),
    date_updated: time::now()
};
"#,
    )
    .bind(("id", id.to_string()))
    .bind(("label", label.to_string()))
    .bind(("color", color.clone()))
    .bind(("icon", icon.clone()))
    .bind(("icon_type", icon_type.clone()))
    .await?
    .check()?;
    Ok(())
}

async fn upsert_item(
    db: &Surreal<Client>,
    group_id: &str,
    item: &SettingsItemSeed,
) -> Result<(), SettingsError> {
    db.query(
        r#"
UPSERT type::record("gl_settings_items", $id) CONTENT {
    label: $label,
    s_key: $s_key,
    s_value: $s_value,
    is_secret: $is_secret,
    description: $description,
    long_description: $long_description,
    date_created: time::now(),
    date_updated: time::now()
};
UPSERT type::record("gl_settings_group_items", $relation_id) CONTENT {
    group: type::record("gl_settings_groups", $group_id),
    item: type::record("gl_settings_items", $item_id),
    date_created: time::now(),
    date_updated: time::now()
};
"#,
    )
    .bind(("id", item.id.clone()))
    .bind(("label", item.label.clone()))
    .bind(("s_key", item.s_key.clone()))
    .bind(("s_value", item.s_value.clone()))
    .bind(("is_secret", item.secret))
    .bind(("description", item.description.clone()))
    .bind(("long_description", item.long_description.clone()))
    .bind(("relation_id", format!("{}_{}", group_id, item.id)))
    .bind(("group_id", group_id.to_string()))
    .bind(("item_id", item.id.clone()))
    .await?
    .check()?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::messaging::bridge_name_from_setting_key;

    #[test]
    fn bridge_setting_keys_round_trip() {
        assert_eq!(
            bridge_name_from_setting_key("bridge.twilio.auth_token"),
            Some("twilio")
        );
        assert_eq!(
            bridge_name_from_setting_key("bridge.open_street_map.base_url"),
            Some("open_street_map")
        );
        assert_eq!(
            bridge_name_from_setting_key("communications.email.provider"),
            None
        );
        assert_eq!(bridge_name_from_setting_key("bridge."), None);
        assert_eq!(bridge_name_from_setting_key("bridge.only"), None);
    }

    #[test]
    fn seed_files_name_at_least_one_field_per_known_callable_bridge() {
        for file in [BRIDGES_SETTINGS_SEED, COMMUNICATIONS_SETTINGS_SEED] {
            let seed: SettingsSeedFile = serde_json::from_str(file).unwrap();
            assert!(!seed.groups.is_empty());
        }
        let paystack = BRIDGES_SETTINGS_SEED.contains("bridge.paystack.secret_key");
        let smtp = COMMUNICATIONS_SETTINGS_SEED.contains("bridge.smtp.host");
        assert!(paystack && smtp);
    }
}
