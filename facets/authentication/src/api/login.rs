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
    let org = org_from_headers(&state, &headers, None);

    // Ensure local method is enabled.
    if let Ok(Some(methods)) =
        get_setting(&state, "auth.enabled_methods", org.as_ref()).await
    {
        let enabled = methods_as_strings(&methods.value);
        if !enabled.iter().any(|m| m == "local") {
            return Err((
                StatusCode::FORBIDDEN,
                Json(json!({ "error": "local authentication is disabled" })),
            ));
        }
    }

    if let Err(err) = state.use_core().await {
        log::error!("login use_core: {err}");
        return Err((
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(json!({ "error": "database error" })),
        ));
    }

    let user = match authenticate_local(&state.db, &body.username, &body.password).await {
        Ok(u) => u,
        Err(aether_orm::UserServiceError::InvalidCredentials) => {
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

    let org_db = org.as_ref().map(|o| o.db_name.clone());
    let session = create_session(&state.db, user.id.clone(), "local", org_db, 24)
        .await
        .map_err(|err| {
            log::error!("create session: {err}");
            (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(json!({ "error": "failed to create session" })),
            )
        })?;

    let jar = set_session_cookie(jar, &session.raw_token);

    Ok((
        jar,
        Json(json!({
            "ok": true,
            "user": {
                "id": format!("{:?}", user.id),
                "username": user.username,
                "email": user.email,
                "display_name": user.display_name,
                "is_super_user": user.is_super_user,
            }
        })),
    ))
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
    state: &AppState,
    headers: &HeaderMap,
    session_org: Option<&OrgRef>,
) -> Option<OrgRef> {
    // Reconstruct a Request solely for tenancy helpers.
    let mut builder = axum::http::Request::builder();
    for (k, v) in headers.iter() {
        builder = builder.header(k, v);
    }
    let req = builder.body(()).ok()?;
    resolve_org_slug(&req, &state.config.tenancy, session_org)
}
