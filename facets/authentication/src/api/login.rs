use aether_core::access::organizations::membership_db_names;
use aether_core::access::visitor::record_id_string;
use aether_core::application::settings::get_setting;
use aether_core::state::AppState;
use aether_core::tenancy::{OrgRef, resolve_org_slug};
use aether_orm::{authenticate_local, create_session};
use axum::{
    Json,
    extract::State,
    http::{HeaderMap, StatusCode},
};
use axum_extra::extract::CookieJar;
use serde::Deserialize;
use serde_json::{Value as JsonValue, json};

use crate::csrf::{cookies_secure, rotate_csrf_cookie};
use crate::session_cookie::set_session_cookie;

#[derive(Debug, Deserialize)]
pub struct LoginRequest {
    pub username: String,
    pub password: String,
}

pub async fn login(
    State(state): State<AppState>,
    jar: CookieJar,
    headers: HeaderMap,
    Json(body): Json<LoginRequest>,
) -> Result<(CookieJar, Json<JsonValue>), (StatusCode, Json<JsonValue>)> {
    // Build a minimal request-like view for org resolution via headers only.
    let org = org_from_headers(&state.config, &headers, None);

    // Ensure local method is enabled.
    if let Ok(Some(methods)) = get_setting(&state, "auth.enabled_methods", org.as_ref()).await {
        let enabled = methods_as_strings(&methods.value);
        if !enabled.iter().any(|m| m == "local") {
            return Err((
                StatusCode::FORBIDDEN,
                Json(json!({ "error": "local authentication is disabled" })),
            ));
        }
    }

    let core = state.core().await.map_err(|err| {
        log::error!("login core database: {err}");
        (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(json!({ "error": "database error" })),
        )
    })?;

    // A name that has collected too many failed passwords is refused, whoever asks, until the
    // minute is over; only failures count, so successes never lock anyone out.
    if state.limits.failed_login_limiter.blocked(&body.username) {
        log::warn!("login refused: too many failed attempts for one username");
        return Err((
            StatusCode::TOO_MANY_REQUESTS,
            Json(json!({ "error": "too many attempts, try again in a minute" })),
        ));
    }

    let user = match authenticate_local(&core, &body.username, &body.password).await {
        Ok(u) => u,
        Err(aether_orm::UserServiceError::InvalidCredentials) => {
            state.limits.failed_login_limiter.record(&body.username);
            // The typed name is not logged: people sometimes type their password into that box.
            log::warn!("login failed: invalid credentials");
            return Err((
                StatusCode::UNAUTHORIZED,
                Json(json!({ "error": "invalid credentials" })),
            ));
        }
        Err(aether_orm::UserServiceError::InactiveUser) => {
            return Err((
                StatusCode::FORBIDDEN,
                Json(json!({ "error": "user inactive" })),
            ));
        }
        Err(err) => {
            log::error!("login failed: {err}");
            return Err((
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(json!({ "error": "login failed" })),
            ));
        }
    };

    let mut org_db = org.as_ref().map(|o| o.db_name.clone());
    // Session-only tenancy: bind the sole membership so /me capabilities match Apps.
    if org_db.is_none() {
        match membership_db_names(&state, &record_id_string(&user.id)).await {
            Ok(memberships) => {
                if let [only] = memberships.as_slice() {
                    org_db = Some(only.clone());
                }
            }
            Err(error) => log::warn!("login memberships for `{}`: {error}", body.username),
        }
    }
    let session = create_session(&core, user.id.clone(), "local", org_db, aether_orm::SESSION_IDLE_HOURS)
        .await
        .map_err(|err| {
            log::error!("create session: {err}");
            (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(json!({ "error": "failed to create session" })),
            )
        })?;
    aether_core::session_cache::warm_created(&state.cache, &session, &user);

    log::info!(
        "login ok: `{}` (organization: {})",
        body.username,
        org_db_for_log(&session.record.org_database_id)
    );
    let secure = cookies_secure(&state);
    let jar = set_session_cookie(jar, &session.raw_token, secure);
    let jar = rotate_csrf_cookie(jar, secure);

    Ok((
        jar,
        Json(json!({
            "ok": true,
            "user": {
                "id": record_id_string(&user.id),
                "username": user.username,
                "email": user.email,
                "display_name": user.display_name,
                "is_super_user": user.is_super_user,
                "is_developer": user.is_super_user,
            }
        })),
    ))
}

fn org_db_for_log(org: &Option<String>) -> &str {
    org.as_deref().unwrap_or("none yet")
}

pub(crate) fn methods_as_strings(value: &JsonValue) -> Vec<String> {
    match value {
        JsonValue::Array(items) => items
            .iter()
            .filter_map(|v| v.as_str().map(str::to_string))
            .collect(),
        JsonValue::String(s) => vec![s.clone()],
        _ => vec!["local".into()],
    }
}

pub(crate) fn org_from_headers(
    config: &aether_core::config_manager::models::AetherConfig,
    headers: &HeaderMap,
    session_org: Option<&OrgRef>,
) -> Option<OrgRef> {
    // Reconstruct a Request solely for tenancy helpers.
    let mut builder = axum::http::Request::builder();
    for (k, v) in headers.iter() {
        builder = builder.header(k, v);
    }
    let req = builder.body(()).ok()?;
    resolve_org_slug(&req, &config.tenancy, session_org)
}
