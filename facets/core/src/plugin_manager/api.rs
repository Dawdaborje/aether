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
            Identity, IdentityError, ensure_visitor, identify, with_retry_after,
            with_visitor_cookie,
        },
    },
    kernel::{CallInfo, DbScope, PluginHostContext},
    plugin_manager::installed,
    state::{AppState, Db},
};

#[derive(Debug, Deserialize, SurrealValue)]
struct InstalledPluginRow {
    #[serde(default)]
    plugin_name: String,
    version: String,
    is_enabled: bool,
}

async fn warm_installed_plugin(
    state: &AppState,
    org: &Db,
    org_db: &str,
    plugin_name: &str,
) -> Result<Option<InstalledPluginRow>, CallError> {
    let rows = installed::list_cached(&state.cache, org_db, org)
        .await
        .map_err(|error| CallError::database("installation lookup", error))?;
    Ok(rows
        .into_iter()
        .find(|row| row.plugin_name == plugin_name)
        .map(|row| InstalledPluginRow {
            plugin_name: row.plugin_name,
            version: row.version,
            is_enabled: row.is_enabled,
        }))
}

/// The plugins a plugin may call and read: its dependencies, then the optional ones.
fn all_dependencies(manifest: &crate::plugin_manager::models::plugin_def::PluginManifest) -> Vec<String> {
    let plugin = &manifest.plugin;
    let mut all = plugin.dependencies.clone();
    for name in &plugin.optional_dependencies {
        if !all.contains(name) {
            all.push(name.clone());
        }
    }
    all
}

/// Read-only grants for every model of each installed dependency, keyed `plugin.model`.
async fn attach_dependency_reads(
    state: &AppState,
    core: &Db,
    org: &Db,
    org_db: &str,
    dependencies: &[String],
    models: &mut std::collections::HashMap<String, crate::kernel::ModelGrant>,
) -> Result<(), CallError> {
    let read = ["read".to_string()];
    // `"*"` stands for every plugin installed in the organization (a reporting plugin's way of
    // reading whatever the user chooses). Those models are attached only when they have record
    // rules, so a model nobody wrote rules for is never opened up by a wildcard.
    let wildcard = dependencies.iter().any(|name| name == "*");
    let mut names: Vec<String> = dependencies.iter().filter(|name| *name != "*").cloned().collect();
    if wildcard
        && let Ok(rows) = installed::list_cached(&state.cache, org_db, org).await
    {
        for row in rows {
            if row.is_enabled && !names.contains(&row.plugin_name) {
                names.push(row.plugin_name);
            }
        }
    }
    for dependency in &names {
        let listed = dependencies.contains(dependency);
        let Some(installed) = warm_installed_plugin(state, org, org_db, dependency).await? else {
            continue;
        };
        if !installed.is_enabled {
            continue;
        }
        let loaded = match state
            .plugin_runtime
            .ensure_loaded(core, dependency, &installed.version)
            .await
        {
            Ok(loaded) => loaded,
            Err(error) => {
                log::warn!("dependency `{dependency}` is not loadable: {error}");
                continue;
            }
        };
        for (name, schema) in &loaded.schemas {
            if !listed && !loaded.rules.contains_key(name) {
                continue;
            }
            let key = format!("{dependency}.{name}");
            let mut grant =
                crate::kernel::ModelGrant::from_access(&key, &read, Some(schema.table.as_str()));
            grant.can_write = false;
            grant.schema = Some(schema.clone());
            grant.rules = loaded.rules.get(name).cloned();
            grant.direct = !listed;
            models.insert(key, grant);
        }
    }
    Ok(())
}

/// A failed call: the HTTP status and the message sent to the client.
struct CallError {
    status: StatusCode,
    message: String,
    /// What actually went wrong, for developers only (the client message stays generic).
    detail: Option<String>,
}

impl CallError {
    fn new(status: StatusCode, message: impl Into<String>) -> Self {
        Self {
            status,
            message: message.into(),
            detail: None,
        }
    }

    fn with_detail(mut self, detail: impl Into<String>) -> Self {
        self.detail = Some(detail.into());
        self
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

/// Run `plugin.function` as the person `identity` stands for, through the same checks as a call over HTTP
/// (installed, enabled, the plugin's capabilities and the person's roles and record rules), and record it in
/// the plugin call audit. For the kernel's AI assistant, which acts on a person's behalf: `Err` carries the
/// status and the message that is safe to show.
pub async fn run_as_caller(
    state: &AppState,
    identity: &mut Identity,
    plugin: &str,
    function: &str,
    payload: Value,
) -> Result<Value, (u16, String)> {
    let trail = vec![format!("{plugin}.{function}")];
    let result = run_call(state, identity, plugin, function, payload, trail, 0).await;
    let status = match &result {
        Ok(_) => StatusCode::OK,
        Err(error) => error.status,
    };
    match state.org(&identity.org_db).await {
        Ok(org) => {
            if let Err(error) = record_plugin_call(&org, &identity.audit, plugin, function, status.as_u16()).await {
                log::error!("assistant plugin call audit failed: {error}");
            }
        }
        Err(error) => log::error!("assistant plugin call audit failed: {error}"),
    }
    result.map_err(|error| (error.status.as_u16(), error.message))
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
    let result = run_call(
        &state,
        &mut identity,
        &plugin_name,
        &function,
        payload,
        trail.clone(),
        0,
    )
    .await;
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
        Err(error) => {
            let mut body = json!({ "error": error.message });
            // A developer is told which function failed and why; everyone else gets the
            // generic message only.
            let mut developer = identity
                .session
                .as_ref()
                .is_some_and(|session| session.user.is_super_user);
            if !developer
                && error.detail.is_some()
                && let crate::access::audit::Actor::User(user_id) = &identity.actor
                && let Ok(org) = state.org(&identity.org_db).await
                && let Ok(roles) =
                    crate::roles::roles_of_cached(&state.cache, &identity.org_db, &org, user_id)
                        .await
            {
                // Organization administrators manage the plugins, so they see why one failed.
                developer = roles.iter().any(|role| role == "org_admin");
            }
            if developer {
                body["function"] = json!(format!("{plugin_name}.{function}"));
                if let Some(detail) = &error.detail {
                    body["detail"] = json!(detail);
                }
            }
            (error.status, jar, Json(body)).into_response()
        }
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

    // Installed in this organization? Prefer Mantle warm-set, then DB.
    let org = state
        .org(&identity.org_db)
        .await
        .map_err(|error| CallError::database("organization selection", error))?;
    let core = state
        .core()
        .await
        .map_err(|error| CallError::database("core selection", error))?;
    state.cache.ensure_org_heat_loaded(&identity.org_db);
    let installed = match warm_installed_plugin(state, &org, &identity.org_db, plugin_name).await {
        Ok(row) => row,
        Err(error) => return Err(error),
    };

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
        let surface = crate::plugin_manager::surface::public_cached(
            &state.cache,
            &core,
            plugin_name,
            &installed.version,
        )
        .await
        .map_err(|error| CallError::database("public surface lookup", error))?;
        if !surface.public_functions.iter().any(|name| name == function) {
            return Err(refuse_anonymous(identity));
        }
        public_models = surface.public_models.into_iter().collect();

        // A visitor identity is issued only now that the call is permitted.
        ensure_visitor(state, identity)
            .await
            .map_err(|error| match error {
                IdentityError::RateLimited => {
                    CallError::new(StatusCode::TOO_MANY_REQUESTS, "too many requests")
                }
                other => CallError::database("visitor creation", other),
            })?;
    }

    let loaded = state
        .plugin_runtime
        .ensure_loaded(&core, plugin_name, &installed.version)
        .await
        .map_err(|error| {
            use super::runtime::PluginRuntimeError as Failure;
            log::error!("plugin version is not available: {error}");
            match error.root() {
                Failure::NotInCatalog { .. } => CallError::new(
                    StatusCode::NOT_FOUND,
                    "plugin version is not in the catalog",
                ),
                Failure::MissingArtifact(_) => {
                    CallError::new(StatusCode::NOT_FOUND, "plugin has no callable functions")
                }
                Failure::WasmTooLarge { .. } | Failure::ExceedsBudget { .. } => CallError::new(
                    StatusCode::UNPROCESSABLE_ENTITY,
                    "plugin is too large to load",
                ),
                Failure::CompileQueueFull | Failure::CompileTimeout { .. } | Failure::Busy => {
                    CallError::new(
                        StatusCode::SERVICE_UNAVAILABLE,
                        "plugin is busy, retry shortly",
                    )
                }
                _ => CallError::new(
                    StatusCode::SERVICE_UNAVAILABLE,
                    "plugin version is unavailable",
                ),
            }
        })?;

    let manifest = &loaded.manifest;
    let (granted, mut models) = if anonymous {
        let models = anonymous_grants(manifest, &loaded.schemas, &public_models);
        let readable = models.values().any(|grant| grant.can_read);
        (anonymous_capabilities(manifest, readable), models)
    } else {
        (
            manifest.plugin.capabilities.iter().cloned().collect(),
            user_grants(manifest, &loaded.schemas),
        )
    };
    for (name, grant) in models.iter_mut() {
        grant.rules = loaded.rules.get(name).cloned();
    }
    // A dependency's models are readable here as `plugin.model`, with that model's own rules.
    // A missing or disabled dependency is skipped; the report command refuses the model then.
    if let Err(error) = attach_dependency_reads(
        state,
        &core,
        &org,
        &identity.org_db,
        &all_dependencies(manifest),
        &mut models,
    )
    .await
    {
        log::warn!(
            "dependency reads for `{plugin_name}` were not attached: {}",
            error.message
        );
    }
    // The kernel's own jobs are not held to a person's rules, and neither are the small
    // functions (`rule_var_*`) that answer a rule's variables: they would need the rules they feed.
    let bypass_rules = matches!(identity.actor, Actor::System(_))
        || function.starts_with(crate::kernel::host::guard::VARIABLE_PREFIX);
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
    .with_rules(bypass_rules, identity.rule_cache.clone())
    .with_bridges(manifest.plugin.bridges.clone())
    .with_http_hosts(manifest.plugin.http_hosts.clone())
    .with_plugin_calls(
        all_dependencies(manifest),
        trail,
        std::sync::Arc::new(NestedCalls {
            state: state.clone(),
            identity: identity.clone(),
            event_depth,
        }),
    );
    let host = match state.org_media(&identity.org_db) {
        Ok(media) => {
            let core = state.core().await.ok();
            let sessions = state.clone();
            host.with_services(crate::kernel::HostServices {
                cache: state.cache.clone(),
                media,
                scheduler: Some(std::sync::Arc::new(crate::scheduler::AppScheduler {
                    state: state.clone(),
                })),
                files_root: crate::app_dir::AppDir::new(&state.config.app_dir)
                    .plugin_files_dir(&identity.org_db, plugin_name)
                    .ok(),
                bridges: Some(std::sync::Arc::new(crate::bridges::AppBridges {
                    state: state.clone(),
                })),
                core,
                public_base: crate::member_mail::public_base_for(&state, &identity.org_db).await,
                open_session: Some(std::sync::Arc::new(move || sessions.fresh_session())),
            })
        }
        Err(error) => {
            // The plugin still runs; only its storage commands fail, and say why.
            log::warn!("storage is unavailable for `{plugin_name}.{function}`: {error}");
            host
        }
    };

    // The built-in record actions (delete, archive) are the kernel's own; they run with this
    // plugin's grants and the model's rules, and are never available to visitors (a visitor may
    // only call functions the plugin lists as public).
    if !anonymous
        && let Some(result) =
            crate::plugin_manager::record_actions::run(&host, function, &payload).await
    {
        return result.map_err(|message| CallError::new(StatusCode::UNPROCESSABLE_ENTITY, message));
    }

    match state
        .plugin_runtime
        .invoke(loaded, function, payload, host)
        .await
    {
        Ok(value) => Ok(value),
        Err(error) => {
            log::error!("plugin invocation `{plugin_name}.{function}` failed: {error}");
            if let Some(message) = error.user_message() {
                // The plugin said, on purpose, what went wrong. A role it names by key
                // (`hr_documents.documents_admin`) is shown by its label.
                let message = name_roles_in(&org, &message.to_string()).await;
                return Err(CallError::new(StatusCode::UNPROCESSABLE_ENTITY, message));
            }
            Err(match error {
                super::runtime::PluginRuntimeError::Busy => CallError::new(
                    StatusCode::SERVICE_UNAVAILABLE,
                    "plugin is busy, retry shortly",
                ),
                _ => CallError::new(StatusCode::BAD_GATEWAY, "plugin invocation failed")
                    .with_detail(error.to_string()),
            })
        }
    }
}

/// Replace a role key written between backticks (`` `plugin.role` ``) by the role's label, so a
/// message says "Documents administrator" and not `hr_documents.documents_admin`. Anything that is
/// not a known role is left as it was.
async fn name_roles_in(org: &crate::state::Db, message: &str) -> String {
    if !message.contains('`') {
        return message.to_string();
    }
    let mut out = String::with_capacity(message.len());
    let mut rest = message;
    while let Some(start) = rest.find('`') {
        out.push_str(&rest[..start]);
        let after = &rest[start + 1..];
        let Some(end) = after.find('`') else {
            out.push_str(&rest[start..]);
            rest = "";
            break;
        };
        let token = &after[..end];
        let looks_like_role = token.contains('.')
            && token
                .chars()
                .all(|c| c.is_ascii_alphanumeric() || c == '_' || c == '.');
        let label = if looks_like_role {
            match org
                .query("SELECT VALUE label FROM roles WHERE name = $name LIMIT 1;")
                .bind(("name", token.to_string()))
                .await
                .and_then(|response| response.check())
            {
                Ok(mut response) => response
                    .take::<Vec<Option<String>>>(0)
                    .ok()
                    .and_then(|labels| labels.into_iter().next().flatten())
                    .filter(|label| !label.trim().is_empty()),
                Err(_) => None,
            }
        } else {
            None
        };
        match label {
            Some(label) => {
                out.push('\u{201c}');
                out.push_str(&label);
                out.push('\u{201d}');
            }
            None => {
                out.push('`');
                out.push_str(token);
                out.push('`');
            }
        }
        rest = &after[end + 1..];
    }
    out.push_str(rest);
    out
}

/// Run `plugin.function` for the person behind `identity`, through the same checks as a call over
/// HTTP, and return its answer. Used by the kernel to ask a plugin a question about the caller
/// (for example whether a menu entry applies to them). The call is not recorded as a request of
/// its own; the failure, if any, is the message to log.
pub(crate) async fn call_as_identity(
    state: &AppState,
    identity: &Identity,
    plugin: &str,
    function: &str,
    payload: Value,
) -> Result<Value, String> {
    let mut identity = identity.clone();
    run_call(state, &mut identity, plugin, function, payload, Vec::new(), 0)
        .await
        .map_err(|error| error.message)
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
        let result = run_call(
            &self.state,
            &mut identity,
            plugin,
            function,
            payload,
            trail,
            self.event_depth,
        )
        .await;
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
                    record_plugin_call(&org, &identity.audit, plugin, function, status.as_u16())
                        .await
                {
                    // The call already ran; the caller is told rather than left with an unrecorded one.
                    log::error!("nested plugin call audit failed: {error}");
                    return Err(crate::kernel::HostError::Message(
                        "audit unavailable".into(),
                    ));
                }
            }
            Err(error) => {
                log::error!("nested plugin call audit failed: {error}");
                return Err(crate::kernel::HostError::Message(
                    "audit unavailable".into(),
                ));
            }
        }
        result.map_err(|error| crate::kernel::HostError::Message(error.message))
    }

    async fn available(&self, plugin: &str) -> bool {
        let Ok(org) = self.state.org(&self.identity.org_db).await else {
            return false;
        };
        matches!(
            warm_installed_plugin(&self.state, &org, &self.identity.org_db, plugin).await,
            Ok(Some(installed)) if installed.is_enabled
        )
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
    run_system_call_as(
        state,
        org_db,
        "system:scheduler",
        plugin,
        function,
        payload,
        request_id,
    )
    .await
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
        rule_cache: Default::default(),
    };
    let trail = vec![format!("{plugin}.{function}")];
    // A handler of an event knows how many handlers came before it (see `plugin_events`).
    let event_depth = payload
        .get("event")
        .and_then(Value::as_str)
        .and_then(|_| payload.get("depth"))
        .and_then(Value::as_u64)
        .map_or(0, |depth| u32::try_from(depth).unwrap_or(u32::MAX));
    let result = run_call(
        state,
        &mut identity,
        plugin,
        function,
        payload,
        trail,
        event_depth,
    )
    .await;
    let status = match &result {
        Ok(_) => StatusCode::OK,
        Err(error) => error.status,
    };
    match state.org(org_db).await {
        Ok(org) => {
            if let Err(error) =
                record_plugin_call(&org, &identity.audit, plugin, function, status.as_u16()).await
            {
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
