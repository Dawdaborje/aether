//! `GET /api/ui/viewers`: the document viewers that the enabled plugins of the caller's organization add
//! (`[[viewer]]` in `plugin.toml`, see `PluginViewerDef`).
//!
//! The web app registers each one next to its built-in viewers. The module itself is fetched from the
//! plugin's assets (`module_url`) and run in a sandboxed frame; nothing here runs plugin code.

use std::net::SocketAddr;

use axum::extract::Extension;
use axum::{
    Json, Router,
    extract::{ConnectInfo, State},
    http::{HeaderMap, StatusCode, Uri},
    response::{IntoResponse, Response},
    routing::get,
};
use serde_json::{Value, json};

use aether_core::access::audit::Actor;
use aether_core::access::identity::{IdentityError, identify, with_retry_after};
use aether_core::plugin_manager::installed;
use aether_core::state::AppState;

pub fn routes() -> Router<AppState> {
    Router::new().route("/api/ui/viewers", get(list))
}

fn failure(status: StatusCode, message: &str) -> Response {
    (status, Json(json!({ "error": message }))).into_response()
}

async fn list(
    State(state): State<AppState>,
    peer: Option<Extension<ConnectInfo<SocketAddr>>>,
    headers: HeaderMap,
    uri: Uri,
) -> Result<Json<Value>, Response> {
    let peer_ip = peer.map(|Extension(ConnectInfo(address))| address.ip());
    let identity = match identify(&state, &headers, peer_ip, &uri).await {
        Ok(identity) => identity,
        Err(error) => {
            if matches!(error, IdentityError::Database(_) | IdentityError::Audit(_)) {
                log::error!("viewers api identity: {error}");
            }
            let (status, message) = error.http();
            return Err(with_retry_after(failure(status, message)));
        }
    };
    if !matches!(identity.actor, Actor::User(_)) {
        return Err(failure(StatusCode::UNAUTHORIZED, "not authenticated"));
    }
    let internal = |what: &str, error: &dyn std::fmt::Display| {
        log::error!("viewers api {what}: {error}");
        failure(StatusCode::INTERNAL_SERVER_ERROR, "database error")
    };
    let org = state
        .org(&identity.org_db)
        .await
        .map_err(|e| internal("org", &e))?;
    let core = state.core().await.map_err(|e| internal("core", &e))?;
    let rows = installed::list_cached(&state.cache, &identity.org_db, &org)
        .await
        .map_err(|e| internal("installed", &e))?;
    // An administrator can switch plugin viewers off for the organization (default: on).
    let allowed = aether_core::application::settings::get_setting(
        &state,
        "viewers.plugin_enabled",
        Some(&aether_core::chatter::org_ref(&identity.org_db)),
    )
    .await
    .ok()
    .flatten()
    .map(|setting| setting.value != Value::Bool(false))
    .unwrap_or(true);
    if !allowed {
        return Ok(Json(json!({ "viewers": [] })));
    }
    let mut viewers = Vec::new();
    for row in rows.into_iter().filter(|row| row.is_enabled) {
        // A plugin that fails to load has no viewers; it must not break the list for the others.
        let Ok(loaded) = state
            .plugin_runtime
            .ensure_loaded(&core, &row.plugin_name, &row.version)
            .await
        else {
            continue;
        };
        for def in &loaded.manifest.viewer {
            viewers.push(json!({
                "plugin": row.plugin_name,
                "name": def.name,
                "label": def.label.clone().unwrap_or_else(|| def.name.clone()),
                "mimes": def.mimes,
                "extensions": def.extensions,
                "priority": def.priority,
                "hosts": def.hosts,
                "module_url": format!("/api/ui/plugins/{}/assets/{}", row.plugin_name, def.module),
            }));
        }
    }
    Ok(Json(json!({ "viewers": viewers })))
}
