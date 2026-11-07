//! Taking a plugin out of an organization.
//!
//! There are three levels, from gentlest to final:
//!
//! * **Disable**: `installed_plugins.is_enabled = false`. Nothing is removed.
//! * **Uninstall, keeping the data** ([`uninstall_plugins`] with [`UninstallMode::KeepData`], the
//!   default): the plugin stops being installed (its pages, schedules, watches, listeners, inbound
//!   hooks and theme go) but the tables its models created, their records and their
//!   `model_schema` rows stay. The data is then *orphaned*: [`list_orphans`] shows it, and
//!   installing the plugin again picks it up where it was left.
//! * **Uninstall and purge** ([`UninstallMode::Purge`], or [`purge_orphans`] later): also drops
//!   the plugin's tables. Final; the caller backs the records up first ([`backup_tables`]).
//!
//! Ownership comes from `model_schema`, which the schema applier writes for every model it
//! creates a table for, so a purge can only ever touch tables a plugin's own models made.

use std::collections::{BTreeSet, HashSet};
use std::path::{Path, PathBuf};

use serde::Deserialize;
use serde_json::{Value, json};
use surrealdb::{Surreal, engine::remote::ws::Client, types::SurrealValue};
use thiserror::Error;

use super::mantle_signals::{self, SignalKind};
use crate::data_model::apply::AppliedModel;

#[derive(Debug, Error)]
pub enum UninstallError {
    #[error("database error: {0}")]
    Database(#[from] surrealdb::Error),

    #[error("organization `{0}` does not exist")]
    OrganizationNotFound(String),

    #[error("plugin `{0}` is not installed in this organization")]
    NotInstalled(String),

    #[error(
        "plugin `{plugin}` is needed by {}; uninstall {} first, or in the same command",
        .dependents.join(", "),
        if .dependents.len() == 1 { "it" } else { "them" }
    )]
    HasDependents {
        plugin: String,
        dependents: Vec<String>,
    },

    #[error(
        "plugin `{plugin}` is needed by {}, which {} enabled; disable {} first",
        .dependents.join(", "),
        if .dependents.len() == 1 { "is" } else { "are" },
        if .dependents.len() == 1 { "it" } else { "them" }
    )]
    EnabledDependents {
        plugin: String,
        dependents: Vec<String>,
    },

    #[error(
        "plugin `{plugin}` needs {}, which {} disabled; enable {} first",
        .dependencies.join(", "),
        if .dependencies.len() == 1 { "is" } else { "are" },
        if .dependencies.len() == 1 { "it" } else { "them" }
    )]
    DisabledDependencies {
        plugin: String,
        dependencies: Vec<String>,
    },

    #[error("plugin `{0}` is installed; uninstall it with --purge instead")]
    StillInstalled(String),

    #[error("plugin `{0}` left no data behind in this organization")]
    NothingToPurge(String),

    #[error("the stored schema of model `{model}` is unreadable: {source}")]
    Snapshot {
        model: String,
        #[source]
        source: serde_json::Error,
    },

    #[error("`{0}` is not a table name the kernel would have created; refusing to drop it")]
    UnsafeTable(String),

    #[error("could not write the backup {path}: {source}")]
    Backup {
        path: PathBuf,
        #[source]
        source: std::io::Error,
    },
}

/// What happens to the plugin's tables.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum UninstallMode {
    /// Leave tables and records in place, orphaned.
    #[default]
    KeepData,
    /// Drop the tables and forget their schema. The records are gone.
    Purge,
}

/// One table a plugin's model created.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct OwnedTable {
    /// The model's name inside the plugin (`invoice`).
    pub model: String,
    /// The table (`accounting_invoice`).
    pub table: String,
    /// Graph edge tables the model's hierarchy and many-to-many fields defined.
    pub edges: Vec<String>,
    pub rows: u64,
}

/// Everything a plugin left in an organization.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PluginData {
    pub plugin: String,
    pub tables: Vec<OwnedTable>,
}

impl PluginData {
    pub fn rows(&self) -> u64 {
        self.tables.iter().map(|table| table.rows).sum()
    }
}

/// Result of [`uninstall_plugins`].
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct UninstallReport {
    /// `(name, version)` removed, in removal order.
    pub uninstalled: Vec<(String, String)>,
    /// Data left orphaned (keep-data mode).
    pub kept: Vec<PluginData>,
    /// Data dropped (purge mode).
    pub purged: Vec<PluginData>,
}

#[derive(Debug, Deserialize, SurrealValue)]
struct InstalledRow {
    plugin_name: String,
    version: String,
}

#[derive(Debug, Deserialize, SurrealValue)]
struct SchemaRow {
    plugin: String,
    name: String,
    model_id: String,
    snapshot: Value,
}

/// A name SurrealQL can take unquoted; everything the schema applier creates qualifies.
fn is_safe_table(name: &str) -> bool {
    let mut chars = name.chars();
    chars
        .next()
        .is_some_and(|first| first.is_ascii_alphabetic() || first == '_')
        && chars.all(|c| c.is_ascii_alphanumeric() || c == '_')
}

async fn use_org(
    db: &Surreal<Client>,
    namespace: &str,
    core_database: &str,
    org_database: &str,
) -> Result<(), UninstallError> {
    db.use_ns(namespace).await?;
    db.use_db(core_database).await?;
    let mut response = db
        .query("SELECT VALUE db_name FROM org_databases WHERE db_name = $org LIMIT 1;")
        .bind(("org", org_database.to_string()))
        .await?
        .check()?;
    let found: Vec<String> = response.take(0)?;
    if found.is_empty() {
        return Err(UninstallError::OrganizationNotFound(org_database.to_string()));
    }
    db.use_db(org_database).await?;
    Ok(())
}

async fn installed(db: &Surreal<Client>) -> Result<Vec<InstalledRow>, UninstallError> {
    let mut response = db
        .query("SELECT plugin_name, version FROM installed_plugins;")
        .await?
        .check()?;
    Ok(response.take(0)?)
}

async fn row_count(db: &Surreal<Client>, table: &str) -> Result<u64, UninstallError> {
    let mut response = db
        .query("SELECT count() AS count FROM type::table($table) GROUP ALL;")
        .bind(("table", table.to_string()))
        .await?
        .check()?;
    let rows: Vec<Value> = response.take(0)?;
    Ok(rows
        .first()
        .and_then(|row| row.get("count"))
        .and_then(Value::as_u64)
        .unwrap_or(0))
}

/// The tables `plugin`'s models created in the organization `db` is on.
pub async fn owned_data(db: &Surreal<Client>, plugin: &str) -> Result<PluginData, UninstallError> {
    let mut response = db
        .query("SELECT plugin, name, model_id, snapshot FROM model_schema WHERE plugin = $plugin ORDER BY model_id;")
        .bind(("plugin", plugin.to_string()))
        .await?
        .check()?;
    let rows: Vec<SchemaRow> = response.take(0)?;
    let mut tables = Vec::with_capacity(rows.len());
    for row in rows {
        let snapshot: AppliedModel =
            serde_json::from_value(row.snapshot).map_err(|source| UninstallError::Snapshot {
                model: row.name.clone(),
                source,
            })?;
        tables.push(OwnedTable {
            rows: row_count(db, &row.model_id).await?,
            model: row.name,
            table: row.model_id,
            edges: snapshot.edges.into_iter().collect(),
        });
    }
    Ok(PluginData {
        plugin: plugin.to_string(),
        tables,
    })
}

/// Plugins that need `plugin` and are installed, by name.
async fn dependents_of(db: &Surreal<Client>, plugin: &str) -> Result<Vec<String>, UninstallError> {
    let mut response = db
        .query(
            "SELECT VALUE in.plugin_name FROM installed_plugin_depends_on \
             WHERE out = type::record('installed_plugins', $name);",
        )
        .bind(("name", plugin.to_string()))
        .await?
        .check()?;
    let mut names: Vec<String> = response.take(0)?;
    names.sort();
    names.dedup();
    Ok(names)
}

/// Check that every plugin in `names` can go (it is installed, and nothing installed outside
/// `names` needs it) and return `(name, version)` in an order that removes dependants first.
pub async fn check_uninstall(
    db: &Surreal<Client>,
    namespace: &str,
    core_database: &str,
    org_database: &str,
    names: &[String],
) -> Result<Vec<(String, String)>, UninstallError> {
    use_org(db, namespace, core_database, org_database).await?;
    plan(db, names).await
}

async fn plan(db: &Surreal<Client>, names: &[String]) -> Result<Vec<(String, String)>, UninstallError> {
    let installed = installed(db).await?;
    let going: HashSet<&str> = names.iter().map(String::as_str).collect();
    let mut chosen = Vec::new();
    for name in names {
        let Some(row) = installed.iter().find(|row| &row.plugin_name == name) else {
            return Err(UninstallError::NotInstalled(name.clone()));
        };
        let staying: Vec<String> = dependents_of(db, name)
            .await?
            .into_iter()
            .filter(|dependent| !going.contains(dependent.as_str()))
            .collect();
        if !staying.is_empty() {
            return Err(UninstallError::HasDependents {
                plugin: name.clone(),
                dependents: staying,
            });
        }
        if !chosen.iter().any(|(seen, _): &(String, String)| seen == name) {
            chosen.push((row.plugin_name.clone(), row.version.clone()));
        }
    }
    // Dependants first: a plugin goes after everything in the set that needs it.
    let mut ordered: Vec<(String, String)> = Vec::with_capacity(chosen.len());
    let mut remaining = chosen;
    while !remaining.is_empty() {
        let mut next = None;
        for (index, (name, _)) in remaining.iter().enumerate() {
            let needed_by_remaining = dependents_of(db, name)
                .await?
                .iter()
                .any(|dependent| remaining.iter().any(|(other, _)| other == dependent));
            if !needed_by_remaining {
                next = Some(index);
                break;
            }
        }
        // A dependency cycle cannot be installed, but never loop on one.
        ordered.push(remaining.remove(next.unwrap_or(0)));
    }
    Ok(ordered)
}

/// Remove plugins from the organization. See the module docs for what each mode leaves behind.
///
/// Callers run the plugins' `on_uninstall` commands *before* this (the plugin must still be
/// installed for its code to run), after [`check_uninstall`] says it can go.
pub async fn uninstall_plugins(
    db: &Surreal<Client>,
    namespace: &str,
    core_database: &str,
    org_database: &str,
    names: &[String],
    mode: UninstallMode,
) -> Result<UninstallReport, UninstallError> {
    use_org(db, namespace, core_database, org_database).await?;
    let ordered = plan(db, names).await?;
    let mut report = UninstallReport::default();
    for (name, version) in ordered {
        let data = owned_data(db, &name).await?;
        db.query(
            r#"
            BEGIN TRANSACTION;
            LET $record = type::record('installed_plugins', $name);
            DELETE installed_plugin_depends_on WHERE in = $record OR out = $record;
            DELETE ui_theme_config WHERE active_theme.plugin_name = $name;
            DELETE ui_themes WHERE plugin_name = $name;
            DELETE scheduled_tasks WHERE plugin = $name;
            DELETE watches WHERE plugin = $name;
            DELETE watch_seen WHERE plugin = $name;
            DELETE event_subscriptions WHERE plugin = $name;
            DELETE inbound_hooks WHERE plugin = $name;
            DELETE $record;
            COMMIT TRANSACTION;
            "#,
        )
        .bind(("name", name.clone()))
        .await?
        .check()?;
        if mode == UninstallMode::Purge {
            drop_tables(db, &data).await?;
            report.purged.push(data);
        } else if !data.tables.is_empty() {
            report.kept.push(data);
        }
        report.uninstalled.push((name, version));
    }
    announce(db, namespace, core_database, org_database, &report.uninstalled).await;
    Ok(report)
}

/// Plugins that are not installed but still own tables here, with what they own.
pub async fn list_orphans(
    db: &Surreal<Client>,
    namespace: &str,
    core_database: &str,
    org_database: &str,
) -> Result<Vec<PluginData>, UninstallError> {
    use_org(db, namespace, core_database, org_database).await?;
    let still: HashSet<String> = installed(db)
        .await?
        .into_iter()
        .map(|row| row.plugin_name)
        .collect();
    let mut response = db
        .query("SELECT VALUE plugin FROM model_schema;")
        .await?
        .check()?;
    let owners: Vec<String> = response.take(0)?;
    let orphans: BTreeSet<String> = owners
        .into_iter()
        .filter(|plugin| !still.contains(plugin))
        .collect();
    let mut found = Vec::with_capacity(orphans.len());
    for plugin in orphans {
        found.push(owned_data(db, &plugin).await?);
    }
    Ok(found)
}

/// What [`purge_orphans`] would drop for `plugin`, so the caller can show it and back it up
/// first. Fails when the plugin is still installed or owns nothing.
pub async fn orphan_data(
    db: &Surreal<Client>,
    namespace: &str,
    core_database: &str,
    org_database: &str,
    plugin: &str,
) -> Result<PluginData, UninstallError> {
    use_org(db, namespace, core_database, org_database).await?;
    if installed(db).await?.iter().any(|row| row.plugin_name == plugin) {
        return Err(UninstallError::StillInstalled(plugin.to_string()));
    }
    let data = owned_data(db, plugin).await?;
    if data.tables.is_empty() {
        return Err(UninstallError::NothingToPurge(plugin.to_string()));
    }
    Ok(data)
}

/// Drop the data an uninstalled plugin left behind. Final.
pub async fn purge_orphans(
    db: &Surreal<Client>,
    namespace: &str,
    core_database: &str,
    org_database: &str,
    plugin: &str,
) -> Result<PluginData, UninstallError> {
    let data = orphan_data(db, namespace, core_database, org_database, plugin).await?;
    drop_tables(db, &data).await?;
    Ok(data)
}

/// `REMOVE TABLE` for each table and edge table in `data`, and forget their schema. An edge table
/// another plugin's model also defined is left alone.
async fn drop_tables(db: &Surreal<Client>, data: &PluginData) -> Result<(), UninstallError> {
    let mut response = db
        .query("SELECT VALUE snapshot FROM model_schema WHERE plugin != $plugin;")
        .bind(("plugin", data.plugin.clone()))
        .await?
        .check()?;
    let others: Vec<Value> = response.take(0)?;
    let shared: HashSet<String> = others
        .into_iter()
        .filter_map(|snapshot| serde_json::from_value::<AppliedModel>(snapshot).ok())
        .flat_map(|snapshot| snapshot.edges)
        .collect();

    let mut script = String::from("BEGIN TRANSACTION;\n");
    for table in &data.tables {
        let edges = table.edges.iter().filter(|edge| !shared.contains(*edge));
        for name in std::iter::once(&table.table).chain(edges) {
            if !is_safe_table(name) {
                return Err(UninstallError::UnsafeTable(name.clone()));
            }
            script.push_str(&format!("REMOVE TABLE IF EXISTS {name};\n"));
        }
    }
    script.push_str("DELETE model_schema WHERE plugin = $plugin;\nCOMMIT TRANSACTION;");
    db.query(script)
        .bind(("plugin", data.plugin.clone()))
        .await?
        .check()?;
    Ok(())
}

/// Write every record of `data`'s tables to `<dir>/<org>.<plugin>.<unix seconds>.json` and return
/// the path. Run before a purge.
pub async fn backup_tables(
    db: &Surreal<Client>,
    org_database: &str,
    data: &PluginData,
    dir: &Path,
) -> Result<PathBuf, UninstallError> {
    let mut tables = serde_json::Map::new();
    for table in &data.tables {
        let mut response = db
            .query("SELECT * FROM type::table($table);")
            .bind(("table", table.table.clone()))
            .await?
            .check()?;
        let rows: Vec<Value> = response.take(0)?;
        tables.insert(table.table.clone(), Value::Array(rows));
    }
    let seconds = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|elapsed| elapsed.as_secs())
        .unwrap_or_default();
    let path = dir.join(format!("{org_database}.{}.{seconds}.json", data.plugin));
    let document = json!({
        "organization": org_database,
        "plugin": data.plugin,
        "tables": tables,
    });
    let io_error = |source| UninstallError::Backup {
        path: path.clone(),
        source,
    };
    tokio::fs::create_dir_all(dir).await.map_err(io_error)?;
    let text = serde_json::to_string_pretty(&document).unwrap_or_default();
    tokio::fs::write(&path, text).await.map_err(io_error)?;
    Ok(path)
}

/// Tell other nodes to drop what they cached about the installed set and the plugin's pages.
/// One plugin installed in an organization, with the installed plugins it is tied to.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct InstalledState {
    pub plugin: String,
    pub version: String,
    pub is_enabled: bool,
    /// Installed plugins this one needs.
    pub dependencies: Vec<String>,
    /// Installed plugins that need this one.
    pub dependents: Vec<String>,
}

#[derive(Debug, Deserialize, SurrealValue)]
struct StateRow {
    plugin_name: String,
    version: String,
    is_enabled: bool,
}

/// Plugins that `plugin` needs and are installed, by name.
async fn dependencies_of(db: &Surreal<Client>, plugin: &str) -> Result<Vec<String>, UninstallError> {
    let mut response = db
        .query(
            "SELECT VALUE out.plugin_name FROM installed_plugin_depends_on \
             WHERE in = type::record('installed_plugins', $name);",
        )
        .bind(("name", plugin.to_string()))
        .await?
        .check()?;
    let mut names: Vec<String> = response.take(0)?;
    names.sort();
    names.dedup();
    Ok(names)
}

/// Every plugin installed in the organization, by name, with its dependency ties.
pub async fn installed_states(
    db: &Surreal<Client>,
    namespace: &str,
    core_database: &str,
    org_database: &str,
) -> Result<Vec<InstalledState>, UninstallError> {
    use_org(db, namespace, core_database, org_database).await?;
    let mut response = db
        .query("SELECT plugin_name, version, is_enabled FROM installed_plugins ORDER BY plugin_name;")
        .await?
        .check()?;
    let rows: Vec<StateRow> = response.take(0)?;
    let mut states = Vec::with_capacity(rows.len());
    for row in rows {
        states.push(InstalledState {
            dependencies: dependencies_of(db, &row.plugin_name).await?,
            dependents: dependents_of(db, &row.plugin_name).await?,
            plugin: row.plugin_name,
            version: row.version,
            is_enabled: row.is_enabled,
        });
    }
    Ok(states)
}

/// Switch an installed plugin on or off, keeping everything it has. A plugin cannot be disabled
/// while an enabled plugin needs it, nor enabled while one it needs is disabled. Returns the
/// installed version.
pub async fn set_enabled(
    db: &Surreal<Client>,
    namespace: &str,
    core_database: &str,
    org_database: &str,
    plugin: &str,
    enabled: bool,
) -> Result<String, UninstallError> {
    let states = installed_states(db, namespace, core_database, org_database).await?;
    let Some(state) = states.iter().find(|state| state.plugin == plugin) else {
        return Err(UninstallError::NotInstalled(plugin.to_string()));
    };
    let is_enabled = |name: &String| states.iter().any(|s| &s.plugin == name && s.is_enabled);
    if enabled {
        let off: Vec<String> = state
            .dependencies
            .iter()
            .filter(|name| !is_enabled(name))
            .cloned()
            .collect();
        if !off.is_empty() {
            return Err(UninstallError::DisabledDependencies {
                plugin: plugin.to_string(),
                dependencies: off,
            });
        }
    } else {
        let on: Vec<String> = state.dependents.iter().filter(|name| is_enabled(name)).cloned().collect();
        if !on.is_empty() {
            return Err(UninstallError::EnabledDependents {
                plugin: plugin.to_string(),
                dependents: on,
            });
        }
    }
    if state.is_enabled != enabled {
        db.query("UPDATE type::record('installed_plugins', $name) SET is_enabled = $enabled;")
            .bind(("name", plugin.to_string()))
            .bind(("enabled", enabled))
            .await?
            .check()?;
        announce(
            db,
            namespace,
            core_database,
            org_database,
            &[(state.plugin.clone(), state.version.clone())],
        )
        .await;
    }
    Ok(state.version.clone())
}

async fn announce(
    db: &Surreal<Client>,
    namespace: &str,
    core_database: &str,
    org_database: &str,
    removed: &[(String, String)],
) {
    if removed.is_empty() {
        return;
    }
    mantle_signals::emit(
        db,
        namespace,
        core_database,
        SignalKind::Installed,
        None,
        None,
        Some(org_database),
    )
    .await;
    for (name, version) in removed {
        mantle_signals::emit(
            db,
            namespace,
            core_database,
            SignalKind::PluginSurface,
            Some(name),
            Some(version),
            None,
        )
        .await;
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn only_plain_identifiers_may_be_dropped() {
        assert!(is_safe_table("accounting_invoice"));
        assert!(is_safe_table("_x1"));
        assert!(!is_safe_table(""));
        assert!(!is_safe_table("1table"));
        assert!(!is_safe_table("a; REMOVE TABLE installed_plugins"));
        assert!(!is_safe_table("a`b"));
        assert!(!is_safe_table("a-b"));
    }

    #[test]
    fn dependents_are_named_in_the_message() {
        let error = UninstallError::HasDependents {
            plugin: "currency".into(),
            dependents: vec!["accounting".into(), "sales".into()],
        };
        let text = error.to_string();
        assert!(text.contains("accounting, sales") && text.contains("them"), "{text}");
    }
}
