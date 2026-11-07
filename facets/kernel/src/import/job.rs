//! An import as a job: the file is read and its rows applied in slices, each slice a scheduler job that
//! queues the next, so a long import never holds a worker past its lease and survives a restart (it
//! carries on from the last row it finished). See `docs/architecture/import_export.md`.
//!
//! Rows are applied **as the person who started the import** (their record and field rules, the model's
//! checks), through the plugin that started it, so an import can do only what that person could do by
//! hand. A dry run takes every write back; what it says is what a real run would do.

use std::time::Duration;

use serde_json::{Value, json};

use crate::scheduler::queue::{self, NewJob};
use crate::databases::Db;

pub const KIND: &str = "import";
pub const QUEUE: &str = "import";
/// How long one slice of an import runs before it queues the next.
pub const SLICE: Duration = Duration::from_secs(90);

pub type Failure = (String, bool);

/// The limits for an organization (settings `import.*`).
#[derive(Debug, Clone, Copy)]
pub struct Settings {
    pub max_bytes: u64,
    pub max_rows: usize,
    pub batch: usize,
}

pub async fn settings(db: &Db, core: Option<&Db>) -> Settings {
    let keys = ["import.max_mb", "import.max_rows", "import.batch_size"];
    let values = crate::kernel::host::settings_read::effective_in(db, core, &keys).await;
    let number = |key: &str, default: u64| values.get(key).and_then(Value::as_u64).filter(|n| *n > 0).unwrap_or(default);
    Settings {
        max_bytes: number("import.max_mb", 50) * 1024 * 1024,
        max_rows: number("import.max_rows", 1_000_000) as usize,
        batch: number("import.batch_size", 500).clamp(1, 5000) as usize,
    }
}

/// What a person asked for.
pub struct Request {
    pub plugin: String,
    pub model: String,
    pub user: String,
    pub dry_run: bool,
    /// `media` or `storage`, and the key.
    pub source: (String, String),
    pub sheet: Option<String>,
    pub mapping: Option<Value>,
    pub stop_on_error: bool,
    pub force: bool,
    pub callback: Option<Value>,
}

/// The stored job, as JSON, or `None`.
pub async fn find(db: &Db, ulid: &str) -> Result<Option<Value>, surrealdb::Error> {
    let mut response = db
        .query("SELECT * FROM import_jobs WHERE ulid = $ulid LIMIT 1;")
        .bind(("ulid", ulid.to_string()))
        .await?
        .check()?;
    let rows: Vec<Value> = response.take(0)?;
    Ok(rows.into_iter().next())
}

/// Make the job and queue its first slice. Returns its id.
pub async fn create(db: &Db, request: Request) -> Result<String, surrealdb::Error> {
    let mut response = db
        .query(
            "CREATE import_jobs SET ulid = rand::ulid(), plugin = $plugin, model = $model, user = $user, mode = $mode, \
             status = 'queued', source_kind = $kind, source_key = $key, sheet = $sheet, mapping = $mapping, \
             stop_on_error = $stop, force = $force, callback = $callback RETURN ulid;",
        )
        .bind(("plugin", request.plugin))
        .bind(("model", request.model))
        .bind(("user", request.user))
        .bind(("mode", if request.dry_run { "dry_run" } else { "run" }))
        .bind(("kind", request.source.0))
        .bind(("key", request.source.1))
        .bind(("sheet", request.sheet))
        .bind(("mapping", request.mapping))
        .bind(("stop", request.stop_on_error))
        .bind(("force", request.force))
        .bind(("callback", request.callback))
        .await?
        .check()?;
    let made: Vec<Value> = response.take(0)?;
    let ulid = made.first().and_then(|r| r.get("ulid")).and_then(Value::as_str).unwrap_or_default().to_string();
    enqueue(db, &ulid, "import", 0).await?;
    Ok(ulid)
}

/// Queue a slice of a job (`phase` is `import` or `undo`).
pub async fn enqueue(db: &Db, ulid: &str, phase: &str, delay_secs: i64) -> Result<(), surrealdb::Error> {
    let mut job = NewJob::new(KIND, QUEUE);
    job.payload = Some(json!({ "job": ulid, "phase": phase }));
    job.max_attempts = 3;
    if delay_secs > 0 {
        job.run_at = chrono::Utc::now() + chrono::Duration::seconds(delay_secs);
    }
    job.enqueued_by = Some("system:import".into());
    if let Err(error) = queue::enqueue(db, job).await {
        log::warn!("an import slice could not be queued: {error}");
    }
    Ok(())
}

