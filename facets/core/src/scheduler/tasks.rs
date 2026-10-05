//! Recurring tasks: what a plugin's `[[schedule]]` and `scheduler::register` create, and the
//! tick that turns a due task into a job.
//!
//! A task does not run anything itself. When it is due, the scheduler puts a job on the queue and
//! moves the task to its next slot, so recurring work gets the same retries, queues and limits as
//! any other job. The job's `unique_key` names the task and the slot, so even if two schedulers
//! (or one after a crash) fire the same slot, only one job exists.

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use serde_json::Value;
use surrealdb::types::SurrealValue;

use surrealdb::{Surreal, engine::remote::ws::Client};

use super::queue::{self, NewJob, QueueError};
use super::schedule::{Recurrence, ScheduleError};

/// Most tasks a plugin may register while running.
pub const MAX_RUNTIME_TASKS_PER_PLUGIN: i64 = 100;
/// A skipped task that is later than this many seconds behind is not run late.
const SKIP_GRACE_SECS: i64 = 300;

#[derive(Debug, thiserror::Error)]
pub enum TaskError {
    #[error(transparent)]
    Schedule(#[from] ScheduleError),
    #[error(transparent)]
    Queue(#[from] QueueError),
    #[error("database error: {0}")]
    Db(#[from] surrealdb::Error),
    #[error("{0}")]
    Refused(String),
}

/// How a task treats slots it missed while nothing was running.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum CatchUp {
    /// Run once for everything missed (the default).
    #[default]
    Once,
    /// Wait for the next slot; a run that is far behind is dropped.
    Skip,
}

impl CatchUp {
    fn as_str(self) -> &'static str {
        match self {
            Self::Once => "once",
            Self::Skip => "skip",
        }
    }
}

/// A task as declared by a plugin.
#[derive(Debug, Clone, PartialEq, Deserialize, Serialize)]
pub struct TaskDef {
    pub name: String,
    pub function: String,
    #[serde(default)]
    pub cron: Option<String>,
    #[serde(default)]
    pub every: Option<String>,
    #[serde(default)]
    pub timezone: Option<String>,
    #[serde(default)]
    pub payload: Option<Value>,
    #[serde(default = "default_queue")]
    pub queue: String,
    #[serde(default = "default_attempts")]
    pub max_attempts: i64,
    #[serde(default)]
    pub catch_up: CatchUp,
}

fn default_queue() -> String {
    "default".into()
}

fn default_attempts() -> i64 {
    3
}

impl TaskDef {
    /// The task's recurrence, checking `cron`, `every` and `timezone`.
    pub fn recurrence(&self) -> Result<Recurrence, ScheduleError> {
        Recurrence::parse(self.cron.as_deref(), self.every.as_deref(), self.timezone.as_deref())
    }

    /// Check everything a manifest can get wrong, so a plugin is refused at load time.
    pub fn validate(&self) -> Result<(), TaskError> {
        let name_ok = !self.name.is_empty()
            && self.name.len() <= 64
            && self.name.chars().all(|c| c.is_ascii_alphanumeric() || c == '_' || c == '-' || c == '.');
        if !name_ok {
            return Err(TaskError::Refused(format!(
                "task name `{}`: use 1 to 64 letters, digits, `_`, `-` or `.`",
                self.name
            )));
        }
        if self.function.is_empty() {
            return Err(TaskError::Refused(format!("task `{}` names no function", self.name)));
        }
        self.recurrence()?;
        if !(1..=20).contains(&self.max_attempts) {
            return Err(TaskError::Refused(format!("task `{}`: max_attempts is 1 to 20", self.name)));
        }
        if self.payload.as_ref().is_some_and(|payload| !payload.is_object()) {
            return Err(TaskError::Refused(format!("task `{}`: payload must be a table/object", self.name)));
        }
        Ok(())
    }
}

#[derive(Debug, Deserialize, SurrealValue)]
struct ExistingRow {
    cron: Option<String>,
    every_secs: Option<i64>,
    timezone: Option<String>,
}

fn same_recurrence(row: &ExistingRow, recurrence: &Recurrence) -> bool {
    match recurrence {
        Recurrence::Cron { expression, timezone } => {
            row.cron.as_deref() == Some(expression.as_str()) && row.timezone == *timezone
        }
        Recurrence::Every { seconds } => row.every_secs == Some(*seconds),
    }
}

/// Create or update one task. `source` is `"manifest"` or `"runtime"`. The next run is only
/// recalculated when the schedule itself changed, so re-installing a plugin does not skip or
/// repeat a slot.
pub async fn upsert(db: &Surreal<Client>, plugin: &str, source: &str, def: &TaskDef) -> Result<(), TaskError> {
    def.validate()?;
    let recurrence = def.recurrence()?;
    let mut response = db
        .query("SELECT cron, every_secs, timezone FROM ONLY scheduled_tasks WHERE plugin = $plugin AND name = $name LIMIT 1;")
        .bind(("plugin", plugin.to_string()))
        .bind(("name", def.name.clone()))
        .await?
        .check()?;
    let existing: Option<ExistingRow> = response.take(0)?;
    let (cron, every_secs, timezone) = match &recurrence {
        Recurrence::Cron { expression, timezone } => (Some(expression.clone()), None, timezone.clone()),
        Recurrence::Every { seconds } => (None, Some(*seconds), None),
    };
    let first_run = recurrence
        .next_after(Utc::now())
        .ok_or_else(|| TaskError::Refused(format!("task `{}` never runs", def.name)))?;
    let reschedule = existing.as_ref().is_none_or(|row| !same_recurrence(row, &recurrence));
    let surql = if existing.is_some() {
        "UPDATE scheduled_tasks SET function_name = $function, cron = $cron, every_secs = $every_secs, timezone = $timezone, \
         payload = $payload, queue = $queue, max_attempts = $max_attempts, catch_up = $catch_up, source = $source, enabled = true, \
         next_run = IF $reschedule { <datetime> $next } ELSE { next_run } \
         WHERE plugin = $plugin AND name = $name;"
    } else {
        "CREATE scheduled_tasks SET plugin = $plugin, name = $name, function_name = $function, cron = $cron, every_secs = $every_secs, \
         timezone = $timezone, payload = $payload, queue = $queue, max_attempts = $max_attempts, catch_up = $catch_up, \
         source = $source, enabled = true, next_run = <datetime> $next;"
    };
    db.query(surql)
        .bind(("plugin", plugin.to_string()))
        .bind(("name", def.name.clone()))
        .bind(("function", def.function.clone()))
        .bind(("cron", cron))
        .bind(("every_secs", every_secs))
        .bind(("timezone", timezone))
        .bind(("payload", def.payload.clone()))
        .bind(("queue", def.queue.clone()))
        .bind(("max_attempts", def.max_attempts))
        .bind(("catch_up", def.catch_up.as_str().to_string()))
        .bind(("source", source.to_string()))
        .bind(("reschedule", reschedule))
        .bind(("next", first_run.to_rfc3339()))
        .await?
        .check()?;
    Ok(())
}

/// Make the plugin's manifest tasks in this organization exactly `defs`: add, update and remove.
/// Tasks the plugin registered while running are left alone.
pub async fn sync_manifest(db: &Surreal<Client>, plugin: &str, defs: &[TaskDef]) -> Result<(), TaskError> {
    for def in defs {
        upsert(db, plugin, "manifest", def).await?;
    }
    let keep: Vec<String> = defs.iter().map(|def| def.name.clone()).collect();
    db.query("DELETE scheduled_tasks WHERE plugin = $plugin AND source = 'manifest' AND name NOT IN $keep;")
        .bind(("plugin", plugin.to_string()))
        .bind(("keep", keep))
        .await?
        .check()?;
    Ok(())
}

/// Register a task from running plugin code, within the per-plugin limit.
pub async fn register_runtime(db: &Surreal<Client>, plugin: &str, def: &TaskDef) -> Result<(), TaskError> {
    let mut response = db
        .query("SELECT count() AS n FROM scheduled_tasks WHERE plugin = $plugin AND source = 'runtime' GROUP ALL;")
        .bind(("plugin", plugin.to_string()))
        .await?
        .check()?;
    let counted: Option<Value> = response.take(0)?;
    let count = counted.and_then(|row| row.get("n").and_then(Value::as_i64)).unwrap_or(0);
    let mut response = db
        .query("SELECT VALUE source FROM ONLY scheduled_tasks WHERE plugin = $plugin AND name = $name LIMIT 1;")
        .bind(("plugin", plugin.to_string()))
        .bind(("name", def.name.clone()))
        .await?
        .check()?;
    let existing_source: Option<String> = response.take(0)?;
    if existing_source.as_deref() == Some("manifest") {
        return Err(TaskError::Refused(format!(
            "`{}` is declared in plugin.toml and cannot be replaced while running",
            def.name
        )));
    }
    if existing_source.is_none() && count >= MAX_RUNTIME_TASKS_PER_PLUGIN {
        return Err(TaskError::Refused(format!(
            "a plugin may register at most {MAX_RUNTIME_TASKS_PER_PLUGIN} tasks"
        )));
    }
    upsert(db, plugin, "runtime", def).await
}

/// Remove a task registered while running. Tasks declared in `plugin.toml` are not removable
/// this way. Returns whether a task was removed.
pub async fn cancel_runtime(db: &Surreal<Client>, plugin: &str, name: &str) -> Result<bool, TaskError> {
    let mut response = db
        .query("DELETE scheduled_tasks WHERE plugin = $plugin AND name = $name AND source = 'runtime' RETURN BEFORE;")
        .bind(("plugin", plugin.to_string()))
        .bind(("name", name.to_string()))
        .await?
        .check()?;
    let removed: Vec<Value> = response.take(0)?;
    Ok(!removed.is_empty())
}

#[derive(Debug, Deserialize, SurrealValue)]
struct DueRow {
    plugin: String,
    name: String,
    function: String,
    cron: Option<String>,
    every_secs: Option<i64>,
    timezone: Option<String>,
    payload: Option<Value>,
    queue: String,
    max_attempts: i64,
    catch_up: String,
    next_run: String,
}

/// A task that fired.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Fired {
    pub plugin: String,
    pub task: String,
    pub slot: DateTime<Utc>,
    pub skipped: bool,
}

/// Turn every due task into a job and move it to its next slot. Safe to run on several
/// schedulers at once: the job key is the same for the same slot, and a task only advances if it
/// is still at the slot that was fired.
pub async fn run_due(db: &Surreal<Client>, now: DateTime<Utc>, default_backoff_secs: i64) -> Result<Vec<Fired>, TaskError> {
    let mut response = db
        .query(
            "SELECT plugin, name, function_name AS function, cron, every_secs, timezone, payload, queue, max_attempts, catch_up, \
             <string> next_run AS next_run FROM scheduled_tasks \
             WHERE enabled = true AND next_run <= <datetime> $now ORDER BY next_run LIMIT 100;",
        )
        .bind(("now", now.to_rfc3339()))
        .await?
        .check()?;
    let due: Vec<DueRow> = response.take(0)?;
    let mut fired = Vec::new();
    for row in due {
        let Ok(slot) = DateTime::parse_from_rfc3339(&row.next_run).map(|time| time.with_timezone(&Utc)) else {
            log::error!("scheduled task {}.{} has an unreadable next_run `{}`", row.plugin, row.name, row.next_run);
            continue;
        };
        let recurrence = match &row.cron {
            Some(expression) => Recurrence::Cron { expression: expression.clone(), timezone: row.timezone.clone() },
            None => Recurrence::Every { seconds: row.every_secs.unwrap_or(3600) },
        };
        let late = (now - slot).num_seconds();
        let skipped = row.catch_up == "skip" && late > (recurrence.period_hint_secs(slot) / 2).max(SKIP_GRACE_SECS);
        if !skipped {
            let mut job = NewJob::new("plugin", row.queue.clone());
            job.plugin = Some(row.plugin.clone());
            job.function = Some(row.function.clone());
            job.payload = row.payload.clone();
            job.max_attempts = row.max_attempts;
            job.backoff_secs = default_backoff_secs;
            job.task = Some(row.name.clone());
            job.unique_key = Some(format!("task:{}:{}", row.name, slot.to_rfc3339()));
            job.enqueued_by = Some("system:scheduler".into());
            if let Err(error) = queue::enqueue(db, job).await {
                log::error!("scheduled task {}.{} could not be queued: {error}", row.plugin, row.name);
                continue;
            }
        }
        // A run that was missed several times over still happens once, then the task resumes
        // from the present.
        let next = recurrence.next_after(now.max(slot));
        let advance = match next {
            Some(next) => {
                db.query(
                    "UPDATE scheduled_tasks SET last_run = time::now(), next_run = <datetime> $next \
                     WHERE plugin = $plugin AND name = $name AND next_run = <datetime> $slot;",
                )
                .bind(("next", next.to_rfc3339()))
                .bind(("plugin", row.plugin.clone()))
                .bind(("name", row.name.clone()))
                .bind(("slot", slot.to_rfc3339()))
                .await
            }
            None => {
                log::warn!("scheduled task {}.{} has no further runs; disabling it", row.plugin, row.name);
                db.query("UPDATE scheduled_tasks SET enabled = false WHERE plugin = $plugin AND name = $name;")
                    .bind(("plugin", row.plugin.clone()))
                    .bind(("name", row.name.clone()))
                    .await
            }
        };
        if let Err(error) = advance.and_then(|response| response.check()) {
            log::error!("scheduled task {}.{} could not be advanced: {error}", row.plugin, row.name);
        }
        fired.push(Fired { plugin: row.plugin, task: row.name, slot, skipped });
    }
    Ok(fired)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn def() -> TaskDef {
        TaskDef {
            name: "nightly".into(),
            function: "run".into(),
            cron: Some("0 2 * * *".into()),
            every: None,
            timezone: None,
            payload: None,
            queue: "default".into(),
            max_attempts: 3,
            catch_up: CatchUp::Once,
        }
    }

    #[test]
    fn a_good_task_validates_and_bad_ones_say_why() {
        assert!(def().validate().is_ok());
        for (label, change) in [
            ("bad name", Box::new(|d: &mut TaskDef| d.name = "has space".into()) as Box<dyn Fn(&mut TaskDef)>),
            ("no function", Box::new(|d| d.function.clear())),
            ("both", Box::new(|d| d.every = Some("5m".into()))),
            ("neither", Box::new(|d| d.cron = None)),
            ("bad cron", Box::new(|d| d.cron = Some("every day".into()))),
            ("attempts", Box::new(|d| d.max_attempts = 0)),
            ("payload", Box::new(|d| d.payload = Some(serde_json::json!([1])))),
        ] {
            let mut task = def();
            change(&mut task);
            assert!(task.validate().is_err(), "{label}");
        }
    }

    #[test]
    fn a_manifest_entry_reads_with_defaults() {
        let task: TaskDef = serde_json::from_value(serde_json::json!({ "name": "t", "function": "f", "every": "15m" })).unwrap();
        assert_eq!(task.queue, "default");
        assert_eq!(task.max_attempts, 3);
        assert_eq!(task.catch_up, CatchUp::Once);
        assert!(task.validate().is_ok());
    }
}
