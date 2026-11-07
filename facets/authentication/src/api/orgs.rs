//! The organization a logged-in user is in.

use aether_core::access::organizations::overview;
use aether_core::session_cache::set_org_and_refresh;
use aether_core::state::AppState;
use aether_orm::LEFT_ORGANIZATION;
use axum::{
    Json,
    extract::State,
    http::{HeaderMap, StatusCode, Uri},
};
use axum_extra::extract::CookieJar;
use serde_json::{Value as JsonValue, json};

use crate::authz::AuthSession;
use crate::session_cookie::read_session_token;

type ApiError = (StatusCode, Json<JsonValue>);

fn database_error(context: &str, error: impl std::fmt::Display) -> ApiError {
    log::error!("{context}: {error}");
    (
        StatusCode::INTERNAL_SERVER_ERROR,
        Json(json!({ "error": "database error" })),
    )
}

/// `GET /api/auth/orgs`: the user's organizations, how this deployment chooses
/// between them, which one the request is already for, and whether the user
/// must pick before anything else can load.
pub async fn list_orgs(
    State(state): State<AppState>,
    AuthSession(session): AuthSession,
    jar: CookieJar,
    headers: HeaderMap,
    uri: Uri,
) -> Result<Json<JsonValue>, ApiError> {
    let overview = overview(&state, &headers, &uri, &session)
        .await
        .map_err(|error| database_error("organization overview", error))?;
    // When overview inferred the sole membership but the session never stored it
    // (common after login before this was fixed), persist so later requests agree.
    let session_blank = session
        .org_database_id
        .as_deref()
        .map(str::trim)
        .filter(|v| !v.is_empty())
        .filter(|v| *v != LEFT_ORGANIZATION)
        .is_none();
    if session_blank && let Some(current) = overview.current.as_deref() {
        let core = state
            .core()
            .await
            .map_err(|error| database_error("core selection", error))?;
        let token = read_session_token(&jar);
        if let Err(error) = set_org_and_refresh(
            &state.cache,
            &core,
            &session.session_id,
            current,
            token.as_deref(),
        )
        .await
        {
            log::warn!("orgs: persist inferred organization `{current}`: {error}");
        }
    }
    Ok(Json(json!(overview)))
}
