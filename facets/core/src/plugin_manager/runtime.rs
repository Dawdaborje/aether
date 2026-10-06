use std::{
    io,
    path::{Path, PathBuf},
    sync::{
        Arc,
        atomic::{AtomicU32, AtomicU64, Ordering},
    },
    time::{Duration, Instant},
};

use extism::{CompiledPlugin, Manifest, PluginBuilder, Wasm};
use moka::{future::Cache as CompiledCache, notification::RemovalCause, policy::EvictionPolicy};
use serde::{Deserialize, Serialize};
use surrealdb::{Surreal, engine::remote::ws::Client};
use surrealdb::types::SurrealValue;
use thiserror::Error;
use tokio::sync::{OwnedSemaphorePermit, Semaphore};

use super::catalog::sha256_hex;
use crate::app_dir::{AppDir, AppDirError};
use crate::config_manager::models::PluginRuntimeConfig;
use super::models::{
    plugin_db_def::PluginDbDefinition,
    plugin_def::{ManifestError, PluginManifest},
};
use super::services::fetch_active_plugin_version;

const BYTES_PER_MB: f64 = 1024.0 * 1024.0;

/// A plugin's error that starts with this is meant for the caller (`Error::msg` in the SDK);
/// any other plugin error is internal and is only logged.
pub const USER_ERROR_PREFIX: &str = "aether:user-error:";

/// Longest plugin-written message passed on to a caller.
const MAX_USER_ERROR_CHARS: usize = 300;

impl PluginRuntimeError {
    /// The message the plugin wrote for the caller, when it wrote one.
    pub fn user_message(&self) -> Option<String> {
        let text = match self {
            Self::Invoke(error) => error.to_string(),
            Self::Script { source: super::script::ScriptError::User(message), .. } => {
                format!("{USER_ERROR_PREFIX}{message}")
            }
            _ => return None,
        };
        let message = text.split_once(USER_ERROR_PREFIX)?.1;
        Some(
            message
                .chars()
                .filter(|character| !character.is_control())
                .take(MAX_USER_ERROR_CHARS)
                .collect(),
        )
    }
}

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
    #[error("plugin `{name}`: its rules are wrong: {problems}")]
    Rules { name: String, problems: String },
    #[error("plugin `{name}@{version}` is not an active catalog version")]
    NotInCatalog { name: String, version: String },
    #[error(
        "plugin `{name}@{version}` is {size_mb:.1} MB, over `plugin_runtime.max_wasm_size_mb` ({max_mb} MB)"
    )]
    WasmTooLarge {
        name: String,
        version: String,
        size_mb: f64,
        max_mb: u64,
    },
    #[error(
        "plugin `{name}@{version}` would take about {estimated_mb:.0} MB compiled, more than the whole `plugin_runtime.max_compiled_memory_mb` budget ({budget_mb} MB); it is not loaded"
    )]
    ExceedsBudget {
        name: String,
        version: String,
        estimated_mb: f64,
        budget_mb: u64,
    },
    #[error("too many plugins are waiting to be compiled; retry shortly")]
    CompileQueueFull,
    #[error("plugin `{name}@{version}` was not ready within {secs}s")]
    CompileTimeout {
        name: String,
        version: String,
        secs: u64,
    },
    #[error("too many plugin calls are running; retry shortly")]
    Busy,
    #[error("could not prepare the compile cache at {path}: {source}")]
    CompileCache {
        path: PathBuf,
        #[source]
        source: io::Error,
    },
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
    #[error("plugin `{name}@{version}`: {source}")]
    Script {
        name: String,
        version: String,
        #[source]
        source: super::script::ScriptError,
    },
    #[error(transparent)]
    AppDir(#[from] AppDirError),
    #[error(transparent)]
    Revision(#[from] super::revisions::RevisionError),
    #[error(transparent)]
    Models(#[from] crate::data_model::ModelFileError),
    /// The same failure, shared by every caller that was waiting on one load.
    #[error(transparent)]
    Shared(Arc<PluginRuntimeError>),
}

impl PluginRuntimeError {
    /// The underlying error, looking through [`Self::Shared`].
    pub fn root(&self) -> &PluginRuntimeError {
        match self {
            Self::Shared(inner) => inner.root(),
            other => other,
        }
    }
}

/// What a loaded plugin runs: a compiled WebAssembly module or a checked Rhai or Lua script.
pub enum Executable {
    Wasm(Arc<CompiledPlugin>),
    Script(Arc<super::script::ScriptProgram>),
}

pub struct LoadedPlugin {
    pub manifest: PluginManifest,
    /// The plugin's models, from its `models/` files.
    pub models: Vec<crate::data_model::ModelDef>,
    /// How each model's fields are stored, by model name.
    pub schemas: std::collections::HashMap<String, Arc<crate::data_model::ModelSchema>>,
    /// Who may see and change which records and fields, by model name (`rules/*.json`).
    pub rules: std::collections::HashMap<String, Arc<crate::data_model::RuleSet>>,
    pub compiled: Executable,
    /// What this plugin is estimated to hold in memory while it is kept ready.
    pub estimated_mb: f64,
    pub wasm_bytes: u64,
}

/// Counters for the compiled-plugin cache.
#[derive(Default)]
struct Counters {
    hits: AtomicU64,
    loads: AtomicU64,
    evictions: AtomicU64,
    refused: AtomicU64,
    queue_full: AtomicU64,
    timeouts: AtomicU64,
    compile_millis: AtomicU64,
    waiting: AtomicU32,
}

/// A snapshot of the cache, for the developer view and for tuning the budget.
#[derive(Debug, Clone, Serialize)]
pub struct RuntimeStats {
    pub budget_mb: u64,
    /// Estimated memory held by the plugins kept ready.
    pub resident_mb: f64,
    pub plugins_ready: u64,
    pub hits: u64,
    /// Plugins loaded (compiled, or read from the disk cache) because they were not ready.
    pub loads: u64,
    /// Plugins pushed out to make room. Many of these, with the same plugins loading
    /// again and again, means the budget is too small.
    pub evictions: u64,
    pub refused: u64,
    pub queue_full: u64,
    pub timeouts: u64,
    pub average_load_millis: u64,
    pub waiting_to_compile: u32,
    pub disk_cache: bool,
}

struct Inner {
    app_dir: PathBuf,
    config: PluginRuntimeConfig,
    /// Compiled modules, weighted by estimated size in KB and evicted least recently
    /// used first. Values are `CompiledPlugin` wrappers; live WASM instances are created
    /// with `Plugin::new_from_compiled` per invocation and never cached.
    compiled: CompiledCache<(String, String), Arc<LoadedPlugin>>,
    compile_permits: Arc<Semaphore>,
    calls: Arc<Semaphore>,
    /// wasmtime's on-disk cache config, when the disk tier is on.
    disk_cache_config: Option<PathBuf>,
    counters: Arc<Counters>,
}

#[derive(Clone)]
pub struct PluginRuntime {
    inner: Arc<Inner>,
}

/// Decrements the waiting count when a caller stops waiting for a compile turn.
struct Waiting<'a>(&'a AtomicU32);

impl Drop for Waiting<'_> {
    fn drop(&mut self) {
        self.0.fetch_sub(1, Ordering::SeqCst);
    }
}

/// The weight of a plugin in the cache, in KB.
fn weight_kb(estimated_mb: f64) -> u32 {
    (estimated_mb * 1024.0).ceil().clamp(1.0, f64::from(u32::MAX)) as u32
}

impl PluginRuntime {
    /// Build the runtime. With the disk tier on, this prepares its directory and the
    /// wasmtime cache config under `app_dir`, so a location that cannot be written fails
    /// at start-up, not at the first call.
    pub fn new(app_dir: PathBuf, config: PluginRuntimeConfig) -> Result<Self, PluginRuntimeError> {
        let disk_cache_config = if config.compile_cache.enabled {
            Some(prepare_disk_cache(&app_dir, &config)?)
        } else {
            None
        };
        let counters = Arc::new(Counters::default());
        let listener_counters = counters.clone();
        let compiled = CompiledCache::builder()
            .max_capacity(config.max_compiled_memory_mb * 1024)
            .weigher(|_key: &(String, String), plugin: &Arc<LoadedPlugin>| weight_kb(plugin.estimated_mb))
            // Strict least-recently-used: the default admission policy could refuse a
            // freshly compiled plugin to protect an older one, which looks like a plugin
            // that compiles on every call.
            .eviction_policy(EvictionPolicy::lru())
            .eviction_listener(move |key: Arc<(String, String)>, plugin: Arc<LoadedPlugin>, cause| {
                if cause == RemovalCause::Size {
                    listener_counters.evictions.fetch_add(1, Ordering::Relaxed);
                    log::info!(
                        "Plugin `{}@{}` (about {:.0} MB) left memory to make room",
                        key.0,
                        key.1,
                        plugin.estimated_mb
                    );
                }
            })
            .build();
        Ok(Self {
            inner: Arc::new(Inner {
                app_dir,
                compile_permits: Arc::new(Semaphore::new(config.max_concurrent_compiles as usize)),
                calls: Arc::new(Semaphore::new(config.max_concurrent_calls as usize)),
                config,
                compiled,
                disk_cache_config,
                counters,
            }),
        })
    }

    pub fn app_dir(&self) -> &Path {
        self.inner.app_dir.as_path()
    }

    pub fn config(&self) -> &PluginRuntimeConfig {
        &self.inner.config
    }

    fn cache_key(name: &str, version: &str) -> (String, String) {
        (name.to_string(), version.to_string())
    }

    /// The cache's state now.
    pub async fn stats(&self) -> RuntimeStats {
        self.inner.compiled.run_pending_tasks().await;
        let counters = &self.inner.counters;
        let loads = counters.loads.load(Ordering::Relaxed);
        RuntimeStats {
            budget_mb: self.inner.config.max_compiled_memory_mb,
            resident_mb: self.inner.compiled.weighted_size() as f64 / 1024.0,
            plugins_ready: self.inner.compiled.entry_count(),
            hits: counters.hits.load(Ordering::Relaxed),
            loads,
            evictions: counters.evictions.load(Ordering::Relaxed),
            refused: counters.refused.load(Ordering::Relaxed),
            queue_full: counters.queue_full.load(Ordering::Relaxed),
            timeouts: counters.timeouts.load(Ordering::Relaxed),
            average_load_millis: counters
                .compile_millis
                .load(Ordering::Relaxed)
                .checked_div(loads)
                .unwrap_or(0),
            waiting_to_compile: counters.waiting.load(Ordering::Relaxed),
            disk_cache: self.inner.disk_cache_config.is_some(),
        }
    }

    /// Ensure `app_dir` exists and report how many catalog versions are active.
    /// Does **not** compile WASM: modules are compiled on first use and kept in the
    /// memory-bounded cache.
    pub async fn load_catalog(&self, db: &Surreal<Client>) -> Result<usize, PluginRuntimeError> {
        AppDir::new(self.inner.app_dir.as_path()).ensure().await?;
        let mut response = db
            .query("SELECT name, version FROM plugins WHERE is_active = true;")
            .await?
            .check()?;
        let records: Vec<CatalogVersionRow> = response.take(0)?;
        let count = records.len();
        let config = &self.inner.config;
        log::info!(
            "Plugin catalog has {count} active version(s); compiled on demand \
             (memory budget {} MB, compile cache on disk: {})",
            config.max_compiled_memory_mb,
            if self.inner.disk_cache_config.is_some() {
                format!("{} MB", config.compile_cache.max_size_mb)
            } else {
                "off".to_string()
            }
        );
        Ok(count)
    }

    /// Return the compiled plugin, loading it first when it is not ready. `db` must be
    /// the core session.
    ///
    /// Callers asking for the same plugin at the same time share one load. A plugin
    /// whose estimated size cannot fit the memory budget is refused, not run uncached.
    pub async fn ensure_loaded(
        &self,
        db: &Surreal<Client>,
        name: &str,
        version: &str,
    ) -> Result<Arc<LoadedPlugin>, PluginRuntimeError> {
        self.ensure_loaded_with(name, version, async {
            fetch_active_plugin_version(db, name, version)
                .await
                .map_err(PluginRuntimeError::from)
        })
        .await
    }

    /// [`Self::ensure_loaded`] with the catalog lookup supplied, which only runs when the
    /// plugin is not already ready.
    pub async fn ensure_loaded_with<F>(
        &self,
        name: &str,
        version: &str,
        fetch: F,
    ) -> Result<Arc<LoadedPlugin>, PluginRuntimeError>
    where
        F: std::future::Future<Output = Result<Option<PluginDbDefinition>, PluginRuntimeError>>,
    {
        let key = Self::cache_key(name, version);
        if let Some(loaded) = self.inner.compiled.get(&key).await {
            self.inner.counters.hits.fetch_add(1, Ordering::Relaxed);
            return Ok(loaded);
        }
        let loaded = self
            .inner
            .compiled
            .try_get_with(key, self.load(name, version, fetch))
            .await
            .map_err(PluginRuntimeError::Shared)?;
        // moka applies inserts and evictions lazily. Loads are rare and expensive, so do
        // it now: the new plugin counts as used (a hit on it right away would otherwise be
        // lost) and what it pushed out is freed at once.
        self.inner.compiled.run_pending_tasks().await;
        Ok(loaded)
    }

    async fn load<F>(
        &self,
        name: &str,
        version: &str,
        fetch: F,
    ) -> Result<Arc<LoadedPlugin>, PluginRuntimeError>
    where
        F: std::future::Future<Output = Result<Option<PluginDbDefinition>, PluginRuntimeError>>,
    {
        let record = fetch.await?.ok_or_else(|| PluginRuntimeError::NotInCatalog {
            name: name.to_string(),
            version: version.to_string(),
        })?;
        let prepared = self.prepare(record).await.inspect_err(|error| {
            if matches!(
                error,
                PluginRuntimeError::WasmTooLarge { .. } | PluginRuntimeError::ExceedsBudget { .. }
            ) {
                self.inner.counters.refused.fetch_add(1, Ordering::Relaxed);
                log::warn!("{error}");
            }
        })?;

        let started = Instant::now();
        let permit = self.acquire_compile_turn(name, version).await?;
        let loaded = self.compile(prepared, permit).await?;
        let counters = &self.inner.counters;
        counters.loads.fetch_add(1, Ordering::Relaxed);
        counters
            .compile_millis
            .fetch_add(started.elapsed().as_millis() as u64, Ordering::Relaxed);
        log::info!(
            "Plugin `{name}@{version}` ready in {} ms (about {:.1} MB in memory)",
            started.elapsed().as_millis(),
            loaded.estimated_mb
        );
        Ok(Arc::new(loaded))
    }

    /// Check the artifact against the limits before spending any memory on it.
    async fn prepare(&self, record: PluginDbDefinition) -> Result<Prepared, PluginRuntimeError> {
        let config = &self.inner.config;
        let artifact = record
            .artifact_path
            .as_deref()
            .filter(|path| !path.is_empty())
            .ok_or_else(|| PluginRuntimeError::MissingArtifact(record.name.clone()))?;
        let artifact_path = self.resolve_app_path(artifact).await?;
        let wasm_bytes = tokio::fs::metadata(&artifact_path).await?.len();

        if super::script::ScriptKind::of_path(&artifact_path).is_some() {
            if wasm_bytes > super::script::MAX_SCRIPT_BYTES {
                return Err(PluginRuntimeError::Script {
                    name: record.name,
                    version: record.version,
                    source: super::script::ScriptError::Failed(format!(
                        "the script is {wasm_bytes} bytes, over the {} byte limit",
                        super::script::MAX_SCRIPT_BYTES
                    )),
                });
            }
            // A script is text held as a syntax tree: small, with no engine of its own.
            let estimated_mb = (wasm_bytes as f64 / BYTES_PER_MB * 50.0).max(0.1);
            return Ok(Prepared { record, artifact_path, wasm_bytes, estimated_mb });
        }

        let size_mb = wasm_bytes as f64 / BYTES_PER_MB;
        if size_mb > config.max_wasm_size_mb as f64 {
            return Err(PluginRuntimeError::WasmTooLarge {
                name: record.name,
                version: record.version,
                size_mb,
                max_mb: config.max_wasm_size_mb,
            });
        }
        let estimated_mb = config.estimated_compiled_mb(wasm_bytes);
        if estimated_mb > config.max_compiled_memory_mb as f64 {
            return Err(PluginRuntimeError::ExceedsBudget {
                name: record.name,
                version: record.version,
                estimated_mb,
                budget_mb: config.max_compiled_memory_mb,
            });
        }
        Ok(Prepared {
            record,
            artifact_path,
            wasm_bytes,
            estimated_mb,
        })
    }

    /// Wait for one of the compile turns, unless too many are already waiting.
    async fn acquire_compile_turn(
        &self,
        name: &str,
        version: &str,
    ) -> Result<OwnedSemaphorePermit, PluginRuntimeError> {
        let counters = &self.inner.counters;
        let config = &self.inner.config;
        if counters.waiting.fetch_add(1, Ordering::SeqCst) >= config.compile_queue_limit {
            counters.waiting.fetch_sub(1, Ordering::SeqCst);
            counters.queue_full.fetch_add(1, Ordering::Relaxed);
            return Err(PluginRuntimeError::CompileQueueFull);
        }
        let _waiting = Waiting(&counters.waiting);
        let secs = config.compile_timeout_secs;
        match tokio::time::timeout(
            Duration::from_secs(secs),
            self.inner.compile_permits.clone().acquire_owned(),
        )
        .await
        {
            Ok(Ok(permit)) => Ok(permit),
            Ok(Err(_)) => Err(PluginRuntimeError::Worker("compile queue closed".into())),
            Err(_) => {
                counters.timeouts.fetch_add(1, Ordering::Relaxed);
                Err(PluginRuntimeError::CompileTimeout {
                    name: name.to_string(),
                    version: version.to_string(),
                    secs,
                })
            }
        }
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

        // Every running instance holds memory, so only so many run at once; the rest
        // wait briefly, then are told to retry. A call made by another plugin's function
        // (`plugins::call`, more than the one step in the trail) runs under the permit of the
        // request that started the chain: the caller is blocked until it answers, so a permit of
        // its own could never be had once every permit is held by a caller waiting for one.
        let permit = if host.call_trail.len() > 1 {
            None
        } else {
            Some(
                tokio::time::timeout(
                    Duration::from_secs(self.inner.config.compile_timeout_secs.min(10)),
                    self.inner.calls.clone().acquire_owned(),
                )
                .await
                .map_err(|_| PluginRuntimeError::Busy)?
                .map_err(|_| PluginRuntimeError::Busy)?,
            )
        };

        if let Executable::Script(program) = &loaded.compiled {
            let program = program.clone();
            let name = name.clone();
            let version = version.clone();
            return tokio::task::spawn_blocking(move || {
                let _permit = permit;
                if !program.has_function(&function) {
                    return Err(PluginRuntimeError::FunctionNotFound { name, version, function });
                }
                program
                    .call(&function, payload, host, runtime_handle)
                    .map_err(|source| PluginRuntimeError::Script { name, version, source })
            })
            .await
            .map_err(|error| PluginRuntimeError::Worker(error.to_string()))?;
        }
        let Executable::Wasm(compiled) = &loaded.compiled else {
            return Err(PluginRuntimeError::Worker("plugin has no code".into()));
        };
        let compiled = compiled.clone();
        let output = tokio::task::spawn_blocking(move || {
            let _permit = permit;
            let mut plugin = extism::Plugin::new_from_compiled(&compiled)
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

    async fn compile(
        &self,
        prepared: Prepared,
        permit: OwnedSemaphorePermit,
    ) -> Result<LoadedPlugin, PluginRuntimeError> {
        let Prepared {
            record,
            artifact_path,
            wasm_bytes,
            estimated_mb,
        } = prepared;
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

        let manifest_path = self.manifest_path(&record, &artifact_path).await?;
        let manifest_text = tokio::fs::read_to_string(&manifest_path).await?;
        let manifest = PluginManifest::parse(&manifest_text).map_err(|source| {
            PluginRuntimeError::Manifest {
                path: manifest_path.clone(),
                source,
            }
        })?;
        // A rebuild is catalogued as `<version>+<stamp>`; the manifest says `<version>`.
        if manifest.plugin.name != record.name
            || manifest.plugin.version != super::catalog::base_version(&record.version)
        {
            return Err(PluginRuntimeError::ManifestIdentityMismatch {
                name: record.name,
                version: record.version,
            });
        }

        let models = self.load_models(&record, &artifact_path).await?;
        let rules = self.load_rules(&record, &artifact_path, &models).await?;

        let name = manifest.plugin.name.clone();
        let version = manifest.plugin.version.clone();
        if manifest.plugin.is_script() {
            drop(permit);
            let script_error = |source| PluginRuntimeError::Script {
                name: name.clone(),
                version: version.clone(),
                source,
            };
            let source = String::from_utf8(bytes)
                .map_err(|error| script_error(super::script::ScriptError::Compile(error.to_string())))?;
            let kind = super::script::ScriptKind::of_path(&artifact_path).ok_or_else(|| {
                script_error(super::script::ScriptError::Compile("a script must end in `.rhai` or `.lua`".into()))
            })?;
            let program = super::script::ScriptProgram::compile(kind, &source).map_err(script_error)?;
            let schemas = crate::data_model::schemas_of(&models);
            return Ok(LoadedPlugin {
                manifest,
                models,
                schemas,
                rules,
                compiled: Executable::Script(Arc::new(program)),
                estimated_mb,
                wasm_bytes,
            });
        }
        // wasmtime counts memory in 64 KiB pages: 16 to the megabyte.
        let memory_pages = u64::from(self.inner.config.instance_memory_mb) * 16;
        let disk_cache = self.inner.disk_cache_config.clone();
        let secs = self.inner.config.compile_timeout_secs;
        let (timeout_name, timeout_version) = (name.clone(), version.clone());
        let compiling = tokio::task::spawn_blocking(move || {
            // Held until the compile really ends, even if the caller gave up waiting, so
            // a timeout cannot let compiles pile up beyond the limit.
            let _permit = permit;
            let builder = PluginBuilder::new(
                Manifest::new([Wasm::data(bytes)])
                    .disallow_all_hosts()
                    .with_memory_max(memory_pages as u32)
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
            );
            // Say where compiled code is cached, or that it is not: otherwise Extism
            // falls back to wasmtime's system-wide default location.
            let builder = match &disk_cache {
                Some(config) => builder.with_cache_config(config),
                None => builder.with_cache_disabled(),
            };
            builder.compile().map_err(|source| PluginRuntimeError::Compile {
                name,
                version,
                source,
            })
        });
        let compiled = tokio::time::timeout(Duration::from_secs(secs), compiling)
            .await
            .map_err(|_| {
                self.inner.counters.timeouts.fetch_add(1, Ordering::Relaxed);
                PluginRuntimeError::CompileTimeout {
                    name: timeout_name,
                    version: timeout_version,
                    secs,
                }
            })?
            .map_err(|error| PluginRuntimeError::Worker(error.to_string()))??;

        let schemas = crate::data_model::schemas_of(&models);
        Ok(LoadedPlugin {
            manifest,
            models,
            schemas,
            rules,
            compiled: Executable::Wasm(Arc::new(compiled)),
            estimated_mb,
            wasm_bytes,
        })
    }

    /// The model definitions of this version: the `models/*.json` of its revision's file index.
    /// A plugin stored before models existed has none.
    async fn load_models(
        &self,
        record: &PluginDbDefinition,
        artifact_path: &Path,
    ) -> Result<Vec<crate::data_model::ModelDef>, PluginRuntimeError> {
        let mut paths = Vec::new();
        if let Some(revision) = &record.revision {
            let layout = AppDir::new(self.inner.app_dir.as_path());
            if let Some(index) = super::revisions::read_index(&layout, &record.name, revision).await? {
                for (logical, entry) in &index.files {
                    if logical.starts_with("models/") && logical.ends_with(".json") {
                        paths.push(layout.revision_dir(&record.name, &entry.revision)?.join(logical));
                    }
                }
            }
        } else if let Some(parent) = artifact_path.parent() {
            let directory = parent.join("models");
            if let Ok(mut entries) = tokio::fs::read_dir(&directory).await {
                while let Some(entry) = entries.next_entry().await? {
                    paths.push(entry.path());
                }
            }
        }
        paths.sort();
        let mut models = Vec::with_capacity(paths.len());
        for path in paths {
            let text = tokio::fs::read_to_string(&path).await?;
            models.push(crate::data_model::parse_model(&text, &path)?);
        }
        crate::data_model::validate_set(&models)?;
        Ok(models)
    }

    /// The rule files of this version, checked against its models.
    async fn load_rules(
        &self,
        record: &PluginDbDefinition,
        artifact_path: &Path,
        models: &[crate::data_model::ModelDef],
    ) -> Result<std::collections::HashMap<String, Arc<crate::data_model::RuleSet>>, PluginRuntimeError> {
        let mut paths = Vec::new();
        if let Some(revision) = &record.revision {
            let layout = AppDir::new(self.inner.app_dir.as_path());
            if let Some(index) = super::revisions::read_index(&layout, &record.name, revision).await? {
                for (logical, entry) in &index.files {
                    if logical.starts_with("rules/") && logical.ends_with(".json") {
                        paths.push((logical.clone(), layout.revision_dir(&record.name, &entry.revision)?.join(logical)));
                    }
                }
            }
        } else if let Some(parent) = artifact_path.parent() {
            if let Ok(mut entries) = tokio::fs::read_dir(parent.join("rules")).await {
                while let Some(entry) = entries.next_entry().await? {
                    paths.push((format!("rules/{}", entry.file_name().to_string_lossy()), entry.path()));
                }
            }
        }
        paths.sort();
        let mut files = Vec::with_capacity(paths.len());
        for (logical, path) in paths {
            files.push((logical, tokio::fs::read_to_string(&path).await?));
        }
        let sets = crate::data_model::rules::parse_all(&files, models).map_err(|problems| {
            PluginRuntimeError::Rules { name: record.name.clone(), problems: problems.join("; ") }
        })?;
        Ok(sets.into_iter().map(|set| (set.model.clone(), Arc::new(set))).collect())
    }

    /// Where this version's `plugin.toml` is: in its revision's file index (the file may be
    /// in an older revision's folder), or beside the artifact for the older layout.
    async fn manifest_path(
        &self,
        record: &PluginDbDefinition,
        artifact_path: &Path,
    ) -> Result<PathBuf, PluginRuntimeError> {
        if let Some(revision) = &record.revision {
            let layout = AppDir::new(self.inner.app_dir.as_path());
            if let Some(index) = super::revisions::read_index(&layout, &record.name, revision).await? {
                if let Some(entry) = index.files.get("plugin.toml") {
                    return Ok(layout.revision_dir(&record.name, &entry.revision)?.join("plugin.toml"));
                }
            }
        }
        Ok(artifact_path
            .parent()
            .unwrap_or_else(|| Path::new("."))
            .join("plugin.toml"))
    }

    async fn resolve_app_path(&self, raw_path: &str) -> Result<PathBuf, PluginRuntimeError> {
        let app_dir = tokio::fs::canonicalize(self.inner.app_dir.as_path()).await?;
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

/// An artifact that passed the size checks.
struct Prepared {
    record: PluginDbDefinition,
    artifact_path: PathBuf,
    wasm_bytes: u64,
    estimated_mb: f64,
}

/// Create the compile cache directory and write the wasmtime config that points at it,
/// with the disk budget from `[plugin_runtime.compile_cache]`. Returns the config file.
fn prepare_disk_cache(
    app_dir: &Path,
    config: &PluginRuntimeConfig,
) -> Result<PathBuf, PluginRuntimeError> {
    let cache = &config.compile_cache;
    let directory = if cache.directory.is_absolute() {
        cache.directory.clone()
    } else {
        app_dir.join(&cache.directory)
    };
    let fail = |path: &Path| {
        let path = path.to_path_buf();
        move |source| PluginRuntimeError::CompileCache { path, source }
    };
    std::fs::create_dir_all(&directory).map_err(fail(&directory))?;
    // wasmtime insists on an absolute path.
    let directory = std::fs::canonicalize(&directory).map_err(fail(&directory))?;
    let conf_dir = app_dir.join("conf");
    std::fs::create_dir_all(&conf_dir).map_err(fail(&conf_dir))?;
    let file = conf_dir.join("wasmtime-cache.toml");
    let contents = format!(
        "# Written by Aether from [plugin_runtime.compile_cache]; edit aether.toml instead.\n\
         [cache]\n\
         directory = {}\n\
         files-total-size-soft-limit = \"{}Mi\"\n\
         cleanup-interval = \"1h\"\n",
        toml::Value::String(directory.to_string_lossy().into_owned()),
        cache.max_size_mb
    );
    std::fs::write(&file, contents).map_err(fail(&file))?;
    Ok(file)
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
    use surrealdb::types::{Datetime, RecordId};

    /// A valid, tiny WebAssembly module (text format; wasmtime reads it as it is).
    const MODULE: &[u8] = b"(module (func (export \"hello\") (result i32) i32.const 7))";

    fn config() -> PluginRuntimeConfig {
        PluginRuntimeConfig {
            compile_cache: crate::config_manager::models::CompileCacheConfig {
                enabled: false,
                ..Default::default()
            },
            ..Default::default()
        }
    }

    fn runtime(directory: &Path, config: PluginRuntimeConfig) -> Result<PluginRuntime, PluginRuntimeError> {
        PluginRuntime::new(directory.join("app_dir"), config)
    }

    /// Put a plugin version on disk the way the catalog does, and return its record.
    fn install(
        app_dir: &Path,
        name: &str,
        version: &str,
        wasm: &[u8],
    ) -> Result<PluginDbDefinition, Box<dyn std::error::Error>> {
        let folder = app_dir.join(format!("plugins/{name}/{version}"));
        std::fs::create_dir_all(&folder)?;
        std::fs::write(folder.join("plugin.wasm"), wasm)?;
        std::fs::write(
            folder.join("plugin.toml"),
            format!("[plugin]\nname = \"{name}\"\nlabel = \"{name}\"\nversion = \"{version}\"\n"),
        )?;
        Ok(PluginDbDefinition {
            id: RecordId::new("plugins", format!("{name}_{version}")),
            name: name.into(),
            label: name.into(),
            version: version.into(),
            description: None,
            long_description: None,
            icon_path: None,
            website: None,
            authors: None,
            categories: None,
            dependencies: None,
            schedules: None,
            commands: None,
            i18n: None,
            watches: None,
            event_listeners: None,
            roles: None,
            workspace: None,
            kind: None,
            artifact_path: Some(format!("plugins/{name}/{version}/plugin.wasm")),
            artifact_hash: Some(sha256_hex(wasm)),
            revision: None,
            is_builtin: false,
            is_active: true,
            date_created: Datetime::now(),
            date_updated: Datetime::now(),
        })
    }

    async fn load(
        runtime: &PluginRuntime,
        record: &PluginDbDefinition,
    ) -> Result<Arc<LoadedPlugin>, PluginRuntimeError> {
        let record = record.clone();
        runtime
            .ensure_loaded_with(&record.name.clone(), &record.version.clone(), async { Ok(Some(record)) })
            .await
    }

    #[tokio::test]
    async fn artifact_paths_cannot_escape_app_dir() -> Result<(), Box<dyn std::error::Error>> {
        let directory = tempfile::tempdir()?;
        let app_dir = directory.path().join("app_dir");
        tokio::fs::create_dir_all(&app_dir).await?;
        tokio::fs::write(directory.path().join("outside.wasm"), b"wasm").await?;

        let runtime = runtime(directory.path(), config())?;
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

        let runtime = runtime(directory.path(), config())?;
        let resolved = runtime
            .resolve_app_path("plugins/sample/1.0.0/plugin.wasm")
            .await?;
        assert!(resolved.starts_with(app_dir.canonicalize()?));
        Ok(())
    }

    #[tokio::test]
    async fn a_plugin_compiles_on_first_use_and_is_ready_after() -> Result<(), Box<dyn std::error::Error>> {
        let directory = tempfile::tempdir()?;
        let runtime = runtime(directory.path(), config())?;
        let record = install(runtime.app_dir(), "sample", "1.0.0", MODULE)?;

        assert_eq!(runtime.stats().await.plugins_ready, 0, "nothing compiles at start-up");
        load(&runtime, &record).await?;
        load(&runtime, &record).await?;
        let stats = runtime.stats().await;
        assert_eq!((stats.loads, stats.hits, stats.plugins_ready), (1, 1, 1));
        assert!(stats.resident_mb > 0.0 && stats.resident_mb <= stats.budget_mb as f64);
        Ok(())
    }

    #[tokio::test]
    async fn two_versions_are_two_separate_entries() -> Result<(), Box<dyn std::error::Error>> {
        let directory = tempfile::tempdir()?;
        let runtime = runtime(directory.path(), config())?;
        let one = install(runtime.app_dir(), "sample", "1.0.0", MODULE)?;
        let two = install(runtime.app_dir(), "sample", "2.0.0", MODULE)?;
        let first = load(&runtime, &one).await?;
        let second = load(&runtime, &two).await?;
        assert_eq!(first.manifest.plugin.version, "1.0.0");
        assert_eq!(second.manifest.plugin.version, "2.0.0");
        assert_eq!(runtime.stats().await.plugins_ready, 2);
        Ok(())
    }

    #[tokio::test]
    async fn callers_asking_at_once_share_one_load() -> Result<(), Box<dyn std::error::Error>> {
        let directory = tempfile::tempdir()?;
        let runtime = runtime(directory.path(), config())?;
        let record = install(runtime.app_dir(), "sample", "1.0.0", MODULE)?;

        let loads: Vec<_> = (0..8)
            .map(|_| {
                let (runtime, record) = (runtime.clone(), record.clone());
                tokio::spawn(async move { load(&runtime, &record).await.map(|_| ()) })
            })
            .collect();
        for handle in loads {
            handle.await??;
        }
        assert_eq!(runtime.stats().await.loads, 1);
        Ok(())
    }

    #[tokio::test]
    async fn the_memory_budget_evicts_the_least_recently_used() -> Result<(), Box<dyn std::error::Error>> {
        let directory = tempfile::tempdir()?;
        // Each tiny plugin is estimated at 6 MB; 16 MB holds two.
        let runtime = runtime(
            directory.path(),
            PluginRuntimeConfig {
                max_compiled_memory_mb: 16,
                engine_overhead_mb: 6.0,
                ..config()
            },
        )?;
        let a = install(runtime.app_dir(), "a", "1.0.0", MODULE)?;
        let b = install(runtime.app_dir(), "b", "1.0.0", MODULE)?;
        let c = install(runtime.app_dir(), "c", "1.0.0", MODULE)?;

        load(&runtime, &a).await?;
        load(&runtime, &b).await?;
        load(&runtime, &a).await?; // a is now more recent than b
        runtime.stats().await;
        load(&runtime, &c).await?;
        let stats = runtime.stats().await;
        assert_eq!(stats.plugins_ready, 2);
        assert!(stats.resident_mb <= 16.0, "resident {} MB", stats.resident_mb);
        assert_eq!(stats.evictions, 1);

        // b was the one pushed out: loading it again is a load, loading a is a hit.
        let before = runtime.stats().await;
        load(&runtime, &a).await?;
        assert_eq!(runtime.stats().await.hits, before.hits + 1);
        load(&runtime, &b).await?;
        assert_eq!(runtime.stats().await.loads, before.loads + 1);
        Ok(())
    }

    #[tokio::test]
    async fn a_plugin_that_cannot_fit_the_budget_is_refused() -> Result<(), Box<dyn std::error::Error>> {
        let directory = tempfile::tempdir()?;
        let runtime = runtime(
            directory.path(),
            PluginRuntimeConfig { max_compiled_memory_mb: 16, engine_overhead_mb: 20.0, ..config() },
        )?;
        let record = install(runtime.app_dir(), "big", "1.0.0", MODULE)?;
        let refused = load(&runtime, &record).await;
        assert!(matches!(
            refused.as_ref().map_err(PluginRuntimeError::root),
            Err(PluginRuntimeError::ExceedsBudget { .. })
        ));
        let stats = runtime.stats().await;
        assert_eq!((stats.refused, stats.plugins_ready), (1, 0));
        Ok(())
    }

    #[tokio::test]
    async fn an_oversized_wasm_is_refused_before_it_is_read() -> Result<(), Box<dyn std::error::Error>> {
        let directory = tempfile::tempdir()?;
        let runtime = runtime(
            directory.path(),
            PluginRuntimeConfig { max_wasm_size_mb: 1, ..config() },
        )?;
        // Not even valid WebAssembly: the size check comes first.
        let record = install(runtime.app_dir(), "huge", "1.0.0", &vec![0u8; 2 * 1024 * 1024])?;
        let refused = load(&runtime, &record).await;
        assert!(matches!(
            refused.as_ref().map_err(PluginRuntimeError::root),
            Err(PluginRuntimeError::WasmTooLarge { .. })
        ));
        Ok(())
    }

    #[tokio::test]
    async fn a_tampered_artifact_is_refused() -> Result<(), Box<dyn std::error::Error>> {
        let directory = tempfile::tempdir()?;
        let runtime = runtime(directory.path(), config())?;
        let mut record = install(runtime.app_dir(), "sample", "1.0.0", MODULE)?;
        record.artifact_hash = Some("0".repeat(64));
        let refused = load(&runtime, &record).await;
        assert!(matches!(
            refused.as_ref().map_err(PluginRuntimeError::root),
            Err(PluginRuntimeError::ArtifactHashMismatch { .. })
        ));
        Ok(())
    }

    #[tokio::test]
    async fn only_so_many_compiles_may_wait_for_a_turn() -> Result<(), Box<dyn std::error::Error>> {
        let directory = tempfile::tempdir()?;
        let runtime = runtime(
            directory.path(),
            PluginRuntimeConfig {
                max_concurrent_compiles: 1,
                compile_queue_limit: 1,
                compile_timeout_secs: 1,
                ..config()
            },
        )?;
        // Someone else is compiling: the only turn is taken.
        let busy = runtime.inner.compile_permits.clone().acquire_owned().await?;

        // One caller may wait; it gives up after the timeout.
        let waiter = {
            let runtime = runtime.clone();
            tokio::spawn(async move { runtime.acquire_compile_turn("a", "1").await.map(|_| ()) })
        };
        while runtime.inner.counters.waiting.load(Ordering::SeqCst) == 0 {
            tokio::task::yield_now().await;
        }
        // The next is turned away at once.
        let turned_away = runtime.acquire_compile_turn("b", "1").await;
        assert!(matches!(turned_away, Err(PluginRuntimeError::CompileQueueFull)));

        assert!(matches!(waiter.await?, Err(PluginRuntimeError::CompileTimeout { .. })));
        drop(busy);
        let stats = runtime.stats().await;
        assert_eq!((stats.queue_full, stats.timeouts), (1, 1));
        assert_eq!(stats.waiting_to_compile, 0);
        Ok(())
    }

    #[tokio::test]
    async fn the_disk_tier_is_prepared_and_written_to() -> Result<(), Box<dyn std::error::Error>> {
        let directory = tempfile::tempdir()?;
        let runtime = runtime(
            directory.path(),
            PluginRuntimeConfig {
                compile_cache: crate::config_manager::models::CompileCacheConfig {
                    enabled: true,
                    directory: "cache/compiled".into(),
                    max_size_mb: 64,
                },
                ..config()
            },
        )?;
        let conf = std::fs::read_to_string(runtime.app_dir().join("conf/wasmtime-cache.toml"))?;
        assert!(conf.contains("files-total-size-soft-limit = \"64Mi\""), "{conf}");
        assert!(runtime.stats().await.disk_cache);

        let record = install(runtime.app_dir(), "sample", "1.0.0", MODULE)?;
        load(&runtime, &record).await?;
        // wasmtime stores compiled code from a background worker.
        let cached = runtime.app_dir().join("cache/compiled");
        let mut found = false;
        for _ in 0..50 {
            found = walk_files(&cached) > 0;
            if found {
                break;
            }
            tokio::time::sleep(Duration::from_millis(100)).await;
        }
        assert!(found, "compiled code was written under {}", cached.display());
        Ok(())
    }

    fn walk_files(directory: &Path) -> usize {
        std::fs::read_dir(directory)
            .map(|entries| {
                entries
                    .flatten()
                    .map(|entry| {
                        let path = entry.path();
                        if path.is_dir() { walk_files(&path) } else { 1 }
                    })
                    .sum()
            })
            .unwrap_or(0)
    }

    #[tokio::test]
    async fn without_the_disk_tier_nothing_is_written_outside_memory() -> Result<(), Box<dyn std::error::Error>> {
        let directory = tempfile::tempdir()?;
        let runtime = runtime(directory.path(), config())?;
        assert!(!runtime.stats().await.disk_cache);
        assert!(!runtime.app_dir().join("conf/wasmtime-cache.toml").exists());
        Ok(())
    }

    #[test]
    fn only_errors_a_plugin_marked_for_the_caller_are_shown_to_them() {
        let marked = PluginRuntimeError::Invoke(extism::Error::msg(format!(
            "call failed: {USER_ERROR_PREFIX}channel is archived\nwith a newline"
        )));
        assert_eq!(marked.user_message().as_deref(), Some("channel is archivedwith a newline"));
        let internal = PluginRuntimeError::Invoke(extism::Error::msg("JSON error: invalid type"));
        assert_eq!(internal.user_message(), None);
        assert_eq!(PluginRuntimeError::Busy.user_message(), None);
        let long = PluginRuntimeError::Invoke(extism::Error::msg(format!("{USER_ERROR_PREFIX}{}", "x".repeat(1000))));
        assert_eq!(long.user_message().map(|m| m.len()), Some(MAX_USER_ERROR_CHARS));
    }

    #[test]
    fn the_defaults_are_valid_and_sizes_are_checked() {
        let config = PluginRuntimeConfig::default();
        assert_eq!(config.validate(), Ok(()));
        assert_eq!(config.max_compiled_memory_mb, 256);
        assert!(config.compile_cache.enabled, "the disk tier is on by default");

        let small = PluginRuntimeConfig { max_compiled_memory_mb: 8, ..Default::default() };
        assert!(small.validate().is_err());
        let cannot_fit = PluginRuntimeConfig { max_wasm_size_mb: 64, ..Default::default() };
        assert!(cannot_fit.validate().is_err(), "the biggest accepted wasm must fit the budget");
        let no_compiles = PluginRuntimeConfig { max_concurrent_compiles: 0, ..Default::default() };
        assert!(no_compiles.validate().is_err());
    }

    /// A synthetic module of about `functions` functions: arithmetic, loops, memory access
    /// and calls, with different constants so nothing is shared.
    fn synthetic_wasm(functions: usize) -> Result<Vec<u8>, Box<dyn std::error::Error>> {
        let mut text = String::from("(module (memory 1) (func $leaf (param i32) (result i32) local.get 0 i32.const 1 i32.add)\n");
        for index in 0..functions {
            let k = index as i32 * 7 + 3;
            text.push_str(&format!(
                "(func $f{index} (param i32 i32) (result i32) (local i32 i32 i64)
                   local.get 0 i32.const {k} i32.mul local.set 2
                   (block (loop
                     local.get 3 i32.const {k} i32.ge_s br_if 1
                     local.get 2 local.get 3 i32.const 4 i32.mul i32.const 1024 i32.rem_u i32.load
                     i32.add local.get 1 i32.xor local.set 2
                     local.get 2 call $leaf local.set 2
                     local.get 3 i32.const 1 i32.add local.set 3
                     br 0))
                   local.get 2 i64.extend_i32_u i64.const {k} i64.shl local.set 4
                   local.get 4 i32.wrap_i64 local.get 0 i32.const {} i32.add i32.store
                   local.get 2)\n",
                index % 1000
            ));
        }
        text.push_str("(export \"f0\" (func $f0)))");
        Ok(wat::parse_str(&text)?)
    }

    fn resident_mb() -> f64 {
        let pages: f64 = std::fs::read_to_string("/proc/self/statm")
            .ok()
            .and_then(|text| text.split_whitespace().nth(1).and_then(|field| field.parse().ok()))
            .unwrap_or(0.0);
        pages * 4096.0 / BYTES_PER_MB
    }

    /// Measures how much memory a compiled plugin holds compared with its `.wasm`, to set
    /// `compiled_size_factor` and `engine_overhead_mb`. Synthetic modules, so treat the
    /// result as a starting point and re-run it with a real plugin.
    /// `cargo test -p aether_core --lib calibrate -- --ignored --nocapture`
    #[test]
    #[ignore = "measurement, not a check"]
    fn calibrate_compiled_size() -> Result<(), Box<dyn std::error::Error>> {
        let compile = |wasm: Vec<u8>| {
            PluginBuilder::new(Manifest::new([Wasm::data(wasm)]))
                .with_wasi(true)
                .with_cache_disabled()
                .compile()
        };
        // The fixed cost of one engine: a tiny module, many times.
        let tiny = wat::parse_str("(module (func (export \"f\")))")?;
        let before = resident_mb();
        let engines: Vec<_> = (0..20).map(|_| compile(tiny.clone())).collect::<Result<_, _>>()?;
        eprintln!("CAL engine overhead: {:.2} MB per compiled plugin ({} of them)", (resident_mb() - before) / 20.0, engines.len());
        drop(engines);

        // What matters is the memory of each plugin kept ready, not what one compile leaves
        // behind: compile the same module several times and measure each extra copy.
        for functions in [500usize, 2_000, 8_000] {
            let wasm = synthetic_wasm(functions)?;
            let wasm_mb = wasm.len() as f64 / BYTES_PER_MB;
            let mut kept = vec![compile(wasm.clone())?];
            let after_first = resident_mb();
            for _ in 0..5 {
                kept.push(compile(wasm.clone())?);
            }
            let per_extra = (resident_mb() - after_first) / 5.0;
            eprintln!(
                "CAL wasm {wasm_mb:.2} MB -> {per_extra:.2} MB for each extra copy kept ({:.1}x, engine included)",
                per_extra / wasm_mb
            );
            drop(kept);
        }
        Ok(())
    }

    /// A script plugin held in memory, with no catalog behind it.
    fn script_plugin(name: &str, kind: super::super::script::ScriptKind, source: &str) -> Result<Arc<LoadedPlugin>, Box<dyn std::error::Error>> {
        let manifest = PluginManifest::parse(&format!("[plugin]\nname = \"{name}\"\nlabel = \"{name}\"\nversion = \"1.0.0\"\n"))?;
        let program = super::super::script::ScriptProgram::compile(kind, source)?;
        Ok(Arc::new(LoadedPlugin {
            manifest,
            models: Vec::new(),
            schemas: Default::default(),
            rules: Default::default(),
            compiled: Executable::Script(Arc::new(program)),
            estimated_mb: 0.1,
            wasm_bytes: source.len() as u64,
        }))
    }

    /// Answers `plugins::call` by running the target on the same runtime, as the kernel does.
    struct Chain {
        runtime: PluginRuntime,
        target: Arc<LoadedPlugin>,
    }

    #[async_trait::async_trait]
    impl crate::kernel::PluginCaller for Chain {
        async fn call(
            &self,
            _plugin: &str,
            function: &str,
            payload: serde_json::Value,
            trail: Vec<String>,
        ) -> Result<serde_json::Value, crate::kernel::HostError> {
            let mut host = crate::kernel::host::test_support::dummy_ctx(&[]);
            host.call_trail = trail;
            self.runtime
                .invoke(self.target.clone(), function, payload, host)
                .await
                .map_err(|error| crate::kernel::HostError::Message(error.to_string()))
        }
    }

    /// With one call permit, a plugin that calls another must not wait for a permit its own
    /// request is holding.
    #[tokio::test]
    async fn a_nested_call_does_not_need_a_permit_of_its_own() -> Result<(), Box<dyn std::error::Error>> {
        use super::super::script::ScriptKind;
        let dir = tempfile::tempdir()?;
        let runtime = runtime(
            dir.path(),
            PluginRuntimeConfig { max_concurrent_calls: 1, compile_timeout_secs: 1, ..config() },
        )?;
        let inner = script_plugin("inner", ScriptKind::Rhai, "fn double(input) { #{ n: input.n * 2 } }")?;
        let outer = script_plugin(
            "outer",
            ScriptKind::Lua,
            r#"function run() return plugins.call("inner", "double", { n = 21 }) end"#,
        )?;
        let caller = Arc::new(Chain { runtime: runtime.clone(), target: inner });
        let host = crate::kernel::host::test_support::dummy_ctx(&["plugins::call"])
            .with_plugin_calls(vec!["inner".into()], vec!["outer.run".into()], caller);
        let answer = runtime.invoke(outer, "run", serde_json::Value::Null, host).await?;
        assert_eq!(answer, serde_json::json!({ "n": 42 }));
        Ok(())
    }
}
