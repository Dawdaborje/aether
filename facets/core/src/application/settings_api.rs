use aether_orm::find_session_by_token;
use axum::{
    Json, Router,
    extract::{Path, Query, State},
    http::{HeaderMap, Request, StatusCode},
    routing::get,
};
use serde::Deserialize;
use serde_json::{Value as JsonValue, json};
use surrealdb::types::ToSql;

use crate::access::identity::SESSION_COOKIE;
use crate::access::organizations::membership_db_names;
use crate::application::settings::{
    SettingValue, SettingsError, get_effective_settings, get_setting, list_catalog, set_setting,
};
use crate::org_admin::can_manage_admin_role;
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

/// Session `org_database_id` is already a Surreal DB name (`hrms_org`), not a public slug.
fn org_ref_from_db(db_name: &str) -> OrgRef {
    OrgRef {
        slug: db_name.to_string(),
        db_name: db_name.to_string(),
    }
}

/// Developer-only gate (also used by the organizations admin API).
pub(crate) async fn require_session(
    state: &AppState,
    headers: &HeaderMap,
) -> Result<(), (StatusCode, Json<JsonValue>)> {
    match load_session(state, headers).await? {
        SessionActor {
            is_developer: true, ..
        } => Ok(()),
        SessionActor {
            is_active: true, ..
        } => Err((
            StatusCode::FORBIDDEN,
            Json(json!({ "error": "developer access required" })),
        )),
        _ => Err((
            StatusCode::UNAUTHORIZED,
            Json(json!({ "error": "invalid session" })),
        )),
    }
}

/// A developer for the organization `db_name`: an unrestricted developer, or a member that an
/// unrestricted developer made a developer of that organization (see `org_admin::is_org_developer`).
pub(crate) async fn require_org_developer(
    state: &AppState,
    headers: &HeaderMap,
    db_name: &str,
) -> Result<(), (StatusCode, Json<JsonValue>)> {
    let actor = load_session(state, headers).await?;
    if !actor.is_active {
        return Err((
            StatusCode::UNAUTHORIZED,
            Json(json!({ "error": "invalid session" })),
        ));
    }
    if actor.is_developer {
        return Ok(());
    }
    let session = state.fresh_session();
    match crate::org_admin::is_org_developer(
        &session,
        &state.namespace,
        db_name,
        &actor.user_id,
        false,
    )
    .await
    {
        Ok(true) => Ok(()),
        Ok(false) => Err((
            StatusCode::FORBIDDEN,
            Json(json!({ "error": "developer access to this organization required" })),
        )),
        Err(err) => {
            log::error!("org developer check `{db_name}`: {err}");
            Err((
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(json!({ "error": "database error" })),
            ))
        }
    }
}

struct SessionActor {
    user_id: String,
    is_developer: bool,
    is_active: bool,
    org_database_id: Option<String>,
}

async fn load_session(
    state: &AppState,
    headers: &HeaderMap,
) -> Result<SessionActor, (StatusCode, Json<JsonValue>)> {
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
        return Err((
            StatusCode::UNAUTHORIZED,
            Json(json!({ "error": "not authenticated" })),
        ));
    };
    let core = state.core().await.map_err(|err| {
        log::error!("settings auth core database: {err}");
        (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(json!({ "error": "database error" })),
        )
    })?;
    match find_session_by_token(&core, &token).await {
        Ok(Some(session)) if session.user.is_active => Ok(SessionActor {
            user_id: format!(
                "{}:{}",
                session.user.id.table.as_str(),
                session.user.id.key.to_sql()
            ),
            is_developer: session.user.is_super_user,
            is_active: true,
            org_database_id: session.org_database_id,
        }),
        Ok(Some(_)) => Ok(SessionActor {
            user_id: String::new(),
            is_developer: false,
            is_active: false,
            org_database_id: None,
        }),
        Ok(None) => Err((
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

/// Tenancy from the request, else the session org, else the caller's sole membership.
async fn resolve_settings_org(
    state: &AppState,
    headers: &HeaderMap,
    path: &str,
    actor: &SessionActor,
) -> Option<OrgRef> {
    let session_org = actor
        .org_database_id
        .as_deref()
        .map(str::trim)
        .filter(|v| !v.is_empty())
        .filter(|v| *v != aether_orm::LEFT_ORGANIZATION)
        .map(org_ref_from_db);

    let mut builder = axum::http::Request::builder().uri(path);
    for (k, v) in headers.iter() {
        builder = builder.header(k, v);
    }
    if let Ok(req) = builder.body(()) {
        if let Some(org) = resolve_org_slug(&req, &state.config.tenancy, session_org.as_ref()) {
            return Some(org);
        }
    } else if let Some(org) = session_org {
        return Some(org);
    }

    // session_only with an empty session org: same sole-membership rule as Apps /me.
    if actor.is_developer {
        return None;
    }
    match membership_db_names(state, &actor.user_id).await {
        Ok(memberships) => match memberships.as_slice() {
            [only] => Some(org_ref_from_db(only)),
            _ => None,
        },
        Err(error) => {
            log::warn!("settings org memberships for `{}`: {error}", actor.user_id);
            None
        }
    }
}

/// Developers always may. Org owner / `org_admin` may when an organization is selected
/// (their writes become organization overrides, never global defaults).
async fn require_settings_access(
    state: &AppState,
    actor: &SessionActor,
    org: Option<&OrgRef>,
) -> Result<(), (StatusCode, Json<JsonValue>)> {
    if !actor.is_active {
        return Err((
            StatusCode::UNAUTHORIZED,
            Json(json!({ "error": "invalid session" })),
        ));
    }
    if actor.is_developer {
        return Ok(());
    }
    let Some(org) = org else {
        return Err((
            StatusCode::FORBIDDEN,
            Json(json!({ "error": "organization required" })),
        ));
    };
    let core = state.fresh_session();
    match can_manage_admin_role(&core, &state.namespace, &org.db_name, &actor.user_id, false).await
    {
        Ok(true) => Ok(()),
        Ok(false) => Err((
            StatusCode::FORBIDDEN,
            Json(json!({ "error": "organization admin access required" })),
        )),
        Err(err) => {
            log::error!("settings auth can_manage_admin_role: {err}");
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
    let actor = load_session(&state, &headers).await?;
    let org = resolve_settings_org(&state, &headers, "/api/settings/catalog", &actor).await;
    require_settings_access(&state, &actor, org.as_ref()).await?;
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
    let headers = req.headers().clone();
    let actor = load_session(&state, &headers).await?;
    let org = resolve_settings_org(&state, &headers, &format!("/api/settings/{key}"), &actor).await;
    require_settings_access(&state, &actor, org.as_ref()).await?;
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
    let headers = req.headers().clone();
    let actor = load_session(&state, &headers).await?;
    let org = resolve_settings_org(&state, &headers, "/api/settings", &actor).await;
    require_settings_access(&state, &actor, org.as_ref()).await?;
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
    let actor = load_session(&state, &headers).await?;
    let org = resolve_settings_org(&state, &headers, &format!("/api/settings/{key}"), &actor).await;
    // Developers may write global defaults (no org) or org overrides.
    // Org owner / org_admin may only write with an organization selected.
    require_settings_access(&state, &actor, org.as_ref()).await?;
    match set_setting(&state, &key, body.value, org.as_ref()).await {
        Ok(value) => Ok(Json(value)),
        Err(SettingsError::NotFound(k)) => Err((
            StatusCode::NOT_FOUND,
            Json(json!({ "error": format!("setting `{k}` not found") })),
        )),
        Err(SettingsError::Invalid(message)) => {
            Err((StatusCode::BAD_REQUEST, Json(json!({ "error": message }))))
        }
        Err(SettingsError::Secrets(err)) => {
            log::error!("settings put: {err}");
            Err((
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(
                    json!({ "error": "secret settings cannot be saved: the encryption key is unavailable" }),
                ),
            ))
        }
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
