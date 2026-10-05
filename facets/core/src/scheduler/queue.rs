//! The job queue in an organization's database.
//!
//! A job is a unit of background work: a plugin function to call, or a message to deliver.
//! Workers [`claim`] due jobs, run them, then [`complete`] or [`fail`] them. Claiming is a single
//! database transaction that re-checks every job's state, so any number of workers can run at once
//! without two of them taking the same job. Delivery is **at-least-once**: a worker that dies
//! holds its jobs only until their lease runs out, after which another worker runs them again.

use chrono::{DateTime, Utc};
use serde::Deserialize;
use serde_json::Value;
use sha2::{Digest, Sha256};
use surrealdb::types::SurrealValue;

use surrealdb::{Surreal, engine::remote::ws::Client};

use super::schedule::backoff_secs;

/// Most payload bytes a job may carry.
pub const MAX_PAYLOAD_BYTES: usize = 64 * 1024;
/// Most waiting jobs a single plugin may have in one organization.
pub const MAX_WAITING_PER_PLUGIN: i64 = 10_000;

#[derive(Debug, thiserror::Error)]
pub enum QueueError {
    #[error("database error: {0}")]
    Db(#[from] surrealdb::Error),
    #[error("{0}")]
    Refused(String),
}

/// What to put on the queue.
#[derive(Debug, Clone)]
pub struct NewJob {
    /// `"plugin"` or `"communication"`.
    pub kind: &'static str,
    pub plugin: Option<String>,
    pub function: Option<String>,
    pub payload: Option<Value>,
    pub queue: String,
    pub run_at: DateTime<Utc>,
    pub max_attempts: i64,
    pub backoff_secs: i64,
    /// The scheduled task that created it, when one did.
    pub task: Option<String>,
    /// While a job with this key is waiting or running, enqueueing another with the same key
    /// (from the same plugin) does nothing.
    pub unique_key: Option<String>,
    pub enqueued_by: Option<String>,
    pub request_id: Option<String>,
}

impl NewJob {
    pub fn new(kind: &'static str, queue: impl Into<String>) -> Self {
        Self {
            kind,
            plugin: None,
            function: None,
            payload: None,
            queue: queue.into(),
            run_at: Utc::now(),
            max_attempts: 3,
            backoff_secs: 30,
            task: None,
            unique_key: None,
            enqueued_by: None,
            request_id: None,
        }
    }
}

/// The result of [`enqueue`].
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Enqueued {
    pub id: String,
    /// False when a job with the same `unique_key` was already waiting or running.
    pub created: bool,
}

fn check(job: &NewJob) -> Result<(), QueueError> {
    if let Some(payload) = &job.payload {
        let size = serde_json::to_vec(payload).map(|bytes| bytes.len()).unwrap_or(usize::MAX);
        if size > MAX_PAYLOAD_BYTES {
            return Err(QueueError::Refused(format!("a job's payload is at most {MAX_PAYLOAD_BYTES} bytes")));
        }
        if !payload.is_object() {
            return Err(QueueError::Refused("a job's payload must be a JSON object".into()));
        }
    }
    if !(1..=20).contains(&job.max_attempts) {
        return Err(QueueError::Refused("max_attempts is 1 to 20".into()));
    }
    let queue_ok = !job.queue.is_empty()
        && job.queue.len() <= 40
        && job.queue.chars().all(|c| c.is_ascii_lowercase() || c.is_ascii_digit() || c == '_' || c == '-');
    if !queue_ok {
        return Err(QueueError::Refused("a queue name is lowercase letters, digits, `_` and `-` (at most 40)".into()));
    }
    if job.unique_key.as_ref().is_some_and(|key| key.is_empty() || key.len() > 200) {
        return Err(QueueError::Refused("a unique_key is 1 to 200 bytes".into()));
    }
    Ok(())
}

/// The record key a unique job lives under: the same plugin and key always give the same one.
fn unique_record_key(job: &NewJob, key: &str) -> String {
    let mut hash = Sha256::new();
    hash.update(job.kind.as_bytes());
    hash.update([0]);
    hash.update(job.plugin.as_deref().unwrap_or("").as_bytes());
    hash.update([0]);
    hash.update(key.as_bytes());
    hash.finalize().iter().take(16).map(|byte| format!("{byte:02x}")).collect()
}

#[derive(Debug, Deserialize, SurrealValue)]
struct CreatedRow {
    key: String,
    created: bool,
}

/// Put a job on the queue.
pub async fn enqueue(db: &Surreal<Client>, job: NewJob) -> Result<Enqueued, QueueError> {
    check(&job)?;
    if let Some(plugin) = &job.plugin {
        let mut response = db
            .query("SELECT count() AS n FROM jobs WHERE plugin = $plugin AND state = 'queued' GROUP ALL;")
            .bind(("plugin", plugin.clone()))
            .await?
            .check()?;
        let waiting: Option<Value> = response.take(0)?;
        let waiting = waiting.and_then(|row| row.get("n").and_then(Value::as_i64)).unwrap_or(0);
        if waiting >= MAX_WAITING_PER_PLUGIN {
            return Err(QueueError::Refused(format!(
                "`{plugin}` already has {waiting} jobs waiting; wait for them to run"
            )));
        }
    }
    let key = job.unique_key.as_deref().map(|key| unique_record_key(&job, key));
    // Fields that are not set are left out: the database distinguishes "absent" from `null`.
    let mut content = serde_json::Map::new();
    content.insert("kind".into(), job.kind.into());
    content.insert("queue".into(), job.queue.clone().into());
    content.insert("state".into(), "queued".into());
    content.insert("max_attempts".into(), job.max_attempts.into());
    content.insert("backoff_secs".into(), job.backoff_secs.into());
    for (name, value) in [
        ("plugin", job.plugin.clone().map(Value::from)),
        ("function_name", job.function.clone().map(Value::from)),
        ("payload", job.payload.clone()),
        ("task", job.task.clone().map(Value::from)),
        ("unique_key", job.unique_key.clone().map(Value::from)),
        ("enqueued_by", job.enqueued_by.clone().map(Value::from)),
        ("request_id", job.request_id.clone().map(Value::from)),
    ] {
        if let Some(value) = value {
            content.insert(name.into(), value);
        }
    }
    let content = Value::Object(content);
    let surql = r#"
        BEGIN TRANSACTION;
        LET $key = $fixed ?? rand::ulid();
        LET $existing = (SELECT VALUE state FROM ONLY type::record('jobs', $key));
        LET $go = $existing = NONE OR $existing IN ['succeeded', 'failed', 'cancelled'];
        IF $go {
            -- Replacing a finished job starts from nothing, so the fields that have defaults are given.
            UPSERT type::record('jobs', $key) CONTENT object::extend($content, {
                attempts: 0, run_at: <datetime> $run_at, date_created: time::now()
            });
        };
        RETURN { key: $key, created: $go };
        COMMIT TRANSACTION;
    "#;
    let mut response = db
        .query(surql)
        .bind(("fixed", key))
        .bind(("content", content))
        .bind(("run_at", job.run_at.to_rfc3339()))
        .await?
        .check()?;
    let row: Option<CreatedRow> = response.take(5)?;
    let row = row.ok_or_else(|| QueueError::Refused("the queue did not answer".into()))?;
    Ok(Enqueued { id: row.key, created: row.created })
}

/// A job a worker has taken.
#[derive(Debug, Clone, Deserialize, SurrealValue)]
pub struct Job {
    pub key: String,
    pub kind: String,
    pub plugin: Option<String>,
    pub function: Option<String>,
    pub payload: Option<Value>,
    pub queue: String,
    pub attempts: i64,
    pub max_attempts: i64,
    pub backoff_secs: i64,
    pub enqueued_by: Option<String>,
    pub request_id: Option<String>,
}

const CLAIMABLE: &str = "((state = 'queued' AND run_at <= time::now()) \
     OR (state = 'running' AND locked_until < time::now() AND attempts < max_attempts))";

/// Take up to `limit` due jobs of `queues` (all queues when empty) for `node`, holding them for
/// `lease_secs`.
pub async fn claim(db: &Surreal<Client>, node: &str, queues: &[String], limit: i64, lease_secs: i64) -> Result<Vec<Job>, QueueError> {
    let surql = format!(
        r#"
        BEGIN TRANSACTION;
        LET $due = (SELECT VALUE id FROM jobs WHERE ($all OR queue IN $queues) AND {CLAIMABLE} ORDER BY run_at LIMIT $limit);
        LET $claimed = (UPDATE jobs SET
                state = 'running', locked_by = $node, locked_until = time::now() + <duration> $lease,
                attempts += 1, date_started = time::now()
            WHERE id IN $due AND {CLAIMABLE}
            RETURN VALUE {{ key: <string> record::id(id), kind: kind, plugin: plugin, function: function_name,
                payload: payload, queue: queue, attempts: attempts, max_attempts: max_attempts,
                backoff_secs: backoff_secs, enqueued_by: enqueued_by, request_id: request_id }});
        RETURN $claimed;
        COMMIT TRANSACTION;
    "#
    );
    let mut response = db
        .query(surql)
        .bind(("all", queues.is_empty()))
        .bind(("queues", queues.to_vec()))
        .bind(("limit", limit))
        .bind(("node", node.to_string()))
        .bind(("lease", format!("{lease_secs}s")))
        .await?
        .check()?;
    Ok(response.take(3)?)
}

/// Mark a claimed job done. Does nothing if the job is no longer this node's (its lease ran
/// out and another worker took it): the other worker's outcome stands.
pub async fn complete(db: &Surreal<Client>, node: &str, key: &str) -> Result<bool, QueueError> {
    let mut response = db
        .query(
            "UPDATE type::record('jobs', $key) SET state = 'succeeded', date_finished = time::now(), \
             locked_by = NONE, locked_until = NONE, last_error = NONE \
             WHERE state = 'running' AND locked_by = $node RETURN VALUE 1;",
        )
        .bind(("key", key.to_string()))
        .bind(("node", node.to_string()))
        .await?
        .check()?;
    let updated: Vec<Value> = response.take(0)?;
    Ok(!updated.is_empty())
}

/// What happened to a job after a failed attempt.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Outcome {
    /// It will run again after a delay.
    Retry { in_secs: i64 },
    /// It is out of attempts, or the failure cannot be fixed by trying again.
    Failed,
}

/// Record a failed attempt of a claimed job: schedule a retry, or give up when the attempts are
/// used up or `permanent` is set (a message the provider rejected will be rejected again).
/// `None` when the job is no longer this node's (its lease ran out and another worker took it).
pub async fn fail(
    db: &Surreal<Client>,
    node: &str,
    job: &Job,
    error: &str,
    permanent: bool,
) -> Result<Option<Outcome>, QueueError> {
    let error: String = error.chars().take(500).collect();
    let (outcome, surql) = if permanent || job.attempts >= job.max_attempts {
        (
            Outcome::Failed,
            "UPDATE type::record('jobs', $key) SET state = 'failed', date_finished = time::now(), \
             locked_by = NONE, locked_until = NONE, last_error = $error \
             WHERE state = 'running' AND locked_by = $node RETURN VALUE 1;",
        )
    } else {
        let wait = backoff_secs(job.backoff_secs, job.attempts);
        (
            Outcome::Retry { in_secs: wait },
            "UPDATE type::record('jobs', $key) SET state = 'queued', run_at = time::now() + <duration> $wait, \
             locked_by = NONE, locked_until = NONE, last_error = $error \
             WHERE state = 'running' AND locked_by = $node RETURN VALUE 1;",
        )
    };
    let wait = match outcome {
        Outcome::Retry { in_secs } => in_secs,
        Outcome::Failed => 0,
    };
    let mut response = db
        .query(surql)
        .bind(("key", job.key.clone()))
        .bind(("node", node.to_string()))
        .bind(("error", error))
        .bind(("wait", format!("{wait}s")))
        .await?
        .check()?;
    let updated: Vec<Value> = response.take(0)?;
    // Nothing changed when the job is no longer this node's: another worker owns its outcome.
    Ok((!updated.is_empty()).then_some(outcome))
}

/// Jobs whose worker vanished and that have used every attempt: they will never be claimed
/// again, so close them as failed.
pub async fn fail_abandoned(db: &Surreal<Client>) -> Result<usize, QueueError> {
    let mut response = db
        .query(
            "UPDATE jobs SET state = 'failed', date_finished = time::now(), locked_by = NONE, locked_until = NONE, \
             last_error = 'the worker running it stopped before it finished, and no attempts are left' \
             WHERE state = 'running' AND locked_until < time::now() AND attempts >= max_attempts RETURN VALUE 1;",
        )
        .await?
        .check()?;
    let closed: Vec<Value> = response.take(0)?;
    Ok(closed.len())
}

/// A job as a plugin may see it.
pub async fn status(db: &Surreal<Client>, plugin: &str, key: &str) -> Result<Option<Value>, QueueError> {
    let mut response = db
        .query(
            "SELECT <string> record::id(id) AS id, kind, function_name AS function, queue, state, attempts, max_attempts, last_error, \
             <string> run_at AS run_at, <string> date_created AS date_created, \
             <string> date_started AS date_started, <string> date_finished AS date_finished \
             FROM ONLY type::record('jobs', $key) WHERE plugin = $plugin;",
        )
        .bind(("key", key.to_string()))
        .bind(("plugin", plugin.to_string()))
        .await?
        .check()?;
    Ok(response.take(0)?)
}

/// Cancel a job that has not started. Returns whether it was cancelled.
pub async fn cancel(db: &Surreal<Client>, plugin: &str, key: &str) -> Result<bool, QueueError> {
    let mut response = db
        .query(
            "UPDATE type::record('jobs', $key) SET state = 'cancelled', date_finished = time::now() \
             WHERE plugin = $plugin AND state = 'queued' RETURN VALUE 1;",
        )
        .bind(("key", key.to_string()))
        .bind(("plugin", plugin.to_string()))
        .await?
        .check()?;
    let updated: Vec<Value> = response.take(0)?;
    Ok(!updated.is_empty())
}

/// Delete finished jobs: successes older than `succeeded_days`, everything else finished older
/// than `failed_days`.
pub async fn purge(db: &Surreal<Client>, succeeded_days: i64, failed_days: i64) -> Result<usize, QueueError> {
    let mut response = db
        .query(
            "DELETE jobs WHERE (state = 'succeeded' AND date_finished < time::now() - <duration> $ok) \
             OR (state IN ['failed', 'cancelled'] AND date_finished < time::now() - <duration> $bad) RETURN BEFORE;",
        )
        .bind(("ok", format!("{succeeded_days}d")))
        .bind(("bad", format!("{failed_days}d")))
        .await?
        .check()?;
    let removed: Vec<Value> = response.take(0)?;
    Ok(removed.len())
}

/// When the next waiting job in this organization is due, if any.
pub async fn next_due(db: &Surreal<Client>) -> Result<Option<DateTime<Utc>>, QueueError> {
    let mut response = db
        .query("SELECT VALUE <string> run_at FROM jobs WHERE state = 'queued' ORDER BY run_at LIMIT 1;")
        .await?
        .check()?;
    let next: Option<String> = response.take(0)?;
    Ok(next.and_then(|text| DateTime::parse_from_rfc3339(&text).ok()).map(|time| time.with_timezone(&Utc)))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn unique_jobs_get_the_same_record_for_the_same_plugin_and_key() {
        let mut a = NewJob::new("plugin", "default");
        a.plugin = Some("billing".into());
        let mut b = a.clone();
        b.plugin = Some("other".into());
        let first = unique_record_key(&a, "invoice-7");
        assert_eq!(first, unique_record_key(&a, "invoice-7"));
        assert_ne!(first, unique_record_key(&a, "invoice-8"));
        assert_ne!(first, unique_record_key(&b, "invoice-7"), "another plugin's key is another job");
        assert_eq!(first.len(), 32);
    }

    #[test]
    fn jobs_are_checked_before_they_are_queued() {
        let ok = NewJob::new("plugin", "default");
        assert!(check(&ok).is_ok());
        let mut big = ok.clone();
        big.payload = Some(serde_json::json!({ "x": "y".repeat(MAX_PAYLOAD_BYTES) }));
        assert!(check(&big).is_err());
        let mut array = ok.clone();
        array.payload = Some(serde_json::json!([1]));
        assert!(check(&array).is_err());
        for attempts in [0, 21] {
            let mut job = ok.clone();
            job.max_attempts = attempts;
            assert!(check(&job).is_err());
        }
        for queue in ["", "Has Space", "UPPER", &"q".repeat(41)] {
            let mut job = ok.clone();
            job.queue = queue.to_string();
            assert!(check(&job).is_err(), "{queue}");
        }
    }
}
