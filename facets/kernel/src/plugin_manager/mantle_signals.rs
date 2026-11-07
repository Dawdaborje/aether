//! DB-backed Mantle invalidation for out-of-process CLI catalog changes.
//!
//! The CLI and `--serve` do not share Mantle memory. After `--load-plugins` /
//! `--upgrade-plugin`, emit a row here; the serving process listens with a Surreal
//! WebSocket `LIVE SELECT` and busts warm pages / installed_plugins immediately.

use futures::StreamExt;
use serde::Deserialize;
use surrealdb::{
    Notification, Surreal,
    engine::remote::ws::Client,
    types::{Action, SurrealValue},
};

use crate::cache::Cache;
use crate::plugin_manager::{installed, surface};

/// Backoff when the live query drops (reconnect).
const RECONNECT_WAIT: std::time::Duration = std::time::Duration::from_secs(2);

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SignalKind {
    /// Bust Tier B pages/components/i18n/public for one catalog version.
    PluginSurface,
    /// Bust org `installed_plugins` (+ apps launcher).
    Installed,
    /// A table was written on another node: its cached rows and queries are stale here.
    /// `plugin` carries the table name (`*` for "anything in the organization").
    TableWrite,
}

impl SignalKind {
    fn as_str(self) -> &'static str {
        match self {
            Self::PluginSurface => "plugin_surface",
            Self::Installed => "installed",
            Self::TableWrite => "table_write",
        }
    }

    fn parse(raw: &str) -> Option<Self> {
        match raw {
            "plugin_surface" => Some(Self::PluginSurface),
            "installed" => Some(Self::Installed),
            "table_write" => Some(Self::TableWrite),
            _ => None,
        }
    }
}

#[derive(Debug, Clone, Deserialize, SurrealValue)]
struct SignalRow {
    id: Option<surrealdb::types::RecordId>,
    kind: String,
    plugin: Option<String>,
    version: Option<String>,
    org: Option<String>,
    /// The process that wrote the row; it has already invalidated its own memory.
    origin: Option<String>,
}

/// Identifies this process among the nodes that share a database.
pub fn node_id() -> &'static str {
    static NODE: std::sync::OnceLock<String> = std::sync::OnceLock::new();
    NODE.get_or_init(|| {
        use rand::RngExt;
        let mut bytes = [0u8; 8];
        rand::rng().fill(&mut bytes);
        bytes.iter().map(|b| format!("{b:02x}")).collect()
    })
}

/// Record a signal in the core database (best-effort; never fails the caller).
pub async fn emit(
    db: &Surreal<Client>,
    namespace: &str,
    core_database: &str,
    kind: SignalKind,
    plugin: Option<&str>,
    version: Option<&str>,
    org: Option<&str>,
) {
    if let Err(error) = emit_inner(db, namespace, core_database, kind, plugin, version, org).await {
        log::warn!("mantle signal emit ({}): {error}", kind.as_str());
    }
}

async fn emit_inner(
    db: &Surreal<Client>,
    namespace: &str,
    core_database: &str,
    kind: SignalKind,
    plugin: Option<&str>,
    version: Option<&str>,
    org: Option<&str>,
) -> Result<(), surrealdb::Error> {
    db.use_ns(namespace).await?;
    db.use_db(core_database).await?;
    db.query(
        "CREATE mantle_signals SET \
            kind = $kind, \
            plugin = $plugin, \
            version = $version, \
            org = $org, \
            origin = $origin, \
            created_at = time::now();",
    )
    .bind(("origin", node_id().to_string()))
    .bind(("kind", kind.as_str().to_string()))
    .bind(("plugin", plugin.map(str::to_string)))
    .bind(("version", version.map(str::to_string)))
    .bind(("org", org.map(str::to_string)))
    .await?
    .check()?;
    Ok(())
}

/// How long writes to one table are folded into a single announcement.
const COALESCE: std::time::Duration = std::time::Duration::from_millis(250);

type Pending = std::sync::Mutex<std::collections::HashMap<(String, String), bool>>;

fn pending() -> &'static Pending {
    static PENDING: std::sync::OnceLock<Pending> = std::sync::OnceLock::new();
    PENDING.get_or_init(Default::default)
}

/// Tell the other nodes that `table` of `org` was written (`*`: anything in it).
///
/// The first write of a burst is announced at once; later writes of the same table within the
/// window are folded into one more announcement when it closes, so a bulk import costs a few
/// inserts, not one per row — and the last write is always announced after it happened.
/// Best effort and off the request path.
pub fn announce_table_write(core: crate::databases::Db, org: &str, table: &str) {
    let Ok(runtime) = tokio::runtime::Handle::try_current() else {
        return;
    };
    let key = (org.to_string(), table.to_string());
    {
        let mut map = pending().lock().unwrap_or_else(|e| e.into_inner());
        if let Some(again) = map.get_mut(&key) {
            *again = true;
            return;
        }
        map.insert(key.clone(), false);
    }
    runtime.spawn(async move {
        loop {
            if let Err(error) = create_table_write(&core, &key.0, &key.1).await {
                log::debug!("mantle table-write signal: {error}");
            }
            tokio::time::sleep(COALESCE).await;
            let mut map = pending().lock().unwrap_or_else(|e| e.into_inner());
            match map.get(&key) {
                Some(true) => {
                    map.insert(key.clone(), false);
                }
                _ => {
                    map.remove(&key);
                    break;
                }
            }
        }
    });
}

async fn create_table_write(
    core: &Surreal<Client>,
    org: &str,
    table: &str,
) -> Result<(), surrealdb::Error> {
    core.query(
        "CREATE mantle_signals SET kind = 'table_write', plugin = $table, org = $org, \
         origin = $origin, created_at = time::now();",
    )
    .bind(("table", table.to_string()))
    .bind(("org", org.to_string()))
    .bind(("origin", node_id().to_string()))
    .await?
    .check()?;
    Ok(())
}

/// Apply pending rows (catch-up after connect / reconnect), then delete them.
pub async fn drain(db: &Surreal<Client>, cache: &Cache) -> Result<usize, surrealdb::Error> {
    let mut response = db
        .query(
            "SELECT id, kind, plugin, version, org, origin, created_at FROM mantle_signals ORDER BY created_at ASC LIMIT 200;",
        )
        .await?
        .check()?;
    let rows: Vec<SignalRow> = response.take(0)?;
    if rows.is_empty() {
        return Ok(0);
    }

    for row in &rows {
        apply_one(cache, row);
        delete_row(db, row).await?;
    }
    Ok(rows.len())
}

async fn delete_row(db: &Surreal<Client>, row: &SignalRow) -> Result<(), surrealdb::Error> {
    let Some(id) = &row.id else {
        return Ok(());
    };
    db.query("DELETE $id;")
        .bind(("id", id.clone()))
        .await?
        .check()?;
    Ok(())
}

fn apply_one(cache: &Cache, row: &SignalRow) {
    // This process already invalidated its own memory when it wrote.
    if row.origin.as_deref() == Some(node_id()) {
        return;
    }
    let Some(kind) = SignalKind::parse(&row.kind) else {
        log::debug!("mantle signal: unknown kind `{}`", row.kind);
        return;
    };
    match kind {
        SignalKind::PluginSurface => {
            let (Some(plugin), Some(version)) = (row.plugin.as_deref(), row.version.as_deref())
            else {
                return;
            };
            surface::invalidate(cache, plugin, version);
            log::debug!("mantle signal: busted surface {plugin}@{version}");
        }
        SignalKind::Installed => {
            let Some(org) = row.org.as_deref() else {
                return;
            };
            installed::invalidate(cache, org);
            log::debug!("mantle signal: busted installed_plugins for {org}");
        }
        SignalKind::TableWrite => {
            let (Some(org), Some(table)) = (row.org.as_deref(), row.plugin.as_deref()) else {
                return;
            };
            if table == "*" {
                cache.bump_org_gen(org);
            } else {
                cache.bump_table_gen(org, table);
            }
        }
    }
}

/// Listen for new `mantle_signals` over the Surreal WebSocket (`LIVE SELECT`).
///
/// On each create: bust Mantle, then delete the row (ack). Drain once on connect
/// so signals emitted while this process was down are not missed. Reconnects on
/// stream end.
pub fn spawn_listener(databases: crate::databases::Databases, cache: Cache) -> tokio::task::JoinHandle<()> {
    tokio::spawn(async move {
        loop {
            match run_live_session(&databases, &cache).await {
                Ok(()) => log::debug!("mantle signal live query ended; reconnecting"),
                Err(error) => {
                    log::debug!("mantle signal live query: {error}; retrying in {RECONNECT_WAIT:?}")
                }
            }
            tokio::time::sleep(RECONNECT_WAIT).await;
        }
    })
}

async fn run_live_session(databases: &crate::databases::Databases, cache: &Cache) -> Result<(), surrealdb::Error> {
    let core = databases.core().await?;
    // Catch anything written while we were offline or between reconnects.
    match drain(core.as_ref(), cache).await {
        Ok(0) => {}
        Ok(n) => log::info!("mantle: applied {n} pending cache signal(s)"),
        Err(error) => log::debug!("mantle signal drain: {error}"),
    }

    let mut stream = core.select("mantle_signals").live().await?;
    while let Some(item) = stream.next().await {
        let Notification { action, data, .. } = match item {
            Ok(note) => note,
            Err(error) => {
                log::warn!("mantle signal live notification: {error}");
                continue;
            }
        };
        // Only CREATE matters; our own DELETE ack would otherwise re-enter.
        if action != Action::Create {
            continue;
        }
        let row: SignalRow = data;
        apply_one(cache, &row);
        if let Err(error) = delete_row(core.as_ref(), &row).await {
            log::debug!("mantle signal ack delete: {error}");
        } else {
            log::info!("mantle: applied live cache signal ({})", row.kind);
        }
    }
    Ok(())
}

/// How often the organizations are rescanned for new or removed ones.
const ORG_SCAN: std::time::Duration = std::time::Duration::from_secs(30);

/// The tables of an organization whose rows feed the role and permission caches, and what a
/// write to each makes stale.
const WATCHED: [&str; 3] = ["roles", "org_user_roles", "installed_plugins"];

/// Tables of the core database that role placement and permissions are read from: the plugin
/// catalog (labels, workspaces, and the rule files of a rebuilt plugin). A write makes every
/// organization's role catalog and permissions stale.
const WATCHED_CORE: [&str; 2] = ["plugins", "plugin_workspaces"];

/// Keep the role catalog, per-person roles and role permissions in step with the database itself.
///
/// For every organization, a `LIVE SELECT` on [`WATCHED`] busts the matching Mantle entries on any
/// write, whoever made it: this process, another node, the CLI or a hand-run query. The code paths
/// that invalidate directly stay as they are (they are instant); this catches the rest.
/// Organizations that appear later are picked up at the next scan, and a dropped stream reconnects.
pub fn spawn_role_watcher(
    databases: crate::databases::Databases,
    cache: Cache,
) -> tokio::task::JoinHandle<()> {
    tokio::spawn(async move {
        let mut watching: std::collections::HashMap<String, Vec<tokio::task::JoinHandle<()>>> =
            Default::default();
        let _core_watchers: Vec<_> = WATCHED_CORE
            .iter()
            .map(|table| {
                tokio::spawn(watch_core_table(databases.clone(), cache.clone(), table))
            })
            .collect();
        loop {
            match org_names(&databases).await {
                Ok(names) => {
                    watching.retain(|org, tasks| {
                        let keep = names.contains(org);
                        if !keep {
                            tasks.iter().for_each(|task| task.abort());
                        }
                        keep
                    });
                    for org in names {
                        watching.entry(org.clone()).or_insert_with(|| {
                            WATCHED
                                .iter()
                                .map(|table| {
                                    tokio::spawn(watch_table(
                                        databases.clone(),
                                        cache.clone(),
                                        org.clone(),
                                        table,
                                    ))
                                })
                                .collect()
                        });
                    }
                }
                Err(error) => log::debug!("mantle role watcher: organizations: {error}"),
            }
            tokio::time::sleep(ORG_SCAN).await;
        }
    })
}

async fn org_names(
    databases: &crate::databases::Databases,
) -> Result<std::collections::HashSet<String>, surrealdb::Error> {
    let core = databases.core().await?;
    let mut response = core.query("SELECT VALUE db_name FROM org_databases;").await?.check()?;
    let names: Vec<String> = response.take(0)?;
    Ok(names.into_iter().collect())
}

async fn watch_table(
    databases: crate::databases::Databases,
    cache: Cache,
    org: String,
    table: &'static str,
) {
    loop {
        if let Err(error) = watch_table_once(&databases, &cache, &org, table).await {
            log::debug!("mantle role watcher {org}.{table}: {error}");
        }
        tokio::time::sleep(RECONNECT_WAIT).await;
    }
}

async fn watch_table_once(
    databases: &crate::databases::Databases,
    cache: &Cache,
    org: &str,
    table: &'static str,
) -> Result<(), surrealdb::Error> {
    let db = databases.org(org).await?;
    let mut stream = db.select(table).live().await?;
    // The stream starts now: anything written while it was down is covered by one bust.
    bust_for_table(cache, org, table);
    while let Some(item) = stream.next().await {
        let note: Notification<surrealdb::types::Value> = match item {
            Ok(note) => note,
            Err(error) => {
                log::debug!("mantle role watcher {org}.{table}: {error}");
                continue;
            }
        };
        let _ = note.action;
        bust_for_table(cache, org, table);
    }
    Ok(())
}

async fn watch_core_table(
    databases: crate::databases::Databases,
    cache: Cache,
    table: &'static str,
) {
    loop {
        if let Err(error) = watch_core_table_once(&databases, &cache, table).await {
            log::debug!("mantle role watcher core.{table}: {error}");
        }
        tokio::time::sleep(RECONNECT_WAIT).await;
    }
}

async fn watch_core_table_once(
    databases: &crate::databases::Databases,
    cache: &Cache,
    table: &'static str,
) -> Result<(), surrealdb::Error> {
    let core = databases.core().await?;
    let mut stream = core.select(table).live().await?;
    bust_all_orgs(databases, cache, table).await;
    while let Some(item) = stream.next().await {
        let _: Notification<surrealdb::types::Value> = match item {
            Ok(note) => note,
            Err(error) => {
                log::debug!("mantle role watcher core.{table}: {error}");
                continue;
            }
        };
        bust_all_orgs(databases, cache, table).await;
    }
    Ok(())
}

async fn bust_all_orgs(databases: &crate::databases::Databases, cache: &Cache, table: &str) {
    match org_names(databases).await {
        Ok(names) => {
            for org in &names {
                let _ = cache.warm_invalidate_role_catalog(org);
            }
            log::debug!("mantle: core.{table} written; role caches busted for {} org(s)", names.len());
        }
        Err(error) => log::debug!("mantle role watcher: organizations: {error}"),
    }
}

fn bust_for_table(cache: &Cache, org: &str, table: &str) {
    match table {
        "installed_plugins" => installed::invalidate(cache, org),
        _ => {
            let _ = cache.warm_invalidate_all_roles(org);
        }
    }
    log::debug!("mantle: {org}.{table} written; role caches busted");
}

#[cfg(test)]
mod tests {
    use super::*;

    fn cache() -> Cache {
        crate::cache::CacheConfig {
            default_ttl_secs: None,
            ..Default::default()
        }
        .build()
        .unwrap()
    }

    fn row(origin: Option<&str>, table: &str) -> SignalRow {
        SignalRow {
            id: None,
            kind: "table_write".into(),
            plugin: Some(table.into()),
            version: None,
            org: Some("acme".into()),
            origin: origin.map(str::to_string),
        }
    }

    #[test]
    fn another_nodes_table_write_makes_local_entries_stale() {
        let cache = cache();
        let before = cache.table_gen("acme", "contact");
        apply_one(&cache, &row(Some("some-other-node"), "contact"));
        assert!(cache.table_gen("acme", "contact") > before);
        let deal = cache.table_gen("acme", "deal");
        apply_one(&cache, &row(Some("some-other-node"), "*"));
        assert!(cache.table_gen("acme", "deal") > deal);
    }

    #[test]
    fn this_nodes_own_signal_is_not_applied_twice() {
        let cache = cache();
        let before = cache.table_gen("acme", "contact");
        apply_one(&cache, &row(Some(node_id()), "contact"));
        assert_eq!(cache.table_gen("acme", "contact"), before);
    }
}
