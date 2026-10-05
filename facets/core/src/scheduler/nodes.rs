//! Which processes run jobs, found through the core database.
//!
//! Each scheduler keeps a row in `scheduler_nodes` and refreshes it while it runs. Other
//! processes read the table: an embedded scheduler stands down while a standalone one is alive,
//! and the HTTP server learns where to send wake-ups.

use crate::state::Db;

/// A node that has not refreshed its row for this long is considered gone.
pub const ALIVE_WITHIN_SECS: i64 = 30;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum NodeKind {
    /// Inside `aether --serve`.
    Embedded,
    /// `aether --start-scheduler`.
    Standalone,
}

impl NodeKind {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Embedded => "embedded",
            Self::Standalone => "standalone",
        }
    }
}

/// Announce this node (or refresh its row).
pub async fn announce(
    core: &Db,
    node_id: &str,
    kind: NodeKind,
    address: Option<&str>,
    queues: &[String],
) -> Result<(), surrealdb::Error> {
    core.query(
        "UPSERT type::record('scheduler_nodes', $id) SET node_id = $id, kind = $kind, address = $address, \
         queues = $queues, version = $version, heartbeat_at = time::now(), \
         started_at = started_at ?? time::now();",
    )
    .bind(("id", node_id.to_string()))
    .bind(("kind", kind.as_str().to_string()))
    .bind(("address", address.map(str::to_string)))
    .bind(("queues", queues.to_vec()))
    .bind(("version", env!("CARGO_PKG_VERSION").to_string()))
    .await?
    .check()?;
    Ok(())
}

/// Remove this node's row on a clean shutdown.
pub async fn retire(core: &Db, node_id: &str) -> Result<(), surrealdb::Error> {
    core.query("DELETE type::record('scheduler_nodes', $id);")
        .bind(("id", node_id.to_string()))
        .await?
        .check()?;
    Ok(())
}

/// The control address of a live standalone scheduler, if there is one.
pub async fn standalone_address(core: &Db) -> Result<Option<String>, surrealdb::Error> {
    let mut response = core
        .query(
            "SELECT VALUE address FROM scheduler_nodes WHERE kind = 'standalone' AND address != NONE \
             AND heartbeat_at > time::now() - <duration> $window ORDER BY heartbeat_at DESC LIMIT 1;",
        )
        .bind(("window", format!("{ALIVE_WITHIN_SECS}s")))
        .await?
        .check()?;
    Ok(response.take::<Option<String>>(0)?)
}

/// Whether any standalone scheduler is alive (with or without an address).
pub async fn standalone_alive(core: &Db) -> Result<bool, surrealdb::Error> {
    let mut response = core
        .query(
            "SELECT VALUE node_id FROM scheduler_nodes WHERE kind = 'standalone' \
             AND heartbeat_at > time::now() - <duration> $window LIMIT 1;",
        )
        .bind(("window", format!("{ALIVE_WITHIN_SECS}s")))
        .await?
        .check()?;
    Ok(response.take::<Option<String>>(0)?.is_some())
}

/// Every node that is alive, for the status endpoint.
pub async fn alive(core: &Db) -> Result<Vec<serde_json::Value>, surrealdb::Error> {
    let mut response = core
        .query(
            "SELECT node_id, kind, address, queues, version, <string> started_at AS started_at, \
             <string> heartbeat_at AS heartbeat_at FROM scheduler_nodes \
             WHERE heartbeat_at > time::now() - <duration> $window ORDER BY started_at;",
        )
        .bind(("window", format!("{ALIVE_WITHIN_SECS}s")))
        .await?
        .check()?;
    Ok(response.take(0)?)
}
