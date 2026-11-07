//! The registries of the message box: `GET /api/ui/emoji` and `GET /api/ui/gifs` list the emoji packs
//! (`[[emoji_pack]]`) and the GIF packs (`[[gif_pack]]`) that the enabled plugins of the caller's organization add
//! (see `PluginPackDef`).
//!
//! A plugin ships a pack, and the message box of any plugin offers the packs of all of them. The index file itself is
//! a static file in the plugin's assets (`url`), so no plugin code runs when it is fetched.

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
use aether_core::plugin_manager::models::plugin_def::{PluginManifest, PluginPackDef};
use aether_core::state::AppState;

pub fn routes() -> Router<AppState> {
    Router::new()
        .route("/api/ui/emoji", get(emoji))
        .route("/api/ui/gifs", get(gifs))
}

fn failure(status: StatusCode, message: &str) -> Response {
    (status, Json(json!({ "error": message }))).into_response()
}

async fn emoji(
    state: State<AppState>,
    peer: Option<Extension<ConnectInfo<SocketAddr>>>,
    headers: HeaderMap,
    uri: Uri,
) -> Result<Json<Value>, Response> {
    list(state, peer, headers, uri, "emoji", |manifest| &manifest.emoji_pack).await
}

async fn gifs(
    state: State<AppState>,
    peer: Option<Extension<ConnectInfo<SocketAddr>>>,
    headers: HeaderMap,
    uri: Uri,
) -> Result<Json<Value>, Response> {
    list(state, peer, headers, uri, "gifs", |manifest| &manifest.gif_pack).await
}

/// The packs of the enabled plugins of the caller's organization, from the manifest field `packs_of` picks.
async fn list(
    State(state): State<AppState>,
    peer: Option<Extension<ConnectInfo<SocketAddr>>>,
    headers: HeaderMap,
    uri: Uri,
    registry: &str,
    packs_of: fn(&PluginManifest) -> &Vec<PluginPackDef>,
) -> Result<Json<Value>, Response> {
    let peer_ip = peer.map(|Extension(ConnectInfo(address))| address.ip());
    let identity = match identify(&state, &headers, peer_ip, &uri).await {
        Ok(identity) => identity,
        Err(error) => {
            if matches!(error, IdentityError::Database(_) | IdentityError::Audit(_)) {
                log::error!("{registry} api identity: {error}");
            }
            let (status, message) = error.http();
            return Err(with_retry_after(failure(status, message)));
        }
    };
    if !matches!(identity.actor, Actor::User(_)) {
        return Err(failure(StatusCode::UNAUTHORIZED, "not authenticated"));
    }
    let internal = |what: &str, error: &dyn std::fmt::Display| {
        log::error!("{registry} api {what}: {error}");
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
    let mut packs = Vec::new();
    for row in rows.into_iter().filter(|row| row.is_enabled) {
        // A plugin that fails to load has no packs; it must not break the list for the others.
        let Ok(loaded) = state
            .plugin_runtime
            .ensure_loaded(&core, &row.plugin_name, &row.version)
            .await
        else {
            continue;
        };
        for def in packs_of(&loaded.manifest) {
            packs.push(json!({
                "plugin": row.plugin_name,
                "name": def.name,
                "label": def.label.clone().unwrap_or_else(|| def.name.clone()),
                "url": format!("/api/ui/plugins/{}/assets/{}", row.plugin_name, def.file),
            }));
        }
    }
    Ok(Json(json!({ "packs": packs })))
}
