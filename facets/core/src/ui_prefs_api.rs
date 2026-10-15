//! UI preferences any signed-in user may read (not the admin settings catalog).

use axum::{
    Json, Router,
    extract::State,
    http::{HeaderMap, StatusCode},
    response::{IntoResponse, Response},
    routing::get,
};
use serde_json::json;

use aether_orm::find_session_by_token;

use crate::access::identity::SESSION_COOKIE;
use crate::application::settings::{get_setting, get_setting_text};
use crate::state::AppState;
use crate::tenancy::resolve_org_slug;

pub const LIST_PAGE_SIZE_KEY: &str = "ui.list.page_size";
/// The application's public name (General settings): shown in the browser tab and the shell.
pub const APP_NAME_KEY: &str = "app_name";
/// Shown when no name is set.
pub const DEFAULT_APP_NAME: &str = "Aether";
pub const DEFAULT_LIST_PAGE_SIZE: u32 = 10;
pub const LIST_PAGE_SIZE_CHOICES: &[u32] = &[10, 25, 50, 100];

pub fn routes() -> Router<AppState> {
    Router::new()
        .route("/api/ui/prefs", get(prefs))
        .route("/api/ui/branding", get(branding))
}

fn failure(status: StatusCode, message: &str) -> Response {
    (status, Json(json!({ "error": message }))).into_response()
}

fn session_token(headers: &HeaderMap) -> Option<String> {
    let cookie = headers
        .get(axum::http::header::COOKIE)
        .and_then(|v| v.to_str().ok())
        .unwrap_or("");
    cookie.split(';').find_map(|part| {
        part.trim()
            .strip_prefix(&format!("{SESSION_COOKIE}="))
            .map(str::to_string)
            .filter(|s| !s.is_empty())
    })
}

/// What the application is called for this visitor: the organization's own name if it set one,
/// else the global one, else `Aether`. The organization is the signed-in person's, or else the
/// one the address names; the login page, before either, gets the global name. Public, because
/// the login page needs it too, and it is only the name.
async fn branding(State(state): State<AppState>, headers: HeaderMap) -> Response {
    let mut org: Option<crate::tenancy::OrgRef> = None;
    if let Some(token) = session_token(&headers)
        && let Ok(core) = state.core().await
        && let Ok(Some(session)) = find_session_by_token(&core, &token).await
        && session.user.is_active
        && let Ok(Some(db_name)) =
            crate::access::organizations::effective_org_database(&state, &session).await
    {
        org = Some(crate::tenancy::OrgRef {
            slug: db_name.clone(),
            db_name,
        });
    }
    if org.is_none() {
        let mut builder = axum::http::Request::builder().uri("/api/ui/branding");
        for (k, v) in headers.iter() {
            builder = builder.header(k, v);
        }
        if let Ok(req) = builder.body(()) {
            org = resolve_org_slug(&req, &state.config.tenancy, None);
        }
    }
    let name = match get_setting_text(&state, APP_NAME_KEY, org.as_ref()).await {
        Ok(name) => name,
        Err(err) => {
            log::warn!("branding get_setting: {err}");
            None
        }
    };
    let mut response = Json(json!({
        "app_name": name.unwrap_or_else(|| DEFAULT_APP_NAME.to_string()),
    }))
    .into_response();
    // It changes with the organization and the settings; never reuse a stale one.
    response.headers_mut().insert(
        axum::http::header::CACHE_CONTROL,
        axum::http::HeaderValue::from_static("no-store, private"),
    );
    response
}

async fn require_active_session(state: &AppState, headers: &HeaderMap) -> Result<(), Response> {
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
        log::error!("ui prefs auth: {err}");
        failure(StatusCode::INTERNAL_SERVER_ERROR, "database error")
    })?;
    match find_session_by_token(&core, &token).await {
        Ok(Some(session)) if session.user.is_active => Ok(()),
        Ok(Some(_)) => Err(failure(StatusCode::FORBIDDEN, "user inactive")),
        Ok(None) => Err(failure(StatusCode::UNAUTHORIZED, "invalid session")),
        Err(err) => {
            log::error!("ui prefs auth: {err}");
            Err(failure(StatusCode::INTERNAL_SERVER_ERROR, "database error"))
        }
    }
}

/// Normalize a setting value to an allowed page size (default 10).
pub fn normalize_list_page_size(value: Option<&serde_json::Value>) -> u32 {
    let n = match value {
        Some(serde_json::Value::Number(n)) => n.as_u64().unwrap_or(0) as u32,
        Some(serde_json::Value::String(s)) => s.trim().parse().unwrap_or(0),
        _ => 0,
    };
    if LIST_PAGE_SIZE_CHOICES.contains(&n) {
        n
    } else {
        DEFAULT_LIST_PAGE_SIZE
    }
}

async fn prefs(
    State(state): State<AppState>,
    headers: HeaderMap,
) -> Result<Json<serde_json::Value>, Response> {
    require_active_session(&state, &headers).await?;
    let mut builder = axum::http::Request::builder().uri("/api/ui/prefs");
    for (k, v) in headers.iter() {
        builder = builder.header(k, v);
    }
    let req = builder
        .body(())
        .map_err(|_| failure(StatusCode::INTERNAL_SERVER_ERROR, "request error"))?;
    let org = resolve_org_slug(&req, &state.config.tenancy, None);
    let raw = match get_setting(&state, LIST_PAGE_SIZE_KEY, org.as_ref()).await {
        Ok(Some(setting)) => Some(setting.value),
        Ok(None) => None,
        Err(err) => {
            log::error!("ui prefs get_setting: {err}");
            None
        }
    };
    let list_page_size = normalize_list_page_size(raw.as_ref());
    Ok(Json(json!({
        "list_page_size": list_page_size,
        "list_page_size_choices": LIST_PAGE_SIZE_CHOICES,
    })))
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn normalize_accepts_allowed_sizes() {
        assert_eq!(normalize_list_page_size(Some(&json!(25))), 25);
        assert_eq!(normalize_list_page_size(Some(&json!("50"))), 50);
    }

    #[test]
    fn normalize_falls_back_to_ten() {
        assert_eq!(normalize_list_page_size(None), 10);
        assert_eq!(normalize_list_page_size(Some(&json!(7))), 10);
        assert_eq!(normalize_list_page_size(Some(&json!("nope"))), 10);
    }
}
