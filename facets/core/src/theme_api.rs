//! `GET /api/ui/theme` and `GET /api/ui/themes`.
//!
//! The theme is the active theme of the caller's organization: the one an
//! installed theme plugin brought with it (colours, the layout component that
//! renders the app, and its navigation). When the organization cannot be
//! determined (a login page with no tenancy configured, say) or has no theme,
//! the built-in enterprise theme is served so the app always renders.

use std::net::SocketAddr;

use aether_web::api::theme::enterprise_theme;
use axum::{
    Extension, Json, Router,
    extract::{ConnectInfo, State},
    http::{HeaderMap, StatusCode, Uri},
    response::{IntoResponse, Response},
    routing::{get, post},
};
use serde::Deserialize;
use serde_json::{Value, json};
use surrealdb::types::SurrealValue;

use crate::access::identity::{IdentityError, identify, with_retry_after};
use crate::state::AppState;

pub fn routes() -> Router<AppState> {
    Router::new()
        .route("/api/ui/theme", get(get_theme))
        .route("/api/ui/themes", get(list_themes))
        .route("/api/ui/themes/active", post(activate_theme))
}

#[derive(Debug, Deserialize, SurrealValue)]
struct ActiveTheme {
    color_mode: Option<String>,
    name: Option<String>,
    label: Option<String>,
    tokens: Option<Value>,
    layout: Option<String>,
    error_pages: Option<String>,
    nav: Option<Value>,
}

#[derive(Debug, Deserialize, SurrealValue)]
struct ThemeListRow {
    name: String,
    label: String,
    is_system: bool,
    layout: Option<String>,
    tokens: Option<Value>,
}

/// A few light-mode colours, enough to draw a preview swatch of a theme.
fn preview(tokens: Option<&Value>) -> Value {
    let light = tokens.and_then(|tokens| tokens.get("light"));
    let pick = |key: &str| light.and_then(|light| light.get(key)).cloned().unwrap_or(Value::Null);
    json!({
        "background": pick("background"),
        "foreground": pick("foreground"),
        "primary": pick("primary"),
        "sidebar": pick("sidebar"),
    })
}

/// The organization's database session, or the response to send when it is unavailable.
async fn org_session(state: &AppState, org_db: &str) -> Result<crate::state::Db, Response> {
    state.org(org_db).await.map_err(|error| {
        log::error!("theme: organization `{org_db}`: {error}");
        (StatusCode::INTERNAL_SERVER_ERROR, Json(json!({ "error": "database error" })))
            .into_response()
    })
}

fn fallback() -> Response {
    Json(json!(enterprise_theme("fallback"))).into_response()
}

async fn get_theme(
    State(state): State<AppState>,
    peer: Option<Extension<ConnectInfo<SocketAddr>>>,
    headers: HeaderMap,
    uri: Uri,
) -> Response {
    let peer_ip = peer.map(|Extension(ConnectInfo(address))| address.ip());
    let identity = match identify(&state, &headers, peer_ip, &uri).await {
        Ok(identity) => identity,
        Err(IdentityError::RateLimited) => {
            return with_retry_after(
                (StatusCode::TOO_MANY_REQUESTS, Json(json!({ "error": "too many requests" })))
                    .into_response(),
            );
        }
        Err(IdentityError::Database(error)) => {
            log::error!("theme request: {error}");
            return (StatusCode::INTERNAL_SERVER_ERROR, Json(json!({ "error": "database error" })))
                .into_response();
        }
        // No organization, or none the caller may see: the default look.
        Err(error) => {
            log::debug!("theme request: serving the fallback theme ({error})");
            return fallback();
        }
    };

    let org_db = match org_session(&state, &identity.org_db).await {
        Ok(db) => db,
        Err(response) => return response,
    };
    let found = org_db
        .query(
            "SELECT color_mode, \
                    active_theme.name AS name, active_theme.label AS label, \
                    active_theme.tokens AS tokens, active_theme.layout AS layout, \
                    active_theme.error_pages AS error_pages, \
                    active_theme.nav AS nav \
             FROM ui_theme_config LIMIT 1;",
        )
        .await
        .and_then(|response| response.check());
    let mut response = match found {
        Ok(response) => response,
        Err(error) => {
            log::error!("theme lookup in `{}`: {error}", identity.org_db);
            return (StatusCode::INTERNAL_SERVER_ERROR, Json(json!({ "error": "database error" })))
                .into_response();
        }
    };
    let rows: Vec<ActiveTheme> = match response.take(0) {
        Ok(rows) => rows,
        Err(error) => {
            log::error!("theme decode in `{}`: {error}", identity.org_db);
            return (StatusCode::INTERNAL_SERVER_ERROR, Json(json!({ "error": "database error" })))
                .into_response();
        }
    };

    let Some(ActiveTheme {
        color_mode,
        name: Some(name),
        label,
        tokens: Some(tokens),
        layout,
        error_pages,
        nav,
    }) = rows.into_iter().next()
    else {
        log::debug!(
            "{} organization `{}` has no active theme; serving the fallback",
            identity.audit.request_id,
            identity.org_db
        );
        return fallback();
    };

    log::debug!(
        "{} theme `{name}` for organization `{}`",
        identity.audit.request_id,
        identity.org_db
    );
    Json(json!({
        "name": name,
        "label": label.unwrap_or_else(|| name.clone()),
        "color_mode": color_mode.unwrap_or_else(|| "system".to_string()),
        "source": "organization",
        "tokens": tokens,
        "layout": layout.unwrap_or_else(|| "default".to_string()),
        "error_pages": error_pages.unwrap_or_else(|| "default".to_string()),
        "nav": nav,
    }))
    .into_response()
}

async fn list_themes(
    State(state): State<AppState>,
    peer: Option<Extension<ConnectInfo<SocketAddr>>>,
    headers: HeaderMap,
    uri: Uri,
) -> Response {
    let peer_ip = peer.map(|Extension(ConnectInfo(address))| address.ip());
    let identity = match identify(&state, &headers, peer_ip, &uri).await {
        Ok(identity) => identity,
        Err(IdentityError::RateLimited) => {
            return with_retry_after(
                (StatusCode::TOO_MANY_REQUESTS, Json(json!({ "error": "too many requests" })))
                    .into_response(),
            );
        }
        Err(_) => {
            return Json(json!({ "themes": [], "active": null, "source": "fallback" })).into_response();
        }
    };

    let org_db = match org_session(&state, &identity.org_db).await {
        Ok(db) => db,
        Err(response) => return response,
    };
    let listed = org_db
        .query(
            "SELECT name, label, is_system, layout, tokens FROM ui_themes ORDER BY name; \
             SELECT VALUE active_theme.name FROM ui_theme_config LIMIT 1;",
        )
        .await
        .and_then(|response| response.check());
    let mut response = match listed {
        Ok(response) => response,
        Err(error) => {
            log::error!("theme list in `{}`: {error}", identity.org_db);
            return (StatusCode::INTERNAL_SERVER_ERROR, Json(json!({ "error": "database error" })))
                .into_response();
        }
    };
    let themes: Vec<ThemeListRow> = response.take(0).unwrap_or_default();
    let active: Vec<String> = response.take(1).unwrap_or_default();
    Json(json!({
        "themes": themes
            .into_iter()
            .map(|row| json!({
                "name": row.name,
                "label": row.label,
                "is_system": row.is_system,
                "layout": row.layout.unwrap_or_else(|| "default".to_string()),
                "preview": preview(row.tokens.as_ref()),
            }))
            .collect::<Vec<_>>(),
        // `null`: no theme is active, so the built-in default is.
        "active": active.into_iter().next(),
        "source": "organization",
    }))
    .into_response()
}

#[derive(Debug, Deserialize)]
struct ActivateBody {
    /// The theme to activate, or `null` to go back to the built-in default.
    name: Option<String>,
}

/// `POST /api/ui/themes/active` `{ "name": "ocean" }`: make an installed theme
/// the organization's active one; `{ "name": null }` returns to the built-in
/// default. Developers only.
async fn activate_theme(
    State(state): State<AppState>,
    peer: Option<Extension<ConnectInfo<SocketAddr>>>,
    headers: HeaderMap,
    uri: Uri,
    Json(body): Json<ActivateBody>,
) -> Response {
    let peer_ip = peer.map(|Extension(ConnectInfo(address))| address.ip());
    let identity = match identify(&state, &headers, peer_ip, &uri).await {
        Ok(identity) => identity,
        Err(error) => {
            if matches!(error, IdentityError::Database(_) | IdentityError::Audit(_)) {
                log::error!("theme activation identity: {error}");
            }
            let (status, message) = error.http();
            return with_retry_after((status, Json(json!({ "error": message }))).into_response());
        }
    };
    let is_developer = identity
        .session
        .as_ref()
        .is_some_and(|session| session.user.is_super_user);
    if !is_developer {
        return (
            StatusCode::FORBIDDEN,
            Json(json!({ "error": "developer access required" })),
        )
            .into_response();
    }

    use crate::plugin_manager::catalog::{CatalogError, activate_theme as activate};
    let Some(name) = body.name else {
        let cleared = async {
            let org = state.org(&identity.org_db).await?;
            org.query("DELETE ui_theme_config;").await?.check()?;
            Ok::<(), surrealdb::Error>(())
        }
        .await;
        return match cleared {
            Ok(()) => {
                log::info!(
                    "{} organization `{}` returned to the built-in theme",
                    identity.audit.request_id,
                    identity.org_db
                );
                Json(json!({ "ok": true, "active": null })).into_response()
            }
            Err(error) => {
                log::error!("clearing the theme of `{}`: {error}", identity.org_db);
                (StatusCode::INTERNAL_SERVER_ERROR, Json(json!({ "error": "database error" })))
                    .into_response()
            }
        };
    };
    match activate(&state.fresh_session(), &state.namespace, &identity.org_db, &name).await {
        Ok(()) => {
            log::info!(
                "{} theme `{name}` activated for organization `{}`",
                identity.audit.request_id,
                identity.org_db
            );
            Json(json!({ "ok": true, "active": name })).into_response()
        }
        Err(CatalogError::ThemeNotInstalled(name)) => (
            StatusCode::NOT_FOUND,
            Json(json!({ "error": format!("theme `{name}` is not installed in this organization") })),
        )
            .into_response(),
        Err(error) => {
            log::error!("theme activation in `{}`: {error}", identity.org_db);
            (StatusCode::INTERNAL_SERVER_ERROR, Json(json!({ "error": "database error" })))
                .into_response()
        }
    }
}
