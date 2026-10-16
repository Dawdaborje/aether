//! Per-user favourite records: the star on a list row or a detail page.
//!
//! `GET  /api/ui/records/favorites?plugin=…&model=…` → `{ "ids": [...] }`, the caller's favourites of a model.
//! `POST /api/ui/records/favorites { plugin, model, ids, favorite }` marks or unmarks records.
//!
//! A favourite belongs to one person and is stored in the core database, one row per
//! `(user, plugin, model, record)`; marking twice changes nothing.

use axum::{
    Json, Router,
    extract::{Query, State},
    http::{HeaderMap, StatusCode},
    response::{IntoResponse, Response},
    routing::get,
};
use serde::Deserialize;
use serde_json::json;
use surrealdb::types::ToSql;

use aether_orm::find_session_by_token;

use crate::access::identity::SESSION_COOKIE;
use crate::state::AppState;

const MAX_IDS: usize = 500;

pub fn routes() -> Router<AppState> {
    Router::new().route("/api/ui/records/favorites", get(list).post(set))
}

fn failure(status: StatusCode, message: &str) -> Response {
    (status, Json(json!({ "error": message }))).into_response()
}

async fn require_user(state: &AppState, headers: &HeaderMap) -> Result<String, Response> {
    let cookie = headers
        .get(axum::http::header::COOKIE)
        .and_then(|v| v.to_str().ok())
        .unwrap_or("");
    let token = cookie.split(';').find_map(|part| {
        part.trim()
            .strip_prefix(&format!("{SESSION_COOKIE}="))
            .map(str::to_string)
            .filter(|s| !s.is_empty())
    });
    let Some(token) = token else {
        return Err(failure(StatusCode::UNAUTHORIZED, "not authenticated"));
    };
    let core = state.core().await.map_err(|err| {
        log::error!("record favorites auth: {err}");
        failure(StatusCode::INTERNAL_SERVER_ERROR, "database error")
    })?;
    match find_session_by_token(&core, &token).await {
        Ok(Some(session)) if session.user.is_active => Ok(format!(
            "{}:{}",
            session.user.id.table.as_str(),
            session.user.id.key.to_sql()
        )),
        Ok(Some(_)) => Err(failure(StatusCode::FORBIDDEN, "user inactive")),
        Ok(None) => Err(failure(StatusCode::UNAUTHORIZED, "invalid session")),
        Err(err) => {
            log::error!("record favorites auth: {err}");
            Err(failure(StatusCode::INTERNAL_SERVER_ERROR, "database error"))
        }
    }
}

#[derive(Debug, Deserialize)]
struct ListQuery {
    plugin: String,
    model: String,
}

async fn list(
    State(state): State<AppState>,
    headers: HeaderMap,
    Query(query): Query<ListQuery>,
) -> Response {
    let user = match require_user(&state, &headers).await {
        Ok(user) => user,
        Err(response) => return response,
    };
    let core = match state.core().await {
        Ok(core) => core,
        Err(error) => {
            log::error!("record favorites: {error}");
            return failure(StatusCode::INTERNAL_SERVER_ERROR, "database error");
        }
    };
    let result = core
        .query(
            "SELECT VALUE record_id FROM record_favorites \
             WHERE user_id = $user AND plugin = $plugin AND model = $model;",
        )
        .bind(("user", user))
        .bind(("plugin", query.plugin))
        .bind(("model", query.model))
        .await
        .and_then(|response| response.check());
    match result {
        Ok(mut response) => match response.take::<Vec<String>>(0) {
            Ok(ids) => Json(json!({ "ids": ids })).into_response(),
            Err(error) => {
                log::error!("record favorites: {error}");
                failure(StatusCode::INTERNAL_SERVER_ERROR, "database error")
            }
        },
        Err(error) => {
            log::error!("record favorites: {error}");
            failure(StatusCode::INTERNAL_SERVER_ERROR, "database error")
        }
    }
}

#[derive(Debug, Deserialize)]
struct SetBody {
    plugin: String,
    model: String,
    ids: Vec<String>,
    favorite: bool,
}

async fn set(
    State(state): State<AppState>,
    headers: HeaderMap,
    Json(body): Json<SetBody>,
) -> Response {
    let user = match require_user(&state, &headers).await {
        Ok(user) => user,
        Err(response) => return response,
    };
    let ids: Vec<String> = body
        .ids
        .into_iter()
        .map(|id| id.trim().to_string())
        .filter(|id| !id.is_empty())
        .collect();
    if body.plugin.trim().is_empty() || body.model.trim().is_empty() || ids.is_empty() {
        return failure(StatusCode::BAD_REQUEST, "name the plugin, the model and the records");
    }
    if ids.len() > MAX_IDS {
        return failure(StatusCode::BAD_REQUEST, "too many records at once");
    }
    let core = match state.core().await {
        Ok(core) => core,
        Err(error) => {
            log::error!("record favorites: {error}");
            return failure(StatusCode::INTERNAL_SERVER_ERROR, "database error");
        }
    };
    for id in &ids {
        // One row per person and record, so marking twice is harmless.
        let key = format!("{user}|{}|{}|{id}", body.plugin, body.model);
        let sql = if body.favorite {
            "UPSERT type::record('record_favorites', $key) SET user_id = $user, plugin = $plugin, \
             model = $model, record_id = $id, date_created = time::now();"
        } else {
            "DELETE type::record('record_favorites', $key);"
        };
        let result = core
            .query(sql)
            .bind(("key", key))
            .bind(("user", user.clone()))
            .bind(("plugin", body.plugin.clone()))
            .bind(("model", body.model.clone()))
            .bind(("id", id.clone()))
            .await
            .and_then(|response| response.check());
        if let Err(error) = result {
            log::error!("record favorites: {error}");
            return failure(StatusCode::INTERNAL_SERVER_ERROR, "database error");
        }
    }
    Json(json!({ "ok": true, "count": ids.len() })).into_response()
}
