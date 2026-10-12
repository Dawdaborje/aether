//! `GET /api/ui/apps`: the apps the caller can open: the Apps launcher.
//!
//! An app is an installed plugin that declares `[app]` in its manifest. The list
//! is for logged-in members of the organization; the launcher is the home
//! screen of everyone who is not a developer. Each tile includes `menus` built
//! from that plugin's main pages for the top bar when the theme has no nav match.

use std::collections::HashMap;
use std::net::SocketAddr;

use axum::{
    Extension, Json, Router,
    extract::{ConnectInfo, State},
    http::{HeaderMap, StatusCode, Uri},
    response::{IntoResponse, Response},
    routing::get,
};
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};
use surrealdb::types::SurrealValue;

use crate::access::{
    audit::Actor,
    identity::{IdentityError, identify, with_retry_after},
};
use crate::app_dir::AppDir;
use crate::pages_api::{locale_chain, translator};
use crate::plugin_assets;
use crate::plugin_manager::revisions;
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
    icon_path: Option<String>,
    revision: Option<String>,
    app: AppRow,
}

#[derive(Debug, Deserialize, SurrealValue)]
struct MenuPageRow {
    route: String,
    title: String,
    icon: Option<String>,
    sort_order: Option<i64>,
    is_menu_visible: Option<bool>,
    is_pattern: bool,
    plugin_name: String,
    plugin_version: String,
}

/// One top-bar menu entry for an installed app.
#[derive(Debug, Serialize, PartialEq, Eq, Clone)]
struct AppMenuItem {
    label: String,
    href: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    icon: Option<String>,
}

/// One tile of the launcher.
#[derive(Debug, Serialize, PartialEq, Eq)]
struct AppTile {
    plugin: String,
    /// The plugin's workspace, which groups apps into categories.
    category: Option<String>,
    label: String,
    icon: Option<String>,
    /// `/api/ui/plugins/{name}/logo` when the revision ships a logo file.
    logo_url: Option<String>,
    route: String,
    description: Option<String>,
    /// Main pages of this plugin for the navbar when theme nav does not match.
    menus: Vec<AppMenuItem>,
}

fn failure(status: StatusCode, message: &str) -> Response {
    (status, Json(json!({ "error": message }))).into_response()
}

fn is_menu_page(row: &MenuPageRow) -> bool {
    if row.is_pattern {
        return false;
    }
    if row.is_menu_visible == Some(false) {
        return false;
    }
    !row.route.ends_with("/new")
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
            .query(
                "SELECT name, version, workspace, icon_path, revision, app \
                 FROM plugins WHERE app != NONE AND is_active = true;",
            )
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

    let installed_pairs: Vec<(String, String)> = installed
        .iter()
        .map(|row| (row.plugin_name.clone(), row.version.clone()))
        .collect();

    let mut menus_by_plugin: HashMap<(String, String), Vec<AppMenuItem>> = HashMap::new();
    if !installed_pairs.is_empty() {
        let pages = async {
            let core = state.core().await?;
            let mut response = core
                .query(
                    "SELECT route, title, icon, sort_order, is_menu_visible, is_pattern, \
                     plugin.name AS plugin_name, plugin.version AS plugin_version \
                     FROM plugin_ui_pages \
                     WHERE is_pattern = false \
                       AND (is_menu_visible = true OR is_menu_visible = NONE);",
                )
                .await?
                .check()?;
            response.take::<Vec<MenuPageRow>>(0)
        }
        .await;
        match pages {
            Ok(rows) => {
                let chain = locale_chain(&state, &identity, &headers);
                let loc = translator(&state, &identity).await.ok();
                let mut grouped: HashMap<(String, String), Vec<MenuPageRow>> = HashMap::new();
                for row in rows {
                    if !is_menu_page(&row) {
                        continue;
                    }
                    if !installed_pairs
                        .iter()
                        .any(|(name, version)| name == &row.plugin_name && version == &row.plugin_version)
                    {
                        continue;
                    }
                    grouped
                        .entry((row.plugin_name.clone(), row.plugin_version.clone()))
                        .or_default()
                        .push(row);
                }
                for ((name, version), mut rows) in grouped {
                    rows.sort_by(|a, b| {
                        a.sort_order
                            .unwrap_or(0)
                            .cmp(&b.sort_order.unwrap_or(0))
                            .then_with(|| a.title.to_lowercase().cmp(&b.title.to_lowercase()))
                    });
                    let mut menus = Vec::with_capacity(rows.len());
                    for row in rows {
                        let mut title = Value::String(row.title);
                        if let Some(translator) = &loc {
                            aether_localization::resolve_tree(
                                &mut title,
                                &name,
                                translator,
                                &chain,
                            );
                        }
                        let label = title.as_str().unwrap_or_default().to_string();
                        menus.push(AppMenuItem {
                            label,
                            href: row.route,
                            icon: row.icon,
                        });
                    }
                    menus_by_plugin.insert((name, version), menus);
                }
            }
            Err(error) => log::warn!("apps: menu pages: {error}"),
        }
    }

    let layout = AppDir::new(&state.config.app_dir);
    let mut tiles: Vec<AppTile> = Vec::new();
    for row in apps {
        if !installed
            .iter()
            .any(|plugin| plugin.plugin_name == row.name && plugin.version == row.version)
        {
            continue;
        }
        let logo_url = {
            let folder = row.revision.as_deref().unwrap_or(row.version.as_str());
            match revisions::read_index(&layout, &row.name, folder).await {
                Ok(Some(index)) => {
                    plugin_assets::logo_url(&row.name, &index, row.icon_path.as_deref())
                }
                Ok(None) => None,
                Err(error) => {
                    log::warn!("apps logo index `{}`: {error}", row.name);
                    None
                }
            }
        };
        let menus = menus_by_plugin
            .remove(&(row.name.clone(), row.version.clone()))
            .unwrap_or_default();
        tiles.push(AppTile {
            plugin: row.name,
            category: row.workspace,
            label: row.app.label,
            icon: row.app.icon,
            logo_url,
            route: row.app.route,
            description: row.app.description,
            menus,
        });
    }
    tiles.sort_by_key(|tile| tile.label.to_lowercase());

    log::info!(
        "{} apps for organization `{}`: {} tile(s)",
        identity.audit.request_id,
        identity.org_db,
        tiles.len()
    );
    Json(json!({ "apps": tiles })).into_response()
}
