//! `aether --serve-docs`: the documentation website on its own, and the places plugin docs are gathered from.
//!
//! Aether's own pages are compiled into the binary, so the site needs no config file and no database. Plugin docs
//! are added from what is at hand: the plugin folders given with `--plugin-docs`, the `plugin_paths` of the config
//! file, and the catalog when the config reaches a database. With no sessions here only `public` docs are shown
//! (`--docs-all` shows every plugin's docs, for previewing on your own machine). `aether --serve` mounts the
//! same site at `[docs] path` with real sessions instead, see `serve.rs`.

use super::serve::wait_for_shutdown_signal;
use aether_core::plugin_manager::docs as plugin_docs_reader;
use aether_core::plugin_manager::load_paths::resolve_load_paths;
use aether_core::plugin_manager::models::plugin_def::PluginManifest;
use aether_docs::PluginDocsProvider;
use aether_docs::plugin_docs::{PluginDocs, Visibility};
use async_trait::async_trait;
use axum::http::HeaderMap;
use std::collections::BTreeMap;
use std::error::Error;
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::time::{Duration, Instant};

/// How long gathered docs are reused: short, so editing a page and reloading the browser shows the change.
const REFRESH: Duration = Duration::from_secs(5);

/// Where plugin docs come from.
#[derive(Clone, Default)]
pub struct DocsSources {
    /// Plugin packages or workspace folders (from `plugin_paths` and `--plugin-docs`).
    pub entries: Vec<PathBuf>,
    /// Where a bare workspace name is looked up.
    pub search_roots: Vec<PathBuf>,
    /// The catalog, when a database is configured and reachable.
    pub catalog: Option<aether_core::state::Db>,
}

/// The newest docs per plugin: the catalog's first, then the folders on disk (which win, being the fresher
/// copy of what an author is editing). Problems with one plugin are logged and skip only that plugin.
pub async fn gather(sources: &DocsSources) -> Vec<PluginDocs> {
    let mut by_plugin: BTreeMap<String, PluginDocs> = BTreeMap::new();
    if let Some(db) = &sources.catalog {
        match aether_core::docs_provider::read_catalog(db).await {
            Ok(list) => by_plugin.extend(list.into_iter().map(|d| (d.plugin.clone(), (*d).clone()))),
            Err(e) => log::warn!("plugin documentation: cannot read the catalog: {e}"),
        }
    }
    if !sources.entries.is_empty() {
        match resolve_load_paths(&sources.entries, &sources.search_roots).await {
            Ok(plan) => {
                for dir in plan.packages {
                    if let Some(docs) = read_package(&dir) {
                        by_plugin.insert(docs.plugin.clone(), docs);
                    }
                }
            }
            Err(e) => log::warn!("plugin documentation: cannot resolve the plugin folders: {e}"),
        }
    }
    let mut docs: Vec<PluginDocs> = by_plugin.into_values().collect();
    docs.sort_by(|a, b| a.label.to_lowercase().cmp(&b.label.to_lowercase()).then_with(|| a.plugin.cmp(&b.plugin)));
    docs
}

/// The docs of one plugin folder; `None` when it has none or they have problems (logged).
fn read_package(dir: &Path) -> Option<PluginDocs> {
    let manifest_path = if dir.is_dir() { dir.join("plugin.toml") } else { dir.to_path_buf() };
    let text = std::fs::read_to_string(&manifest_path).ok()?;
    let manifest = match PluginManifest::parse(&text) {
        Ok(m) => m,
        Err(e) => {
            log::warn!("plugin documentation: {} is not a valid manifest: {e}", manifest_path.display());
            return None;
        }
    };
    let package_dir = manifest_path.parent()?;
    match plugin_docs_reader::read(package_dir, &manifest) {
        Ok(docs) => docs,
        Err(problems) => {
            log::warn!("plugin documentation: the docs of `{}` have problems and are not shown: {}", manifest.plugin.name, problems.join("; "));
            None
        }
    }
}

/// The docs found by [`gather`], offered to the site. Reloaded every few seconds.
struct GatheredDocs {
    sources: DocsSources,
    /// Show every plugin's docs whatever its visibility.
    all: bool,
    cached: tokio::sync::Mutex<Option<(Instant, Vec<Arc<PluginDocs>>)>>,
}

#[async_trait]
impl PluginDocsProvider for GatheredDocs {
    async fn visible(&self, _headers: &HeaderMap) -> Vec<Arc<PluginDocs>> {
        let mut cached = self.cached.lock().await;
        if let Some((at, docs)) = cached.as_ref()
            && at.elapsed() < REFRESH
        {
            return docs.clone();
        }
        let first = cached.is_none();
        let found = gather(&self.sources).await;
        if first {
            let hidden: Vec<String> = found.iter().filter(|d| !self.all && d.visibility != Visibility::Public).map(|d| format!("{} ({})", d.plugin, d.visibility.as_str())).collect();
            log::info!("Plugin documentation: {} plugin(s) found", found.len());
            if !hidden.is_empty() {
                log::warn!(
                    "Not shown, because this server has no sign-in and shows only `public` docs: {}. \
                     Use --docs-all to preview them on this machine, or serve them with `aether --serve`.",
                    hidden.join(", ")
                );
            }
        }
        let shown: Vec<Arc<PluginDocs>> = found.into_iter().filter(|d| self.all || d.visibility == Visibility::Public).map(Arc::new).collect();
        *cached = Some((Instant::now(), shown.clone()));
        shown
    }
}

pub async fn run_docs_server(bind: &str, sources: DocsSources, all: bool) -> Result<(), Box<dyn Error>> {
    let listener = tokio::net::TcpListener::bind(bind).await?;
    let addr = listener.local_addr()?;
    if all && !addr.ip().is_loopback() {
        return Err(format!("--docs-all shows every plugin's docs to anyone who can reach the server, so it only runs on a loopback address such as 127.0.0.1 (this one is {addr})").into());
    }
    let has_plugins = !sources.entries.is_empty() || sources.catalog.is_some();
    let provider: Option<Arc<dyn PluginDocsProvider>> =
        has_plugins.then(|| Arc::new(GatheredDocs { sources, all, cached: tokio::sync::Mutex::new(None) }) as Arc<dyn PluginDocsProvider>);
    log::info!("Serving the documentation: http://{addr}");
    if addr.ip().is_unspecified() {
        log::info!("Serving the documentation: http://{}:{}", super::serve::get_local_ip(), addr.port());
    }
    axum::serve(listener, aether_docs::router_with("", provider))
        .with_graceful_shutdown(wait_for_shutdown_signal())
        .await?;
    Ok(())
}
