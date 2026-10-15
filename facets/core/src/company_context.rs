//! Current company for the signed-in user inside an organization.
//!
//! Parties and other shared masters are **not** filtered by this. It is the Odoo-style
//! working company: shown in the shell, available on `context::get`, and the default
//! for company-scoped documents later.

use serde::{Deserialize, Serialize};
use serde_json::{Map, Value as JsonValue};
use surrealdb::types::SurrealValue;
use thiserror::Error;

use crate::state::Db;

#[derive(Debug, Error)]
pub enum CompanyContextError {
    #[error("database: {0}")]
    Database(String),
    #[error("company plugin is not available in this organization")]
    Unavailable,
    #[error("company not found")]
    NotFound,
    #[error("company is archived")]
    Archived,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CompanyChoice {
    pub id: String,
    pub name: String,
    pub is_default: bool,
}

#[derive(Debug, Deserialize, SurrealValue)]
struct SchemaRow {
    model_id: Option<String>,
    snapshot: Option<JsonValue>,
}

#[derive(Debug, Deserialize, SurrealValue)]
struct CompanyRow {
    id: Option<String>,
    name: String,
    #[serde(default)]
    is_default: Option<bool>,
    #[serde(default)]
    is_active: Option<bool>,
}

#[derive(Debug, Deserialize, SurrealValue)]
struct PrefRow {
    company_id: Option<String>,
}

struct CompanyMeta {
    table: String,
    name_field: String,
    is_default_field: String,
    is_active_field: String,
}

fn record_key(raw: &str) -> String {
    match raw.split_once(':') {
        Some((_, key)) => key.to_string(),
        None => raw.to_string(),
    }
}

fn company_id_string(table: &str, raw: Option<&str>) -> String {
    let text = raw.unwrap_or_default();
    if text.is_empty() {
        return String::new();
    }
    if text.contains(':') {
        text.to_string()
    } else {
        format!("{table}:{text}")
    }
}

fn safe_ident(raw: &str) -> Option<&str> {
    let ok = !raw.is_empty() && raw.chars().all(|c| c.is_ascii_alphanumeric() || c == '_');
    ok.then_some(raw)
}

fn field_id(snapshot: &JsonValue, logical: &str) -> Option<String> {
    let fields = snapshot.get("fields")?.as_object()?;
    for (id, def) in fields {
        let name = def.get("name").and_then(|v| v.as_str())?;
        if name == logical {
            return safe_ident(id).map(str::to_string);
        }
    }
    None
}

async fn resolve_meta(org: &Db) -> Result<Option<CompanyMeta>, CompanyContextError> {
    let response = match org
        .query(
            r#"
            SELECT model_id, snapshot FROM model_schema
            WHERE plugin = 'company' AND name = 'company'
            LIMIT 1;
            "#,
        )
        .await
    {
        Ok(response) => response,
        Err(error) => {
            let message = error.to_string();
            if message.contains("does not exist") || message.contains("model_schema") {
                return Ok(None);
            }
            return Err(CompanyContextError::Database(message));
        }
    };
    let mut response = match response.check() {
        Ok(response) => response,
        Err(error) => {
            let message = error.to_string();
            if message.contains("does not exist") || message.contains("model_schema") {
                return Ok(None);
            }
            return Err(CompanyContextError::Database(message));
        }
    };

    let rows: Vec<SchemaRow> = response
        .take(0)
        .map_err(|e| CompanyContextError::Database(e.to_string()))?;
    let Some(row) = rows.into_iter().next() else {
        return Ok(None);
    };
    let table = match row.model_id.as_deref().and_then(safe_ident) {
        Some(table) => table.to_string(),
        None => return Ok(None),
    };
    let snapshot = row.snapshot.unwrap_or(JsonValue::Object(Map::new()));
    let (Some(name_field), Some(is_default_field), Some(is_active_field)) = (
        field_id(&snapshot, "name"),
        field_id(&snapshot, "is_default"),
        field_id(&snapshot, "is_active"),
    ) else {
        return Err(CompanyContextError::Database(
            "company model is missing name/is_default/is_active fields".into(),
        ));
    };

    Ok(Some(CompanyMeta {
        table,
        name_field,
        is_default_field,
        is_active_field,
    }))
}

/// Active companies when the company model is applied; empty when it is not.
pub async fn list_active(org: &Db) -> Result<Vec<CompanyChoice>, CompanyContextError> {
    let Some(meta) = resolve_meta(org).await? else {
        return Err(CompanyContextError::Unavailable);
    };

    let sql = format!(
        r#"
        SELECT id, {name} AS name, {is_default} AS is_default, {is_active} AS is_active
        FROM {table}
        WHERE {is_active} = true OR {is_active} = NONE
        ORDER BY {name} ASC
        LIMIT 500;
        "#,
        name = meta.name_field,
        is_default = meta.is_default_field,
        is_active = meta.is_active_field,
        table = meta.table,
    );

    let mut response = org
        .query(sql)
        .await
        .map_err(|e| CompanyContextError::Database(e.to_string()))?
        .check()
        .map_err(|e| CompanyContextError::Database(e.to_string()))?;

    let rows: Vec<CompanyRow> = response
        .take(0)
        .map_err(|e| CompanyContextError::Database(e.to_string()))?;

    Ok(rows
        .into_iter()
        .filter_map(|row| {
            let id = company_id_string(&meta.table, row.id.as_deref());
            if id.is_empty() {
                return None;
            }
            Some(CompanyChoice {
                id,
                name: row.name,
                is_default: row.is_default.unwrap_or(false),
            })
        })
        .collect())
}

async fn preferred_id(org: &Db, user_id: &str) -> Result<Option<String>, CompanyContextError> {
    let response = match org
        .query(
            r#"
            SELECT company_id FROM aether_user_defaults
            WHERE user_id = $user
            LIMIT 1;
            "#,
        )
        .bind(("user", user_id.to_string()))
        .await
    {
        Ok(response) => response,
        Err(error) => {
            let message = error.to_string();
            if message.contains("does not exist") || message.contains("aether_user_defaults") {
                return Ok(None);
            }
            return Err(CompanyContextError::Database(message));
        }
    };
    let mut response = match response.check() {
        Ok(response) => response,
        Err(error) => {
            let message = error.to_string();
            if message.contains("does not exist") || message.contains("aether_user_defaults") {
                return Ok(None);
            }
            return Err(CompanyContextError::Database(message));
        }
    };

    let rows: Vec<PrefRow> = response
        .take(0)
        .map_err(|e| CompanyContextError::Database(e.to_string()))?;
    Ok(rows.into_iter().next().and_then(|row| row.company_id))
}

/// Resolve the user's current company: preference → default → first active.
pub async fn resolve_current(
    org: &Db,
    user_id: &str,
) -> Result<Option<CompanyChoice>, CompanyContextError> {
    let companies = match list_active(org).await {
        Ok(rows) => rows,
        Err(CompanyContextError::Unavailable) => return Ok(None),
        Err(err) => return Err(err),
    };
    if companies.is_empty() {
        return Ok(None);
    }

    if let Some(preferred) = preferred_id(org, user_id).await? {
        let key = record_key(&preferred);
        if let Some(found) = companies
            .iter()
            .find(|c| record_key(&c.id) == key || c.id == preferred)
        {
            return Ok(Some(found.clone()));
        }
    }

    if let Some(default) = companies.iter().find(|c| c.is_default) {
        return Ok(Some(default.clone()));
    }
    Ok(companies.into_iter().next())
}

/// Persist the user's working company (must be an active company).
pub async fn set_current(
    org: &Db,
    user_id: &str,
    company_id: &str,
) -> Result<CompanyChoice, CompanyContextError> {
    let companies = list_active(org).await?;
    if companies.is_empty() {
        return Err(CompanyContextError::Unavailable);
    }

    let key = record_key(company_id);
    let chosen = companies
        .into_iter()
        .find(|c| record_key(&c.id) == key || c.id == company_id)
        .ok_or(CompanyContextError::NotFound)?;

    org.query(
        r#"
        UPSERT type::record("aether_user_defaults", $key) SET
            user_id = $user,
            company_id = $company,
            date_updated = time::now();
        "#,
    )
    .bind(("key", format!("u_{}", user_id.replace(':', "_"))))
    .bind(("user", user_id.to_string()))
    .bind(("company", chosen.id.clone()))
    .await
    .map_err(|e| CompanyContextError::Database(e.to_string()))?
    .check()
    .map_err(|e| CompanyContextError::Database(e.to_string()))?;

    Ok(chosen)
}

/// Overview for the shell API.
pub async fn overview(org: &Db, user_id: &str) -> Result<JsonValue, CompanyContextError> {
    let companies = match list_active(org).await {
        Ok(rows) => rows,
        Err(CompanyContextError::Unavailable) => {
            return Ok(serde_json::json!({
                "available": false,
                "current": null,
                "companies": [],
            }));
        }
        Err(err) => return Err(err),
    };

    let current = resolve_current(org, user_id).await?;
    Ok(serde_json::json!({
        "available": !companies.is_empty(),
        "current": current,
        "companies": companies,
    }))
}
