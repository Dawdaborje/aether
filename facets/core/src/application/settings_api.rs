use axum::{
    Json, Router,
    extract::{Path, Query, State},
    http::{HeaderMap, Request, StatusCode},
    routing::get,
};
use aether_orm::find_session_by_token;
use serde::Deserialize;
use serde_json::{Value as JsonValue, json};

use crate::application::settings::{
    SettingsError, SettingValue, get_effective_settings, get_setting, list_catalog, set_setting,
};
use crate::state::AppState;
use crate::tenancy::{OrgRef, resolve_org_slug};

#[derive(Debug, Deserialize)]
pub struct KeysQuery {
    /// Comma-separated keys: `?keys=auth.primary_method,auth.enabled_methods`
    pub keys: Option<String>,
}

#[derive(Debug, Deserialize)]
pub struct UpdateBody {
    pub value: JsonValue,
}

fn org_from_request(state: &AppState, headers: &HeaderMap, path: &str) -> Option<OrgRef> {
    let mut builder = axum::http::Request::builder().uri(path);
    for (k, v) in headers.iter() {
        builder = builder.header(k, v);
    }
    let req = builder.body(()).ok()?;
    resolve_org_slug(&req, &state.config.tenancy, None)
}

async fn require_session(
    state: &AppState,
    headers: &HeaderMap,
) -> Result<(), (StatusCode, Json<JsonValue>)> {
    let cookie = headers
        .get(axum::http::header::COOKIE)
        .and_then(|v| v.to_str().ok())
        .unwrap_or("");
    let token = cookie.split(';').find_map(|part| {
        let part = part.trim();
        part.strip_prefix("aether_session=")
            .map(str::to_string)
            .filter(|s| !s.is_empty())
    });
    let Some(token) = token else {
        return Err((
            StatusCode::UNAUTHORIZED,
            Json(json!({ "error": "not authenticated" })),
        ));
    };
    let _ = state.use_core().await;
    match find_session_by_token(&state.db, &token).await {
        Ok(Some(session)) if session.user.is_active => Ok(()),
        Ok(_) => Err((
            StatusCode::UNAUTHORIZED,
            Json(json!({ "error": "invalid session" })),
        )),
        Err(err) => {
            log::error!("settings auth: {err}");
            Err((
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(json!({ "error": "database error" })),
            ))
        }
    }
}

async fn catalog(
    State(state): State<AppState>,
    headers: HeaderMap,
) -> Result<Json<JsonValue>, (StatusCode, Json<JsonValue>)> {
    let org = org_from_request(&state, &headers, "/api/settings/catalog");
    match list_catalog(&state, org.as_ref()).await {
        Ok(groups) => Ok(Json(json!({
            "groups": groups,
            "org": org.map(|o| o.slug),
        }))),
        Err(err) => {
            log::error!("settings catalog failed: {err}");
            Err((
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(json!({ "error": "failed to load settings catalog" })),
            ))
        }
    }
}

async fn get_one(
    State(state): State<AppState>,
    Path(key): Path<String>,
    req: Request<axum::body::Body>,
) -> Result<Json<SettingValue>, (StatusCode, Json<JsonValue>)> {
    let org = resolve_org_slug(&req, &state.config.tenancy, None);
    match get_setting(&state, &key, org.as_ref()).await {
        Ok(Some(value)) => Ok(Json(value)),
        Ok(None) => Err((
            StatusCode::NOT_FOUND,
            Json(json!({ "error": format!("setting `{key}` not found") })),
        )),
        Err(err) => {
            log::error!("settings get failed: {err}");
            Err((
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(json!({ "error": "failed to read setting" })),
            ))
        }
    }
}

async fn get_many(
    State(state): State<AppState>,
    Query(query): Query<KeysQuery>,
    req: Request<axum::body::Body>,
) -> Result<Json<Vec<SettingValue>>, (StatusCode, Json<JsonValue>)> {
    let org = resolve_org_slug(&req, &state.config.tenancy, None);
    let Some(keys_raw) = query.keys.filter(|s| !s.is_empty()) else {
        return Err((
            StatusCode::BAD_REQUEST,
            Json(json!({ "error": "query param `keys` is required" })),
        ));
    };
    let keys: Vec<&str> = keys_raw
        .split(',')
        .map(str::trim)
        .filter(|s| !s.is_empty())
        .collect();
    match get_effective_settings(&state, &keys, org.as_ref()).await {
        Ok(values) => Ok(Json(values)),
        Err(err) => {
            log::error!("settings batch get failed: {err}");
            Err((
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(json!({ "error": "failed to read settings" })),
            ))
        }
    }
}

async fn put_one(
    State(state): State<AppState>,
    Path(key): Path<String>,
    headers: HeaderMap,
    Json(body): Json<UpdateBody>,
) -> Result<Json<SettingValue>, (StatusCode, Json<JsonValue>)> {
    require_session(&state, &headers).await?;
    let org = org_from_request(&state, &headers, &format!("/api/settings/{key}"));
    match set_setting(&state, &key, body.value, org.as_ref()).await {
        Ok(value) => Ok(Json(value)),
        Err(SettingsError::NotFound(k)) => Err((
            StatusCode::NOT_FOUND,
            Json(json!({ "error": format!("setting `{k}` not found") })),
        )),
        Err(err) => {
            log::error!("settings put failed: {err}");
            Err((
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(json!({ "error": "failed to update setting" })),
            ))
        }
    }
}

pub fn routes() -> Router<AppState> {
    Router::new()
        .route("/", get(get_many))
        .route("/catalog", get(catalog))
        .route("/{*key}", get(get_one).put(put_one))
}
