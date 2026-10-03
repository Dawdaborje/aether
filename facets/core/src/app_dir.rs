//! On-disk layout of Aether's `app_dir`.
//!
//! ```text
//! <app_dir>/
//! ├── plugins/<name>/<version>/   plugin.toml, the WASM artifact, page XML
//! ├── views/<name>/<version>/     compiled page views (JSON)
//! ├── orgs/<organization>/        files belonging to one organization
//! │   └── conf/                   its configuration
//! └── conf/                       other per-plugin configuration
//! ```
//!
//! Plugin names and versions become path components, so they are validated
//! here and nowhere else builds these paths by hand.

use std::io;
use std::path::{Path, PathBuf};

use thiserror::Error;

const PLUGINS: &str = "plugins";
const VIEWS: &str = "views";
const CONF: &str = "conf";
const ORGS: &str = "orgs";

#[derive(Debug, Error)]
pub enum AppDirError {
    #[error("`{0}` cannot be used as a plugin name or version in a path")]
    InvalidComponent(String),

    #[error("cannot prepare app_dir directory {path}: {source}")]
    Io {
        path: PathBuf,
        #[source]
        source: io::Error,
    },
}

#[derive(Debug, Clone)]
pub struct AppDir {
    root: PathBuf,
}

impl AppDir {
    pub fn new(root: impl Into<PathBuf>) -> Self {
        Self { root: root.into() }
    }

    pub fn root(&self) -> &Path {
        &self.root
    }

    /// Create the root and its standard subdirectories if missing.
    pub async fn ensure(&self) -> Result<(), AppDirError> {
        for directory in [
            self.root.clone(),
            self.root.join(PLUGINS),
            self.root.join(VIEWS),
            self.root.join(CONF),
            self.root.join(ORGS),
        ] {
            tokio::fs::create_dir_all(&directory)
                .await
                .map_err(|source| AppDirError::Io {
                    path: directory,
                    source,
                })?;
        }
        Ok(())
    }

    /// `plugins/<name>/<version>`, relative to the app_dir root.
    pub fn plugin_relative_dir(name: &str, version: &str) -> Result<PathBuf, AppDirError> {
        Ok(Path::new(PLUGINS)
            .join(component(name)?)
            .join(component(version)?))
    }

    pub fn plugin_dir(&self, name: &str, version: &str) -> Result<PathBuf, AppDirError> {
        Ok(self.root.join(Self::plugin_relative_dir(name, version)?))
    }

    pub fn views_dir(&self, name: &str, version: &str) -> Result<PathBuf, AppDirError> {
        Ok(self
            .root
            .join(VIEWS)
            .join(component(name)?)
            .join(component(version)?))
    }

    pub fn conf_dir(&self) -> PathBuf {
        self.root.join(CONF)
    }

    /// `orgs/<organization>`: where one organization's files live. The name is
    /// an organization database name (letters, digits and `_`).
    pub fn org_dir(&self, organization: &str) -> Result<PathBuf, AppDirError> {
        let valid = !organization.is_empty()
            && organization
                .chars()
                .all(|c| c.is_ascii_alphanumeric() || c == '_');
        if !valid {
            return Err(AppDirError::InvalidComponent(organization.to_string()));
        }
        Ok(self.root.join(ORGS).join(organization))
    }

    /// Create `orgs/<organization>/` and its `conf/` folder (idempotent).
    pub async fn ensure_org(&self, organization: &str) -> Result<PathBuf, AppDirError> {
        let directory = self.org_dir(organization)?;
        let conf = directory.join(CONF);
        tokio::fs::create_dir_all(&conf)
            .await
            .map_err(|source| AppDirError::Io { path: conf, source })?;
        Ok(directory)
    }
}

/// A single safe path component: ASCII letters, digits and `._+-`, not `.`/`..`.
fn component(value: &str) -> Result<&str, AppDirError> {
    let valid = !value.is_empty()
        && value != "."
        && value != ".."
        && value
            .chars()
            .all(|c| c.is_ascii_alphanumeric() || matches!(c, '.' | '_' | '+' | '-'));
    if valid {
        Ok(value)
    } else {
        Err(AppDirError::InvalidComponent(value.to_string()))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn builds_versioned_plugin_paths() -> Result<(), AppDirError> {
        let app_dir = AppDir::new("/srv/aether");
        assert_eq!(
            app_dir.plugin_dir("partner", "0.1.0-beta+1")?,
            Path::new("/srv/aether/plugins/partner/0.1.0-beta+1")
        );
        assert_eq!(
            app_dir.views_dir("partner", "0.1.0")?,
            Path::new("/srv/aether/views/partner/0.1.0")
        );
        Ok(())
    }

    #[test]
    fn rejects_path_traversal_in_identity() {
        let app_dir = AppDir::new("/srv/aether");
        for bad in ["", ".", "..", "../x", "a/b", "a\\b", "sp ace"] {
            assert!(app_dir.plugin_dir(bad, "1.0.0").is_err(), "{bad:?}");
            assert!(app_dir.views_dir("ok", bad).is_err(), "{bad:?}");
        }
    }

    #[tokio::test]
    async fn ensure_creates_the_layout() -> Result<(), Box<dyn std::error::Error>> {
        let directory = tempfile::tempdir()?;
        let app_dir = AppDir::new(directory.path().join("app"));
        app_dir.ensure().await?;
        for sub in ["plugins", "views", "conf", "orgs"] {
            assert!(app_dir.root().join(sub).is_dir());
        }
        Ok(())
    }

    #[tokio::test]
    async fn organization_folders_are_created_and_validated() -> Result<(), Box<dyn std::error::Error>> {
        let directory = tempfile::tempdir()?;
        let app_dir = AppDir::new(directory.path());
        let created = app_dir.ensure_org("acme_2").await?;
        assert_eq!(created, directory.path().join("orgs/acme_2"));
        assert!(created.join("conf").is_dir());
        app_dir.ensure_org("acme_2").await?;
        for bad in ["", "..", "a/b", "a-b", "../x"] {
            assert!(app_dir.org_dir(bad).is_err(), "{bad:?}");
        }
        Ok(())
    }
}
