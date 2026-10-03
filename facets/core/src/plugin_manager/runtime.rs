use std::{
    io,
    path::{Path, PathBuf},
    sync::Arc,
    time::Duration,
};

use extism::{CompiledPlugin, Manifest, PluginBuilder, Wasm};
use moka::sync::Cache as CompiledCache;
use serde::Deserialize;
use surrealdb::{Surreal, engine::remote::ws::Client};
use surrealdb::types::SurrealValue;
use thiserror::Error;
use tokio::sync::Mutex;

use super::catalog::sha256_hex;
use crate::app_dir::{AppDir, AppDirError};
use super::models::{
    plugin_db_def::PluginDbDefinition,
    plugin_def::{ManifestError, PluginManifest},
};
use super::services::fetch_active_plugin_version;

/// Default number of Extism `CompiledPlugin` modules kept in RAM.
/// Instances (`Plugin`) are never cached — they are created per call.
pub const DEFAULT_MAX_COMPILED_PLUGINS: u64 = 8;

#[derive(Debug, Error)]
pub enum PluginRuntimeError {
    #[error("plugin database error: {0}")]
    Database(#[from] surrealdb::Error),
    #[error("plugin filesystem error: {0}")]
    Io(#[from] io::Error),
    #[error("plugin manifest error at {path}: {source}")]
    Manifest {
        path: PathBuf,
        #[source]
        source: ManifestError,
    },
    #[error("failed to compile plugin `{name}@{version}`: {source}")]
    Compile {
        name: String,
        version: String,
        #[source]
        source: extism::Error,
    },
    #[error("plugin `{0}` has no artifact_path")]
    MissingArtifact(String),
    #[error("plugin artifact path is outside app_dir: {0}")]
    ArtifactOutsideAppDir(PathBuf),
    #[error("plugin artifact hash mismatch for `{name}@{version}`")]
    ArtifactHashMismatch { name: String, version: String },
    #[error("plugin manifest identity does not match catalog record for `{name}@{version}`")]
    ManifestIdentityMismatch { name: String, version: String },
    #[error("plugin `{name}@{version}` is not in the compiled-module cache")]
    NotLoaded { name: String, version: String },
    #[error("plugin `{name}@{version}` is not an active catalog version")]
    NotInCatalog { name: String, version: String },
    #[error("plugin compilation worker failed: {0}")]
    Worker(String),
    #[error("plugin export `{function}` is missing from `{name}@{version}`")]
    FunctionNotFound {
        name: String,
        version: String,
        function: String,
    },
    #[error("plugin invocation failed: {0}")]
    Invoke(#[source] extism::Error),
    #[error(transparent)]
    AppDir(#[from] AppDirError),
}

pub struct LoadedPlugin {
    pub manifest: PluginManifest,
    pub compiled: Arc<CompiledPlugin>,
}

#[derive(Clone)]
pub struct PluginRuntime {
    app_dir: Arc<PathBuf>,
    /// Extism compile cache. Values are `CompiledPlugin` wrappers; live WASM
    /// instances are created with `Plugin::new_from_compiled` per invocation.
    compiled: CompiledCache<(String, String), Arc<LoadedPlugin>>,
    /// Serialize WASM compilation so a burst of new plugins cannot spike RAM.
    compile_gate: Arc<Mutex<()>>,
    max_compiled: u64,
}

impl PluginRuntime {
    pub fn new(app_dir: PathBuf) -> Self {
        Self::with_capacity(app_dir, DEFAULT_MAX_COMPILED_PLUGINS)
    }

    pub fn with_capacity(app_dir: PathBuf, max_compiled: u64) -> Self {
        let max_compiled = max_compiled.max(1);
        Self {
            app_dir: Arc::new(app_dir),
            compiled: CompiledCache::builder()
                .max_capacity(max_compiled)
                .build(),
            compile_gate: Arc::new(Mutex::new(())),
            max_compiled,
        }
    }

    pub fn app_dir(&self) -> &Path {
        self.app_dir.as_path()
    }

    pub fn max_compiled(&self) -> u64 {
        self.max_compiled
    }

    fn cache_key(name: &str, version: &str) -> (String, String) {
        (name.to_string(), version.to_string())
    }

    /// Ensure `app_dir` exists and report how many catalog versions are active.
    /// Does **not** compile WASM — modules are compiled on first use and kept
    /// in a bounded Extism `CompiledPlugin` cache.
    pub async fn load_catalog(&self, db: &Surreal<Client>) -> Result<usize, PluginRuntimeError> {
        AppDir::new(self.app_dir.as_path()).ensure().await?;
        let mut response = db
            .query("SELECT name, version FROM plugins WHERE is_active = true;")
            .await?
            .check()?;
        let records: Vec<CatalogVersionRow> = response.take(0)?;
        let count = records.len();
        log::info!(
            "Plugin catalog has {count} active version(s); compiling on demand (max {} compiled modules in RAM)",
            self.max_compiled
        );
        Ok(count)
    }

    /// Compile a catalog row and insert it into the bounded cache (may evict LRU).
    /// In-flight calls keep their `Arc<LoadedPlugin>` until they finish.
    pub async fn register_version(
        &self,
        record: PluginDbDefinition,
    ) -> Result<Arc<LoadedPlugin>, PluginRuntimeError> {
        let _gate = self.compile_gate.lock().await;
        Ok(self.insert_compiled(self.compile_record(record).await?))
    }

    pub fn get(&self, name: &str, version: &str) -> Result<Arc<LoadedPlugin>, PluginRuntimeError> {
        self.compiled
            .get(&Self::cache_key(name, version))
            .ok_or_else(|| PluginRuntimeError::NotLoaded {
                name: name.to_string(),
                version: version.to_string(),
            })
    }

    /// Return a cached `CompiledPlugin`, or load the version from the core
    /// `plugins` table, compile it, and cache it. `db` must be the core session.
    pub async fn ensure_loaded(
        &self,
        db: &Surreal<Client>,
        name: &str,
        version: &str,
    ) -> Result<Arc<LoadedPlugin>, PluginRuntimeError> {
        if let Some(loaded) = self.compiled.get(&Self::cache_key(name, version)) {
            return Ok(loaded);
        }

        let _gate = self.compile_gate.lock().await;
        if let Some(loaded) = self.compiled.get(&Self::cache_key(name, version)) {
            return Ok(loaded);
        }

        let record = fetch_active_plugin_version(db, name, version)
            .await?
            .ok_or_else(|| PluginRuntimeError::NotInCatalog {
                name: name.to_string(),
                version: version.to_string(),
            })?;

        Ok(self.insert_compiled(self.compile_record(record).await?))
    }

    fn insert_compiled(&self, plugin: LoadedPlugin) -> Arc<LoadedPlugin> {
        let key = Self::cache_key(&plugin.manifest.plugin.name, &plugin.manifest.plugin.version);
        let plugin = Arc::new(plugin);
        self.compiled.insert(key, plugin.clone());
        plugin
    }

    pub async fn invoke(
        &self,
        loaded: Arc<LoadedPlugin>,
        function: &str,
        payload: serde_json::Value,
        host: crate::kernel::PluginHostContext,
    ) -> Result<serde_json::Value, PluginRuntimeError> {
        let name = loaded.manifest.plugin.name.clone();
        let version = loaded.manifest.plugin.version.clone();
        let function = function.to_string();
        let input = serde_json::to_string(&payload)
            .map_err(|error| PluginRuntimeError::Worker(error.to_string()))?;
        let runtime_handle = tokio::runtime::Handle::current();

        let output = tokio::task::spawn_blocking(move || {
            let mut plugin = extism::Plugin::new_from_compiled(&loaded.compiled)
                .map_err(PluginRuntimeError::Invoke)?;
            if !plugin.function_exists(&function) {
                return Err(PluginRuntimeError::FunctionNotFound {
                    name,
                    version,
                    function,
                });
            }
            plugin
                .call_with_host_context::<&str, String, PluginCallContext>(
                    &function,
                    &input,
                    PluginCallContext {
                        host,
                        runtime: runtime_handle,
                    },
                )
                .map_err(PluginRuntimeError::Invoke)
        })
        .await
        .map_err(|error| PluginRuntimeError::Worker(error.to_string()))??;

        Ok(serde_json::from_str(&output).unwrap_or_else(|_| serde_json::Value::String(output)))
    }

    async fn compile_record(
        &self,
        record: PluginDbDefinition,
    ) -> Result<LoadedPlugin, PluginRuntimeError> {
        let artifact = record
            .artifact_path
            .as_deref()
            .filter(|path| !path.is_empty())
            .ok_or_else(|| PluginRuntimeError::MissingArtifact(record.name.clone()))?;
        let artifact_path = self.resolve_app_path(artifact).await?;
        let bytes = tokio::fs::read(&artifact_path).await?;
        if let Some(expected_hash) = record.artifact_hash.as_deref() {
            let expected_hash = expected_hash
                .strip_prefix("sha256:")
                .unwrap_or(expected_hash);
            let actual_hash = sha256_hex(&bytes);
            if !actual_hash.eq_ignore_ascii_case(expected_hash) {
                return Err(PluginRuntimeError::ArtifactHashMismatch {
                    name: record.name,
                    version: record.version,
                });
            }
        }

        let manifest_path = artifact_path
            .parent()
            .unwrap_or_else(|| Path::new("."))
            .join("plugin.toml");
        let manifest_text = tokio::fs::read_to_string(&manifest_path).await?;
        let manifest = PluginManifest::parse(&manifest_text).map_err(|source| {
            PluginRuntimeError::Manifest {
                path: manifest_path.clone(),
                source,
            }
        })?;
        if manifest.plugin.name != record.name || manifest.plugin.version != record.version {
            return Err(PluginRuntimeError::ManifestIdentityMismatch {
                name: record.name,
                version: record.version,
            });
        }

        let name = manifest.plugin.name.clone();
        let version = manifest.plugin.version.clone();
        let compiled = tokio::task::spawn_blocking(move || {
            PluginBuilder::new(
                Manifest::new([Wasm::data(bytes)])
                    .disallow_all_hosts()
                    .with_memory_max(256)
                    .with_timeout(Duration::from_secs(10)),
            )
            .with_wasi(true)
            .with_fuel_limit(50_000_000)
            .with_function_in_namespace(
                "aether",
                "command",
                [extism::ValType::I64],
                [extism::ValType::I64],
                extism::UserData::default(),
                kernel_command_host_function,
            )
            .compile()
            .map_err(|source| PluginRuntimeError::Compile {
                name,
                version,
                source,
            })
        })
        .await
        .map_err(|error| PluginRuntimeError::Worker(error.to_string()))??;

        Ok(LoadedPlugin {
            manifest,
            compiled: Arc::new(compiled),
        })
    }

    async fn resolve_app_path(&self, raw_path: &str) -> Result<PathBuf, PluginRuntimeError> {
        let app_dir = tokio::fs::canonicalize(self.app_dir.as_path()).await?;
        let input = PathBuf::from(raw_path);
        let candidate = if input.is_absolute() {
            input
        } else {
            app_dir.join(input)
        };
        let canonical = tokio::fs::canonicalize(candidate).await?;
        if !canonical.starts_with(&app_dir) {
            return Err(PluginRuntimeError::ArtifactOutsideAppDir(canonical));
        }
        Ok(canonical)
    }
}

#[derive(Debug, Deserialize, SurrealValue)]
struct CatalogVersionRow {
    name: String,
    version: String,
}

#[derive(Debug, serde::Deserialize)]
struct KernelCommandRequest {
    command: String,
    #[serde(default)]
    payload: serde_json::Value,
}

pub struct PluginCallContext {
    pub host: crate::kernel::PluginHostContext,
    pub runtime: tokio::runtime::Handle,
}

fn kernel_command_host_function(
    plugin: &mut extism::CurrentPlugin,
    inputs: &[extism::Val],
    outputs: &mut [extism::Val],
    _user_data: extism::UserData<()>,
) -> Result<(), extism::Error> {
    let request_text = match inputs.first() {
        Some(input) => plugin.memory_get_val::<String>(input)?,
        None => String::new(),
    };
    let context = plugin.host_context::<PluginCallContext>()?;
    let result = match serde_json::from_str::<KernelCommandRequest>(&request_text) {
        Ok(request) => context
            .runtime
            .block_on(crate::kernel::kernel_command(
                &context.host,
                &request.command,
                request.payload,
            ))
            .unwrap_or_else(|error| error.to_json()),
        Err(error) => {
            serde_json::json!({ "error": format!("invalid kernel command request: {error}") })
        }
    };
    if let Some(output) = outputs.first_mut() {
        plugin.memory_set_val(output, result.to_string())?;
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn artifact_paths_cannot_escape_app_dir() -> Result<(), Box<dyn std::error::Error>> {
        let directory = tempfile::tempdir()?;
        let app_dir = directory.path().join("app_dir");
        tokio::fs::create_dir_all(&app_dir).await?;
        tokio::fs::write(directory.path().join("outside.wasm"), b"wasm").await?;

        let runtime = PluginRuntime::new(app_dir);
        let result = runtime.resolve_app_path("../outside.wasm").await;
        assert!(matches!(
            result,
            Err(PluginRuntimeError::ArtifactOutsideAppDir(_))
        ));
        Ok(())
    }

    #[tokio::test]
    async fn artifact_paths_resolve_inside_app_dir() -> Result<(), Box<dyn std::error::Error>> {
        let directory = tempfile::tempdir()?;
        let app_dir = directory.path().join("app_dir");
        let plugin_dir = app_dir.join("plugins/sample/1.0.0");
        tokio::fs::create_dir_all(&plugin_dir).await?;
        tokio::fs::write(plugin_dir.join("plugin.wasm"), b"wasm").await?;

        let runtime = PluginRuntime::new(app_dir.clone());
        let resolved = runtime
            .resolve_app_path("plugins/sample/1.0.0/plugin.wasm")
            .await?;
        assert!(resolved.starts_with(app_dir.canonicalize()?));
        Ok(())
    }
}
