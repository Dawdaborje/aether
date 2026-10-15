//! `GET /api/ui/apps`: the apps the caller can open: the Apps launcher.
//!
//! An app is an installed plugin that declares `[app]` in its manifest. The list
//! is for logged-in members of the organization; the launcher is the home
//! screen of everyone who is not a developer. Each tile includes `menus` built
//! from that plugin's main pages for the sidebar when the theme has no nav match.
//!
//! Workspace `launcher = "folder"` (default) groups remaining tiles under the
//! workspace label; `launcher = "flat"` (Base) lists each member as its own
//! tile. Plugins may also set `[app] folder = "…"` to create or join a named
//! folder within the same workspace category. Sibling plugins without `[app]`,
//! and `[app]` modules with at most one sidebar menu (unless `standalone =
//! true`), fold into the workspace host tile (shortest plugin name — e.g.
//! `hr`), grouped by `menu_group`.

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
use crate::plugin_manager::installed;
use crate::plugin_manager::revisions;
use crate::roles;
use crate::state::AppState;

pub fn routes() -> Router<AppState> {
    Router::new().route("/api/ui/apps", get(list_apps))
}

#[derive(Debug, Deserialize, SurrealValue)]
struct InstalledRow {
    plugin_name: String,
    version: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, SurrealValue)]
struct AppRow {
    label: String,
    icon: Option<String>,
    route: String,
    description: Option<String>,
    #[serde(default)]
    standalone: Option<bool>,
    #[serde(default)]
    folder: Option<String>,
    /// Roles that may see this tile (OR). Empty = any member.
    #[serde(default)]
    roles: Option<Vec<String>>,
}

#[derive(Debug, Clone, Serialize, Deserialize, SurrealValue)]
struct PluginAppRow {
    name: String,
    version: String,
    workspace: Option<String>,
    icon_path: Option<String>,
    revision: Option<String>,
    app: AppRow,
}

#[derive(Debug, Clone, Serialize, Deserialize, SurrealValue)]
struct MenuPageRow {
    route: String,
    title: String,
    icon: Option<String>,
    menu_group: Option<String>,
    sort_order: Option<i64>,
    is_menu_visible: Option<bool>,
    is_pattern: bool,
    #[serde(default)]
    required_roles: Option<Vec<String>>,
    plugin_name: String,
    plugin_version: String,
    workspace: Option<String>,
}

/// One top-bar / sidebar menu entry for an installed app.
#[derive(Debug, Serialize, PartialEq, Eq, Clone)]
struct AppMenuItem {
    label: String,
    href: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    icon: Option<String>,
    /// Sidebar section (collapsible); from the page's `menu_group`.
    #[serde(skip_serializing_if = "Option::is_none")]
    group: Option<String>,
    /// From the page's `sort` attribute; not sent to the client.
    #[serde(skip)]
    sort_order: i64,
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
    /// Named Apps-home folder (`[app] folder`); same workspace category for install.
    #[serde(skip_serializing_if = "Option::is_none")]
    folder: Option<String>,
    /// Main pages of this plugin for the navbar when theme nav does not match.
    menus: Vec<AppMenuItem>,
    /// Not sent to the client; used when folding thin workspace modules.
    #[serde(skip)]
    standalone: bool,
}

/// One workspace group on the Apps launcher (folder / Frappe-style workspace).
#[derive(Debug, Serialize, PartialEq, Eq)]
struct AppCategory {
    /// Workspace name (`base`, `crm`, …); matches `AppTile.category`.
    id: String,
    label: String,
    /// Lucide icon id when `group_icon` is not a file path.
    #[serde(skip_serializing_if = "Option::is_none")]
    icon: Option<String>,
    /// `/api/ui/workspaces/{id}/logo` when the workspace staged a logo file.
    #[serde(skip_serializing_if = "Option::is_none")]
    logo_url: Option<String>,
    /// `folder` (default) or `flat` — flat lists each member on the home grid.
    #[serde(skip_serializing_if = "Option::is_none")]
    launcher: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, SurrealValue)]
struct WorkspaceRow {
    name: String,
    label: Option<String>,
    group_label: Option<String>,
    group_icon: Option<String>,
    logo_file: Option<String>,
    #[serde(default)]
    launcher: Option<String>,
}

/// Pre-role catalog for the Apps launcher (Mantle Tier B).
#[derive(Debug, Clone, Serialize, Deserialize)]
struct LauncherCatalog {
    apps: Vec<PluginAppRow>,
    menus: Vec<MenuPageRow>,
    workspaces: Vec<WorkspaceRow>,
}

async fn launcher_catalog(
    state: &AppState,
    org: &str,
    installed_pairs: &[(String, String)],
) -> Result<LauncherCatalog, surrealdb::Error> {
    if let Ok(Some(json)) = state.cache.warm_get_apps_launcher_json(org)
        && let Ok(catalog) = serde_json::from_value::<LauncherCatalog>(json)
    {
        return Ok(catalog);
    }
    let core = state.core().await?;
    let mut apps_resp = core
        .query(
            "SELECT name, version, workspace, icon_path, revision, app \
             FROM plugins WHERE app != NONE AND is_active = true;",
        )
        .await?
        .check()?;
    let apps: Vec<PluginAppRow> = apps_resp
        .take::<Vec<PluginAppRow>>(0)?
        .into_iter()
        .filter(|row| {
            installed_pairs
                .iter()
                .any(|(name, version)| name == &row.name && version == &row.version)
        })
        .collect();
    let mut menus = Vec::new();
    if !installed_pairs.is_empty() {
        let mut pages_resp = core
            .query(
                "SELECT route, title, icon, menu_group, sort_order, is_menu_visible, is_pattern, \
                 required_roles, \
                 plugin.name AS plugin_name, plugin.version AS plugin_version, \
                 plugin.workspace AS workspace \
                 FROM plugin_ui_pages \
                 WHERE is_pattern = false \
                   AND (is_menu_visible = true OR is_menu_visible = NONE);",
            )
            .await?
            .check()?;
        let pages: Vec<MenuPageRow> = pages_resp.take(0)?;
        menus = pages
            .into_iter()
            .filter(|row| {
                is_menu_page(row)
                    && installed_pairs.iter().any(|(name, version)| {
                        name == &row.plugin_name && version == &row.plugin_version
                    })
            })
            .collect();
    }
    let mut ws_resp = core
        .query(
            "SELECT name, label, group_label, group_icon, logo_file, launcher FROM plugin_workspaces;",
        )
        .await?
        .check()?;
    let workspaces: Vec<WorkspaceRow> = ws_resp.take(0)?;
    let catalog = LauncherCatalog {
        apps,
        menus,
        workspaces,
    };
    if let Ok(json) = serde_json::to_value(&catalog) {
        let _ = state.cache.warm_set_apps_launcher_json(org, &json);
    }
    Ok(catalog)
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
    // Create / hire forms are opened from list "New" actions, not the sidebar.
    let route = row.route.as_str();
    !(route.ends_with("/new") || route.ends_with("/hire"))
}

/// Shortest installed `[app]` tile in a workspace hosts menus from sibling plugins without `[app]`.
fn workspace_host_index(tiles: &[AppTile], workspace: &str) -> Option<usize> {
    tiles
        .iter()
        .enumerate()
        .filter(|(_, tile)| tile.category.as_deref() == Some(workspace))
        .min_by_key(|(_, tile)| (tile.plugin.len(), tile.route.len(), tile.plugin.as_str()))
        .map(|(index, _)| index)
}

/// `hr_recruitment` → `Recruitment` when a contributed page has no `menu_group`.
fn humanize_plugin_group(plugin: &str) -> String {
    let trimmed = plugin
        .strip_prefix("hr_")
        .or_else(|| plugin.strip_prefix("erp_"))
        .or_else(|| plugin.strip_prefix("crm_"))
        .unwrap_or(plugin);
    humanize_workspace(trimmed)
}

fn merge_menus_into(host: &mut AppTile, menus: &mut Vec<AppMenuItem>, default_group: &str) {
    for item in menus.iter_mut() {
        if item.group.is_none() {
            item.group = Some(default_group.to_string());
        }
    }
    host.menus.append(menus);
    host.menus.sort_by(|a, b| {
        a.group
            .as_deref()
            .unwrap_or("")
            .cmp(b.group.as_deref().unwrap_or(""))
            .then(a.sort_order.cmp(&b.sort_order))
            .then_with(|| a.label.to_lowercase().cmp(&b.label.to_lowercase()))
    });
}

/// Drop workspace tiles that only expose one (or zero) menu links; their menus
/// join the host sidebar. The host itself and `standalone` apps are kept.
fn fold_thin_workspace_apps(tiles: &mut Vec<AppTile>) {
    let workspaces: Vec<String> = tiles
        .iter()
        .filter_map(|tile| tile.category.clone())
        .collect::<std::collections::BTreeSet<_>>()
        .into_iter()
        .collect();
    for workspace in workspaces {
        let Some(host_idx) = workspace_host_index(tiles, &workspace) else {
            continue;
        };
        let host_plugin = tiles[host_idx].plugin.clone();
        let mut fold_idxs: Vec<usize> = tiles
            .iter()
            .enumerate()
            .filter(|(index, tile)| {
                *index != host_idx
                    && tile.category.as_deref() == Some(workspace.as_str())
                    && !tile.standalone
                    && tile.menus.len() <= 1
            })
            .map(|(index, _)| index)
            .collect();
        fold_idxs.sort_unstable();
        fold_idxs.reverse();
        for index in fold_idxs {
            let mut tile = tiles.remove(index);
            let host_idx = workspace_host_index(tiles, &workspace).unwrap_or_else(|| {
                // Host may have shifted after removals; fall back by plugin name.
                tiles
                    .iter()
                    .position(|t| t.plugin == host_plugin)
                    .unwrap_or(0)
            });
            let group = tile.label.clone();
            merge_menus_into(&mut tiles[host_idx], &mut tile.menus, &group);
        }
    }
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
    let Actor::User(user_id) = &identity.actor else {
        return failure(
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
        );
    };
    let is_developer = identity
        .session
        .as_ref()
        .map(|session| session.user.is_super_user)
        .unwrap_or(false);

    // `identify` leaves the connection on the organization's database.
    let org_db = match state.org(&identity.org_db).await {
        Ok(db) => db,
        Err(error) => {
            log::error!("apps: organization `{}`: {error}", identity.org_db);
            return failure(StatusCode::INTERNAL_SERVER_ERROR, "database error");
        }
    };
    let user_roles = if is_developer {
        Vec::new()
    } else {
        match roles::roles_of_cached(&state.cache, &identity.org_db, &org_db, user_id).await {
            Ok(names) => names,
            Err(error) => {
                log::error!(
                    "apps: roles of `{user_id}` in `{}`: {error}",
                    identity.org_db
                );
                return failure(StatusCode::INTERNAL_SERVER_ERROR, "database error");
            }
        }
    };
    let installed = match installed::list_cached(&state.cache, &identity.org_db, &org_db).await {
        Ok(rows) => rows
            .into_iter()
            .filter(|row| row.is_enabled)
            .map(|row| InstalledRow {
                plugin_name: row.plugin_name,
                version: row.version,
            })
            .collect::<Vec<_>>(),
        Err(error) => {
            log::error!("apps: installed plugins of `{}`: {error}", identity.org_db);
            return failure(StatusCode::INTERNAL_SERVER_ERROR, "database error");
        }
    };

    let installed_pairs: Vec<(String, String)> = installed
        .iter()
        .map(|row| (row.plugin_name.clone(), row.version.clone()))
        .collect();
    let catalog = match launcher_catalog(&state, &identity.org_db, &installed_pairs).await {
        Ok(catalog) => catalog,
        Err(error) => {
            log::error!("apps: catalog lookup: {error}");
            return failure(StatusCode::INTERNAL_SERVER_ERROR, "database error");
        }
    };

    let mut menus_by_plugin: HashMap<(String, String), Vec<AppMenuItem>> = HashMap::new();
    let mut workspace_by_plugin: HashMap<(String, String), Option<String>> = HashMap::new();
    if !catalog.menus.is_empty() {
        let chain = locale_chain(&state, &identity, &headers);
        let loc = translator(&state, &identity).await.ok();
        let mut grouped: HashMap<(String, String), Vec<MenuPageRow>> = HashMap::new();
        for row in catalog.menus {
            if !is_developer
                && !roles::allows(&user_roles, row.required_roles.as_deref().unwrap_or(&[]))
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
            let workspace = rows.first().and_then(|row| row.workspace.clone());
            workspace_by_plugin.insert((name.clone(), version.clone()), workspace);
            let mut menus = Vec::with_capacity(rows.len());
            for row in rows {
                let mut title = Value::String(row.title);
                if let Some(translator) = &loc {
                    aether_localization::resolve_tree(&mut title, &name, translator, &chain);
                }
                let label = title.as_str().unwrap_or_default().to_string();
                let group = row
                    .menu_group
                    .as_deref()
                    .map(str::trim)
                    .filter(|g| !g.is_empty())
                    .map(str::to_string);
                menus.push(AppMenuItem {
                    label,
                    href: row.route,
                    icon: row.icon,
                    group,
                    sort_order: row.sort_order.unwrap_or(0),
                });
            }
            menus_by_plugin.insert((name, version), menus);
        }
    }

    let layout = AppDir::new(&state.config.app_dir);
    let mut member_tiles: Vec<AppTile> = Vec::new();
    for row in catalog.apps {
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
        let app_roles = row.app.roles.clone().unwrap_or_default();
        if !is_developer && !roles::allows(&user_roles, &app_roles) {
            // Drop menus so they are not folded into a host the caller cannot open.
            menus_by_plugin.remove(&(row.name.clone(), row.version.clone()));
            continue;
        }
        let menus = menus_by_plugin
            .remove(&(row.name.clone(), row.version.clone()))
            .unwrap_or_default();
        let folder = row
            .app
            .folder
            .as_deref()
            .map(str::trim)
            .filter(|name| !name.is_empty())
            .map(str::to_string);
        member_tiles.push(AppTile {
            plugin: row.name,
            category: row.workspace,
            label: row.app.label,
            icon: row.app.icon,
            logo_url,
            route: row.app.route,
            description: row.app.description,
            folder,
            menus,
            standalone: row.app.standalone.unwrap_or(false),
        });
    }

    // Plugins without `[app]` (e.g. recruitment under HRMS) fold into the workspace host tile.
    for ((name, version), mut menus) in menus_by_plugin {
        let Some(workspace) = workspace_by_plugin
            .remove(&(name.clone(), version))
            .flatten()
            .map(|w| w.trim().to_string())
            .filter(|w| !w.is_empty())
        else {
            continue;
        };
        let Some(host) = workspace_host_index(&member_tiles, &workspace) else {
            continue;
        };
        let default_group = humanize_plugin_group(&name);
        merge_menus_into(&mut member_tiles[host], &mut menus, &default_group);
    }

    // Thin `[app]` modules (≤1 menu) fold into the host unless `standalone = true`.
    fold_thin_workspace_apps(&mut member_tiles);
    // After role filtering and folding, drop tiles with nothing left to open.
    member_tiles.retain(|tile| !tile.menus.is_empty());

    let by_name: HashMap<String, WorkspaceRow> = catalog
        .workspaces
        .into_iter()
        .map(|row| (row.name.clone(), row))
        .collect();

    // Each `[app]` is its own tile; workspace metadata only labels the folder.
    let tiles = member_tiles;
    let mut tiles = tiles;
    tiles.sort_by_key(|tile| tile.label.to_lowercase());

    let mut categories: Vec<AppCategory> = Vec::new();
    let mut seen = std::collections::HashSet::new();
    for tile in &tiles {
        let id = tile
            .category
            .as_deref()
            .map(str::trim)
            .filter(|v| !v.is_empty())
            .unwrap_or("other");
        if !seen.insert(id.to_string()) {
            continue;
        }
        let meta = by_name.get(id);
        let label = meta
            .and_then(|row| {
                row.group_label
                    .as_deref()
                    .or(row.label.as_deref())
                    .map(str::trim)
                    .filter(|v| !v.is_empty())
                    .map(str::to_string)
            })
            .unwrap_or_else(|| humanize_workspace(id));
        let icon = meta.and_then(|row| {
            row.group_icon.as_deref().and_then(|icon| {
                let trimmed = icon.trim();
                if trimmed.is_empty() || crate::plugin_manager::workspaces::is_asset_icon(trimmed) {
                    None
                } else {
                    Some(trimmed.to_string())
                }
            })
        });
        let logo_url = meta
            .and_then(|row| {
                crate::plugin_manager::workspaces::logo_url_for_workspace(
                    &row.name,
                    row.logo_file.as_deref(),
                    row.group_icon.as_deref(),
                )
            })
            .or_else(|| tile.logo_url.clone());
        let launcher = meta
            .and_then(|row| {
                row.launcher
                    .as_deref()
                    .map(str::trim)
                    .filter(|v| !v.is_empty())
            })
            .map(|v| {
                if v == "flat" {
                    "flat".to_string()
                } else {
                    "folder".to_string()
                }
            });
        categories.push(AppCategory {
            id: id.to_string(),
            label,
            icon,
            logo_url,
            launcher,
        });
    }
    categories.sort_by(|a, b| a.label.to_lowercase().cmp(&b.label.to_lowercase()));

    log::info!(
        "{} apps for organization `{}`: {} tile(s), {} group(s)",
        identity.audit.request_id,
        identity.org_db,
        tiles.len(),
        categories.len()
    );
    Json(json!({ "apps": tiles, "categories": categories })).into_response()
}

/// `erp_sales` → `ERP Sales` (fallback when the workspace is not in the catalog yet).
fn humanize_workspace(name: &str) -> String {
    const ACRONYMS: &[&str] = &["api", "crm", "erp", "hr", "hrms", "pos", "ui", "uom"];
    if name == "other" {
        return "Other".into();
    }
    name.split([' ', '_', '-'])
        .filter(|part| !part.is_empty())
        .map(|word| {
            let lower = word.to_ascii_lowercase();
            if ACRONYMS.contains(&lower.as_str()) {
                lower.to_ascii_uppercase()
            } else {
                let mut chars = lower.chars();
                match chars.next() {
                    Some(first) => first.to_uppercase().collect::<String>() + chars.as_str(),
                    None => String::new(),
                }
            }
        })
        .collect::<Vec<_>>()
        .join(" ")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn humanizes_workspace_ids_for_folder_labels() {
        assert_eq!(humanize_workspace("crm"), "CRM");
        assert_eq!(humanize_workspace("hrms"), "HRMS");
        assert_eq!(humanize_workspace("erp_sales"), "ERP Sales");
        assert_eq!(humanize_workspace("other"), "Other");
    }

    #[test]
    fn menu_pages_skip_create_and_hire_routes() {
        let base = MenuPageRow {
            route: "/hr/employees".into(),
            title: "Employees".into(),
            icon: None,
            menu_group: None,
            sort_order: Some(0),
            is_menu_visible: Some(true),
            is_pattern: false,
            required_roles: None,
            plugin_name: "hr".into(),
            plugin_version: "0.1.0".into(),
            workspace: Some("hrms".into()),
        };
        assert!(is_menu_page(&base));
        assert!(!is_menu_page(&MenuPageRow {
            route: "/hr/employees/hire".into(),
            ..base.clone()
        }));
        assert!(!is_menu_page(&MenuPageRow {
            route: "/hr/departments/new".into(),
            ..base.clone()
        }));
        assert!(!is_menu_page(&MenuPageRow {
            is_menu_visible: Some(false),
            ..base
        }));
    }

    fn sample_tile(plugin: &str, label: &str, menus: usize, standalone: bool) -> AppTile {
        AppTile {
            plugin: plugin.into(),
            category: Some("hrms".into()),
            label: label.into(),
            icon: None,
            logo_url: None,
            route: format!("/hr/{plugin}"),
            description: None,
            folder: None,
            menus: (0..menus)
                .map(|i| AppMenuItem {
                    label: format!("Menu {i}"),
                    href: format!("/hr/{plugin}/{i}"),
                    icon: None,
                    group: None,
                    sort_order: i as i64,
                })
                .collect(),
            standalone,
        }
    }

    #[test]
    fn picks_shortest_plugin_as_workspace_host() {
        let tiles = vec![
            sample_tile("hr_attendance", "Attendance", 1, false),
            sample_tile("hr", "Human Resources", 2, false),
        ];
        assert_eq!(workspace_host_index(&tiles, "hrms"), Some(1));
        assert_eq!(humanize_plugin_group("hr_recruitment"), "Recruitment");
    }

    #[test]
    fn folds_single_menu_workspace_apps_into_host() {
        let mut tiles = vec![
            sample_tile("hr_attendance", "Attendance", 1, false),
            sample_tile("hr", "Human Resources", 2, false),
            sample_tile("hr_leave", "Leave", 2, false),
            sample_tile("hr_payroll", "Payroll", 1, true),
        ];
        fold_thin_workspace_apps(&mut tiles);
        let names: Vec<_> = tiles.iter().map(|t| t.plugin.as_str()).collect();
        assert_eq!(names, ["hr", "hr_leave", "hr_payroll"]);
        let hr = tiles.iter().find(|t| t.plugin == "hr").unwrap();
        assert!(
            hr.menus
                .iter()
                .any(|m| m.group.as_deref() == Some("Attendance"))
        );
        assert_eq!(hr.menus.len(), 3); // 2 own + 1 folded
    }
}
