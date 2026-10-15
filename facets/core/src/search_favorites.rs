//! Per-user saved list searches (faceted search favorites).

use serde::{Deserialize, Serialize};
use serde_json::{Map, Value as JsonValue};
use surrealdb::types::SurrealValue;
use thiserror::Error;

use crate::state::AppState;

#[derive(Debug, Error)]
pub enum FavoriteError {
    #[error("database: {0}")]
    Database(String),
    #[error("{0}")]
    Invalid(String),
    #[error("not found")]
    NotFound,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SearchFavorite {
    pub id: String,
    pub user_id: String,
    pub model: String,
    pub label: String,
    #[serde(default)]
    pub filter: Map<String, JsonValue>,
    #[serde(default)]
    pub q: Option<String>,
    #[serde(default)]
    pub group_by: Vec<String>,
    #[serde(default)]
    pub is_default: bool,
    #[serde(default)]
    pub date_created: Option<String>,
    #[serde(default)]
    pub date_updated: Option<String>,
}

#[derive(Debug, Deserialize, SurrealValue)]
struct FavoriteRow {
    id: Option<String>,
    user_id: String,
    model: String,
    label: String,
    #[serde(default)]
    filter: Option<JsonValue>,
    #[serde(default)]
    q: Option<String>,
    #[serde(default)]
    group_by: Vec<String>,
    #[serde(default)]
    is_default: Option<bool>,
    #[serde(default)]
    date_created: Option<String>,
    #[serde(default)]
    date_updated: Option<String>,
}

fn row_id(raw: Option<&str>) -> String {
    raw.unwrap_or_default()
        .strip_prefix("search_favorites:")
        .unwrap_or(raw.unwrap_or_default())
        .to_string()
}

fn filter_map(value: Option<JsonValue>) -> Map<String, JsonValue> {
    match value {
        Some(JsonValue::Object(map)) => map,
        _ => Map::new(),
    }
}

fn to_favorite(row: FavoriteRow) -> SearchFavorite {
    SearchFavorite {
        id: row_id(row.id.as_deref()),
        user_id: row.user_id,
        model: row.model,
        label: row.label,
        filter: filter_map(row.filter),
        q: row.q,
        group_by: row.group_by,
        is_default: row.is_default.unwrap_or(false),
        date_created: row.date_created,
        date_updated: row.date_updated,
    }
}

pub async fn list_for_user(
    state: &AppState,
    user_id: &str,
    model: &str,
) -> Result<Vec<SearchFavorite>, FavoriteError> {
    let model = model.trim();
    if model.is_empty() {
        return Err(FavoriteError::Invalid("model is required".into()));
    }
    let core = state
        .core()
        .await
        .map_err(|error| FavoriteError::Database(error.to_string()))?;
    let mut response = core
        .query(
            "SELECT * FROM search_favorites WHERE user_id = $user AND model = $model ORDER BY label;",
        )
        .bind(("user", user_id.to_string()))
        .bind(("model", model.to_string()))
        .await
        .and_then(|response| response.check())
        .map_err(|error| FavoriteError::Database(error.to_string()))?;
    let rows: Vec<FavoriteRow> = response
        .take(0)
        .map_err(|error| FavoriteError::Database(error.to_string()))?;
    Ok(rows.into_iter().map(to_favorite).collect())
}

pub async fn create(
    state: &AppState,
    user_id: &str,
    model: &str,
    label: &str,
    filter: Map<String, JsonValue>,
    q: Option<String>,
    group_by: Vec<String>,
    is_default: bool,
) -> Result<SearchFavorite, FavoriteError> {
    let model = model.trim();
    let label = label.trim();
    if model.is_empty() {
        return Err(FavoriteError::Invalid("model is required".into()));
    }
    if label.is_empty() {
        return Err(FavoriteError::Invalid("label is required".into()));
    }
    if label.len() > 120 {
        return Err(FavoriteError::Invalid("label is too long".into()));
    }
    let core = state
        .core()
        .await
        .map_err(|error| FavoriteError::Database(error.to_string()))?;
    let mut response = core
        .query(
            "CREATE search_favorites SET
                user_id = $user,
                model = $model,
                label = $label,
                filter = $filter,
                q = $q,
                group_by = $group_by,
                is_default = $is_default,
                date_created = time::now(),
                date_updated = time::now()
             RETURN AFTER;",
        )
        .bind(("user", user_id.to_string()))
        .bind(("model", model.to_string()))
        .bind(("label", label.to_string()))
        .bind(("filter", JsonValue::Object(filter)))
        .bind(("q", q))
        .bind(("group_by", group_by))
        .bind(("is_default", is_default))
        .await
        .and_then(|response| response.check())
        .map_err(|error| FavoriteError::Database(error.to_string()))?;
    let rows: Vec<FavoriteRow> = response
        .take(0)
        .map_err(|error| FavoriteError::Database(error.to_string()))?;
    rows.into_iter()
        .next()
        .map(to_favorite)
        .ok_or_else(|| FavoriteError::Database("create returned no row".into()))
}

pub async fn delete_for_user(
    state: &AppState,
    user_id: &str,
    id: &str,
) -> Result<(), FavoriteError> {
    let id = id
        .trim()
        .strip_prefix("search_favorites:")
        .unwrap_or(id.trim());
    if id.is_empty() {
        return Err(FavoriteError::Invalid("id is required".into()));
    }
    let core = state
        .core()
        .await
        .map_err(|error| FavoriteError::Database(error.to_string()))?;
    let mut response = core
        .query("DELETE type::record('search_favorites', $id) WHERE user_id = $user RETURN BEFORE;")
        .bind(("id", id.to_string()))
        .bind(("user", user_id.to_string()))
        .await
        .and_then(|response| response.check())
        .map_err(|error| FavoriteError::Database(error.to_string()))?;
    let rows: Vec<FavoriteRow> = response
        .take(0)
        .map_err(|error| FavoriteError::Database(error.to_string()))?;
    if rows.is_empty() {
        return Err(FavoriteError::NotFound);
    }
    Ok(())
}
