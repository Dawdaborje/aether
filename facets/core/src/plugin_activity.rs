//! The plugin activity log: who loaded a plugin into the catalog, and who installed or upgraded
//! it in an organization.
//!
//! One row per event in the core database, append-only and never trimmed. An organization's page
//! shows its own installs and upgrades together with the catalog loads (those apply to every
//! organization, so they are shown in all of them).
//!
//! Recording is best effort: a failure to write a log row is logged and never fails the work it
//! describes.

use axum::http::HeaderMap;
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};
use surrealdb::engine::remote::ws::Client;
use surrealdb::types::{SurrealValue, ToSql};
use surrealdb::Surreal;

use aether_orm::find_session_by_token;

use crate::access::identity::SESSION_COOKIE;
use crate::state::AppState;

/// What happened.
pub mod kind {
    /// New or changed files were read from disk (or a zip) into the catalog.
    pub const CATALOG_LOAD: &str = "catalog_load";
    /// A plugin was installed in an organization.
    pub const INSTALL: &str = "install";
    /// An installed plugin was moved to another revision in an organization.
    pub const UPGRADE: &str = "upgrade";
}

/// Who did it.
#[derive(Debug, Clone, Default)]
pub struct Actor {
    /// `user`, `system` (the CLI, the file watcher, the scheduler) or `unknown`.
    pub kind: String,
    /// `users:…` for a person.
    pub id: Option<String>,
    /// The username (or email) of a person; the label of a system actor (`cli:ann`).
    pub name: Option<String>,
}

impl Actor {
    pub fn system(label: &str) -> Self {
        Self {
            kind: "system".into(),
            id: None,
            name: Some(label.to_string()),
        }
    }

    /// The person behind a request's session cookie.
    pub async fn from_headers(state: &AppState, headers: &HeaderMap) -> Self {
        let cookie = headers
            .get(axum::http::header::COOKIE)
            .and_then(|v| v.to_str().ok())
            .unwrap_or("");
        let token = cookie.split(';').find_map(|part| {
            part.trim()
                .strip_prefix(&format!("{SESSION_COOKIE}="))
                .map(str::to_string)
                .filter(|s| !s.is_empty())
        });
        let Some(token) = token else {
            return Self {
                kind: "unknown".into(),
                ..Self::default()
            };
        };
        let Ok(core) = state.core().await else {
            return Self {
                kind: "unknown".into(),
                ..Self::default()
            };
        };
        match find_session_by_token(&core, &token).await {
            Ok(Some(session)) => Self {
                kind: "user".into(),
                id: Some(format!(
                    "{}:{}",
                    session.user.id.table.as_str(),
                    session.user.id.key.to_sql()
                )),
                name: session
                    .user
                    .username
                    .clone()
                    .or_else(|| session.user.email.clone()),
            },
            _ => Self {
                kind: "unknown".into(),
                ..Self::default()
            },
        }
    }
}

/// One event to record.
#[derive(Debug, Clone)]
pub struct Entry {
    pub kind: &'static str,
    pub plugin: String,
    pub from_version: Option<String>,
    pub to_version: Option<String>,
    /// The organization's database name; `None` for a catalog load.
    pub organization: Option<String>,
    pub actor: Actor,
    /// Where it was done from: `desk:reload`, `desk:upload`, `desk:install`, `cli`, `watch`, …
    pub source: String,
    pub ok: bool,
    pub error: Option<String>,
    /// Anything else worth keeping (revision, files written, seeds).
    pub detail: Value,
}

/// Write one row. Never fails the caller.
pub async fn record_in(db: &Surreal<Client>, namespace: &str, core_database: &str, entry: &Entry) {
    let result = async {
        db.use_ns(namespace).await?;
        db.use_db(core_database).await?;
        db.query(
            "CREATE plugin_activity SET at = time::now(), kind = $kind, plugin = $plugin, \
             from_version = $from, to_version = $to, organization = $org, \
             actor_kind = $actor_kind, actor_id = $actor_id, actor_name = $actor_name, \
             source = $source, ok = $ok, error = $error, detail = $detail;",
        )
        .bind(("kind", entry.kind.to_string()))
        .bind(("plugin", entry.plugin.clone()))
        .bind(("from", entry.from_version.clone()))
        .bind(("to", entry.to_version.clone()))
        .bind(("org", entry.organization.clone()))
        .bind(("actor_kind", entry.actor.kind.clone()))
        .bind(("actor_id", entry.actor.id.clone()))
        .bind(("actor_name", entry.actor.name.clone()))
        .bind(("source", entry.source.clone()))
        .bind(("ok", entry.ok))
        .bind(("error", entry.error.clone()))
        .bind(("detail", entry.detail.clone()))
        .await?
        .check()?;
        Ok::<(), surrealdb::Error>(())
    }
    .await;
    if let Err(error) = result {
        log::warn!(
            "plugin activity for `{}` could not be recorded: {error}",
            entry.plugin
        );
    }
}

/// Write one row, using the application's core connection.
pub async fn record(state: &AppState, entry: Entry) {
    match state.core().await {
        Ok(core) => record_in(&core, &state.namespace, &state.core_database, &entry).await,
        Err(error) => log::warn!("plugin activity could not be recorded: {error}"),
    }
}

/// An event as the web app shows it.
#[derive(Debug, Serialize, Deserialize, SurrealValue)]
pub struct Row {
    pub at: Option<String>,
    pub kind: String,
    pub plugin: String,
    #[serde(default)]
    pub from_version: Option<String>,
    #[serde(default)]
    pub to_version: Option<String>,
    #[serde(default)]
    pub organization: Option<String>,
    #[serde(default)]
    pub actor_kind: Option<String>,
    #[serde(default)]
    pub actor_id: Option<String>,
    #[serde(default)]
    pub actor_name: Option<String>,
    #[serde(default)]
    pub source: Option<String>,
    #[serde(default)]
    pub ok: Option<bool>,
    #[serde(default)]
    pub error: Option<String>,
}

/// The newest events of an organization: its own installs and upgrades and every catalog load.
pub async fn list_for_org(
    state: &AppState,
    organization: &str,
    limit: u32,
    offset: u32,
) -> Result<Value, String> {
    let core = state.core().await.map_err(|error| error.to_string())?;
    let limit = limit.clamp(1, 500);
    let mut response = core
        .query(
            "SELECT <string> at AS at, kind, plugin, from_version, to_version, organization, \
             actor_kind, actor_id, actor_name, source, ok, error FROM plugin_activity \
             WHERE organization = $org OR kind = 'catalog_load' \
             ORDER BY at DESC LIMIT $limit START $offset;",
        )
        .bind(("org", organization.to_string()))
        .bind(("limit", i64::from(limit)))
        .bind(("offset", i64::from(offset)))
        .await
        .and_then(|response| response.check())
        .map_err(|error| error.to_string())?;
    let rows: Vec<Row> = response.take(0).map_err(|error| error.to_string())?;
    Ok(json!({ "events": rows, "limit": limit, "offset": offset }))
}
