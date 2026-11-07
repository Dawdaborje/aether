//! The watcher service: one listener per watch of every organization (the rest of watching is in
//! `aether_kernel::plugin_files::watch`).

use std::collections::HashMap;

use serde::Deserialize;
use surrealdb::types::SurrealValue;

use crate::{
    app_dir::AppDir,
    scheduler::queue::{self, NewJob},
    state::{AppState, Databases},
};
use aether_kernel::plugin_files::watch::*;

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
        Self {
            state,
            active: HashMap::new(),
        }
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
                .map(|row| {
                    (
                        row.path,
                        Seen {
                            size: u64::try_from(row.size).unwrap_or(0),
                            modified_ns: row.modified_ns,
                        },
                    )
                })
                .collect(),
            Err(error) => {
                log::warn!(
                    "{org}: what `{watch}` of `{plugin}` reported before could not be read: {error}"
                );
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
            let Ok(db) = self.state.org(org).await else {
                continue;
            };
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
            let enabled: Vec<String> =
                match crate::plugin_manager::installed::list_cached(&self.state.cache, org, &db)
                    .await
                {
                    Ok(rows) => rows
                        .into_iter()
                        .filter(|row| row.is_enabled)
                        .map(|row| row.plugin_name)
                        .collect(),
                    Err(error) => {
                        log::warn!("{org}: installed plugins could not be read: {error}");
                        continue;
                    }
                };
            for spec in specs
                .into_iter()
                .filter(|spec| enabled.contains(&spec.plugin))
            {
                wanted.insert((org.clone(), spec.plugin.clone(), spec.name.clone()), spec);
            }
        }
        self.active
            .retain(|key, active| wanted.get(key).is_some_and(|spec| *spec == active.spec));
        for (key, spec) in wanted {
            if self.active.contains_key(&key) {
                continue;
            }
            let (org, plugin, name) = key.clone();
            let root = match AppDir::new(&self.state.config.app_dir).plugin_files_dir(&org, &plugin)
            {
                Ok(root) => root,
                Err(error) => {
                    log::warn!("{org}: watch `{name}` of `{plugin}` cannot be placed: {error}");
                    continue;
                }
            };
            let known = if spec.catches_up() {
                self.load_seen(&org, &plugin, &name).await
            } else {
                HashMap::new()
            };
            match start(&spec, &root, known).await {
                Ok((running, mut events)) => {
                    log::info!(
                        "{org}: watching {} for `{plugin}` ({name})",
                        root.join(&spec.path).display()
                    );
                    let (state, consumer_spec, consumer_org) =
                        (self.state.clone(), spec.clone(), org.clone());
                    let consumer = tokio::spawn(async move {
                        while let Some(event) = events.recv().await {
                            // The memory is updated only once the change is safely queued; if the
                            // database is unreachable the next start reports it again.
                            let queued = !event.notify
                                || enqueue(&state, &consumer_org, &consumer_spec, &event).await;
                            if queued {
                                remember(state.databases(), &consumer_org, &consumer_spec, &event).await;
                            }
                        }
                    });
                    self.active.insert(
                        key,
                        Active {
                            spec,
                            _running: running,
                            consumer,
                        },
                    );
                }
                Err(error) => {
                    log::warn!("{org}: watch `{name}` of `{plugin}` could not start: {error}")
                }
            }
        }
    }
}

/// Queue the plugin's function for one settled change.
async fn enqueue(state: &AppState, org: &str, spec: &WatchSpec, event: &WatchEvent) -> bool {
    let slot = match (event.size, event.modified_ns) {
        (Some(size), Some(modified)) => format!("{size}:{modified}"),
        // A deletion has no size or time: one report per second is as precise as it gets.
        _ => format!(
            "gone:{}",
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .map_or(0, |d| d.as_secs())
        ),
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
    job.unique_key = Some(format!(
        "watch:{}:{}:{}:{slot}",
        spec.name,
        event.change.as_str(),
        event.path
    ));
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
            log::warn!(
                "{org}: a file change in `{}` could not be queued: {error}",
                spec.plugin
            );
            false
        }
    }
}

/// Record what the watch has now reported about a file (or forget it when it is gone).
async fn remember(state: &Databases, org: &str, spec: &WatchSpec, event: &WatchEvent) {
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
        log::warn!(
            "{org}: the memory of watch `{}` could not be updated: {error}",
            spec.name
        );
    }
}
