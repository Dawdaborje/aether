use std::collections::{HashMap, HashSet};

use serde::{Deserialize, Serialize};

use crate::access::audit::AuditContext;
use crate::notifications::NotificationHub;

/// Per-model access granted to a plugin (from `access_models` / plugin.toml).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ModelGrant {
    /// Logical model name plugins use (e.g. `partner`).
    pub name: String,
    /// Physical Surreal table (e.g. `base_partner`). Defaults to `name` when unset.
    pub table: String,
    pub can_read: bool,
    pub can_write: bool,
    /// How the model's fields are stored; none for a grant made without a model definition.
    #[serde(skip)]
    pub schema: Option<std::sync::Arc<crate::data_model::ModelSchema>>,
}

impl ModelGrant {
    pub fn from_access(name: &str, permissions: &[String], table: Option<&str>) -> Self {
        let perms: HashSet<_> = permissions.iter().map(|p| p.to_lowercase()).collect();
        Self {
            name: name.to_string(),
            table: table.unwrap_or(name).to_string(),
            can_read: perms.contains("read") || perms.is_empty(),
            can_write: perms.contains("write")
                || perms.contains("create")
                || perms.contains("update")
                || perms.contains("delete"),
            schema: None,
        }
    }

    /// Build the model allowlist from a plugin manifest's `access_models`.
    pub fn map_from_access(
        access: &[crate::plugin_manager::models::plugin_def::AccessModelDef],
    ) -> HashMap<String, ModelGrant> {
        access
            .iter()
            .map(|m| {
                let grant = Self::from_access(&m.name, &m.permissions, None);
                (m.name.clone(), grant)
            })
            .collect()
    }
}

/// Which SurrealDB namespace and database a plugin invocation works in
/// (always an organization's database).
#[derive(Debug, Clone)]
pub struct DbScope {
    pub namespace: String,
    pub database: String,
}

impl DbScope {
    pub fn new(namespace: impl Into<String>, database: impl Into<String>) -> Self {
        Self {
            namespace: namespace.into(),
            database: database.into(),
        }
    }
}

/// Who is calling which plugin function, for the audit trail.
#[derive(Debug, Clone)]
pub struct CallInfo {
    pub audit: AuditContext,
    pub function: String,
}

impl CallInfo {
    pub fn new(audit: AuditContext, function: impl Into<String>) -> Self {
        Self {
            audit,
            function: function.into(),
        }
    }
}

/// Runs another plugin's function for `plugins::call`. The kernel supplies it per call, already
/// holding the original caller's identity: the called plugin runs as that same actor, under its
/// own capabilities and model grants, so a call can never gain access the caller did not have.
#[async_trait::async_trait]
pub trait PluginCaller: Send + Sync {
    /// `trail` lists the `plugin.function` calls that led here, the current one last.
    async fn call(
        &self,
        plugin: &str,
        function: &str,
        payload: serde_json::Value,
        trail: Vec<String>,
    ) -> Result<serde_json::Value, super::error::HostError>;
}

/// Defaults for jobs, from the settings of the organization the job is for.
#[derive(Debug, Clone, Copy)]
pub struct JobDefaults {
    pub max_attempts: i64,
    pub backoff_secs: i64,
}

/// What `scheduler::*` and `communication::send` need from the rest of the kernel.
#[async_trait::async_trait]
pub trait SchedulerHandle: Send + Sync {
    /// Work was added to the organization's queue: make the scheduler look now.
    fn wake(&self, org: &str);
    /// A plugin's schedules changed.
    fn reload(&self);
    /// Attempts and retry delay for a job that does not say.
    async fn defaults(&self, org: &str) -> JobDefaults;
    /// Why a message of this type cannot be sent in this organization (no provider chosen, or
    /// its settings incomplete), or `Ok` when it can.
    async fn check_messaging(&self, org: &str, kind: &str) -> Result<(), String>;
}

/// Kernel services a plugin reaches through host commands. They are handed in already scoped
/// to the organization of the call, so a command never has to pick the scope itself.
#[derive(Clone)]
pub struct HostServices {
    /// The application cache. Keys are namespaced per organization and plugin by the commands.
    pub cache: crate::cache::Cache,
    /// The organization's media storage (every key already lives under `orgs/<organization>/`).
    pub media: std::sync::Arc<dyn aether_storage::MediaBackend>,
    /// The job scheduler; absent where background work is not available.
    pub scheduler: Option<std::sync::Arc<dyn SchedulerHandle>>,
}

/// Execution context for one plugin invocation.
#[derive(Clone)]
pub struct PluginHostContext {
    pub plugin_name: String,
    pub granted_capabilities: HashSet<String>,
    pub models: HashMap<String, ModelGrant>,
    pub namespace: String,
    pub database: String,
    /// The organization database's session (shared; never switched).
    pub db: crate::state::Db,
    pub notifications: NotificationHub,
    /// Who is calling and for which request; every database access is
    /// recorded against it.
    pub audit: AuditContext,
    /// The plugin function being run.
    pub function: String,
    /// Cache and storage; commands that need them fail with a clear error when absent.
    pub services: Option<HostServices>,
    /// Hosts `http::request` may call (`http_hosts` in plugin.toml). Empty: none.
    pub http_hosts: Vec<String>,
    /// Plugins this one may call with `plugins::call` (`dependencies` in plugin.toml).
    pub dependencies: Vec<String>,
    /// The `plugin.function` calls that led to this one, this call last.
    pub call_trail: Vec<String>,
    /// Runs other plugins' functions; absent when the call cannot make plugin calls.
    pub caller: Option<std::sync::Arc<dyn PluginCaller>>,
}

impl PluginHostContext {
    pub fn new(
        plugin_name: impl Into<String>,
        granted: HashSet<String>,
        models: HashMap<String, ModelGrant>,
        db: crate::state::Db,
        scope: DbScope,
        notifications: NotificationHub,
        call: CallInfo,
    ) -> Self {
        Self {
            plugin_name: plugin_name.into(),
            granted_capabilities: granted,
            models,
            namespace: scope.namespace,
            database: scope.database,
            db,
            notifications,
            audit: call.audit,
            function: call.function,
            services: None,
            http_hosts: Vec::new(),
            dependencies: Vec::new(),
            call_trail: Vec::new(),
            caller: None,
        }
    }

    /// Give the call access to the kernel's cache and storage.
    #[must_use]
    pub fn with_services(mut self, services: HostServices) -> Self {
        self.services = Some(services);
        self
    }

    /// Allow `http::request` to the hosts the plugin declared.
    #[must_use]
    pub fn with_http_hosts(mut self, hosts: Vec<String>) -> Self {
        self.http_hosts = hosts;
        self
    }

    /// Allow `plugins::call`: to the plugin's declared dependencies, through `caller`.
    #[must_use]
    pub fn with_plugin_calls(
        mut self,
        dependencies: Vec<String>,
        trail: Vec<String>,
        caller: std::sync::Arc<dyn PluginCaller>,
    ) -> Self {
        self.dependencies = dependencies;
        self.call_trail = trail;
        self.caller = Some(caller);
        self
    }

    pub fn services(&self) -> Result<&HostServices, super::error::HostError> {
        self.services
            .as_ref()
            .ok_or_else(|| super::error::HostError::Message("this call has no cache or storage attached".into()))
    }

    pub fn require_cap(
        &self,
        key: &str,
    ) -> Result<(), aether_security::capabilities::CapabilityError> {
        aether_security::capabilities::require_capability(&self.granted_capabilities, key)
    }

    pub fn model(&self, name: &str) -> Option<&ModelGrant> {
        self.models.get(name)
    }
}
