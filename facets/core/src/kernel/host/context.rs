use std::collections::{HashMap, HashSet};

use serde::{Deserialize, Serialize};
use surrealdb::{Surreal, engine::remote::ws::Client};

/// Per-model access granted to a plugin (from `access_models` / plugin.toml).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ModelGrant {
    /// Logical model name plugins use (e.g. `partner`).
    pub name: String,
    /// Physical Surreal table (e.g. `base_partner`). Defaults to `name` when unset.
    pub table: String,
    pub can_read: bool,
    pub can_write: bool,
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

/// Execution context for one plugin invocation.
#[derive(Clone)]
pub struct PluginHostContext {
    pub plugin_name: String,
    pub granted_capabilities: HashSet<String>,
    pub models: HashMap<String, ModelGrant>,
    pub namespace: String,
    pub database: String,
    pub db: Surreal<Client>,
}

impl PluginHostContext {
    pub fn new(
        plugin_name: impl Into<String>,
        granted: HashSet<String>,
        models: HashMap<String, ModelGrant>,
        db: Surreal<Client>,
        namespace: impl Into<String>,
        database: impl Into<String>,
    ) -> Self {
        Self {
            plugin_name: plugin_name.into(),
            granted_capabilities: granted,
            models,
            namespace: namespace.into(),
            database: database.into(),
            db,
        }
    }

    pub async fn use_scoped_db(&self) -> Result<(), surrealdb::Error> {
        self.db.use_ns(&self.namespace).await?;
        self.db.use_db(&self.database).await?;
        Ok(())
    }

    pub fn require_cap(&self, key: &str) -> Result<(), aether_security::capabilities::CapabilityError> {
        aether_security::capabilities::require_capability(&self.granted_capabilities, key)
    }

    pub fn model(&self, name: &str) -> Option<&ModelGrant> {
        self.models.get(name)
    }
}
