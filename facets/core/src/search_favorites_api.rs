//! HTTP API for per-user faceted search favorites.

use axum::{
    Json, Router,
    extract::{Path, Query, State},
    http::{HeaderMap, StatusCode},
    response::{IntoResponse, Response},
    routing::{delete, get},
};
use serde::Deserialize;
use serde_json::{Map, Value as JsonValue, json};

use aether_orm::find_session_by_token;

use crate::access::identity::SESSION_COOKIE;
use crate::search_favorites::{self, FavoriteError};
use crate::state::AppState;

pub fn routes() -> Router<AppState> {
    Router::new()
        .route("/api/ui/search/favorites", get(list).post(create))
        .route("/api/ui/search/favorites/{id}", delete(remove))
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
        let part = part.trim();
        part.strip_prefix(&format!("{SESSION_COOKIE}="))
            .map(str::to_string)
            .filter(|s| !s.is_empty())
    });
    let Some(token) = token else {
        return Err(failure(StatusCode::UNAUTHORIZED, "not authenticated"));
    };
    let core = state.core().await.map_err(|err| {
        log::error!("search favorites auth: {err}");
        failure(StatusCode::INTERNAL_SERVER_ERROR, "database error")
    })?;
    match find_session_by_token(&core, &token).await {
        Ok(Some(session)) if session.user.is_active => {
            use surrealdb::types::ToSql;
            Ok(format!(
                "{}:{}",
                session.user.id.table.as_str(),
                session.user.id.key.to_sql()
            ))
        }
        Ok(Some(_)) => Err(failure(StatusCode::FORBIDDEN, "user inactive")),
        Ok(None) => Err(failure(StatusCode::UNAUTHORIZED, "invalid session")),
        Err(err) => {
            log::error!("search favorites auth: {err}");
            Err(failure(StatusCode::INTERNAL_SERVER_ERROR, "database error"))
        }
    }
}

fn map_error(error: FavoriteError) -> Response {
    match error {
        FavoriteError::NotFound => failure(StatusCode::NOT_FOUND, "favorite not found"),
        FavoriteError::Invalid(message) => failure(StatusCode::BAD_REQUEST, &message),
        FavoriteError::Database(message) => {
            log::error!("search favorites: {message}");
            failure(StatusCode::INTERNAL_SERVER_ERROR, "database error")
        }
    }
}

#[derive(Debug, Deserialize)]
struct ListQuery {
    model: String,
}

async fn list(
    State(state): State<AppState>,
    headers: HeaderMap,
    Query(query): Query<ListQuery>,
) -> Response {
    let user_id = match require_user(&state, &headers).await {
        Ok(id) => id,
        Err(response) => return response,
    };
    match search_favorites::list_for_user(&state, &user_id, &query.model).await {
        Ok(favorites) => Json(json!({ "favorites": favorites })).into_response(),
        Err(error) => map_error(error),
    }
}

#[derive(Debug, Deserialize)]
struct CreateBody {
    model: String,
    label: String,
    #[serde(default)]
    filter: Map<String, JsonValue>,
    #[serde(default)]
    q: Option<String>,
    #[serde(default)]
    group_by: Vec<String>,
    #[serde(default)]
    is_default: bool,
}

async fn create(
    State(state): State<AppState>,
    headers: HeaderMap,
    Json(body): Json<CreateBody>,
) -> Response {
    let user_id = match require_user(&state, &headers).await {
        Ok(id) => id,
        Err(response) => return response,
    };
    match search_favorites::create(
        &state,
        &user_id,
        &body.model,
        &body.label,
        body.filter,
        body.q,
        body.group_by,
        body.is_default,
    )
    .await
    {
        Ok(favorite) => Json(json!({ "favorite": favorite })).into_response(),
        Err(error) => map_error(error),
    }
}

async fn remove(
    State(state): State<AppState>,
    headers: HeaderMap,
    Path(id): Path<String>,
) -> Response {
    let user_id = match require_user(&state, &headers).await {
        Ok(id) => id,
        Err(response) => return response,
    };
    match search_favorites::delete_for_user(&state, &user_id, &id).await {
        Ok(()) => Json(json!({ "ok": true })).into_response(),
        Err(error) => map_error(error),
    }
}
