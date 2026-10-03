//! `GET /api/ui/apps`: the apps the caller can open: the Apps launcher.
//!
//! An app is an installed plugin that declares `[app]` in its manifest. The list
//! is for logged-in members of the organization; the launcher is the home
//! screen of everyone who is not a developer.

use std::net::SocketAddr;

use axum::{
    Extension, Json, Router,
    extract::{ConnectInfo, State},
    http::{HeaderMap, StatusCode, Uri},
    response::{IntoResponse, Response},
    routing::get,
};
use serde::{Deserialize, Serialize};
use serde_json::json;
use surrealdb::types::SurrealValue;

use crate::access::{
    audit::Actor,
    identity::{IdentityError, identify, with_retry_after},
};
use crate::state::AppState;

pub fn routes() -> Router<AppState> {
    Router::new().route("/api/ui/apps", get(list_apps))
}

#[derive(Debug, Deserialize, SurrealValue)]
struct InstalledRow {
    plugin_name: String,
    version: String,
}

#[derive(Debug, Deserialize, SurrealValue)]
struct AppRow {
    label: String,
    icon: Option<String>,
    route: String,
    description: Option<String>,
}

#[derive(Debug, Deserialize, SurrealValue)]
struct PluginAppRow {
    name: String,
    version: String,
    workspace: Option<String>,
    app: AppRow,
}

/// One tile of the launcher.
#[derive(Debug, Serialize, PartialEq, Eq)]
struct AppTile {
    plugin: String,
    /// The plugin's workspace, which groups apps into categories.
    category: Option<String>,
    label: String,
    icon: Option<String>,
    route: String,
    description: Option<String>,
}

fn failure(status: StatusCode, message: &str) -> Response {
    (status, Json(json!({ "error": message }))).into_response()
}

async fn list_apps(
    State(state): State<AppState>,
    peer: Option<Extension<ConnectInfo<SocketAddr>>>,
    headers: HeaderMap,
    uri: Uri,
) -> Response {
    let peer_ip = peer.map(|Extension(ConnectInfo(address))| address.ip());
    let identity = match identify(&state, &headers, peer_ip, &uri).await {
        Ok(identity) => identity,
        Err(error) => {
            if matches!(error, IdentityError::Database(_) | IdentityError::Audit(_)) {
                log::error!("apps request identity: {error}");
            }
            let (status, message) = error.http();
            return with_retry_after(failure(status, message));
        }
    };

    // The launcher is for people who are logged in and belong here.
    if !matches!(identity.actor, Actor::User(_)) {
        return failure(
            if identity.foreign_user { StatusCode::FORBIDDEN } else { StatusCode::UNAUTHORIZED },
            if identity.foreign_user { "not a member of this organization" } else { "not authenticated" },
        );
    }

    // `identify` leaves the connection on the organization's database.
    let org_db = match state.org(&identity.org_db).await {
        Ok(db) => db,
        Err(error) => {
            log::error!("apps: organization `{}`: {error}", identity.org_db);
            return failure(StatusCode::INTERNAL_SERVER_ERROR, "database error");
        }
    };
    let installed = org_db
        .query("SELECT plugin_name, version FROM installed_plugins WHERE is_enabled = true;")
        .await
        .and_then(|response| response.check());
    let installed: Vec<InstalledRow> = match installed.and_then(|mut response| response.take(0)) {
        Ok(rows) => rows,
        Err(error) => {
            log::error!("apps: installed plugins of `{}`: {error}", identity.org_db);
            return failure(StatusCode::INTERNAL_SERVER_ERROR, "database error");
        }
    };

    let apps = async {
        let core = state.core().await?;
        let mut response = core
            .query("SELECT name, version, workspace, app FROM plugins WHERE app != NONE AND is_active = true;")
            .await?
            .check()?;
        response.take::<Vec<PluginAppRow>>(0)
    }
    .await;
    let apps = match apps {
        Ok(rows) => rows,
        Err(error) => {
            log::error!("apps: catalog lookup: {error}");
            return failure(StatusCode::INTERNAL_SERVER_ERROR, "database error");
        }
    };

    let mut tiles: Vec<AppTile> = apps
        .into_iter()
        .filter(|row| {
            installed
                .iter()
                .any(|plugin| plugin.plugin_name == row.name && plugin.version == row.version)
        })
        .map(|row| AppTile {
            plugin: row.name,
            category: row.workspace,
            label: row.app.label,
            icon: row.app.icon,
            route: row.app.route,
            description: row.app.description,
        })
        .collect();
    tiles.sort_by_key(|tile| tile.label.to_lowercase());

    log::info!(
        "{} apps for organization `{}`: {} tile(s)",
        identity.audit.request_id,
        identity.org_db,
        tiles.len()
    );
    Json(json!({ "apps": tiles })).into_response()
}
