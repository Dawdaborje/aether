use std::collections::HashSet;

use include_dir::{Dir, include_dir};
use serde::Deserialize;
use serde_json::Value;
use surrealdb::{Surreal, engine::remote::ws::Client, types::SurrealValue};
use thiserror::Error;

static SEEDS_SETTINGS_FILES: Dir<'_> = include_dir!("$CARGO_MANIFEST_DIR/../seeds/settings");
static SEEDS_BRIDGE_FILES: Dir<'_> = include_dir!("$CARGO_MANIFEST_DIR/../seeds/bridges");

#[derive(Debug, Error)]
pub enum SeedError {
    #[error("surrealdb error: {0}")]
    Surreal(#[from] surrealdb::Error),

    #[error("seed file `{0}` is not valid UTF-8")]
    InvalidUtf8(String),

    #[error("seed `{version}` failed: {message}")]
    ApplyFailed { version: String, message: String },

    #[error("bridge seed `{0}` is invalid: {1}")]
    InvalidBridge(String, String),
}

#[derive(Debug, Deserialize, SurrealValue)]
struct AppliedSeed {
    version: String,
}

pub async fn seed_system(
    db: &Surreal<Client>,
    namespace: &str,
    database: &str,
) -> Result<Vec<String>, SeedError> {
    db.use_ns(namespace).await?;
    db.use_db(database).await?;
    ensure_seed_tracking_table(db).await?;

    let mut newly_applied = seed_global_settings(db).await?;
    newly_applied.extend(seed_bridges(db, namespace, database).await?);

    Ok(newly_applied)
}

#[derive(Debug, Deserialize)]
struct GlobalSettingsSeed {
    groups: Vec<SettingsGroupSeed>,
    auth_providers: Vec<AuthProviderSeed>,
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
    s_value: Value,
    description: Option<String>,
    long_description: Option<String>,
}

#[derive(Debug, Deserialize)]
struct AuthProviderSeed {
    id: String,
    name: String,
    label: String,
    provider_type: String,
    issuer_url: Option<String>,
    client_id: Option<String>,
    scopes: Option<Vec<String>>,
    enabled: bool,
    config: Option<Value>,
}

async fn seed_global_settings(db: &Surreal<Client>) -> Result<Vec<String>, SeedError> {
    let version = "settings/global_settings";
    let filename = "global_settings.json";
    let applied = load_applied_seeds(db).await?;
    if applied.contains(version) {
        log::debug!("Skipping already-applied seed {version}");
        return Ok(Vec::new());
    }
    if applied.contains("0002_global_settings") {
        record_seed(db, version, filename).await?;
        log::info!("Skipping JSON settings seed; legacy SQL settings are already applied");
        return Ok(vec![version.to_string()]);
    }

    let file = SEEDS_SETTINGS_FILES
        .get_file(filename)
        .ok_or_else(|| SeedError::InvalidUtf8(filename.to_string()))?;
    let contents = file
        .contents_utf8()
        .ok_or_else(|| SeedError::InvalidUtf8(filename.to_string()))?;
    let seed: GlobalSettingsSeed = serde_json::from_str(contents)
        .map_err(|err| SeedError::InvalidBridge(filename.to_string(), err.to_string()))?;

    for group in &seed.groups {
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
        .bind(("id", group.id.clone()))
        .bind(("label", group.label.clone()))
        .bind(("color", group.color.clone()))
        .bind(("icon", group.icon.clone()))
        .bind(("icon_type", group.icon_type.clone()))
        .await?
        .check()
        .map_err(|err| SeedError::ApplyFailed {
            version: version.to_string(),
            message: err.to_string(),
        })?;

        for item in &group.items {
            db.query(
                r#"
UPSERT type::record("gl_settings_items", $id) CONTENT {
    label: $label,
    s_key: $s_key,
    s_value: $s_value,
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
            .bind(("description", item.description.clone()))
            .bind(("long_description", item.long_description.clone()))
            .bind(("relation_id", format!("{}_{}", group.id, item.id)))
            .bind(("group_id", group.id.clone()))
            .bind(("item_id", item.id.clone()))
            .await?
            .check()
            .map_err(|err| SeedError::ApplyFailed {
                version: version.to_string(),
                message: err.to_string(),
            })?;
        }
    }

    for provider in &seed.auth_providers {
        db.query(
            r#"
UPSERT type::record("auth_providers", $id) CONTENT {
    name: $name,
    label: $label,
    provider_type: $provider_type,
    issuer_url: $issuer_url,
    client_id: $client_id,
    scopes: $scopes,
    enabled: $enabled,
    config: $config,
    date_created: time::now(),
    date_updated: time::now()
};
"#,
        )
        .bind(("id", provider.id.clone()))
        .bind(("name", provider.name.clone()))
        .bind(("label", provider.label.clone()))
        .bind(("provider_type", provider.provider_type.clone()))
        .bind(("issuer_url", provider.issuer_url.clone()))
        .bind(("client_id", provider.client_id.clone()))
        .bind(("scopes", provider.scopes.clone()))
        .bind(("enabled", provider.enabled))
        .bind(("config", provider.config.clone()))
        .await?
        .check()
        .map_err(|err| SeedError::ApplyFailed {
            version: version.to_string(),
            message: err.to_string(),
        })?;
    }

    record_seed(db, version, filename).await?;
    log::info!("Seeded {filename}");
    Ok(vec![version.to_string()])
}

#[derive(Debug, Deserialize)]
struct BridgeCategory {
    code: String,
    items: Vec<BridgeItem>,
}

#[derive(Debug, Deserialize)]
struct BridgeItem {
    feature_key: String,
    name: String,
    label: String,
    description: Option<String>,
    version: Option<String>,
    is_builtin: bool,
    enabled_globally: bool,
    config_schema: Option<Value>,
}

/// Seed bridge catalog JSON files into deterministic `bridges:<code>` records.
pub async fn seed_bridges(
    db: &Surreal<Client>,
    namespace: &str,
    database: &str,
) -> Result<Vec<String>, SeedError> {
    db.use_ns(namespace).await?;
    db.use_db(database).await?;
    ensure_seed_tracking_table(db).await?;

    let mut applied = load_applied_seeds(db).await?;
    let mut newly_applied = Vec::new();
    let mut seen_codes = HashSet::new();

    let mut files: Vec<_> = SEEDS_BRIDGE_FILES.files().collect();
    files.sort_by_key(|file| file.path().to_path_buf());

    for file in files {
        let name = file
            .path()
            .file_name()
            .and_then(|name| name.to_str())
            .unwrap_or("unknown.json");
        if !name.ends_with(".json") {
            continue;
        }

        let contents = file
            .contents_utf8()
            .ok_or_else(|| SeedError::InvalidUtf8(name.to_string()))?;
        let category: BridgeCategory = serde_json::from_str(contents)
            .map_err(|err| SeedError::InvalidBridge(name.to_string(), err.to_string()))?;
        if category.code.trim().is_empty() {
            return Err(SeedError::InvalidBridge(
                name.to_string(),
                "category code cannot be empty".to_string(),
            ));
        }

        for bridge in category.items {
            if bridge.feature_key.trim().is_empty()
                || bridge.name.trim().is_empty()
                || bridge.label.trim().is_empty()
            {
                return Err(SeedError::InvalidBridge(
                    name.to_string(),
                    "bridge code and name cannot be empty".to_string(),
                ));
            }
            if !seen_codes.insert(bridge.feature_key.clone()) {
                return Err(SeedError::InvalidBridge(
                    name.to_string(),
                    format!("duplicate bridge feature key `{}`", bridge.feature_key),
                ));
            }

            let version = format!("bridge/{}", bridge.feature_key);
            if applied.contains(&version) {
                continue;
            }

            db.query(
                r#"
UPSERT type::record("bridges", $feature_key) CONTENT {
    name: $name,
    label: $label,
    description: $description,
    category: $category,
    feature_key: $feature_key,
    version: $version,
    is_builtin: $is_builtin,
    enabled_globally: $enabled_globally,
    config_schema: $config_schema,
    date_created: time::now(),
    date_updated: time::now()
};
"#,
            )
            .bind(("feature_key", bridge.feature_key.clone()))
            .bind(("name", bridge.name))
            .bind(("label", bridge.label))
            .bind(("description", bridge.description))
            .bind(("category", category.code.clone()))
            .bind(("version", bridge.version))
            .bind(("is_builtin", bridge.is_builtin))
            .bind(("enabled_globally", bridge.enabled_globally))
            .bind(("config_schema", bridge.config_schema))
            .await?
            .check()
            .map_err(|err| SeedError::ApplyFailed {
                version: version.clone(),
                message: err.to_string(),
            })?;

            record_seed(db, &version, &version).await?;
            applied.insert(version.clone());
            newly_applied.push(version);
            log::info!("Seeded bridge {}", bridge.feature_key);
        }
    }

    Ok(newly_applied)
}

async fn ensure_seed_tracking_table(db: &Surreal<Client>) -> Result<(), SeedError> {
    db.query(
        r#"
DEFINE TABLE IF NOT EXISTS schema_seeds;
DEFINE FIELD IF NOT EXISTS version ON TABLE schema_seeds TYPE string;
DEFINE FIELD IF NOT EXISTS filename ON TABLE schema_seeds TYPE string;
DEFINE FIELD IF NOT EXISTS applied_at ON TABLE schema_seeds TYPE datetime DEFAULT time::now();
DEFINE INDEX IF NOT EXISTS schema_seeds_version ON TABLE schema_seeds COLUMNS version UNIQUE;
DEFINE INDEX IF NOT EXISTS schema_seeds_filename ON TABLE schema_seeds COLUMNS filename UNIQUE;
"#,
    )
    .await?
    .check()?;
    Ok(())
}

async fn load_applied_seeds(db: &Surreal<Client>) -> Result<HashSet<String>, SeedError> {
    let mut response = db
        .query("SELECT version FROM schema_seeds;")
        .await?
        .check()?;
    let rows: Vec<AppliedSeed> = response.take(0)?;
    Ok(rows.into_iter().map(|seed| seed.version).collect())
}

async fn record_seed(db: &Surreal<Client>, version: &str, filename: &str) -> Result<(), SeedError> {
    db.query(
        r#"
CREATE schema_seeds SET
    version = $version,
    filename = $filename,
    applied_at = time::now();
"#,
    )
    .bind(("version", version.to_string()))
    .bind(("filename", filename.to_string()))
    .await?
    .check()?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn bridge_catalogs_are_valid_and_globally_unique() {
        let mut codes = HashSet::new();
        let files: Vec<_> = SEEDS_BRIDGE_FILES.files().collect();

        assert!(!files.is_empty());
        for file in files {
            let category: BridgeCategory =
                serde_json::from_slice(file.contents()).expect("valid bridge catalog");
            assert!(!category.code.is_empty());
            for bridge in category.items {
                assert!(!bridge.feature_key.is_empty());
                assert!(
                    codes.insert(bridge.feature_key),
                    "duplicate bridge feature key"
                );
                assert!(!bridge.name.is_empty());
                assert!(!bridge.label.is_empty());
            }
        }
    }

    #[test]
    fn global_settings_catalog_is_valid_json() {
        let file = SEEDS_SETTINGS_FILES
            .get_file("global_settings.json")
            .expect("global settings seed");
        let seed: GlobalSettingsSeed =
            serde_json::from_slice(file.contents()).expect("valid global settings JSON");

        assert_eq!(seed.groups.len(), 3);
        assert_eq!(seed.auth_providers.len(), 1);
        assert!(
            seed.groups
                .iter()
                .flat_map(|group| group.items.iter())
                .all(|item| !item.s_key.is_empty())
        );
    }
}
