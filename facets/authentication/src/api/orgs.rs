//! The organizations a logged-in user can enter, and switching between them.

use aether_core::access::organizations::{SelectionMode, overview, user_organizations};
use aether_core::session_cache::set_org_and_refresh;
use aether_core::state::AppState;
use aether_orm::LEFT_ORGANIZATION;
use axum::{
    Json,
    extract::State,
    http::{HeaderMap, StatusCode, Uri},
};
use axum_extra::extract::CookieJar;
use serde::Deserialize;
use serde_json::{Value as JsonValue, json};

use crate::authz::AuthSession;
use crate::csrf::{cookies_secure, rotate_csrf_cookie};
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

#[derive(Debug, Deserialize)]
pub struct SwitchOrg {
    /// The organization's database name, as listed by `GET /api/auth/orgs`.
    pub org: String,
}

/// `POST /api/auth/org`: remember the chosen organization in the session.
///
/// Where the address decides (`subdomain`, `path`) there is nothing to choose,
/// so this refuses. Otherwise the user must belong to the organization (a
/// developer may enter any).
pub async fn switch_org(
    State(state): State<AppState>,
    AuthSession(session): AuthSession,
    jar: CookieJar,
    Json(body): Json<SwitchOrg>,
) -> Result<(CookieJar, Json<JsonValue>), ApiError> {
    if SelectionMode::of(&state.config.tenancy.org_resolution) == SelectionMode::Address {
        return Err((
            StatusCode::CONFLICT,
            Json(
                json!({ "error": "the organization is chosen by the address, not by the session" }),
            ),
        ));
    }

    let organizations = user_organizations(&state, &session)
        .await
        .map_err(|error| database_error("organization list", error))?;
    let Some(chosen) = organizations.iter().find(|org| org.db_name == body.org) else {
        log::info!(
            "organization switch refused: `{}` is not one of the user's organizations",
            body.org
        );
        return Err((
            StatusCode::FORBIDDEN,
            Json(json!({ "error": "you are not a member of that organization" })),
        ));
    };

    let core = state
        .core()
        .await
        .map_err(|error| database_error("core selection", error))?;
    let token = read_session_token(&jar);
    set_org_and_refresh(
        &state.cache,
        &core,
        &session.session_id,
        &chosen.db_name,
        token.as_deref(),
    )
    .await
    .map_err(|error| database_error("saving the organization", error))?;
    log::info!("organization switched to `{}`", chosen.db_name);
    let jar = rotate_csrf_cookie(jar, cookies_secure(&state));
    Ok((
        jar,
        Json(json!({ "ok": true, "org": chosen.db_name, "name": chosen.name })),
    ))
}

/// `DELETE /api/auth/org`: leave the organization you are in, back to the Organizations
/// page. For developers only: everyone else belongs to their organizations and is always
/// in one. Where the address decides the organization there is nothing to leave.
pub async fn leave_org(
    State(state): State<AppState>,
    AuthSession(session): AuthSession,
    jar: CookieJar,
) -> Result<Json<JsonValue>, ApiError> {
    if !session.user.is_super_user {
        return Err((
            StatusCode::FORBIDDEN,
            Json(json!({ "error": "only developers can leave an organization" })),
        ));
    }
    if SelectionMode::of(&state.config.tenancy.org_resolution) == SelectionMode::Address {
        return Err((
            StatusCode::CONFLICT,
            Json(
                json!({ "error": "the organization is chosen by the address, not by the session" }),
            ),
        ));
    }
    let core = state
        .core()
        .await
        .map_err(|error| database_error("core selection", error))?;
    let token = read_session_token(&jar);
    set_org_and_refresh(
        &state.cache,
        &core,
        &session.session_id,
        LEFT_ORGANIZATION,
        token.as_deref(),
    )
    .await
    .map_err(|error| database_error("leaving the organization", error))?;
    log::info!("developer left their organization");
    Ok(Json(json!({ "ok": true })))
}
