use serde::{Deserialize, Serialize};
use serde_json::Value as JsonValue;
use surrealdb::{Surreal, engine::remote::ws::Client, types::RecordId, types::SurrealValue};
use thiserror::Error;

use crate::state::AppState;
use crate::tenancy::OrgRef;

#[derive(Debug, Error)]
pub enum SettingsError {
    #[error("database error: {0}")]
    Db(#[from] surrealdb::Error),
    #[error("setting `{0}` not found")]
    NotFound(String),
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SettingValue {
    pub key: String,
    pub value: JsonValue,
    /// `"global"` or `"org"`
    pub source: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub org: Option<String>,
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

/// Read a setting key: global first, then org override if `org` is set and has a row.
pub async fn get_setting(
    state: &AppState,
    key: &str,
    org: Option<&OrgRef>,
) -> Result<Option<SettingValue>, SettingsError> {
    state.use_core().await?;

    let mut response = state
        .db
        .query("SELECT s_key, s_value FROM gl_settings_items WHERE s_key = $key LIMIT 1;")
        .bind(("key", key.to_string()))
        .await?
        .check()?;
    let global_rows: Vec<SettingRow> = response.take(0)?;
    let global = global_rows.into_iter().next();

    if let Some(org) = org {
        if let Err(err) = state.use_org(&org.db_name).await {
            log::warn!(
                "org DB `{}` unavailable for settings override: {err}",
                org.db_name
            );
        } else {
            let mut org_response = state
                .db
                .query("SELECT s_key, s_value FROM settings_items WHERE s_key = $key LIMIT 1;")
                .bind(("key", key.to_string()))
                .await?
                .check()?;
            let org_rows: Vec<SettingRow> = org_response.take(0)?;
            if let Some(row) = org_rows.into_iter().next() {
                let _ = state.use_core().await;
                return Ok(Some(SettingValue {
                    key: row.s_key,
                    value: row.s_value,
                    source: "org".into(),
                    org: Some(org.slug.clone()),
                }));
            }
        }
        let _ = state.use_core().await;
    }

    Ok(global.map(|row| SettingValue {
        key: row.s_key,
        value: row.s_value,
        source: "global".into(),
        org: None,
    }))
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
    state.use_core().await?;

    let mut groups_resp = state
        .db
        .query("SELECT id, label, color, icon, icon_type FROM gl_settings_groups ORDER BY label ASC;")
        .await?
        .check()?;
    let groups: Vec<GroupRow> = groups_resp.take(0)?;

    let mut links_resp = state
        .db
        .query("SELECT group, item FROM gl_settings_group_items;")
        .await?
        .check()?;
    let links: Vec<GroupItemRow> = links_resp.take(0)?;

    let mut items_resp = state
        .db
        .query(
            r#"
            SELECT id, label, s_key, s_value, description, long_description
            FROM gl_settings_items;
            "#,
        )
        .await?
        .check()?;
    let items: Vec<ItemMetaRow> = items_resp.take(0)?;

    // Preload org overrides once.
    let mut org_overrides: std::collections::HashMap<String, JsonValue> =
        std::collections::HashMap::new();
    if let Some(org) = org {
        if state.use_org(&org.db_name).await.is_ok() {
            if let Ok(resp) = state
                .db
                .query("SELECT s_key, s_value FROM settings_items;")
                .await
            {
                if let Ok(mut checked) = resp.check() {
                    if let Ok(rows) = checked.take::<Vec<SettingRow>>(0) {
                        for row in rows {
                            org_overrides.insert(row.s_key, row.s_value);
                        }
                    }
                }
            }
        }
        let _ = state.use_core().await;
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

            catalog_items.push(CatalogItem {
                key: item.s_key.clone(),
                label: item.label.clone(),
                value: value.clone(),
                source,
                description: item.description.clone(),
                long_description: item.long_description.clone(),
                value_type: infer_value_type(&value),
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
pub async fn set_setting(
    state: &AppState,
    key: &str,
    value: JsonValue,
    org: Option<&OrgRef>,
) -> Result<SettingValue, SettingsError> {
    if let Some(org) = org {
        state.use_core().await?;
        let mut meta = state
            .db
            .query(
                r#"
                SELECT label, description, long_description FROM gl_settings_items
                WHERE s_key = $key LIMIT 1;
                "#,
            )
            .bind(("key", key.to_string()))
            .await?
            .check()?;
        #[derive(Debug, Deserialize, SurrealValue)]
        struct Meta {
            label: String,
            description: Option<String>,
            long_description: Option<String>,
        }
        let meta_rows: Vec<Meta> = meta.take(0)?;
        let meta = meta_rows
            .into_iter()
            .next()
            .ok_or_else(|| SettingsError::NotFound(key.into()))?;

        state.use_org(&org.db_name).await?;
        let mut existing = state
            .db
            .query("SELECT s_key, s_value FROM settings_items WHERE s_key = $key LIMIT 1;")
            .bind(("key", key.to_string()))
            .await?
            .check()?;
        let rows: Vec<SettingRow> = existing.take(0).unwrap_or_default();
        if rows.is_empty() {
            state
                .db
                .query(
                    r#"
                    CREATE settings_items SET
                        label = $label,
                        s_key = $key,
                        s_value = $value,
                        description = $description,
                        long_description = $long_description;
                    "#,
                )
                .bind(("key", key.to_string()))
                .bind(("label", meta.label))
                .bind(("value", value.clone()))
                .bind(("description", meta.description))
                .bind(("long_description", meta.long_description))
                .await?
                .check()?;
        } else {
            state
                .db
                .query("UPDATE settings_items SET s_value = $value WHERE s_key = $key;")
                .bind(("key", key.to_string()))
                .bind(("value", value.clone()))
                .await?
                .check()?;
        }

        let _ = state.use_core().await;
        return Ok(SettingValue {
            key: key.into(),
            value,
            source: "org".into(),
            org: Some(org.slug.clone()),
        });
    }

    state.use_core().await?;
    let mut updated = state
        .db
        .query(
            r#"
            UPDATE gl_settings_items SET s_value = $value WHERE s_key = $key RETURN AFTER;
            "#,
        )
        .bind(("key", key.to_string()))
        .bind(("value", value.clone()))
        .await?
        .check()?;
    let rows: Vec<SettingRow> = updated.take(0)?;
    if rows.is_empty() {
        return Err(SettingsError::NotFound(key.into()));
    }

    Ok(SettingValue {
        key: key.into(),
        value,
        source: "global".into(),
        org: None,
    })
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
            }));
        }
        db.use_db(core_db).await?;
    }

    Ok(global.map(|row| SettingValue {
        key: row.s_key,
        value: row.s_value,
        source: "global".into(),
        org: None,
    }))
}
