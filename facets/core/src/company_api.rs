//! `GET` / `PUT /api/ui/company` — current working company for the shell.

use std::net::SocketAddr;

use axum::extract::Extension;
use axum::{
    Json, Router,
    extract::{ConnectInfo, State},
    http::{HeaderMap, StatusCode, Uri},
    response::{IntoResponse, Response},
    routing::get,
};
use serde::Deserialize;
use serde_json::json;

use crate::access::audit::Actor;
use crate::access::identity::{IdentityError, identify, with_retry_after};
use crate::company_context::{self, CompanyContextError};
use crate::plugin_manager::installed;
use crate::state::AppState;

pub fn routes() -> Router<AppState> {
    Router::new().route("/api/ui/company", get(get_company).put(put_company))
}

fn failure(status: StatusCode, message: &str) -> Response {
    (status, Json(json!({ "error": message }))).into_response()
}

fn map_error(error: CompanyContextError) -> Response {
    match error {
        CompanyContextError::Unavailable => {
            failure(StatusCode::NOT_FOUND, "company plugin is not available")
        }
        CompanyContextError::NotFound => failure(StatusCode::NOT_FOUND, "company not found"),
        CompanyContextError::Archived => failure(StatusCode::BAD_REQUEST, "company is archived"),
        CompanyContextError::Database(message) => {
            log::error!("company context: {message}");
            failure(StatusCode::INTERNAL_SERVER_ERROR, "database error")
        }
    }
}

struct ActorCtx {
    user_id: String,
    org_db: String,
}

async fn require_org_user(
    state: &AppState,
    headers: &HeaderMap,
    peer_ip: Option<std::net::IpAddr>,
    uri: &Uri,
) -> Result<ActorCtx, Response> {
    let identity = match identify(state, headers, peer_ip, uri).await {
        Ok(identity) => identity,
        Err(error) => {
            if matches!(error, IdentityError::Database(_) | IdentityError::Audit(_)) {
                log::error!("company api identity: {error}");
            }
            let (status, message) = error.http();
            return Err(with_retry_after(failure(status, message)));
        }
    };
    let Actor::User(user_id) = &identity.actor else {
        return Err(failure(
            if identity.foreign_user {
                StatusCode::FORBIDDEN
            } else {
                StatusCode::UNAUTHORIZED
            },
            if identity.foreign_user {
                "not a member of this organization"
            } else {
                "not authenticated"
            },
        ));
    };
    Ok(ActorCtx {
        user_id: user_id.clone(),
        org_db: identity.org_db,
    })
}

async fn company_plugin_installed(state: &AppState, org_db: &str) -> bool {
    let Ok(org) = state.org(org_db).await else {
        return false;
    };
    match installed::list_cached(&state.cache, org_db, &org).await {
        Ok(rows) => rows
            .iter()
            .any(|row| row.plugin_name == "company" && row.is_enabled),
        Err(_) => false,
    }
}

async fn get_company(
    State(state): State<AppState>,
    peer: Option<Extension<ConnectInfo<SocketAddr>>>,
    headers: HeaderMap,
    uri: Uri,
) -> Result<Json<serde_json::Value>, Response> {
    let peer_ip = peer.map(|Extension(ConnectInfo(address))| address.ip());
    let actor = require_org_user(&state, &headers, peer_ip, &uri).await?;
    if !company_plugin_installed(&state, &actor.org_db).await {
        return Ok(Json(json!({
            "available": false,
            "current": null,
            "companies": [],
        })));
    }
    let org = state.org(&actor.org_db).await.map_err(|err| {
        log::error!("company api org: {err}");
        failure(StatusCode::INTERNAL_SERVER_ERROR, "database error")
    })?;
    company_context::overview(&org, &actor.user_id)
        .await
        .map(Json)
        .map_err(map_error)
}

#[derive(Debug, Deserialize)]
struct PutBody {
    id: String,
}

async fn put_company(
    State(state): State<AppState>,
    peer: Option<Extension<ConnectInfo<SocketAddr>>>,
    headers: HeaderMap,
    uri: Uri,
    Json(body): Json<PutBody>,
) -> Result<Json<serde_json::Value>, Response> {
    let peer_ip = peer.map(|Extension(ConnectInfo(address))| address.ip());
    let actor = require_org_user(&state, &headers, peer_ip, &uri).await?;
    if !company_plugin_installed(&state, &actor.org_db).await {
        return Err(map_error(CompanyContextError::Unavailable));
    }
    let id = body.id.trim();
    if id.is_empty() {
        return Err(failure(StatusCode::BAD_REQUEST, "company id is required"));
    }
    let org = state.org(&actor.org_db).await.map_err(|err| {
        log::error!("company api org: {err}");
        failure(StatusCode::INTERNAL_SERVER_ERROR, "database error")
    })?;
    let current = company_context::set_current(&org, &actor.user_id, id)
        .await
        .map_err(map_error)?;
    let companies = company_context::list_active(&org)
        .await
        .map_err(map_error)?;
    Ok(Json(json!({
        "available": true,
        "current": current,
        "companies": companies,
    })))
}
