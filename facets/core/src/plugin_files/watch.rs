//! Telling a plugin that a file appeared, changed or went away.
//!
//! A plugin declares `[[watch]]` entries in its manifest. Installing the plugin copies them into the
//! organization's `watches` table, and a scheduler node (embedded or standalone) listens to each
//! folder with the operating system's own file notifications (inotify on Linux), so nothing polls.
//! A change becomes a **background job** that calls the plugin's function, which gets
//! `{ watch, path, event, size, modified }` with `path` relative to the plugin's folder, ready to hand
//! to `fs::read`. Because it is a job it is retried if it fails, runs under the plugin's own
//! permissions, and cannot be queued twice for the same change even when several scheduler nodes
//! watch the same folder.
//!
//! Details that matter:
//!
//! * **Quiet first.** A file being copied produces a burst of events. Events for one path are merged
//!   and reported once the path has been quiet for `debounce_ms`: `created` then `modified` is one
//!   `created`; `created` then `deleted` is nothing; `deleted` then `created` is `modified`.
//! * **Not everything is reported:** names starting with `.`, ending in `~`, `.part`, `.tmp`, `.swp`,
//!   `.crdownload` or `.partial` (what editors and downloaders write while working), directories, and
//!   anything that resolves outside the plugin's folder (a symbolic link elsewhere).
//! * **Missed events.** A listener only sees changes while it runs, so the kernel remembers what each
//!   watch has reported (path, size, modification time, in `watch_seen`). When a node starts
//!   listening it compares the folder with that memory and reports what arrived, changed or went away
//!   in the meantime (`catch_up`, on by default; on the first start every file already there is new).
//!   Moving handled files away is still good practice, but nothing depends on it.
//! * **Network file systems** do not deliver notifications; set `poll_secs` to look at the folder at
//!   that interval instead.
//! * **Limits.** inotify has a per-user limit on watched folders (`fs.inotify.max_user_watches`);
//!   watching a very large tree recursively can hit it, and the error is logged.

use std::{
    collections::HashMap,
    path::{Path, PathBuf},
    time::{Duration, Instant},
};

use globset::{Glob, GlobBuilder, GlobMatcher};
use notify::{
    Config, EventKind, PollWatcher, RecommendedWatcher, RecursiveMode, Watcher,
    event::{AccessKind, AccessMode, ModifyKind, RenameMode},
};
use serde::Deserialize;
use serde_json::Value;
use surrealdb::{Surreal, engine::remote::ws::Client, types::SurrealValue};
use tokio::sync::mpsc;

use crate::{
    app_dir::AppDir,
    plugin_manager::models::plugin_def::PluginWatchDef,
    scheduler::queue::{self, NewJob},
    state::AppState,
};

use super::{PathError, relative_to, resolve};

#[derive(Debug, thiserror::Error)]
pub enum WatchError {
    #[error(transparent)]
    Path(#[from] PathError),
    #[error("the folder {path} cannot be prepared: {source}")]
    Folder {
        path: PathBuf,
        #[source]
        source: std::io::Error,
    },
    #[error("cannot listen to {path}: {source} (on Linux this can mean fs.inotify.max_user_watches is too low)")]
    Listen {
        path: PathBuf,
        #[source]
        source: notify::Error,
    },
    #[error("pattern `{0}` is not a valid glob")]
    Pattern(String),
}

/// What happened to a file.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Change {
    Created,
    Modified,
    Deleted,
}

impl Change {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Created => "created",
            Self::Modified => "modified",
            Self::Deleted => "deleted",
        }
    }
}

/// One thing a plugin is told about.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct WatchEvent {
    /// Relative to the plugin's folder.
    pub path: String,
    pub change: Change,
    pub size: Option<u64>,
    /// Modification time, nanoseconds since the epoch.
    pub modified_ns: Option<u128>,
    /// Whether the plugin asked to hear about this kind of change. Changes it did not ask about
    /// are still sent so the kernel's memory of the folder stays right.
    pub notify: bool,
}

/// What a watch last reported about a file.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Seen {
    pub size: u64,
    pub modified_ns: String,
}

/// Merges the many events one file produces into one, after it has been quiet for a while.
#[derive(Debug)]
pub struct Coalescer {
    debounce: Duration,
    pending: HashMap<String, (Change, Instant)>,
}

impl Coalescer {
    pub fn new(debounce: Duration) -> Self {
        Self { debounce, pending: HashMap::new() }
    }

    /// Something happened to `path` at `now`.
    pub fn note(&mut self, path: String, change: Change, now: Instant) {
        use Change::{Created, Deleted, Modified};
        let merged = match self.pending.get(&path).map(|(previous, _)| *previous) {
            None => Some(change),
            Some(Created) => match change {
                Created | Modified => Some(Created),
                // It came and went before anyone was told.
                Deleted => None,
            },
            Some(Modified) => match change {
                Deleted => Some(Deleted),
                _ => Some(Modified),
            },
            Some(Deleted) => match change {
                Created | Modified => Some(Modified),
                Deleted => Some(Deleted),
            },
        };
        match merged {
            Some(change) => {
                self.pending.insert(path, (change, now));
            }
            None => {
                self.pending.remove(&path);
            }
        }
    }

    /// The paths that have been quiet long enough, removed from the pending set.
    pub fn due(&mut self, now: Instant) -> Vec<(String, Change)> {
        let ready: Vec<String> = self
            .pending
            .iter()
            .filter(|(_, (_, last))| now.duration_since(*last) >= self.debounce)
            .map(|(path, _)| path.clone())
            .collect();
        let mut out: Vec<(String, Change)> =
            ready.into_iter().filter_map(|path| self.pending.remove(&path).map(|(change, _)| (path, change))).collect();
        out.sort_by(|a, b| a.0.cmp(&b.0));
        out
    }

    pub fn is_empty(&self) -> bool {
        self.pending.is_empty()
    }
}

/// Whether a file name is something editors and downloaders leave behind while working.
fn is_scratch(path: &str) -> bool {
    path.split('/').any(|part| part.starts_with('.'))
        || path.ends_with('~')
        || [".part", ".tmp", ".swp", ".crdownload", ".partial"].iter().any(|suffix| path.ends_with(suffix))
}

/// What to watch, resolved for one organization.
#[derive(Debug, Clone, PartialEq, Eq, Deserialize, SurrealValue)]
pub struct WatchSpec {
    pub plugin: String,
    pub name: String,
    pub path: String,
    pub function: String,
    pub events: Vec<String>,
    pub pattern: Option<String>,
    pub debounce_ms: i64,
    pub recursive: bool,
    pub catch_up: Option<bool>,
    pub poll_secs: Option<i64>,
    pub queue: String,
    pub max_attempts: i64,
}

impl WatchSpec {
    /// Rows made before catch-up existed have no value; the default is on.
    fn catches_up(&self) -> bool {
        self.catch_up.unwrap_or(true)
    }

    fn wants(&self, change: Change) -> bool {
        self.events.iter().any(|event| event == change.as_str())
    }
}

/// A running watch. Dropping it stops listening.
pub struct RunningWatch {
    // Held only to keep listening; the mutex makes the handle shareable between threads.
    _watcher: std::sync::Mutex<Box<dyn Watcher + Send>>,
    task: tokio::task::JoinHandle<()>,
}

impl Drop for RunningWatch {
    fn drop(&mut self) {
        self.task.abort();
    }
}

fn matcher(pattern: Option<&str>) -> Result<Option<GlobMatcher>, WatchError> {
    pattern
        .map(|pattern| {
            // `*` stays inside one folder level; `**` crosses them.
            GlobBuilder::new(pattern)
                .literal_separator(true)
                .build()
                .map(|glob| glob.compile_matcher())
                .map_err(|_| WatchError::Pattern(pattern.to_string()))
        })
        .transpose()
}

/// Start listening to the folder `spec` names inside `root` (the plugin's folder). Events arrive on
/// the returned channel once they are merged and quiet. `known` is what this watch reported before
/// (path to size and time); with `catch_up` the folder is compared with it first.
pub async fn start(
    spec: &WatchSpec,
    root: &Path,
    known: HashMap<String, Seen>,
) -> Result<(RunningWatch, mpsc::UnboundedReceiver<WatchEvent>), WatchError> {
    tokio::fs::create_dir_all(root).await.map_err(|source| WatchError::Folder { path: root.to_path_buf(), source })?;
    let watch_dir = {
        let wanted = root.join(&spec.path);
        tokio::fs::create_dir_all(&wanted).await.map_err(|source| WatchError::Folder { path: wanted.clone(), source })?;
        resolve(root, &spec.path).await?
    };
    let real_root = tokio::fs::canonicalize(root).await.map_err(|source| WatchError::Folder { path: root.to_path_buf(), source })?;
    let glob = matcher(spec.pattern.as_deref())?;
    let (raw_tx, mut raw_rx) = mpsc::unbounded_channel::<notify::Result<notify::Event>>();
    let handler = move |result: notify::Result<notify::Event>| {
        // The receiver is gone once the watch is dropped.
        let _ = raw_tx.send(result);
    };
    let mode = if spec.recursive { RecursiveMode::Recursive } else { RecursiveMode::NonRecursive };
    let mut watcher: Box<dyn Watcher + Send> = match spec.poll_secs {
        Some(seconds) => Box::new(
            PollWatcher::new(handler, Config::default().with_poll_interval(Duration::from_secs(u64::try_from(seconds).unwrap_or(5).max(1))))
                .map_err(|source| WatchError::Listen { path: watch_dir.clone(), source })?,
        ),
        None => Box::new(RecommendedWatcher::new(handler, Config::default()).map_err(|source| WatchError::Listen { path: watch_dir.clone(), source })?),
    };
    watcher.watch(&watch_dir, mode).map_err(|source| WatchError::Listen { path: watch_dir.clone(), source })?;

    let (tx, rx) = mpsc::unbounded_channel();
    let spec = spec.clone();
    let debounce = Duration::from_millis(u64::try_from(spec.debounce_ms).unwrap_or(500));
    let task = tokio::spawn(async move {
        let mut coalescer = Coalescer::new(debounce);
        let base = relative_to(&real_root, &watch_dir).unwrap_or_default();
        // Is this path (relative to the plugin's folder) one the watch is interested in?
        let interesting = |relative: &str| -> bool {
            if is_scratch(relative) {
                return false;
            }
            let below = match base.as_str() {
                "" => Some(relative),
                prefix => relative.strip_prefix(prefix).and_then(|rest| rest.strip_prefix('/')),
            };
            let Some(below) = below else { return false };
            glob.as_ref().is_none_or(|glob| glob.is_match(below))
        };
        if spec.catches_up() {
            let mut found = Vec::new();
            list_files(&watch_dir, spec.recursive, &mut found).await;
            let mut present = std::collections::HashSet::new();
            for file in found {
                let Some(relative) = relative_to(&real_root, &file).filter(|relative| interesting(relative)) else { continue };
                present.insert(relative.clone());
                match known.get(&relative) {
                    None => coalescer.note(relative, Change::Created, Instant::now()),
                    Some(seen) => {
                        let now = tokio::fs::metadata(&file).await.ok();
                        let size = now.as_ref().map(std::fs::Metadata::len);
                        let modified = now.as_ref().and_then(modified_ns);
                        if size != Some(seen.size) || modified.map(|ns| ns.to_string()).as_deref() != Some(seen.modified_ns.as_str()) {
                            coalescer.note(relative, Change::Modified, Instant::now());
                        }
                    }
                }
            }
            for gone in known.keys().filter(|path| !present.contains(*path) && interesting(path)) {
                coalescer.note(gone.clone(), Change::Deleted, Instant::now());
            }
        }
        let mut tick = tokio::time::interval((debounce / 2).clamp(Duration::from_millis(25), Duration::from_secs(1)));
        loop {
            tokio::select! {
                received = raw_rx.recv() => {
                    let Some(result) = received else { break };
                    match result {
                        Ok(event) => {
                            for (path, change) in classify(&event) {
                                if let Some(relative) = relative_to(&real_root, &path).filter(|relative| interesting(relative)) {
                                    coalescer.note(relative, change, Instant::now());
                                }
                            }
                        }
                        Err(error) => log::warn!("watch `{}` of `{}`: {error}", spec.name, spec.plugin),
                    }
                }
                _ = tick.tick() => {
                    for (relative, change) in coalescer.due(Instant::now()) {
                        let Some(mut event) = describe(&real_root, relative, change).await else { continue };
                        event.notify = spec.wants(change);
                        if tx.send(event).is_err() {
                            return;
                        }
                    }
                }
            }
        }
    });
    Ok((RunningWatch { _watcher: std::sync::Mutex::new(watcher), task }, rx))
}

/// What a raw file system event means for each path it names.
fn classify(event: &notify::Event) -> Vec<(PathBuf, Change)> {
    let paths = &event.paths;
    match &event.kind {
        EventKind::Create(_) => paths.iter().map(|path| (path.clone(), Change::Created)).collect(),
        EventKind::Remove(_) => paths.iter().map(|path| (path.clone(), Change::Deleted)).collect(),
        EventKind::Modify(ModifyKind::Name(RenameMode::From)) => paths.iter().map(|path| (path.clone(), Change::Deleted)).collect(),
        EventKind::Modify(ModifyKind::Name(RenameMode::To)) => paths.iter().map(|path| (path.clone(), Change::Created)).collect(),
        EventKind::Modify(ModifyKind::Name(RenameMode::Both)) => {
            let mut out = Vec::new();
            if let Some(from) = paths.first() {
                out.push((from.clone(), Change::Deleted));
            }
            if let Some(to) = paths.get(1) {
                out.push((to.clone(), Change::Created));
            }
            out
        }
        EventKind::Modify(_) => paths.iter().map(|path| (path.clone(), Change::Modified)).collect(),
        // The writer closed the file: the best sign that it is complete.
        EventKind::Access(AccessKind::Close(AccessMode::Write)) => paths.iter().map(|path| (path.clone(), Change::Modified)).collect(),
        _ => Vec::new(),
    }
}

/// Turn a settled change into the event a plugin gets, looking at the file as it is now. A file
/// that has gone again, a directory, or something that resolves outside the folder is not reported.
async fn describe(real_root: &Path, relative: String, change: Change) -> Option<WatchEvent> {
    if change == Change::Deleted {
        return Some(WatchEvent { path: relative, change, size: None, modified_ns: None, notify: true });
    }
    let full = real_root.join(&relative);
    let real = tokio::fs::canonicalize(&full).await.ok()?;
    if !real.starts_with(real_root) {
        return None;
    }
    let metadata = tokio::fs::metadata(&real).await.ok()?;
    if !metadata.is_file() {
        return None;
    }
    Some(WatchEvent { path: relative, change, size: Some(metadata.len()), modified_ns: modified_ns(&metadata), notify: true })
}

fn modified_ns(metadata: &std::fs::Metadata) -> Option<u128> {
    metadata
        .modified()
        .ok()
        .and_then(|time| time.duration_since(std::time::UNIX_EPOCH).ok())
        .map(|elapsed| elapsed.as_nanos())
}

async fn list_files(dir: &Path, recursive: bool, out: &mut Vec<PathBuf>) {
    let mut stack = vec![dir.to_path_buf()];
    while let Some(current) = stack.pop() {
        let Ok(mut entries) = tokio::fs::read_dir(&current).await else { continue };
        while let Ok(Some(entry)) = entries.next_entry().await {
            let Ok(kind) = entry.file_type().await else { continue };
            if kind.is_file() {
                out.push(entry.path());
            } else if kind.is_dir() && recursive {
                stack.push(entry.path());
            }
        }
    }
}

// ---- storing and running watches ----------------------------------------------------------

/// Make the organization's watches for `plugin` match the catalog version's `[[watch]]` entries.
/// Caller must be on the organization's database.
pub async fn sync_watches(db: &Surreal<Client>, plugin: &str, watches: Option<&[Value]>) -> Result<(), surrealdb::Error> {
    let defs: Vec<PluginWatchDef> =
        watches.unwrap_or_default().iter().filter_map(|value| serde_json::from_value(value.clone()).ok()).collect();
    for def in &defs {
        db.query(
            "UPSERT watches SET plugin = $plugin, name = $name, path = $path, function_name = $function, events = $events, \
             pattern = $pattern, debounce_ms = $debounce, recursive = $recursive, scan_on_start = $catch_up, catch_up = $catch_up, poll_secs = $poll, \
             queue = $queue, max_attempts = $attempts, enabled = true WHERE plugin = $plugin AND name = $name;",
        )
        .bind(("plugin", plugin.to_string()))
        .bind(("name", def.name.clone()))
        .bind(("path", def.path.clone()))
        .bind(("function", def.function.clone()))
        .bind(("events", def.events.clone()))
        .bind(("pattern", def.pattern.clone()))
        .bind(("debounce", i64::try_from(def.debounce_ms).unwrap_or(500)))
        .bind(("recursive", def.recursive))
        .bind(("catch_up", def.catch_up))
        .bind(("poll", def.poll_secs.and_then(|secs| i64::try_from(secs).ok())))
        .bind(("queue", def.queue.clone().unwrap_or_else(|| "default".into())))
        .bind(("attempts", def.max_attempts.unwrap_or(3)))
        .await?
        .check()?;
    }
    let keep: Vec<String> = defs.iter().map(|def| def.name.clone()).collect();
    // A watch the plugin no longer declares is forgotten, with what it had reported.
    db.query("DELETE watches WHERE plugin = $plugin AND name NOT IN $keep; DELETE watch_seen WHERE plugin = $plugin AND watch NOT IN $keep;")
        .bind(("plugin", plugin.to_string()))
        .bind(("keep", keep))
        .await?
        .check()?;
    Ok(())
}

struct Active {
    spec: WatchSpec,
    _running: RunningWatch,
    consumer: tokio::task::JoinHandle<()>,
}

impl Drop for Active {
    fn drop(&mut self) {
        self.consumer.abort();
    }
}

/// Keeps one listener per watch of every organization, in step with what is installed.
pub struct WatchManager {
    state: AppState,
    active: HashMap<(String, String, String), Active>,
}

impl WatchManager {
    pub fn new(state: AppState) -> Self {
        Self { state, active: HashMap::new() }
    }

    /// What the watch has reported before, from the organization's `watch_seen`.
    async fn load_seen(&self, org: &str, plugin: &str, watch: &str) -> HashMap<String, Seen> {
        #[derive(Deserialize, SurrealValue)]
        struct Row {
            path: String,
            size: i64,
            modified_ns: String,
        }
        let rows = async {
            let db = self.state.org(org).await?;
            let mut response = db
                .query("SELECT path, size, modified_ns FROM watch_seen WHERE plugin = $plugin AND watch = $watch;")
                .bind(("plugin", plugin.to_string()))
                .bind(("watch", watch.to_string()))
                .await?
                .check()?;
            response.take::<Vec<Row>>(0)
        }
        .await;
        match rows {
            Ok(rows) => rows
                .into_iter()
                .map(|row| (row.path, Seen { size: u64::try_from(row.size).unwrap_or(0), modified_ns: row.modified_ns }))
                .collect(),
            Err(error) => {
                log::warn!("{org}: what `{watch}` of `{plugin}` reported before could not be read: {error}");
                HashMap::new()
            }
        }
    }

    /// Stop listening to everything (the node is standing down or shutting down).
    pub fn stop_all(&mut self) {
        if !self.active.is_empty() {
            log::info!("Stopped {} file watch(es)", self.active.len());
        }
        self.active.clear();
    }

    /// Start, replace and stop listeners so they match the `watches` of `orgs`.
    pub async fn sync(&mut self, orgs: &[String]) {
        let mut wanted: HashMap<(String, String, String), WatchSpec> = HashMap::new();
        for org in orgs {
            let Ok(db) = self.state.org(org).await else { continue };
            let rows = db
                .query(
                    "SELECT plugin, name, path, function_name AS function, events, pattern, debounce_ms, recursive, catch_up, \
                     poll_secs, queue, max_attempts FROM watches WHERE enabled = true;",
                )
                .await
                .and_then(|response| response.check())
                .and_then(|mut response| response.take::<Vec<WatchSpec>>(0));
            let specs = match rows {
                Ok(specs) => specs,
                Err(error) => {
                    log::warn!("{org}: watches could not be read: {error}");
                    continue;
                }
            };
            if specs.is_empty() {
                continue;
            }
            let enabled: Vec<String> = db
                .query("SELECT VALUE plugin_name FROM installed_plugins WHERE is_enabled = true;")
                .await
                .and_then(|response| response.check())
                .and_then(|mut response| response.take(0))
                .unwrap_or_default();
            for spec in specs.into_iter().filter(|spec| enabled.contains(&spec.plugin)) {
                wanted.insert((org.clone(), spec.plugin.clone(), spec.name.clone()), spec);
            }
        }
        self.active.retain(|key, active| wanted.get(key).is_some_and(|spec| *spec == active.spec));
        for (key, spec) in wanted {
            if self.active.contains_key(&key) {
                continue;
            }
            let (org, plugin, name) = key.clone();
            let root = match AppDir::new(&self.state.config.app_dir).plugin_files_dir(&org, &plugin) {
                Ok(root) => root,
                Err(error) => {
                    log::warn!("{org}: watch `{name}` of `{plugin}` cannot be placed: {error}");
                    continue;
                }
            };
            let known = if spec.catches_up() { self.load_seen(&org, &plugin, &name).await } else { HashMap::new() };
            match start(&spec, &root, known).await {
                Ok((running, mut events)) => {
                    log::info!("{org}: watching {} for `{plugin}` ({name})", root.join(&spec.path).display());
                    let (state, consumer_spec, consumer_org) = (self.state.clone(), spec.clone(), org.clone());
                    let consumer = tokio::spawn(async move {
                        while let Some(event) = events.recv().await {
                            // The memory is updated only once the change is safely queued; if the
                            // database is unreachable the next start reports it again.
                            let queued = !event.notify || enqueue(&state, &consumer_org, &consumer_spec, &event).await;
                            if queued {
                                remember(&state, &consumer_org, &consumer_spec, &event).await;
                            }
                        }
                    });
                    self.active.insert(key, Active { spec, _running: running, consumer });
                }
                Err(error) => log::warn!("{org}: watch `{name}` of `{plugin}` could not start: {error}"),
            }
        }
    }
}

/// Queue the plugin's function for one settled change.
async fn enqueue(state: &AppState, org: &str, spec: &WatchSpec, event: &WatchEvent) -> bool {
    let slot = match (event.size, event.modified_ns) {
        (Some(size), Some(modified)) => format!("{size}:{modified}"),
        // A deletion has no size or time: one report per second is as precise as it gets.
        _ => format!("gone:{}", std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).map_or(0, |d| d.as_secs())),
    };
    let mut job = NewJob::new("plugin", spec.queue.clone());
    job.plugin = Some(spec.plugin.clone());
    job.function = Some(spec.function.clone());
    job.payload = Some(serde_json::json!({
        "watch": spec.name,
        "path": event.path.clone(),
        "event": event.change.as_str(),
        "size": event.size,
        "modified_ns": event.modified_ns.map(|ns| ns.to_string()),
    }));
    job.max_attempts = spec.max_attempts;
    job.unique_key = Some(format!("watch:{}:{}:{}:{slot}", spec.name, event.change.as_str(), event.path));
    job.enqueued_by = Some("system:watch".into());
    let db = match state.org(org).await {
        Ok(db) => db,
        Err(error) => {
            log::warn!("{org}: a file change could not be queued: {error}");
            return false;
        }
    };
    match queue::enqueue(&db, job).await {
        Ok(enqueued) => {
            if enqueued.created {
                state.scheduler.wake(org);
            }
            true
        }
        Err(error) => {
            log::warn!("{org}: a file change in `{}` could not be queued: {error}", spec.plugin);
            false
        }
    }
}

/// Record what the watch has now reported about a file (or forget it when it is gone).
async fn remember(state: &AppState, org: &str, spec: &WatchSpec, event: &WatchEvent) {
    let result = async {
        let db = state.org(org).await?;
        match (event.change, event.size, event.modified_ns) {
            (Change::Deleted, _, _) => {
                db.query("DELETE watch_seen WHERE plugin = $plugin AND watch = $watch AND path = $path;")
                    .bind(("plugin", spec.plugin.clone()))
                    .bind(("watch", spec.name.clone()))
                    .bind(("path", event.path.clone()))
                    .await?
                    .check()?;
            }
            (_, Some(size), Some(modified)) => {
                db.query(
                    "UPSERT watch_seen SET plugin = $plugin, watch = $watch, path = $path, size = $size, modified_ns = $modified \
                     WHERE plugin = $plugin AND watch = $watch AND path = $path;",
                )
                .bind(("plugin", spec.plugin.clone()))
                .bind(("watch", spec.name.clone()))
                .bind(("path", event.path.clone()))
                .bind(("size", i64::try_from(size).unwrap_or(i64::MAX)))
                .bind(("modified", modified.to_string()))
                .await?
                .check()?;
            }
            _ => {}
        }
        Ok::<(), surrealdb::Error>(())
    }
    .await;
    if let Err(error) = result {
        log::warn!("{org}: the memory of watch `{}` could not be updated: {error}", spec.name);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use Change::{Created, Deleted, Modified};

    fn spec(path: &str, pattern: Option<&str>, events: &[&str]) -> WatchSpec {
        WatchSpec {
            plugin: "invoices".into(),
            name: "inbox".into(),
            path: path.into(),
            function: "import".into(),
            events: events.iter().map(|e| e.to_string()).collect(),
            pattern: pattern.map(str::to_string),
            debounce_ms: 100,
            recursive: false,
            catch_up: Some(false),
            poll_secs: None,
            queue: "default".into(),
            max_attempts: 3,
        }
    }

    #[test]
    fn a_burst_of_events_for_one_file_becomes_one() {
        let start = Instant::now();
        let at = |ms: u64| start + Duration::from_millis(ms);
        let mut c = Coalescer::new(Duration::from_millis(100));
        c.note("a.csv".into(), Created, at(0));
        c.note("a.csv".into(), Modified, at(40));
        c.note("a.csv".into(), Modified, at(80));
        assert!(c.due(at(150)).is_empty(), "still being written: quiet only since 80 ms");
        assert_eq!(c.due(at(181)), [("a.csv".to_string(), Created)]);
        assert!(c.is_empty());
    }

    #[test]
    fn events_merge_by_what_the_plugin_needs_to_know() {
        let now = Instant::now();
        let merged = |first: Change, second: Change| {
            let mut c = Coalescer::new(Duration::ZERO);
            c.note("f".into(), first, now);
            c.note("f".into(), second, now);
            c.due(now).into_iter().map(|(_, change)| change).collect::<Vec<_>>()
        };
        assert_eq!(merged(Created, Modified), [Created]);
        assert_eq!(merged(Created, Deleted), [], "it came and went");
        assert_eq!(merged(Modified, Deleted), [Deleted]);
        assert_eq!(merged(Modified, Modified), [Modified]);
        assert_eq!(merged(Deleted, Created), [Modified], "replaced");
        assert_eq!(merged(Deleted, Deleted), [Deleted]);
    }

    #[test]
    fn different_files_settle_independently() {
        let start = Instant::now();
        let mut c = Coalescer::new(Duration::from_millis(100));
        c.note("old.csv".into(), Created, start);
        c.note("new.csv".into(), Created, start + Duration::from_millis(90));
        assert_eq!(c.due(start + Duration::from_millis(120)), [("old.csv".to_string(), Created)]);
        assert_eq!(c.due(start + Duration::from_millis(200)), [("new.csv".to_string(), Created)]);
    }

    #[test]
    fn what_editors_leave_behind_is_ignored() {
        for scratch in [".hidden", "dir/.hidden/a.csv", "a.csv~", "a.csv.part", "b.tmp", ".a.csv.swp", "x.crdownload", "y.partial"] {
            assert!(is_scratch(scratch), "{scratch}");
        }
        for fine in ["a.csv", "dir/a.csv", "report.partial.csv", "my.tmpfile"] {
            assert!(!is_scratch(fine), "{fine}");
        }
    }

    #[test]
    fn patterns_do_not_cross_folders_unless_asked() -> Result<(), WatchError> {
        let top = matcher(Some("*.csv"))?.ok_or(WatchError::Pattern(String::new()))?;
        assert!(top.is_match("a.csv") && !top.is_match("sub/a.csv") && !top.is_match("a.txt"));
        let deep = matcher(Some("**/*.csv"))?.ok_or(WatchError::Pattern(String::new()))?;
        assert!(deep.is_match("sub/a.csv") && deep.is_match("a/b/c.csv"));
        assert!(matcher(Some("[")).is_err(), "a broken glob is refused");
        Ok(())
    }

    // ---- against the real file system -----------------------------------------------------

    /// The next event the plugin asked to hear about.
    async fn next(events: &mut mpsc::UnboundedReceiver<WatchEvent>) -> Option<WatchEvent> {
        let deadline = tokio::time::Instant::now() + Duration::from_secs(5);
        loop {
            let event = tokio::time::timeout_at(deadline, events.recv()).await.ok().flatten()?;
            if event.notify {
                return Some(event);
            }
        }
    }

    /// Nothing the plugin asked to hear about arrives for a while.
    async fn quiet(events: &mut mpsc::UnboundedReceiver<WatchEvent>) -> bool {
        let deadline = tokio::time::Instant::now() + Duration::from_millis(600);
        loop {
            match tokio::time::timeout_at(deadline, events.recv()).await {
                Err(_) => return true,
                Ok(Some(event)) if event.notify => return false,
                Ok(Some(_)) => {}
                Ok(None) => return true,
            }
        }
    }

    #[tokio::test]
    async fn the_real_file_system_reports_one_event_per_settled_change() -> Result<(), Box<dyn std::error::Error>> {
        let dir = tempfile::tempdir()?;
        let root = dir.path().join("plugin");
        let (_running, mut events) = start(&spec("inbox", Some("*.csv"), &["created", "modified", "deleted"]), &root, HashMap::new()).await?;
        let inbox = root.join("inbox");

        // A file written in pieces is reported once, as created, with its final size.
        std::fs::write(inbox.join("a.csv"), "id,total\n")?;
        std::fs::write(inbox.join("a.csv"), "id,total\n1,500\n")?;
        let created = next(&mut events).await.ok_or("no event for the new file")?;
        assert_eq!((created.path.as_str(), created.change, created.size), ("inbox/a.csv", Change::Created, Some(15)));
        assert!(quiet(&mut events).await, "the burst must not be reported twice");

        // Things that do not match, or are scratch, or are folders, stay silent.
        std::fs::write(inbox.join("notes.txt"), "x")?;
        std::fs::write(inbox.join(".b.csv.swp"), "x")?;
        std::fs::write(inbox.join("c.csv.part"), "x")?;
        std::fs::create_dir(inbox.join("folder.csv"))?;
        assert!(quiet(&mut events).await, "nothing here is for the plugin");

        // A later change is `modified`; removing the file is `deleted`.
        std::fs::write(inbox.join("a.csv"), "id,total\n1,500\n2,700\n")?;
        let modified = next(&mut events).await.ok_or("no event for the change")?;
        assert_eq!((modified.path.as_str(), modified.change, modified.size), ("inbox/a.csv", Change::Modified, Some(21)));
        std::fs::remove_file(inbox.join("a.csv"))?;
        let deleted = next(&mut events).await.ok_or("no event for the deletion")?;
        assert_eq!((deleted.path.as_str(), deleted.change), ("inbox/a.csv", Change::Deleted));
        Ok(())
    }

    #[tokio::test]
    async fn only_the_wanted_events_are_reported_and_a_move_is_a_deletion_and_a_creation() -> Result<(), Box<dyn std::error::Error>> {
        let dir = tempfile::tempdir()?;
        let root = dir.path().join("plugin");
        let (_running, mut events) = start(&spec("", None, &["created"]), &root, HashMap::new()).await?;
        std::fs::write(root.join("one.txt"), "1")?;
        assert_eq!(next(&mut events).await.map(|e| e.path), Some("one.txt".into()));
        // A modification is not wanted.
        std::fs::write(root.join("one.txt"), "12")?;
        assert!(quiet(&mut events).await);
        // Renaming into the folder shows up as a new file; renaming out of it is a deletion nobody asked for.
        std::fs::write(dir.path().join("elsewhere.txt"), "x")?;
        std::fs::rename(dir.path().join("elsewhere.txt"), root.join("arrived.txt"))?;
        assert_eq!(next(&mut events).await.map(|e| e.path), Some("arrived.txt".into()));
        Ok(())
    }

    #[tokio::test]
    async fn on_the_first_start_every_file_already_there_is_new_and_subfolders_only_when_recursive() -> Result<(), Box<dyn std::error::Error>> {
        let dir = tempfile::tempdir()?;
        let root = dir.path().join("plugin");
        std::fs::create_dir_all(root.join("in/deeper"))?;
        std::fs::write(root.join("in/before.csv"), "1")?;
        std::fs::write(root.join("in/deeper/nested.csv"), "2")?;
        let mut wanted = spec("in", Some("**/*.csv"), &["created"]);
        wanted.catch_up = Some(true);
        wanted.recursive = true;
        let (_running, mut events) = start(&wanted, &root, HashMap::new()).await?;
        let mut seen = vec![
            next(&mut events).await.ok_or("no first scan event")?.path,
            next(&mut events).await.ok_or("no second scan event")?.path,
        ];
        seen.sort();
        assert_eq!(seen, ["in/before.csv", "in/deeper/nested.csv"]);
        // A new file in a subfolder is seen because the watch is recursive.
        std::fs::write(root.join("in/deeper/later.csv"), "3")?;
        assert_eq!(next(&mut events).await.map(|e| e.path), Some("in/deeper/later.csv".into()));
        Ok(())
    }

    fn seen_of(path: &Path) -> Result<Seen, Box<dyn std::error::Error>> {
        let metadata = std::fs::metadata(path)?;
        let modified = modified_ns(&metadata).ok_or("no modification time")?;
        Ok(Seen { size: metadata.len(), modified_ns: modified.to_string() })
    }

    #[tokio::test]
    async fn catch_up_reports_only_what_changed_while_nobody_was_listening() -> Result<(), Box<dyn std::error::Error>> {
        let dir = tempfile::tempdir()?;
        let root = dir.path().join("plugin");
        std::fs::create_dir_all(root.join("in"))?;
        std::fs::write(root.join("in/unchanged.csv"), "same")?;
        std::fs::write(root.join("in/changed.csv"), "before")?;
        // What the watch reported last time: the two files above, and one that has since been removed.
        let mut known = HashMap::new();
        known.insert("in/unchanged.csv".to_string(), seen_of(&root.join("in/unchanged.csv"))?);
        known.insert("in/changed.csv".to_string(), seen_of(&root.join("in/changed.csv"))?);
        known.insert("in/removed.csv".to_string(), Seen { size: 3, modified_ns: "1".into() });
        // While nothing listened: one file changed, one arrived.
        std::fs::write(root.join("in/changed.csv"), "after, and longer")?;
        std::fs::write(root.join("in/arrived.csv"), "new")?;

        let mut wanted = spec("in", Some("*.csv"), &["created", "modified", "deleted"]);
        wanted.catch_up = Some(true);
        let (_running, mut events) = start(&wanted, &root, known).await?;
        let mut got = Vec::new();
        for _ in 0..3 {
            let event = next(&mut events).await.ok_or("a catch-up event is missing")?;
            got.push((event.path, event.change));
        }
        got.sort_by(|a, b| a.0.cmp(&b.0));
        assert_eq!(
            got,
            [
                ("in/arrived.csv".to_string(), Change::Created),
                ("in/changed.csv".to_string(), Change::Modified),
                ("in/removed.csv".to_string(), Change::Deleted),
            ]
        );
        assert!(quiet(&mut events).await, "the unchanged file is not reported");
        Ok(())
    }

    #[tokio::test]
    async fn changes_the_plugin_did_not_ask_about_are_still_sent_for_the_memory() -> Result<(), Box<dyn std::error::Error>> {
        let dir = tempfile::tempdir()?;
        let root = dir.path().join("plugin");
        let (_running, mut events) = start(&spec("", None, &["created"]), &root, HashMap::new()).await?;
        std::fs::write(root.join("a.txt"), "1")?;
        let created = events.recv().await.ok_or("no event")?;
        assert!(created.notify && created.change == Change::Created);
        std::fs::write(root.join("a.txt"), "12")?;
        let modified = tokio::time::timeout(Duration::from_secs(5), events.recv()).await?.ok_or("no event")?;
        assert!(!modified.notify && modified.change == Change::Modified && modified.size == Some(2), "sent, but not for the plugin");
        Ok(())
    }

    #[tokio::test]
    async fn without_catch_up_only_live_changes_count() -> Result<(), Box<dyn std::error::Error>> {
        let dir = tempfile::tempdir()?;
        let root = dir.path().join("plugin");
        std::fs::create_dir_all(&root)?;
        std::fs::write(root.join("old.txt"), "x")?;
        let (_running, mut events) = start(&spec("", None, &["created"]), &root, HashMap::new()).await?;
        assert!(quiet(&mut events).await, "what was already there is left alone");
        std::fs::write(root.join("fresh.txt"), "x")?;
        assert_eq!(next(&mut events).await.map(|e| e.path), Some("fresh.txt".into()));
        Ok(())
    }

    #[cfg(unix)]
    #[tokio::test]
    async fn a_link_to_a_file_outside_the_folder_is_never_reported() -> Result<(), Box<dyn std::error::Error>> {
        let dir = tempfile::tempdir()?;
        let root = dir.path().join("plugin");
        let (_running, mut events) = start(&spec("", None, &["created", "modified"]), &root, HashMap::new()).await?;
        let secret = dir.path().join("secret.txt");
        std::fs::write(&secret, "classified")?;
        std::os::unix::fs::symlink(&secret, root.join("innocent.txt"))?;
        assert!(quiet(&mut events).await, "the link resolves outside the plugin's folder");
        std::fs::write(root.join("real.txt"), "ok")?;
        assert_eq!(next(&mut events).await.map(|e| e.path), Some("real.txt".into()));
        Ok(())
    }

    #[tokio::test]
    async fn polling_works_where_notifications_do_not() -> Result<(), Box<dyn std::error::Error>> {
        let dir = tempfile::tempdir()?;
        let root = dir.path().join("plugin");
        let mut polled = spec("", None, &["created"]);
        polled.poll_secs = Some(1);
        let (_running, mut events) = start(&polled, &root, HashMap::new()).await?;
        tokio::time::sleep(Duration::from_millis(300)).await;
        std::fs::write(root.join("found.txt"), "x")?;
        let event = tokio::time::timeout(Duration::from_secs(8), events.recv()).await?.ok_or("no polled event")?;
        assert_eq!(event.path, "found.txt");
        Ok(())
    }
}
