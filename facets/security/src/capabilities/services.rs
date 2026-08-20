use std::collections::{HashMap, HashSet};
use std::fs;
use std::path::{Path, PathBuf};

use thiserror::Error;

use super::models::{parse_capability_key, CapabilityGroup};

#[derive(Debug, Error)]
pub enum CapabilityError {
    #[error("capabilities directory not found: {0}")]
    DirectoryNotFound(PathBuf),

    #[error("failed to read {path}: {source}")]
    Io {
        path: PathBuf,
        #[source]
        source: std::io::Error,
    },

    #[error("failed to parse {path}: {source}")]
    Parse {
        path: PathBuf,
        #[source]
        source: serde_json::Error,
    },

    #[error("duplicate capability group id `{0}`")]
    DuplicateGroup(String),

    #[error("invalid capability key `{0}` (expected group::capability)")]
    InvalidKey(String),

    #[error("unknown capability `{0}`")]
    UnknownCapability(String),

    #[error("capability `{0}` is not granted to this plugin")]
    Denied(String),
}

/// In-memory catalog of all capability groups loaded from `capabilities/`.
#[derive(Debug, Clone, Default)]
pub struct CapabilityCatalog {
    groups: HashMap<String, CapabilityGroup>,
    /// Fast lookup of every valid `group::capability` key.
    keys: HashSet<String>,
}

impl CapabilityCatalog {
    pub fn new() -> Self {
        Self::default()
    }

    /// Load every `*.json` file in `dir` as a [`CapabilityGroup`].
    pub fn load_from_dir(dir: impl AsRef<Path>) -> Result<Self, CapabilityError> {
        let dir = dir.as_ref();
        if !dir.is_dir() {
            return Err(CapabilityError::DirectoryNotFound(dir.to_path_buf()));
        }

        let mut catalog = Self::new();

        let entries = fs::read_dir(dir).map_err(|source| CapabilityError::Io {
            path: dir.to_path_buf(),
            source,
        })?;

        for entry in entries {
            let entry = entry.map_err(|source| CapabilityError::Io {
                path: dir.to_path_buf(),
                source,
            })?;
            let path = entry.path();
            if path.extension().and_then(|e| e.to_str()) != Some("json") {
                continue;
            }
            catalog.insert_file(&path)?;
        }

        Ok(catalog)
    }

    fn insert_file(&mut self, path: &Path) -> Result<(), CapabilityError> {
        let raw = fs::read_to_string(path).map_err(|source| CapabilityError::Io {
            path: path.to_path_buf(),
            source,
        })?;
        let group: CapabilityGroup =
            serde_json::from_str(&raw).map_err(|source| CapabilityError::Parse {
                path: path.to_path_buf(),
                source,
            })?;

        if self.groups.contains_key(&group.id) {
            return Err(CapabilityError::DuplicateGroup(group.id.clone()));
        }

        for key in group.keys() {
            self.keys.insert(key);
        }
        self.groups.insert(group.id.clone(), group);
        Ok(())
    }

    pub fn groups(&self) -> impl Iterator<Item = &CapabilityGroup> {
        self.groups.values()
    }

    pub fn get_group(&self, id: &str) -> Option<&CapabilityGroup> {
        self.groups.get(id)
    }

    pub fn contains(&self, key: &str) -> bool {
        self.keys.contains(key)
    }

    pub fn all_keys(&self) -> impl Iterator<Item = &String> {
        self.keys.iter()
    }

    /// Returns `Ok(())` if `key` is a known catalog entry.
    pub fn validate_key(&self, key: &str) -> Result<(), CapabilityError> {
        if parse_capability_key(key).is_none() {
            return Err(CapabilityError::InvalidKey(key.to_string()));
        }
        if !self.contains(key) {
            return Err(CapabilityError::UnknownCapability(key.to_string()));
        }
        Ok(())
    }

    /// Validate a plugin's declared required/optional capability lists.
    pub fn validate_declared(
        &self,
        required: &[String],
        optional: &[String],
    ) -> Result<(), CapabilityError> {
        for key in required.iter().chain(optional.iter()) {
            self.validate_key(key)?;
        }
        Ok(())
    }
}

/// Check whether `granted` includes `required`.
/// Does not hit the DB — use when you already loaded the plugin's granted set.
pub fn plugin_has_capability(granted: &HashSet<String>, required: &str) -> bool {
    granted.contains(required)
}

/// Assert a plugin may use `required`, returning an error if not.
pub fn require_capability(
    granted: &HashSet<String>,
    required: &str,
) -> Result<(), CapabilityError> {
    if plugin_has_capability(granted, required) {
        Ok(())
    } else {
        Err(CapabilityError::Denied(required.to_string()))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::PathBuf;

    fn repo_capabilities_dir() -> PathBuf {
        PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../capabilities")
    }

    #[test]
    fn loads_repo_capabilities() {
        let catalog = CapabilityCatalog::load_from_dir(repo_capabilities_dir())
            .expect("load capabilities/");
        assert!(catalog.contains("db::query"));
        assert!(catalog.contains("db::mutate"));
        assert!(catalog.contains("storage::read"));
        assert!(catalog.contains("email::send"));
        assert!(catalog.contains("sms::send"));
        assert!(catalog.contains("events::emit"));
        assert!(catalog.contains("plugins::call"));
        assert!(catalog.contains("http::request"));
        assert!(catalog.contains("bridge::call"));
        assert!(catalog.contains("scheduler::register"));
        assert!(catalog.contains("cache::get"));
        assert!(catalog.contains("cache::set"));
        assert!(catalog.contains("cache::invalidate"));
        assert!(catalog.contains("cache::clear"));
        assert!(catalog.contains("db::surql"));
        assert!(!catalog.contains("db::write"));
        assert!(!catalog.contains("fs::read"));
    }

    #[test]
    fn validate_declared_rejects_unknown() {
        let catalog = CapabilityCatalog::load_from_dir(repo_capabilities_dir()).unwrap();
        let err = catalog
            .validate_declared(&["db::query".into()], &["nope::thing".into()])
            .unwrap_err();
        assert!(matches!(err, CapabilityError::UnknownCapability(_)));
    }

    #[test]
    fn parse_key() {
        assert_eq!(parse_capability_key("db::query"), Some(("db", "query")));
        assert!(parse_capability_key("db").is_none());
        assert!(parse_capability_key("db::").is_none());
    }
}
