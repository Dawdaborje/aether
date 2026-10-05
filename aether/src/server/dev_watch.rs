//! `aether --serve --watch`: reload plugins when their source changes, for development.
//!
//! The catalog remembers where each plugin was loaded from (`plugins.source_path`). With `--watch`
//! the server listens to those folders and, when something in one changes and then stays quiet for a
//! moment, does what you would do by hand: `--load-plugin` the folder again (identical content is
//! recognized and ignored; changed content becomes a new revision of the same version) and
//! `--upgrade-plugin` every organization that has the plugin installed to it. The next call to the
//! plugin runs the new code; its models, pages, schedules and watches follow.
//!
//! A change that does not load (a script that does not compile, a manifest error, a model change the
//! data cannot take) is logged and the running version is left alone. This is a development tool:
//! it moves every organization to the new revision, so do not use it on a server with real data.

use std::{
    collections::HashMap,
    path::{Path, PathBuf},
    time::{Duration, Instant},
};

use aether_core::{
    plugin_manager::catalog::{self, PluginSpec},
    state::AppState,
};
use notify::{Config, EventKind, RecommendedWatcher, RecursiveMode, Watcher, event::ModifyKind};
use serde::Deserialize;
use surrealdb::types::SurrealValue;
use tokio::sync::mpsc;

/// A plugin must be quiet this long before it is reloaded: an editor or a build writes several files.
const QUIET: Duration = Duration::from_millis(500);
/// How often the catalog is read for plugins that were loaded since.
const REFRESH: Duration = Duration::from_secs(10);

/// Whether an event means a file's content or name changed. Reading a file (what loading a plugin
/// does) also produces events, "opened" and "closed after reading"; reacting to those would make every
/// reload cause the next one.
fn is_change(kind: &EventKind) -> bool {
    match kind {
        EventKind::Create(_) | EventKind::Remove(_) => true,
        EventKind::Modify(ModifyKind::Metadata(_)) => false,
        EventKind::Modify(_) => true,
        EventKind::Access(_) | EventKind::Any | EventKind::Other => false,
    }
}

/// Parts of a plugin folder whose changes never matter.
fn ignored(relative: &Path) -> bool {
    relative.components().any(|part| {
        let part = part.as_os_str().to_string_lossy();
        part.starts_with('.') || part == "target" || part == "node_modules" || part.ends_with('~') || part.ends_with(".swp")
    })
}

#[derive(Debug, Deserialize, SurrealValue)]
struct SourceRow {
    name: String,
    source_path: String,
}
// `created` is selected only so the rows can be ordered by it.

/// The folders plugins were loaded from, one per plugin (the newest load wins), that still exist.
async fn sources(state: &AppState, app_dir: &Path) -> Vec<(String, PathBuf)> {
    let rows = async {
        let core = state.core().await?;
        let mut response = core
            .query("SELECT name, source_path, <string> date_created AS created FROM plugins WHERE is_active = true AND source_path != NONE ORDER BY created ASC;")
            .await?
            .check()?;
        response.take::<Vec<SourceRow>>(0)
    }
    .await;
    let rows = match rows {
        Ok(rows) => rows,
        Err(error) => {
            log::warn!("--watch: the plugin catalog could not be read: {error}");
            return Vec::new();
        }
    };
    let mut newest: HashMap<String, PathBuf> = HashMap::new();
    for row in rows {
        let path = PathBuf::from(&row.source_path);
        // A copy inside app_dir is the kernel's own; the original is the one being edited.
        if path.is_dir() && !path.starts_with(app_dir) {
            newest.insert(row.name, path);
        }
    }
    let mut found: Vec<(String, PathBuf)> = newest.into_iter().collect();
    found.sort();
    found
}

/// Organizations (database names) that have `plugin` installed.
async fn organizations_with(state: &AppState, plugin: &str) -> Vec<String> {
    let names: Vec<String> = async {
        let core = state.core().await?;
        let mut response = core.query("SELECT VALUE db_name FROM organizations;").await?.check()?;
        response.take(0)
    }
    .await
    .unwrap_or_default();
    let mut with = Vec::new();
    for org in names {
        let installed: Vec<String> = async {
            let db = state.org(&org).await?;
            let mut response = db
                .query("SELECT VALUE plugin_name FROM installed_plugins WHERE plugin_name = $name;")
                .bind(("name", plugin.to_string()))
                .await?
                .check()?;
            response.take(0)
        }
        .await
        .unwrap_or_default();
        if !installed.is_empty() {
            with.push(org);
        }
    }
    with
}

async fn reload(state: &AppState, namespace: &str, app_dir: &Path, name: &str, dir: &Path) {
    let session = state.fresh_session();
    let loaded = match catalog::load_plugin(&session, namespace, &state.core_database, app_dir, dir).await {
        Ok(loaded) => loaded,
        Err(error) => {
            log::error!("--watch: `{name}` was not reloaded, the running version is unchanged: {error}");
            return;
        }
    };
    if !loaded.created {
        log::debug!("--watch: `{name}` changed on disk but its content is the same");
        return;
    }
    let spec = PluginSpec { name: name.to_string(), version: Some(loaded.version.clone()) };
    let mut moved = Vec::new();
    for org in organizations_with(state, name).await {
        let session = state.fresh_session();
        match catalog::upgrade_plugins(&session, namespace, &state.core_database, &org, std::slice::from_ref(&spec)).await {
            Ok(_) => moved.push(org),
            Err(error) => log::error!("--watch: `{name}` could not be moved to {} in `{org}`: {error}", loaded.version),
        }
    }
    log::info!(
        "--watch: reloaded `{name}` as {}{}",
        loaded.version,
        if moved.is_empty() { " (no organization has it installed)".to_string() } else { format!(" in {}", moved.join(", ")) }
    );
    state.scheduler.reload();
}

/// Start listening to plugin sources. Dropping the returned handle (or aborting it) stops it.
pub fn spawn(state: AppState, namespace: String) -> tokio::task::JoinHandle<()> {
    tokio::spawn(async move {
        let app_dir = state.config.app_dir.clone();
        let (changed_tx, mut changed_rx) = mpsc::unbounded_channel::<PathBuf>();
        let handler = move |result: notify::Result<notify::Event>| {
            if let Some(event) = result.ok().filter(|event| is_change(&event.kind)) {
                for path in event.paths {
                    // The receiver is gone when the task ends.
                    let _ = changed_tx.send(path);
                }
            }
        };
        let mut watcher = match RecommendedWatcher::new(handler, Config::default()) {
            Ok(watcher) => watcher,
            Err(error) => {
                log::error!("--watch cannot listen to files: {error}");
                return;
            }
        };
        let mut watched: HashMap<String, PathBuf> = HashMap::new();
        let mut pending: HashMap<String, Instant> = HashMap::new();
        let mut refresh = tokio::time::interval(REFRESH);
        let mut flush = tokio::time::interval(Duration::from_millis(150));
        loop {
            tokio::select! {
                _ = refresh.tick() => {
                    for (name, dir) in sources(&state, &app_dir).await {
                        if watched.get(&name) == Some(&dir) {
                            continue;
                        }
                        match watcher.watch(&dir, RecursiveMode::Recursive) {
                            Ok(()) => {
                                log::info!("--watch: reloading `{name}` when {} changes", dir.display());
                                watched.insert(name, dir);
                            }
                            Err(error) => log::warn!("--watch: cannot listen to {}: {error}", dir.display()),
                        }
                    }
                }
                Some(path) = changed_rx.recv() => {
                    // The plugin whose folder holds the path (the deepest, if folders nest).
                    let owner = watched
                        .iter()
                        .filter(|(_, dir)| path.starts_with(dir))
                        .max_by_key(|(_, dir)| dir.components().count());
                    if let Some((name, dir)) = owner {
                        let relative = path.strip_prefix(dir).unwrap_or(&path);
                        if !ignored(relative) {
                            pending.insert(name.clone(), Instant::now());
                        }
                    }
                }
                _ = flush.tick() => {
                    let due: Vec<String> = pending.iter().filter(|(_, at)| at.elapsed() >= QUIET).map(|(name, _)| name.clone()).collect();
                    for name in due {
                        pending.remove(&name);
                        if let Some(dir) = watched.get(&name).cloned() {
                            reload(&state, &namespace, &app_dir, &name, &dir).await;
                        }
                    }
                }
            }
        }
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn reading_a_file_is_not_a_change() {
        use notify::event::{AccessKind, AccessMode, CreateKind, DataChange, MetadataKind, RemoveKind, RenameMode};
        for kind in [
            EventKind::Create(CreateKind::File),
            EventKind::Remove(RemoveKind::File),
            EventKind::Modify(ModifyKind::Data(DataChange::Content)),
            EventKind::Modify(ModifyKind::Name(RenameMode::To)),
            EventKind::Modify(ModifyKind::Any),
        ] {
            assert!(is_change(&kind), "{kind:?}");
        }
        for kind in [
            EventKind::Access(AccessKind::Open(AccessMode::Any)),
            EventKind::Access(AccessKind::Close(AccessMode::Read)),
            EventKind::Modify(ModifyKind::Metadata(MetadataKind::Any)),
            EventKind::Any,
            EventKind::Other,
        ] {
            assert!(!is_change(&kind), "{kind:?}");
        }
    }

    #[test]
    fn build_output_and_editor_files_do_not_trigger_a_reload() {
        for path in ["target/release/x.wasm", ".git/index", "node_modules/a/b.js", "src/.main.rs.swp", "main.rhai~", "pages/.hidden.xml"] {
            assert!(ignored(Path::new(path)), "{path}");
        }
        for path in ["main.rhai", "plugin.toml", "models/note.json", "pages/notes.xml", "out/plugin.wasm"] {
            assert!(!ignored(Path::new(path)), "{path}");
        }
    }
}
