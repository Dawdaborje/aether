use serde::{Deserialize, Serialize};

/// A namespaced group of capabilities (e.g. `db`, `storage`, `email`).
/// Full capability keys are `{id}::{capability.id}` → `db::query`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct CapabilityGroup {
    pub id: String,
    pub name: String,
    pub description: Option<String>,
    pub capabilities: Vec<Capability>,
}

/// A single capability inside a group.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Capability {
    pub id: String,
    pub name: String,
    pub description: Option<String>,
}

impl CapabilityGroup {
    /// Full key for a capability in this group, e.g. `db::query`.
    pub fn key_for(&self, capability_id: &str) -> String {
        format!("{}::{}", self.id, capability_id)
    }

    /// All full keys declared by this group.
    pub fn keys(&self) -> Vec<String> {
        self.capabilities
            .iter()
            .map(|c| self.key_for(&c.id))
            .collect()
    }

    pub fn find(&self, capability_id: &str) -> Option<&Capability> {
        self.capabilities.iter().find(|c| c.id == capability_id)
    }
}

impl Capability {
    pub fn full_key(group_id: &str, capability_id: &str) -> String {
        format!("{group_id}::{capability_id}")
    }
}

/// Parse a full capability key into `(group, capability)`.
/// Returns `None` if the key is not `group::capability`.
pub fn parse_capability_key(key: &str) -> Option<(&str, &str)> {
    let (group, capability) = key.split_once("::")?;
    if group.is_empty() || capability.is_empty() || capability.contains("::") {
        return None;
    }
    Some((group, capability))
}
