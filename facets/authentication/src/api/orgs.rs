//! The organizations a logged-in user can enter, and switching between them.

use aether_core::access::organizations::{SelectionMode, overview, user_organizations};
use aether_core::state::AppState;
use aether_orm::set_session_org;
use axum::{
    Json,
    extract::State,
    http::{HeaderMap, StatusCode, Uri},
};
use serde::Deserialize;
use serde_json::{Value as JsonValue, json};

use crate::authz::AuthSession;

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
    headers: HeaderMap,
    uri: Uri,
) -> Result<Json<JsonValue>, ApiError> {
    let overview = overview(&state, &headers, &uri, &session)
        .await
        .map_err(|error| database_error("organization overview", error))?;
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
    Json(body): Json<SwitchOrg>,
) -> Result<Json<JsonValue>, ApiError> {
    if SelectionMode::of(&state.config.tenancy.org_resolution) == SelectionMode::Address {
        return Err((
            StatusCode::CONFLICT,
            Json(json!({ "error": "the organization is chosen by the address, not by the session" })),
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

    let core = state.core()
        .await
        .map_err(|error| database_error("core selection", error))?;
    set_session_org(&core, &session.session_id, &chosen.db_name)
        .await
        .map_err(|error| database_error("saving the organization", error))?;
    log::info!("organization switched to `{}`", chosen.db_name);
    Ok(Json(json!({ "ok": true, "org": chosen.db_name, "name": chosen.name })))
}
