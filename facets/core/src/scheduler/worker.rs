//! The loop that runs jobs.
//!
//! One worker serves every organization. It wakes when told there is work (the HTTP server pings
//! it right after a job is enqueued) and in any case every `poll` seconds. On each wake-up it
//! claims due jobs up to its free capacity and runs them concurrently; on each periodic look it
//! also turns due recurring tasks into jobs, closes jobs whose worker vanished, and now and then
//! deletes old finished jobs.

use std::{
    sync::Arc,
    time::{Duration, Instant},
};

use chrono::Utc;
use serde_json::Value;
use tokio::sync::{Semaphore, watch};

use crate::{
    application::settings::{Scope, get_setting_plain_in},
    messaging,
    plugin_manager::api::run_system_call,
    state::{AppState, Db},
    tenancy::OrgRef,
};

use super::{
    nodes::{self, NodeKind},
    queue::{self, Job, Outcome},
    tasks,
};

/// How long to wait for running jobs when shutting down.
const DRAIN_SECS: u64 = 30;
/// How often old finished jobs are deleted.
const PURGE_EVERY: Duration = Duration::from_secs(3600);

#[derive(Debug, Clone)]
pub struct WorkerOptions {
    pub node_id: String,
    pub kind: NodeKind,
    /// Where the control API listens (standalone only).
    pub address: Option<String>,
    /// Queues to serve; empty means all.
    pub queues: Vec<String>,
    pub concurrency: usize,
    pub poll: Duration,
    pub lease_secs: i64,
}

/// A name for this process in `scheduler_nodes` and in the `locked_by` of its jobs.
pub fn new_node_id(kind: NodeKind) -> String {
    format!("{}-{}-{}", kind.as_str(), std::process::id(), crate::access::audit::new_request_id().chars().take(8).collect::<String>())
}

async fn org_names(core: &Db) -> Result<Vec<String>, surrealdb::Error> {
    let mut response = core.query("SELECT VALUE db_name FROM organizations;").await?.check()?;
    response.take(0)
}

/// An organization's setting, falling back to the global one.
pub(crate) async fn number_setting(state: &AppState, org: &OrgRef, key: &str, default: i64) -> i64 {
    for scope in [Scope::Org(org), Scope::Global] {
        if let Ok(Some(value)) = get_setting_plain_in(state, key, scope).await {
            if let Some(number) = value.as_i64() {
                return number;
            }
        }
    }
    default
}

async fn bool_setting(state: &AppState, org: &OrgRef, key: &str, default: bool) -> bool {
    for scope in [Scope::Org(org), Scope::Global] {
        if let Ok(Some(value)) = get_setting_plain_in(state, key, scope).await {
            if let Some(flag) = value.as_bool() {
                return flag;
            }
        }
    }
    default
}

/// Run until `shutdown` turns true, then wait for running jobs to finish.
pub async fn run(state: AppState, options: WorkerOptions, mut shutdown: watch::Receiver<bool>) {
    let link = state.scheduler.clone();
    let semaphore = Arc::new(Semaphore::new(options.concurrency));
    let core = match state.core().await {
        Ok(core) => core,
        Err(error) => {
            log::error!("scheduler cannot start: {error}");
            return;
        }
    };
    log::info!(
        "Scheduler {} started ({} job(s) at a time, looking every {}s, queues: {})",
        options.node_id,
        options.concurrency,
        options.poll.as_secs(),
        if options.queues.is_empty() { "all".to_string() } else { options.queues.join(", ") }
    );

    let mut last_sweep: Option<Instant> = None;
    let mut last_purge = Instant::now();
    let mut standing_down = false;
    loop {
        if *shutdown.borrow() {
            break;
        }
        let sweep = last_sweep.is_none_or(|at| at.elapsed() >= options.poll) || link.take_reload();
        let mut orgs = link.take_hot();
        if sweep {
            last_sweep = Some(Instant::now());
            if let Err(error) = nodes::announce(&core, &options.node_id, options.kind, options.address.as_deref(), &options.queues).await {
                log::warn!("scheduler heartbeat failed: {error}");
            }
            let must_stand_down = options.kind == NodeKind::Embedded && nodes::standalone_alive(&core).await.unwrap_or(false);
            if must_stand_down != standing_down {
                standing_down = must_stand_down;
                if standing_down {
                    log::info!("A standalone scheduler is running; the embedded one stands down");
                } else {
                    log::info!("No standalone scheduler is alive; the embedded one takes over");
                }
            }
            match org_names(&core).await {
                Ok(all) => orgs.extend(all),
                Err(error) => log::warn!("scheduler could not list organizations: {error}"),
            }
        }
        orgs.sort();
        orgs.dedup();

        if !standing_down && !link.is_paused() {
            let purge = sweep && last_purge.elapsed() >= PURGE_EVERY;
            if purge {
                last_purge = Instant::now();
            }
            for org in orgs {
                process_org(&state, &options, &semaphore, &org, sweep, purge).await;
            }
        }

        let until_sweep = options.poll.saturating_sub(last_sweep.map_or(Duration::ZERO, |at| at.elapsed()));
        tokio::select! {
            () = link.wait(until_sweep.max(Duration::from_millis(50))) => {}
            _ = shutdown.changed() => {}
        }
    }

    log::info!("Scheduler {} stopping; waiting for running jobs", options.node_id);
    let drained = tokio::time::timeout(
        Duration::from_secs(DRAIN_SECS),
        semaphore.acquire_many(u32::try_from(options.concurrency).unwrap_or(u32::MAX)),
    )
    .await;
    if drained.is_err() {
        log::warn!("Some jobs were still running after {DRAIN_SECS}s; they will be run again when their lease ends");
    }
    if let Err(error) = nodes::retire(&core, &options.node_id).await {
        log::warn!("scheduler could not retire its node row: {error}");
    }
}

async fn process_org(state: &AppState, options: &WorkerOptions, semaphore: &Arc<Semaphore>, org_db: &str, sweep: bool, purge: bool) {
    let org = OrgRef { slug: org_db.to_string(), db_name: org_db.to_string() };
    if !bool_setting(state, &org, "scheduler.enabled", true).await {
        return;
    }
    let db = match state.org(org_db).await {
        Ok(db) => db,
        Err(error) => {
            log::warn!("scheduler cannot open organization `{org_db}`: {error}");
            return;
        }
    };
    if sweep {
        let backoff = number_setting(state, &org, "scheduler.retry_backoff_secs", 30).await;
        match tasks::run_due(&db, Utc::now(), backoff).await {
            Ok(fired) => {
                for task in fired {
                    log::info!(
                        "{org_db}: task {}.{} {} (slot {})",
                        task.plugin,
                        task.task,
                        if task.skipped { "skipped, too late" } else { "queued a job" },
                        task.slot
                    );
                }
            }
            Err(error) => log::warn!("{org_db}: scheduled tasks could not be checked: {error}"),
        }
        match queue::fail_abandoned(&db).await {
            Ok(0) => {}
            Ok(count) => log::warn!("{org_db}: {count} job(s) lost their worker and are out of attempts; marked failed"),
            Err(error) => log::warn!("{org_db}: abandoned jobs could not be checked: {error}"),
        }
        if purge {
            let ok = number_setting(state, &org, "scheduler.job_retention_days", 7).await.max(1);
            let bad = number_setting(state, &org, "scheduler.failed_job_retention_days", 30).await.max(1);
            match queue::purge(&db, ok, bad).await {
                Ok(0) => {}
                Ok(count) => log::info!("{org_db}: deleted {count} old finished job(s)"),
                Err(error) => log::warn!("{org_db}: old jobs could not be deleted: {error}"),
            }
        }
    }
    loop {
        let free = semaphore.available_permits();
        if free == 0 {
            break;
        }
        let jobs = match queue::claim(&db, &options.node_id, &options.queues, i64::try_from(free).unwrap_or(i64::MAX), options.lease_secs).await {
            Ok(jobs) => jobs,
            Err(error) => {
                log::warn!("{org_db}: jobs could not be claimed: {error}");
                break;
            }
        };
        if jobs.is_empty() {
            break;
        }
        for job in jobs {
            let Ok(permit) = semaphore.clone().acquire_owned().await else { return };
            let (state, db, node, org_db) = (state.clone(), db.clone(), options.node_id.clone(), org_db.to_string());
            // Leave a margin so the job is closed before its lease could run out.
            let time_limit = Duration::from_secs(u64::try_from(options.lease_secs - 5).unwrap_or(5).max(5));
            tokio::spawn(async move {
                let _permit = permit;
                run_job(&state, &db, &node, &org_db, job, time_limit).await;
                // There may be more waiting now that a place is free.
                state.scheduler.wake_local(&org_db);
            });
        }
    }
}

/// Why a job's attempt failed.
struct JobFailure {
    message: String,
    permanent: bool,
}

async fn execute(state: &AppState, org_db: &str, job: &Job) -> Result<(), JobFailure> {
    let payload = job.payload.clone().unwrap_or(Value::Object(serde_json::Map::new()));
    match job.kind.as_str() {
        "plugin" => {
            let (Some(plugin), Some(function)) = (&job.plugin, &job.function) else {
                return Err(JobFailure { message: "the job names no plugin function".into(), permanent: true });
            };
            let request_id = format!("job-{}", job.key);
            run_system_call(state, org_db, plugin, function, payload, &request_id)
                .await
                .map(|_| ())
                .map_err(|error| JobFailure { message: format!("{plugin}.{function}: {}", error.message), permanent: error.permanent })
        }
        "communication" => messaging::deliver(state, org_db, &payload)
            .await
            .map_err(|error| JobFailure { message: error.message, permanent: error.permanent }),
        other => Err(JobFailure { message: format!("unknown job kind `{other}`"), permanent: true }),
    }
}

/// Run one claimed job and record how it went.
pub async fn run_job(state: &AppState, db: &Db, node: &str, org_db: &str, job: Job, time_limit: Duration) {
    let started = Instant::now();
    let outcome = match tokio::time::timeout(time_limit, execute(state, org_db, &job)).await {
        Ok(result) => result,
        Err(_) => Err(JobFailure { message: format!("it ran longer than {}s", time_limit.as_secs()), permanent: false }),
    };
    let label = match (&job.plugin, &job.function) {
        (Some(plugin), Some(function)) => format!("{plugin}.{function}"),
        _ => job.kind.clone(),
    };
    match outcome {
        Ok(()) => match queue::complete(db, node, &job.key).await {
            Ok(true) => log::info!("{org_db}: job {} ({label}) succeeded in {}ms (attempt {})", job.key, started.elapsed().as_millis(), job.attempts),
            Ok(false) => log::warn!("{org_db}: job {} ({label}) finished but its lease had passed to another worker", job.key),
            Err(error) => log::error!("{org_db}: job {} ({label}) succeeded but could not be recorded: {error}", job.key),
        },
        Err(failure) => match queue::fail(db, node, &job, &failure.message, failure.permanent).await {
            Ok(None) => log::warn!("{org_db}: job {} ({label}) failed but its lease had passed to another worker: {}", job.key, failure.message),
            Ok(Some(Outcome::Retry { in_secs })) => log::warn!(
                "{org_db}: job {} ({label}) failed, attempt {} of {}; trying again in {in_secs}s: {}",
                job.key, job.attempts, job.max_attempts, failure.message
            ),
            Ok(Some(Outcome::Failed)) => log::error!("{org_db}: job {} ({label}) failed for good: {}", job.key, failure.message),
            Err(error) => log::error!("{org_db}: job {} ({label}) failed and could not be recorded: {error}", job.key),
        },
    }
}
