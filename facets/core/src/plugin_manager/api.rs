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
    message: String,
}

impl CallError {
    fn new(status: StatusCode, message: impl Into<String>) -> Self {
        Self { status, message: message.into() }
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

    let trail = vec![format!("{plugin_name}.{function}")];
    let result = run_call(&state, &mut identity, &plugin_name, &function, payload, trail.clone(), 0).await;
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
    trail: Vec<String>,
    event_depth: u32,
) -> Result<Value, CallError> {
    // The kernel's own jobs run with the plugin's full manifest, like a member; they are not visitors.
    let anonymous = !matches!(identity.actor, Actor::User(_) | Actor::System(_));

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
            use super::runtime::PluginRuntimeError as Failure;
            log::error!("plugin version is not available: {error}");
            match error.root() {
                Failure::NotInCatalog { .. } => CallError::new(
                    StatusCode::NOT_FOUND,
                    "plugin version is not in the catalog",
                ),
                Failure::MissingArtifact(_) => CallError::new(
                    StatusCode::NOT_FOUND,
                    "plugin has no callable functions",
                ),
                Failure::WasmTooLarge { .. } | Failure::ExceedsBudget { .. } => CallError::new(
                    StatusCode::UNPROCESSABLE_ENTITY,
                    "plugin is too large to load",
                ),
                Failure::CompileQueueFull | Failure::CompileTimeout { .. } | Failure::Busy => {
                    CallError::new(StatusCode::SERVICE_UNAVAILABLE, "plugin is busy, retry shortly")
                }
                _ => CallError::new(
                    StatusCode::SERVICE_UNAVAILABLE,
                    "plugin version is unavailable",
                ),
            }
        })?;

    let manifest = &loaded.manifest;
    let (granted, models) = if anonymous {
        let models = anonymous_grants(manifest, &loaded.schemas, &public_models);
        let readable = models.values().any(|grant| grant.can_read);
        (anonymous_capabilities(manifest, readable), models)
    } else {
        (
            manifest.plugin.capabilities.iter().cloned().collect(),
            user_grants(manifest, &loaded.schemas),
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
    )
    .with_event_depth(event_depth)
    .with_bridges(manifest.plugin.bridges.clone())
    .with_http_hosts(manifest.plugin.http_hosts.clone())
    .with_plugin_calls(
        manifest.plugin.dependencies.clone(),
        trail,
        std::sync::Arc::new(NestedCalls { state: state.clone(), identity: identity.clone(), event_depth }),
    );
    let host = match state.org_media(&identity.org_db) {
        Ok(media) => host.with_services(crate::kernel::HostServices {
            cache: state.cache.clone(),
            media,
            scheduler: Some(std::sync::Arc::new(crate::scheduler::AppScheduler { state: state.clone() })),
            files_root: crate::app_dir::AppDir::new(&state.config.app_dir).plugin_files_dir(&identity.org_db, plugin_name).ok(),
            bridges: Some(std::sync::Arc::new(crate::bridges::AppBridges { state: state.clone() })),
        }),
        Err(error) => {
            // The plugin still runs; only its storage commands fail, and say why.
            log::warn!("storage is unavailable for `{plugin_name}.{function}`: {error}");
            host
        }
    };

    state
        .plugin_runtime
        .invoke(loaded, function, payload, host)
        .await
        .map_err(|error| {
            log::error!("plugin invocation `{plugin_name}.{function}` failed: {error}");
            if let Some(message) = error.user_message() {
                // The plugin said, on purpose, what went wrong.
                return CallError::new(StatusCode::UNPROCESSABLE_ENTITY, message);
            }
            match error {
                super::runtime::PluginRuntimeError::Busy => {
                    CallError::new(StatusCode::SERVICE_UNAVAILABLE, "plugin is busy, retry shortly")
                }
                _ => CallError::new(StatusCode::BAD_GATEWAY, "plugin invocation failed"),
            }
        })
}

/// `plugins::call`: runs another plugin's function as the same actor as the request, through
/// the same checks as a call over HTTP (installed, enabled, public functions for visitors, the
/// target's own capabilities and model grants), and records it in the plugin call audit.
struct NestedCalls {
    state: AppState,
    identity: Identity,
    event_depth: u32,
}

#[async_trait::async_trait]
impl crate::kernel::PluginCaller for NestedCalls {
    async fn call(
        &self,
        plugin: &str,
        function: &str,
        payload: Value,
        trail: Vec<String>,
    ) -> Result<Value, crate::kernel::HostError> {
        let mut identity = self.identity.clone();
        let result = run_call(&self.state, &mut identity, plugin, function, payload, trail, self.event_depth).await;
        let status = match &result {
            Ok(_) => StatusCode::OK,
            Err(error) => error.status,
        };
        log::info!(
            "{} nested plugin call {plugin}.{function} in organization `{}`: {}",
            identity.audit.request_id,
            identity.org_db,
            status
        );
        match self.state.org(&identity.org_db).await {
            Ok(org) => {
                if let Err(error) =
                    record_plugin_call(&org, &identity.audit, plugin, function, status.as_u16()).await
                {
                    // The call already ran; the caller is told rather than left with an unrecorded one.
                    log::error!("nested plugin call audit failed: {error}");
                    return Err(crate::kernel::HostError::Message("audit unavailable".into()));
                }
            }
            Err(error) => {
                log::error!("nested plugin call audit failed: {error}");
                return Err(crate::kernel::HostError::Message("audit unavailable".into()));
            }
        }
        result.map_err(|error| crate::kernel::HostError::Message(error.message))
    }
}

/// A background job's call to a plugin function failed.
#[derive(Debug, Clone)]
pub struct SystemCallError {
    pub status: u16,
    pub message: String,
    /// Trying again cannot help (the plugin or function does not exist).
    pub permanent: bool,
}

/// Run `plugin.function` for the scheduler: in `org_db`, as the kernel's `system:scheduler`
/// actor, through the same checks as a call over HTTP (installed, enabled, the plugin's own
/// capabilities and model grants). Recorded in the plugin call audit under `request_id`.
pub async fn run_system_call(
    state: &AppState,
    org_db: &str,
    plugin: &str,
    function: &str,
    payload: Value,
    request_id: &str,
) -> Result<Value, SystemCallError> {
    run_system_call_as(state, org_db, "system:scheduler", plugin, function, payload, request_id).await
}

/// Like [`run_system_call`], recorded under another kernel actor such as `system:cli:ann`.
pub async fn run_system_call_as(
    state: &AppState,
    org_db: &str,
    actor_name: &str,
    plugin: &str,
    function: &str,
    payload: Value,
    request_id: &str,
) -> Result<Value, SystemCallError> {
    let actor = Actor::System(actor_name.to_string());
    let mut identity = Identity {
        org_db: org_db.to_string(),
        session: None,
        actor: actor.clone(),
        foreign_user: false,
        client_ip: None,
        audit: crate::access::audit::AuditContext {
            actor,
            request_id: request_id.to_string(),
            ip: None,
            user_agent: Some("kernel".into()),
        },
        new_visitor_token: None,
    };
    let trail = vec![format!("{plugin}.{function}")];
    // A handler of an event knows how many handlers came before it (see `plugin_events`).
    let event_depth = payload
        .get("event")
        .and_then(Value::as_str)
        .and_then(|_| payload.get("depth"))
        .and_then(Value::as_u64)
        .map_or(0, |depth| u32::try_from(depth).unwrap_or(u32::MAX));
    let result = run_call(state, &mut identity, plugin, function, payload, trail, event_depth).await;
    let status = match &result {
        Ok(_) => StatusCode::OK,
        Err(error) => error.status,
    };
    match state.org(org_db).await {
        Ok(org) => {
            if let Err(error) = record_plugin_call(&org, &identity.audit, plugin, function, status.as_u16()).await {
                log::error!("scheduled plugin call audit failed: {error}");
            }
        }
        Err(error) => log::error!("scheduled plugin call audit failed: {error}"),
    }
    result.map_err(|error| SystemCallError {
        status: error.status.as_u16(),
        permanent: error.status == StatusCode::NOT_FOUND || error.status == StatusCode::FORBIDDEN,
        message: error.message,
    })
}
