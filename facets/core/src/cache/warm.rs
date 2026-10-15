//! Kernel warm-set helpers (Tier A/B/C keys under `kernel:`).

use std::time::Duration;

use serde::{Deserialize, Serialize};
use serde_json::Value as JsonValue;

use super::Cache;
use super::error::CacheError;

const INSTALLED_KEY: &str = "installed_plugins";
const ORG_DATABASES_KEY: &str = "org_databases";
const CAP_CATALOG_KEY: &str = "cap_catalog";
const SETTINGS_CATALOG_KEY: &str = "settings:catalog";

/// Soft TTL for warm-set entries that also invalidate on write.
const WARM_TTL: Duration = Duration::from_secs(3600);

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct InstalledPluginWarm {
    pub plugin_name: String,
    pub version: String,
    pub is_enabled: bool,
}

impl Cache {
    pub fn warm_get_installed_plugins(
        &self,
        org: &str,
    ) -> Result<Option<Vec<InstalledPluginWarm>>, CacheError> {
        let Some(value) = self.kernel_org_get(org, INSTALLED_KEY)? else {
            return Ok(None);
        };
        let rows: Vec<InstalledPluginWarm> = serde_json::from_slice(value.as_slice())
            .map_err(|e| CacheError::Backend(format!("warm installed_plugins decode: {e}")))?;
        Ok(Some(rows))
    }

    pub fn warm_set_installed_plugins(
        &self,
        org: &str,
        rows: &[InstalledPluginWarm],
    ) -> Result<(), CacheError> {
        let bytes = serde_json::to_vec(rows)
            .map_err(|e| CacheError::Backend(format!("warm installed_plugins encode: {e}")))?;
        self.kernel_org_set(org, INSTALLED_KEY, bytes, Some(WARM_TTL))
    }

    pub fn warm_invalidate_installed_plugins(&self, org: &str) -> Result<bool, CacheError> {
        self.kernel_org_invalidate(org, INSTALLED_KEY)
    }

    pub fn warm_get_org_databases(&self) -> Result<Option<Vec<String>>, CacheError> {
        let Some(value) = self.kernel_get(ORG_DATABASES_KEY)? else {
            return Ok(None);
        };
        let rows: Vec<String> = serde_json::from_slice(value.as_slice())
            .map_err(|e| CacheError::Backend(format!("warm org_databases decode: {e}")))?;
        Ok(Some(rows))
    }

    pub fn warm_set_org_databases(&self, names: &[String]) -> Result<(), CacheError> {
        let bytes = serde_json::to_vec(names)
            .map_err(|e| CacheError::Backend(format!("warm org_databases encode: {e}")))?;
        self.kernel_set(ORG_DATABASES_KEY, bytes, Some(WARM_TTL))
    }

    pub fn warm_invalidate_org_databases(&self) -> Result<bool, CacheError> {
        self.kernel_invalidate(ORG_DATABASES_KEY)
    }

    pub fn warm_set_cap_catalog_present(&self) -> Result<(), CacheError> {
        self.kernel_set(CAP_CATALOG_KEY, br#""warm""#.as_slice(), None)
    }

    pub fn warm_cap_catalog_present(&self) -> bool {
        matches!(self.kernel_get(CAP_CATALOG_KEY), Ok(Some(_)))
    }

    pub fn warm_get_json(&self, key: &str) -> Result<Option<JsonValue>, CacheError> {
        let Some(value) = self.kernel_get(key)? else {
            return Ok(None);
        };
        let json = serde_json::from_slice(value.as_slice())
            .map_err(|e| CacheError::Backend(format!("warm json decode: {e}")))?;
        Ok(Some(json))
    }

    pub fn warm_set_json(
        &self,
        key: &str,
        value: &JsonValue,
        ttl: Option<Duration>,
    ) -> Result<(), CacheError> {
        let bytes = serde_json::to_vec(value)
            .map_err(|e| CacheError::Backend(format!("warm json encode: {e}")))?;
        self.kernel_set(key, bytes, ttl.or(Some(WARM_TTL)))
    }

    pub fn warm_memberships_key(user_id: &str) -> String {
        format!("user:{user_id}:memberships")
    }

    pub fn warm_roles_key(user_id: &str) -> String {
        format!("user:{user_id}:roles")
    }

    pub fn warm_setting_global_key(key: &str) -> String {
        format!("setting:g:{key}")
    }

    pub fn warm_setting_resolved_key(key: &str) -> String {
        format!("setting:r:{key}")
    }

    pub fn warm_settings_catalog_key() -> &'static str {
        SETTINGS_CATALOG_KEY
    }

    /// Tier B catalog surface: `plugin:{name}@{ver}:{kind}` (pages|components|i18n|public).
    pub fn warm_plugin_surface_key(name: &str, version: &str, kind: &str) -> String {
        format!("plugin:{name}@{version}:{kind}")
    }

    pub fn warm_get_plugin_json(
        &self,
        name: &str,
        version: &str,
        kind: &str,
    ) -> Result<Option<JsonValue>, CacheError> {
        let key = Self::warm_plugin_surface_key(name, version, kind);
        let Some(value) = self.kernel_get(&key)? else {
            return Ok(None);
        };
        let json = serde_json::from_slice(value.as_slice())
            .map_err(|e| CacheError::Backend(format!("warm plugin {kind} decode: {e}")))?;
        Ok(Some(json))
    }

    pub fn warm_set_plugin_json(
        &self,
        name: &str,
        version: &str,
        kind: &str,
        value: &JsonValue,
    ) -> Result<(), CacheError> {
        let key = Self::warm_plugin_surface_key(name, version, kind);
        let bytes = serde_json::to_vec(value)
            .map_err(|e| CacheError::Backend(format!("warm plugin {kind} encode: {e}")))?;
        self.kernel_set(&key, bytes, Some(WARM_TTL))
    }

    pub fn warm_invalidate_plugin_surface(
        &self,
        name: &str,
        version: &str,
    ) -> Result<(), CacheError> {
        for kind in ["pages", "components", "i18n", "public"] {
            let _ = self.kernel_invalidate(&Self::warm_plugin_surface_key(name, version, kind))?;
        }
        Ok(())
    }

    pub fn warm_theme_key() -> &'static str {
        "theme"
    }

    pub fn warm_apps_launcher_key() -> &'static str {
        "apps_launcher"
    }

    pub fn warm_get_theme_json(&self, org: &str) -> Result<Option<JsonValue>, CacheError> {
        let Some(value) = self.kernel_org_get(org, Self::warm_theme_key())? else {
            return Ok(None);
        };
        Ok(Some(serde_json::from_slice(value.as_slice()).map_err(
            |e| CacheError::Backend(format!("warm theme decode: {e}")),
        )?))
    }

    pub fn warm_set_theme_json(&self, org: &str, value: &JsonValue) -> Result<(), CacheError> {
        let bytes = serde_json::to_vec(value)
            .map_err(|e| CacheError::Backend(format!("warm theme encode: {e}")))?;
        self.kernel_org_set(org, Self::warm_theme_key(), bytes, Some(WARM_TTL))
    }

    pub fn warm_invalidate_theme(&self, org: &str) -> Result<bool, CacheError> {
        self.kernel_org_invalidate(org, Self::warm_theme_key())
    }

    pub fn warm_get_apps_launcher_json(&self, org: &str) -> Result<Option<JsonValue>, CacheError> {
        let Some(value) = self.kernel_org_get(org, Self::warm_apps_launcher_key())? else {
            return Ok(None);
        };
        Ok(Some(serde_json::from_slice(value.as_slice()).map_err(
            |e| CacheError::Backend(format!("warm apps_launcher decode: {e}")),
        )?))
    }

    pub fn warm_set_apps_launcher_json(
        &self,
        org: &str,
        value: &JsonValue,
    ) -> Result<(), CacheError> {
        let bytes = serde_json::to_vec(value)
            .map_err(|e| CacheError::Backend(format!("warm apps_launcher encode: {e}")))?;
        self.kernel_org_set(org, Self::warm_apps_launcher_key(), bytes, Some(WARM_TTL))
    }

    pub fn warm_invalidate_apps_launcher(&self, org: &str) -> Result<bool, CacheError> {
        self.kernel_org_invalidate(org, Self::warm_apps_launcher_key())
    }

    pub fn warm_get_roles(
        &self,
        org: &str,
        user_id: &str,
    ) -> Result<Option<Vec<String>>, CacheError> {
        let Some(value) = self.kernel_org_get(org, &Self::warm_roles_key(user_id))? else {
            return Ok(None);
        };
        let roles: Vec<String> = serde_json::from_slice(value.as_slice())
            .map_err(|e| CacheError::Backend(format!("warm roles decode: {e}")))?;
        Ok(Some(roles))
    }

    pub fn warm_set_roles(
        &self,
        org: &str,
        user_id: &str,
        roles: &[String],
    ) -> Result<(), CacheError> {
        let bytes = serde_json::to_vec(roles)
            .map_err(|e| CacheError::Backend(format!("warm roles encode: {e}")))?;
        self.kernel_org_set(org, &Self::warm_roles_key(user_id), bytes, Some(WARM_TTL))
    }

    pub fn warm_invalidate_roles(&self, org: &str, user_id: &str) -> Result<bool, CacheError> {
        self.kernel_org_invalidate(org, &Self::warm_roles_key(user_id))
    }

    pub fn warm_get_memberships(&self, user_id: &str) -> Result<Option<Vec<String>>, CacheError> {
        let Some(value) = self.kernel_get(&Self::warm_memberships_key(user_id))? else {
            return Ok(None);
        };
        let orgs: Vec<String> = serde_json::from_slice(value.as_slice())
            .map_err(|e| CacheError::Backend(format!("warm memberships decode: {e}")))?;
        Ok(Some(orgs))
    }

    pub fn warm_set_memberships(&self, user_id: &str, orgs: &[String]) -> Result<(), CacheError> {
        let bytes = serde_json::to_vec(orgs)
            .map_err(|e| CacheError::Backend(format!("warm memberships encode: {e}")))?;
        self.kernel_set(&Self::warm_memberships_key(user_id), bytes, Some(WARM_TTL))
    }

    pub fn warm_invalidate_memberships(&self, user_id: &str) -> Result<bool, CacheError> {
        self.kernel_invalidate(&Self::warm_memberships_key(user_id))
    }

    pub fn warm_get_setting_json(
        &self,
        org: Option<&str>,
        key: &str,
    ) -> Result<Option<JsonValue>, CacheError> {
        match org {
            Some(org) => {
                let Some(value) =
                    self.kernel_org_get(org, &Self::warm_setting_resolved_key(key))?
                else {
                    return Ok(None);
                };
                Ok(Some(serde_json::from_slice(value.as_slice()).map_err(
                    |e| CacheError::Backend(format!("warm setting decode: {e}")),
                )?))
            }
            None => {
                let Some(value) = self.kernel_get(&Self::warm_setting_global_key(key))? else {
                    return Ok(None);
                };
                Ok(Some(serde_json::from_slice(value.as_slice()).map_err(
                    |e| CacheError::Backend(format!("warm setting decode: {e}")),
                )?))
            }
        }
    }

    pub fn warm_set_setting_json(
        &self,
        org: Option<&str>,
        key: &str,
        value: &JsonValue,
    ) -> Result<(), CacheError> {
        let bytes = serde_json::to_vec(value)
            .map_err(|e| CacheError::Backend(format!("warm setting encode: {e}")))?;
        match org {
            Some(org) => self.kernel_org_set(
                org,
                &Self::warm_setting_resolved_key(key),
                bytes,
                Some(WARM_TTL),
            ),
            None => self.kernel_set(&Self::warm_setting_global_key(key), bytes, Some(WARM_TTL)),
        }
    }

    pub fn warm_invalidate_setting(&self, org: Option<&str>, key: &str) -> Result<(), CacheError> {
        match org {
            Some(org) => {
                let _ = self.kernel_org_invalidate(org, &Self::warm_setting_resolved_key(key))?;
            }
            None => {
                let _ = self.kernel_invalidate(&Self::warm_setting_global_key(key))?;
                // Global change may affect orgs that fall through to global.
                if let Ok(Some(orgs)) = self.warm_get_org_databases() {
                    for org in orgs {
                        let _ =
                            self.kernel_org_invalidate(&org, &Self::warm_setting_resolved_key(key));
                    }
                }
            }
        }
        let _ = self.kernel_invalidate(SETTINGS_CATALOG_KEY);
        if let Some(org) = org {
            let _ = self.kernel_org_invalidate(org, SETTINGS_CATALOG_KEY);
        }
        Ok(())
    }

    pub fn tier_presence(&self) -> TierPresence {
        TierPresence {
            a: vec![
                TierKeyStatus {
                    key: "kernel::cap_catalog".into(),
                    present: self.warm_cap_catalog_present(),
                },
                TierKeyStatus {
                    key: "kernel::org_databases".into(),
                    present: matches!(self.warm_get_org_databases(), Ok(Some(_))),
                },
                TierKeyStatus {
                    key: "kernel::session:*".into(),
                    present: self.mantle_snapshot().session_entries > 0,
                },
                TierKeyStatus {
                    key: "kernel:{org}::user:*:roles".into(),
                    present: self
                        .stats()
                        .rollups_snapshot()
                        .iter()
                        .any(|(k, r)| k.kind == "kernel" && r.entries > 0 && !k.org.is_empty()),
                },
                TierKeyStatus {
                    key: "kernel:{org}::installed_plugins".into(),
                    present: self
                        .stats()
                        .rollups_snapshot()
                        .iter()
                        .any(|(k, r)| k.kind == "kernel" && !k.org.is_empty() && r.entries > 0),
                },
            ],
            b: vec![
                TierKeyStatus {
                    key: "kernel::plugin:*@*:pages".into(),
                    present: self
                        .stats()
                        .rollups_snapshot()
                        .iter()
                        .any(|(k, r)| k.kind == "kernel" && k.org.is_empty() && r.entries > 0),
                },
                TierKeyStatus {
                    key: "kernel:{org}::theme".into(),
                    present: self
                        .stats()
                        .rollups_snapshot()
                        .iter()
                        .any(|(k, r)| k.kind == "kernel" && !k.org.is_empty() && r.entries > 0),
                },
                TierKeyStatus {
                    key: "kernel:{org}::apps_launcher".into(),
                    present: self
                        .stats()
                        .rollups_snapshot()
                        .iter()
                        .any(|(k, r)| k.kind == "kernel" && !k.org.is_empty() && r.entries > 0),
                },
            ],
            c: vec![TierKeyStatus {
                key: "kernel::settings:catalog".into(),
                present: matches!(self.kernel_get(SETTINGS_CATALOG_KEY), Ok(Some(_))),
            }],
        }
    }
}

#[derive(Debug, Clone, Serialize)]
pub struct TierKeyStatus {
    pub key: String,
    pub present: bool,
}

#[derive(Debug, Clone, Serialize)]
pub struct TierPresence {
    pub a: Vec<TierKeyStatus>,
    pub b: Vec<TierKeyStatus>,
    pub c: Vec<TierKeyStatus>,
}
