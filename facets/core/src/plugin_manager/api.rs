use std::{collections::BTreeSet, net::SocketAddr};

use axum::{
    Extension, Json,
    extract::{ConnectInfo, Path, State},
    http::{HeaderMap, StatusCode, Uri},
    response::{IntoResponse, Response},
};
use axum_extra::extract::CookieJar;
use serde::Deserialize;
use serde_json::{Value, json};
use surrealdb::types::SurrealValue;

use super::access::{anonymous_capabilities, anonymous_grants, user_grants};
use crate::{
    access::{
        audit::{Actor, record_plugin_call},
        identity::{
            Identity, IdentityError, ensure_visitor, identify, with_retry_after, with_visitor_cookie,
        },
    },
    kernel::{CallInfo, DbScope, PluginHostContext},
    state::AppState,
};

#[derive(Debug, Deserialize, SurrealValue)]
struct InstalledPluginRow {
    version: String,
    is_enabled: bool,
}

#[derive(Debug, Deserialize, SurrealValue)]
struct PublicSurfaceRow {
    public_functions: Vec<String>,
}

#[derive(Debug, Deserialize, SurrealValue)]
struct PublicPageModels {
    models: Vec<String>,
}

/// A failed call: the HTTP status and the message sent to the client.
struct CallError {
    status: StatusCode,
    message: &'static str,
}

impl CallError {
    fn new(status: StatusCode, message: &'static str) -> Self {
        Self { status, message }
    }

    fn database(context: &str, error: impl std::fmt::Display) -> Self {
        log::error!("plugin invocation {context}: {error}");
        Self::new(StatusCode::INTERNAL_SERVER_ERROR, "database error")
    }
}

/// `POST /api/plugins/{plugin}/{function}`.
///
/// Logged-in users may call any exported function of a plugin installed in
/// their organization. Anonymous visitors may call only the functions the
/// plugin lists in `public_functions`, with the reduced access described in
/// [`super::access`]. Every call is recorded, including refused ones.
pub async fn invoke_plugin(
    State(state): State<AppState>,
    peer: Option<Extension<ConnectInfo<SocketAddr>>>,
    Path((plugin_name, function)): Path<(String, String)>,
    headers: HeaderMap,
    uri: Uri,
    jar: CookieJar,
    Json(payload): Json<Value>,
) -> Response {
    let call = Call {
        plugin_name,
        function,
        payload,
    };
    with_retry_after(invoke(state, peer, call, headers, uri, jar).await)
}

/// What a request asks a plugin to do.
struct Call {
    plugin_name: String,
    function: String,
    payload: Value,
}

async fn invoke(
    state: AppState,
    peer: Option<Extension<ConnectInfo<SocketAddr>>>,
    call: Call,
    headers: HeaderMap,
    uri: Uri,
    jar: CookieJar,
) -> Response {
    let Call {
        plugin_name,
        function,
        payload,
    } = call;
    let peer_ip = peer.map(|Extension(ConnectInfo(address))| address.ip());
    let mut identity = match identify(&state, &headers, peer_ip, &uri).await {
        Ok(identity) => identity,
        Err(error) => return identity_failure(&jar, error),
    };

    let result = run_call(&state, &mut identity, &plugin_name, &function, payload).await;
    let status = match &result {
        Ok(_) => StatusCode::OK,
        Err(error) => error.status,
    };
    log::info!(
        "{} plugin call {plugin_name}.{function} in organization `{}` by {}: {}",
        identity.audit.request_id,
        identity.org_db,
        identity.actor.kind(),
        match &result {
            Ok(_) => "ok".to_string(),
            Err(error) => format!("refused or failed ({}): {}", error.status, error.message),
        }
    );

    // The audit write is part of the request: if it fails, so does the call.
    let recorded = async {
        let org = state.org(&identity.org_db).await?;
        record_plugin_call(
            &org,
            &identity.audit,
            &plugin_name,
            &function,
            status.as_u16(),
        )
        .await
        .map_err(|error| surrealdb::Error::internal(error.to_string()))
    }
    .await;
    if let Err(error) = recorded {
        log::error!("plugin call audit failed: {error}");
        return (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(json!({ "error": "audit unavailable" })),
        )
            .into_response();
    }

    let jar = with_visitor_cookie(&state, &identity, jar);
    match result {
        Ok(value) => (jar, Json(value)).into_response(),
        Err(error) => (error.status, jar, Json(json!({ "error": error.message }))).into_response(),
    }
}

fn identity_failure(jar: &CookieJar, error: IdentityError) -> Response {
    if matches!(error, IdentityError::Database(_) | IdentityError::Audit(_)) {
        log::error!("plugin invocation identity: {error}");
    }
    let (status, message) = error.http();
    (status, jar.clone(), Json(json!({ "error": message }))).into_response()
}

/// 403 for a logged-in user of another organization, 401 for everyone else,
/// so the web app only asks people who could succeed to log in.
fn refuse_anonymous(identity: &Identity) -> CallError {
    if identity.foreign_user {
        CallError::new(StatusCode::FORBIDDEN, "not a member of this organization")
    } else {
        CallError::new(StatusCode::UNAUTHORIZED, "not authenticated")
    }
}

async fn run_call(
    state: &AppState,
    identity: &mut Identity,
    plugin_name: &str,
    function: &str,
    payload: Value,
) -> Result<Value, CallError> {
    let anonymous = !matches!(identity.actor, Actor::User(_));

    // Installed in this organization?
    let org = state
        .org(&identity.org_db)
        .await
        .map_err(|error| CallError::database("organization selection", error))?;
    let core = state
        .core()
        .await
        .map_err(|error| CallError::database("core selection", error))?;
    let mut response = org
        .query("SELECT version, is_enabled FROM installed_plugins WHERE plugin_name = $plugin_name LIMIT 1;")
        .bind(("plugin_name", plugin_name.to_string()))
        .await
        .and_then(|response| response.check())
        .map_err(|error| CallError::database("installation lookup", error))?;
    let installed: Vec<InstalledPluginRow> = response
        .take(0)
        .map_err(|error| CallError::database("installation decode", error))?;
    let installed = installed.into_iter().next();

    // Anonymous callers learn nothing about what is installed or private.
    let Some(installed) = installed else {
        return Err(if anonymous {
            refuse_anonymous(identity)
        } else {
            CallError::new(StatusCode::NOT_FOUND, "plugin is not installed")
        });
    };
    if !installed.is_enabled {
        return Err(CallError::new(StatusCode::FORBIDDEN, "plugin is disabled"));
    }

    // Anonymous visitors: the function must be declared public. Checked from
    // the catalog before any WASM is compiled.
    let mut public_models = BTreeSet::new();
    if anonymous {
        let mut response = core
            .query("SELECT public_functions FROM plugins WHERE name = $name AND version = $version LIMIT 1;")
            .bind(("name", plugin_name.to_string()))
            .bind(("version", installed.version.clone()))
            .await
            .and_then(|response| response.check())
            .map_err(|error| CallError::database("public surface lookup", error))?;
        let surface: Vec<PublicSurfaceRow> = response
            .take(0)
            .map_err(|error| CallError::database("public surface decode", error))?;
        let allowed = surface
            .first()
            .is_some_and(|row| row.public_functions.iter().any(|name| name == function));
        if !allowed {
            return Err(refuse_anonymous(identity));
        }

        let mut response = core
            .query(
                "SELECT models FROM plugin_ui_pages \
                 WHERE is_public = true AND plugin.name = $name AND plugin.version = $version;",
            )
            .bind(("name", plugin_name.to_string()))
            .bind(("version", installed.version.clone()))
            .await
            .and_then(|response| response.check())
            .map_err(|error| CallError::database("public pages lookup", error))?;
        let pages: Vec<PublicPageModels> = response
            .take(0)
            .map_err(|error| CallError::database("public pages decode", error))?;
        public_models = pages.into_iter().flat_map(|page| page.models).collect();

        // A visitor identity is issued only now that the call is permitted.
        ensure_visitor(state, identity).await.map_err(|error| match error {
            IdentityError::RateLimited => {
                CallError::new(StatusCode::TOO_MANY_REQUESTS, "too many requests")
            }
            other => CallError::database("visitor creation", other),
        })?;
    }

    let loaded = state
        .plugin_runtime
        .ensure_loaded(
            &core,
            plugin_name,
            &installed.version,
        )
        .await
        .map_err(|error| {
            log::error!("plugin version is not available: {error}");
            match error {
                super::runtime::PluginRuntimeError::NotInCatalog { .. } => CallError::new(
                    StatusCode::NOT_FOUND,
                    "plugin version is not in the catalog",
                ),
                super::runtime::PluginRuntimeError::MissingArtifact(_) => CallError::new(
                    StatusCode::NOT_FOUND,
                    "plugin has no callable functions",
                ),
                _ => CallError::new(
                    StatusCode::SERVICE_UNAVAILABLE,
                    "plugin version is unavailable",
                ),
            }
        })?;

    let manifest = &loaded.manifest;
    let (granted, models) = if anonymous {
        let models = anonymous_grants(manifest, &public_models);
        let readable = models.values().any(|grant| grant.can_read);
        (anonymous_capabilities(manifest, readable), models)
    } else {
        (
            manifest.plugin.capabilities.iter().cloned().collect(),
            user_grants(manifest),
        )
    };
    let host = PluginHostContext::new(
        plugin_name,
        granted,
        models,
        org.clone(),
        DbScope::new(state.namespace.clone(), identity.org_db.clone()),
        state.notifications.clone(),
        CallInfo::new(identity.audit.clone(), function),
    );

    state
        .plugin_runtime
        .invoke(loaded, function, payload, host)
        .await
        .map_err(|error| {
            log::error!("plugin invocation `{plugin_name}.{function}` failed: {error}");
            CallError::new(StatusCode::BAD_GATEWAY, "plugin invocation failed")
        })
}
