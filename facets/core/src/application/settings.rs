use serde::{Deserialize, Serialize};
use serde_json::Value as JsonValue;
use surrealdb::{Surreal, engine::remote::ws::Client, types::RecordId, types::SurrealValue};
use thiserror::Error;

use crate::secrets::SecretBox;
use crate::state::AppState;
use crate::tenancy::OrgRef;

#[derive(Debug, Error)]
pub enum SettingsError {
    #[error("database error: {0}")]
    Db(#[from] surrealdb::Error),
    #[error("setting `{0}` not found")]
    NotFound(String),
    #[error("{0}")]
    Invalid(String),
    #[error(transparent)]
    Secrets(#[from] crate::secrets::SecretError),
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SettingValue {
    pub key: String,
    pub value: JsonValue,
    /// `"global"` or `"org"`
    pub source: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub org: Option<String>,
    /// A secret setting (API key, password): `value` is blank here whatever is stored; see
    /// `has_value` for whether one is set.
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    pub secret: bool,
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    pub has_value: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CatalogItem {
    pub key: String,
    pub label: String,
    pub value: JsonValue,
    pub source: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub description: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub long_description: Option<String>,
    pub value_type: String,
    /// A secret setting: `value` is blank and `has_value` says whether one is saved.
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    pub secret: bool,
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    pub has_value: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CatalogGroup {
    pub id: String,
    pub slug: String,
    pub label: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub color: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub icon: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub icon_type: Option<String>,
    pub items: Vec<CatalogItem>,
}

#[derive(Debug, Deserialize, SurrealValue)]
struct SettingRow {
    s_key: String,
    s_value: JsonValue,
    is_secret: Option<bool>,
}

#[derive(Debug, Deserialize, SurrealValue)]
struct GroupRow {
    id: RecordId,
    label: String,
    color: Option<String>,
    icon: Option<String>,
    icon_type: Option<String>,
}

#[derive(Debug, Deserialize, SurrealValue)]
struct ItemMetaRow {
    id: RecordId,
    label: String,
    s_key: String,
    s_value: JsonValue,
    description: Option<String>,
    long_description: Option<String>,
    is_secret: Option<bool>,
}

#[derive(Debug, Deserialize, SurrealValue)]
struct GroupItemRow {
    group: RecordId,
    item: RecordId,
}

fn slugify(label: &str) -> String {
    let lower = label.to_lowercase();
    let mut out = String::new();
    let mut prev_dash = false;
    for ch in lower.chars() {
        if ch.is_ascii_alphanumeric() {
            out.push(ch);
            prev_dash = false;
        } else if !prev_dash {
            out.push('-');
            prev_dash = true;
        }
    }
    let slug = out.trim_matches('-').to_string();
    // Prefer short stable slugs for known groups.
    match slug.as_str() {
        "general-settings" => "general".into(),
        "security-privacy" => "security".into(),
        other => other.to_string(),
    }
}

fn infer_value_type(value: &JsonValue) -> String {
    match value {
        JsonValue::Bool(_) => "boolean".into(),
        JsonValue::Number(_) => "number".into(),
        JsonValue::Array(_) => "list".into(),
        JsonValue::Object(_) => "json".into(),
        _ => "string".into(),
    }
}

/// A setting as stored: a secret's value is still encrypted.
struct Stored {
    value: SettingValue,
    secret: bool,
}

/// Read a setting key: the organization's own value if it has one, else the global one.
async fn lookup(state: &AppState, key: &str, org: Option<&OrgRef>) -> Result<Option<Stored>, SettingsError> {
    let core = state.core().await?;

    let mut response = core
        .query("SELECT s_key, s_value, is_secret FROM gl_settings_items WHERE s_key = $key LIMIT 1;")
        .bind(("key", key.to_string()))
        .await?
        .check()?;
    let global_rows: Vec<SettingRow> = response.take(0)?;
    let global = global_rows.into_iter().next();
    let secret = global.as_ref().and_then(|row| row.is_secret).unwrap_or(false);

    if let Some(org) = org {
        match state.org(&org.db_name).await {
            Err(err) => log::warn!(
                "org DB `{}` unavailable for settings override: {err}",
                org.db_name
            ),
            Ok(org_db) => {
                let mut org_response = org_db
                    .query("SELECT s_key, s_value, is_secret FROM settings_items WHERE s_key = $key LIMIT 1;")
                    .bind(("key", key.to_string()))
                    .await?
                    .check()?;
                let org_rows: Vec<SettingRow> = org_response.take(0)?;
                if let Some(row) = org_rows.into_iter().next() {
                    return Ok(Some(Stored {
                        secret: secret || row.is_secret.unwrap_or(false),
                        value: SettingValue {
                            key: row.s_key,
                            value: row.s_value,
                            source: "org".into(),
                            org: Some(org.slug.clone()),
                            secret: false,
                            has_value: false,
                        },
                    }));
                }
            }
        }
    }

    Ok(global.map(|row| Stored {
        secret,
        value: SettingValue {
            key: row.s_key,
            value: row.s_value,
            source: "global".into(),
            org: None,
            secret: false,
            has_value: false,
        },
    }))
}

fn is_blank(value: &JsonValue) -> bool {
    matches!(value, JsonValue::Null) || value.as_str().is_some_and(str::is_empty)
}

/// A secret's value as it may be shown: blank, with a flag saying whether one is saved.
fn masked(mut stored: Stored) -> SettingValue {
    if stored.secret {
        stored.value.has_value = !is_blank(&stored.value.value);
        stored.value.value = JsonValue::String(String::new());
        stored.value.secret = true;
    }
    stored.value
}

/// Read a setting key for display or the API: the organization's value if it has one, else the
/// global one. A secret setting's value is never returned (see [`get_setting_plain`]).
pub async fn get_setting(
    state: &AppState,
    key: &str,
    org: Option<&OrgRef>,
) -> Result<Option<SettingValue>, SettingsError> {
    Ok(lookup(state, key, org).await?.map(masked))
}

/// The value the kernel itself uses, with secrets decrypted. Only for server-side code (bridges,
/// the scheduler): never return it from an API. A blank or missing value is `None`.
pub async fn get_setting_plain(
    state: &AppState,
    key: &str,
    org: Option<&OrgRef>,
) -> Result<Option<JsonValue>, SettingsError> {
    let Some(stored) = lookup(state, key, org).await? else { return Ok(None) };
    let value = stored.value.value;
    if is_blank(&value) {
        return Ok(None);
    }
    if stored.secret {
        let Some(text) = value.as_str() else { return Ok(None) };
        // A value saved before the setting became secret is still plain text.
        if !SecretBox::is_encrypted(text) {
            return Ok(Some(value));
        }
        let plain = state.secrets().and_then(|secrets| secrets.decrypt(text))?;
        return Ok(Some(JsonValue::String(plain)));
    }
    Ok(Some(value))
}

/// Where to look for a setting.
#[derive(Debug, Clone, Copy)]
pub enum Scope<'a> {
    /// Only the global value.
    Global,
    /// Only this organization's own value (no fallback to the global one).
    Org(&'a OrgRef),
}

/// A setting's value from exactly one place, decrypted if it is a secret, or `None` when it is
/// not set there (or blank). For server-side code that must know *where* a value came from.
pub async fn get_setting_plain_in(
    state: &AppState,
    key: &str,
    scope: Scope<'_>,
) -> Result<Option<JsonValue>, SettingsError> {
    let (row, global_secret) = match scope {
        Scope::Global => {
            let core = state.core().await?;
            let mut response = core
                .query("SELECT s_key, s_value, is_secret FROM gl_settings_items WHERE s_key = $key LIMIT 1;")
                .bind(("key", key.to_string()))
                .await?
                .check()?;
            (response.take::<Vec<SettingRow>>(0)?.into_iter().next(), false)
        }
        Scope::Org(org) => {
            let core = state.core().await?;
            let mut meta = core
                .query("SELECT VALUE is_secret FROM gl_settings_items WHERE s_key = $key LIMIT 1;")
                .bind(("key", key.to_string()))
                .await?
                .check()?;
            let global_secret = meta.take::<Option<Option<bool>>>(0)?.flatten().unwrap_or(false);
            let org_db = state.org(&org.db_name).await?;
            let mut response = org_db
                .query("SELECT s_key, s_value, is_secret FROM settings_items WHERE s_key = $key LIMIT 1;")
                .bind(("key", key.to_string()))
                .await?
                .check()?;
            (response.take::<Vec<SettingRow>>(0)?.into_iter().next(), global_secret)
        }
    };
    let Some(row) = row else { return Ok(None) };
    if is_blank(&row.s_value) {
        return Ok(None);
    }
    let secret = global_secret || row.is_secret.unwrap_or(false);
    match (secret, row.s_value.as_str()) {
        (true, Some(text)) if SecretBox::is_encrypted(text) => {
            Ok(Some(JsonValue::String(state.secrets()?.decrypt(text)?)))
        }
        _ => Ok(Some(row.s_value)),
    }
}

pub async fn get_effective_settings(
    state: &AppState,
    keys: &[&str],
    org: Option<&OrgRef>,
) -> Result<Vec<SettingValue>, SettingsError> {
    let mut out = Vec::with_capacity(keys.len());
    for key in keys {
        if let Some(value) = get_setting(state, key, org).await? {
            out.push(value);
        }
    }
    Ok(out)
}

/// Full settings catalog with groups and effective values.
pub async fn list_catalog(
    state: &AppState,
    org: Option<&OrgRef>,
) -> Result<Vec<CatalogGroup>, SettingsError> {
    let core = state.core().await?;

    let mut groups_resp = core
        .query("SELECT id, label, color, icon, icon_type FROM gl_settings_groups ORDER BY label ASC;")
        .await?
        .check()?;
    let groups: Vec<GroupRow> = groups_resp.take(0)?;

    let mut links_resp = core
        .query("SELECT group, item FROM gl_settings_group_items;")
        .await?
        .check()?;
    let links: Vec<GroupItemRow> = links_resp.take(0)?;

    let mut items_resp = core
        .query(
            r#"
            SELECT id, label, s_key, s_value, description, long_description, is_secret
            FROM gl_settings_items;
            "#,
        )
        .await?
        .check()?;
    let items: Vec<ItemMetaRow> = items_resp.take(0)?;

    // Preload org overrides once.
    let mut org_overrides: std::collections::HashMap<String, JsonValue> =
        std::collections::HashMap::new();
    if let Some(org) = org
        && let Ok(org_db) = state.org(&org.db_name).await
        && let Ok(resp) = org_db.query("SELECT s_key, s_value FROM settings_items;").await
        && let Ok(mut checked) = resp.check()
        && let Ok(rows) = checked.take::<Vec<SettingRow>>(0)
    {
        for row in rows {
            org_overrides.insert(row.s_key, row.s_value);
        }
    }

    let mut catalog = Vec::with_capacity(groups.len());
    for group in groups {
        let group_id = group.id.clone();
        let item_ids: Vec<_> = links
            .iter()
            .filter(|l| l.group == group_id)
            .map(|l| l.item.clone())
            .collect();

        let mut catalog_items = Vec::new();
        for item in items.iter().filter(|i| item_ids.iter().any(|id| id == &i.id)) {
            let (value, source) = if let Some(v) = org_overrides.get(&item.s_key) {
                (v.clone(), "org".to_string())
            } else {
                (item.s_value.clone(), "global".to_string())
            };

            let secret = item.is_secret.unwrap_or(false);
            let has_value = secret && !is_blank(&value);
            let (value, value_type) = if secret {
                (JsonValue::String(String::new()), "secret".to_string())
            } else {
                (value.clone(), infer_value_type(&value))
            };
            catalog_items.push(CatalogItem {
                key: item.s_key.clone(),
                label: item.label.clone(),
                value,
                source,
                description: item.description.clone(),
                long_description: item.long_description.clone(),
                value_type,
                secret,
                has_value,
            });
        }

        catalog_items.sort_by(|a, b| a.label.cmp(&b.label));

        catalog.push(CatalogGroup {
            id: format!("{:?}", group.id),
            slug: slugify(&group.label),
            label: group.label,
            color: group.color,
            icon: group.icon,
            icon_type: group.icon_type,
            items: catalog_items,
        });
    }

    Ok(catalog)
}

/// Persist a setting value. With org context → org `settings_items`; else global.
///
/// A secret setting is encrypted before it is stored and the answer never repeats it. Saving a
/// blank secret for an organization removes the organization's value, so the global one applies
/// again.
pub async fn set_setting(
    state: &AppState,
    key: &str,
    value: JsonValue,
    org: Option<&OrgRef>,
) -> Result<SettingValue, SettingsError> {
    #[derive(Debug, Deserialize, SurrealValue)]
    struct Meta {
        label: String,
        description: Option<String>,
        long_description: Option<String>,
        is_secret: Option<bool>,
    }
    let core = state.core().await?;
    let mut meta = core
        .query("SELECT label, description, long_description, is_secret FROM gl_settings_items WHERE s_key = $key LIMIT 1;")
        .bind(("key", key.to_string()))
        .await?
        .check()?;
    let meta = meta
        .take::<Vec<Meta>>(0)?
        .into_iter()
        .next()
        .ok_or_else(|| SettingsError::NotFound(key.into()))?;
    let secret = meta.is_secret.unwrap_or(false);

    let stored = if secret {
        let text = value
            .as_str()
            .ok_or_else(|| SettingsError::Invalid(format!("`{key}` is a secret and must be text")))?;
        if text.is_empty() {
            JsonValue::String(String::new())
        } else {
            JsonValue::String(state.secrets()?.encrypt(text))
        }
    } else {
        value
    };

    if let Some(org) = org {
        let org_db = state.org(&org.db_name).await?;
        if secret && is_blank(&stored) {
            org_db
                .query("DELETE settings_items WHERE s_key = $key;")
                .bind(("key", key.to_string()))
                .await?
                .check()?;
            return lookup(state, key, Some(org))
                .await?
                .map(masked)
                .ok_or_else(|| SettingsError::NotFound(key.into()));
        }
        let mut existing = org_db
            .query("SELECT s_key, s_value, is_secret FROM settings_items WHERE s_key = $key LIMIT 1;")
            .bind(("key", key.to_string()))
            .await?
            .check()?;
        let rows: Vec<SettingRow> = existing.take(0).unwrap_or_default();
        if rows.is_empty() {
            org_db
                .query(
                    r#"
                    CREATE settings_items SET
                        label = $label,
                        s_key = $key,
                        s_value = $value,
                        is_secret = $secret,
                        description = $description,
                        long_description = $long_description;
                    "#,
                )
                .bind(("key", key.to_string()))
                .bind(("label", meta.label))
                .bind(("value", stored.clone()))
                .bind(("secret", secret))
                .bind(("description", meta.description))
                .bind(("long_description", meta.long_description))
                .await?
                .check()?;
        } else {
            org_db
                .query("UPDATE settings_items SET s_value = $value, is_secret = $secret WHERE s_key = $key;")
                .bind(("key", key.to_string()))
                .bind(("value", stored.clone()))
                .bind(("secret", secret))
                .await?
                .check()?;
        }

        return Ok(masked(Stored {
            secret,
            value: SettingValue {
                key: key.into(),
                value: stored,
                source: "org".into(),
                org: Some(org.slug.clone()),
                secret: false,
                has_value: false,
            },
        }));
    }

    let mut updated = core
        .query("UPDATE gl_settings_items SET s_value = $value WHERE s_key = $key RETURN AFTER;")
        .bind(("key", key.to_string()))
        .bind(("value", stored.clone()))
        .await?
        .check()?;
    let rows: Vec<SettingRow> = updated.take(0)?;
    if rows.is_empty() {
        return Err(SettingsError::NotFound(key.into()));
    }

    Ok(masked(Stored {
        secret,
        value: SettingValue {
            key: key.into(),
            value: stored,
            source: "global".into(),
            org: None,
            secret: false,
            has_value: false,
        },
    }))
}

/// Convenience against a raw Surreal handle (e.g. CLI seeds).
pub async fn get_setting_on_db(
    db: &Surreal<Client>,
    namespace: &str,
    core_db: &str,
    key: &str,
    org: Option<&OrgRef>,
) -> Result<Option<SettingValue>, SettingsError> {
    db.use_ns(namespace).await?;
    db.use_db(core_db).await?;

    let mut response = db
        .query("SELECT s_key, s_value FROM gl_settings_items WHERE s_key = $key LIMIT 1;")
        .bind(("key", key.to_string()))
        .await?
        .check()?;
    let global_rows: Vec<SettingRow> = response.take(0)?;
    let global = global_rows.into_iter().next();

    if let Some(org) = org {
        db.use_db(&org.db_name).await?;
        let mut org_response = db
            .query("SELECT s_key, s_value FROM settings_items WHERE s_key = $key LIMIT 1;")
            .bind(("key", key.to_string()))
            .await?
            .check()?;
        let org_rows: Vec<SettingRow> = org_response.take(0)?;
        if let Some(row) = org_rows.into_iter().next() {
            db.use_db(core_db).await?;
            return Ok(Some(SettingValue {
                key: row.s_key,
                value: row.s_value,
                source: "org".into(),
                org: Some(org.slug.clone()),
                secret: false,
                has_value: false,
            }));
        }
        db.use_db(core_db).await?;
    }

    Ok(global.map(|row| SettingValue {
        key: row.s_key,
        value: row.s_value,
        source: "global".into(),
        org: None,
        secret: false,
        has_value: false,
    }))
}
