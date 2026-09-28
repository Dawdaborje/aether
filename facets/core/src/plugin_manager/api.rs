use std::collections::HashSet;

use axum::{
    Json,
    extract::{Path, State},
    http::StatusCode,
};
use axum_extra::extract::CookieJar;
use serde::Deserialize;
use serde_json::{Value, json};
use surrealdb::types::SurrealValue;

use crate::{
    kernel::{ModelGrant, PluginHostContext},
    state::AppState,
};

const SESSION_COOKIE: &str = "aether_session";

#[derive(Debug, Deserialize)]
struct InstalledPluginVersion {
    version: String,
    is_enabled: bool,
}

#[derive(Debug, Deserialize, SurrealValue)]
struct InstalledPluginRow {
    version: String,
    is_enabled: bool,
}

pub async fn invoke_plugin(
    State(state): State<AppState>,
    Path((plugin_name, function)): Path<(String, String)>,
    jar: CookieJar,
    Json(payload): Json<Value>,
) -> Result<Json<Value>, (StatusCode, Json<Value>)> {
    let token = jar
        .get(SESSION_COOKIE)
        .map(|cookie| cookie.value())
        .filter(|token| !token.is_empty())
        .ok_or_else(|| api_error(StatusCode::UNAUTHORIZED, "not authenticated"))?;

    state.use_core().await.map_err(|error| {
        log::error!("plugin invocation core database selection failed: {error}");
        api_error(StatusCode::INTERNAL_SERVER_ERROR, "database error")
    })?;
    let session = aether_orm::find_session_by_token(&state.db, token)
        .await
        .map_err(|error| {
            log::error!("plugin invocation session lookup failed: {error}");
            api_error(StatusCode::INTERNAL_SERVER_ERROR, "database error")
        })?
        .filter(|session| session.user.is_active)
        .ok_or_else(|| api_error(StatusCode::UNAUTHORIZED, "invalid session"))?;
    let org_database = session
        .org_database_id
        .ok_or_else(|| api_error(StatusCode::FORBIDDEN, "an organization is required"))?;

    state.db.use_ns(&state.namespace).await.map_err(|error| {
        log::error!("plugin invocation namespace selection failed: {error}");
        api_error(StatusCode::INTERNAL_SERVER_ERROR, "database error")
    })?;
    state.db.use_db(&org_database).await.map_err(|error| {
        log::error!("plugin invocation organization selection failed: {error}");
        api_error(StatusCode::INTERNAL_SERVER_ERROR, "database error")
    })?;

    let query = state
        .db
        .query(
            "SELECT version, is_enabled FROM installed_plugins WHERE plugin_name = $plugin_name LIMIT 1;",
        )
        .bind(("plugin_name", plugin_name.clone()))
        .await
        .map_err(|error| {
            log::error!("plugin installation lookup failed: {error}");
            api_error(StatusCode::INTERNAL_SERVER_ERROR, "database error")
        })?;
    let mut response = query.check().map_err(|error| {
        log::error!("plugin installation query failed: {error}");
        api_error(StatusCode::INTERNAL_SERVER_ERROR, "database error")
    })?;
    let installed: Vec<InstalledPluginRow> = response.take(0).map_err(|error| {
        log::error!("plugin installation decode failed: {error}");
        api_error(StatusCode::INTERNAL_SERVER_ERROR, "database error")
    })?;
    let installed = installed
        .into_iter()
        .next()
        .ok_or_else(|| api_error(StatusCode::NOT_FOUND, "plugin is not installed"))?;
    let installed = InstalledPluginVersion {
        version: installed.version,
        is_enabled: installed.is_enabled,
    };
    if !installed.is_enabled {
        return Err(api_error(StatusCode::FORBIDDEN, "plugin is disabled"));
    }

    let loaded = state
        .plugin_runtime
        .ensure_loaded(
            &state.db,
            &state.namespace,
            &state.core_database,
            &plugin_name,
            &installed.version,
        )
        .await
        .map_err(|error| {
            log::error!("plugin version is not available: {error}");
            match error {
                crate::plugin_manager::runtime::PluginRuntimeError::NotInCatalog { .. } => {
                    api_error(StatusCode::NOT_FOUND, "plugin version is not in the catalog")
                }
                _ => api_error(
                    StatusCode::SERVICE_UNAVAILABLE,
                    "plugin version is unavailable",
                ),
            }
        })?;
    let definition = &loaded.manifest.plugin;
    let granted = definition
        .capabilities
        .iter()
        .cloned()
        .collect::<HashSet<_>>();
    let models = ModelGrant::map_from_access(&definition.access_models);
    let host = PluginHostContext::new(
        plugin_name.clone(),
        granted,
        models,
        state.db.clone(),
        state.namespace.clone(),
        org_database,
        state.notifications.clone(),
    );

    state
        .plugin_runtime
        .invoke(loaded, &function, payload, host)
        .await
        .map(Json)
        .map_err(|error| {
            log::error!("plugin invocation `{plugin_name}.{function}` failed: {error}");
            api_error(StatusCode::BAD_GATEWAY, "plugin invocation failed")
        })
}

fn api_error(status: StatusCode, message: &str) -> (StatusCode, Json<Value>) {
    (status, Json(json!({ "error": message })))
}
