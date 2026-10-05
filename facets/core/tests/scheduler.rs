//! The job queue, recurring tasks, secret settings and message routing against a real SurrealDB.
//!
//! Needs a server, so the tests are `#[ignore]`d. Run them with:
//!
//! ```text
//! surreal start memory --user root --pass root --bind 127.0.0.1:18000
//! AETHER_TEST_DB=127.0.0.1:18000 cargo test -p aether_core --test scheduler -- --ignored --test-threads=1
//! ```

use std::{collections::HashSet, sync::Arc, time::Duration};

use aether_core::{
    access::audit::{Actor, AuditContext},
    application::settings::{get_setting, get_setting_plain, get_setting_plain_in, set_setting, Scope},
    config_manager::models::AetherConfig,
    kernel::{CallInfo, DbScope, HostError, HostServices, JobDefaults, PluginHostContext, SchedulerHandle, kernel_command},
    messaging,
    notifications::NotificationHub,
    scheduler::{
        queue::{self, NewJob, Outcome},
        tasks::{self, CatchUp, TaskDef},
    },
    state::AppState,
    tenancy::OrgRef,
};
use chrono::Utc;
use serde_json::{Value, json};
use surrealdb::{Surreal, engine::remote::ws::{Client, Ws}, opt::auth::Root};

type TestResult = Result<(), Box<dyn std::error::Error>>;

const NAMESPACE: &str = "aether_sched_test";

async fn connect() -> Result<Option<Surreal<Client>>, Box<dyn std::error::Error>> {
    let Ok(address) = std::env::var("AETHER_TEST_DB") else {
        eprintln!("AETHER_TEST_DB is not set; skipping");
        return Ok(None);
    };
    let db = Surreal::<Client>::init();
    db.connect::<Ws>(address).await?;
    db.signin(Root { username: "root".into(), password: "root".into() }).await?;
    Ok(Some(db))
}

fn unique(label: &str) -> String {
    let nanos = std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).map(|d| d.subsec_nanos()).unwrap_or(7);
    format!("{label}_{nanos}")
}

async fn fresh_org(db: &Surreal<Client>, label: &str) -> Result<(String, Arc<Surreal<Client>>), Box<dyn std::error::Error>> {
    let name = unique(&format!("org_{label}"));
    aether_orm::migrate_org(db, NAMESPACE, &name).await?;
    Ok((name.clone(), session(db, &name).await?))
}

async fn session(db: &Surreal<Client>, database: &str) -> Result<Arc<Surreal<Client>>, Box<dyn std::error::Error>> {
    let session = db.clone();
    session.use_ns(NAMESPACE).use_db(database).await?;
    Ok(Arc::new(session))
}

fn job(plugin: &str) -> NewJob {
    let mut job = NewJob::new("plugin", "default");
    job.plugin = Some(plugin.into());
    job.function = Some("run".into());
    job
}

#[tokio::test]
#[ignore = "needs SurrealDB (AETHER_TEST_DB)"]
async fn workers_never_share_a_job() -> TestResult {
    let Some(db) = connect().await? else { return Ok(()) };
    let (org, first) = fresh_org(&db, "claim").await?;
    let second = session(&db, &org).await?;
    for n in 0..30 {
        let mut j = job("billing");
        j.payload = Some(json!({ "n": n }));
        queue::enqueue(&first, j).await?;
    }
    let claimer = |session: Arc<Surreal<Client>>, node: &'static str| {
        tokio::spawn(async move {
            let mut taken = Vec::new();
            let mut idle = 0;
            while idle < 5 {
                match queue::claim(&session, node, &[], 4, 60).await {
                    Ok(jobs) if jobs.is_empty() => idle += 1,
                    Ok(jobs) => taken.extend(jobs.into_iter().map(|j| j.key)),
                    // Two transactions touched the same job: the loser is told and asks again.
                    Err(_) => tokio::time::sleep(Duration::from_millis(20)).await,
                }
            }
            taken
        })
    };
    let (a, b) = (claimer(first.clone(), "node-a"), claimer(second, "node-b"));
    let (a, b) = (a.await?, b.await?);
    let all: HashSet<_> = a.iter().chain(&b).cloned().collect();
    assert_eq!(a.len() + b.len(), all.len(), "a job was claimed twice");
    assert_eq!(all.len(), 30, "every job was claimed exactly once");
    Ok(())
}

#[tokio::test]
#[ignore = "needs SurrealDB (AETHER_TEST_DB)"]
async fn a_job_runs_retries_and_gives_up() -> TestResult {
    let Some(db) = connect().await? else { return Ok(()) };
    let (_, org) = fresh_org(&db, "retry").await?;
    let mut j = job("billing");
    j.max_attempts = 3;
    j.backoff_secs = 1;
    let queued = queue::enqueue(&org, j).await?;
    assert!(queued.created);

    // Attempt 1 fails: retried after the backoff, not before.
    let taken = queue::claim(&org, "n1", &[], 5, 60).await?;
    assert_eq!(taken.len(), 1);
    assert_eq!(taken[0].attempts, 1);
    assert_eq!(queue::fail(&org, "n1", &taken[0], "boom", false).await?, Some(Outcome::Retry { in_secs: 1 }));
    assert!(queue::claim(&org, "n1", &[], 5, 60).await?.is_empty(), "not due yet");
    tokio::time::sleep(Duration::from_millis(1300)).await;

    // Attempt 2 fails, attempt 3 fails: out of attempts.
    let taken = queue::claim(&org, "n1", &[], 5, 60).await?;
    assert_eq!(taken[0].attempts, 2);
    assert_eq!(queue::fail(&org, "n1", &taken[0], "boom", false).await?, Some(Outcome::Retry { in_secs: 2 }));
    tokio::time::sleep(Duration::from_millis(2300)).await;
    let taken = queue::claim(&org, "n1", &[], 5, 60).await?;
    assert_eq!(taken[0].attempts, 3);
    assert_eq!(queue::fail(&org, "n1", &taken[0], "still boom", false).await?, Some(Outcome::Failed));
    let status = queue::status(&org, "billing", &queued.id).await?.ok_or("no status")?;
    assert_eq!(status["state"], "failed");
    assert_eq!(status["last_error"], "still boom");

    // A permanent failure is final at once; success is final too.
    let permanent = queue::enqueue(&org, job("billing")).await?;
    let taken = queue::claim(&org, "n1", &[], 5, 60).await?;
    assert_eq!(queue::fail(&org, "n1", &taken[0], "rejected", true).await?, Some(Outcome::Failed));
    let fine = queue::enqueue(&org, job("billing")).await?;
    let taken = queue::claim(&org, "n1", &[], 5, 60).await?;
    assert!(queue::complete(&org, "n1", &taken[0].key).await?);
    assert_eq!(queue::status(&org, "billing", &fine.id).await?.ok_or("none")?["state"], "succeeded");
    assert_eq!(queue::status(&org, "billing", &permanent.id).await?.ok_or("none")?["state"], "failed");
    // Another plugin cannot see or cancel it.
    assert!(queue::status(&org, "other", &fine.id).await?.is_none());
    Ok(())
}

#[tokio::test]
#[ignore = "needs SurrealDB (AETHER_TEST_DB)"]
async fn a_dead_workers_job_is_taken_over_and_its_late_answer_is_ignored() -> TestResult {
    let Some(db) = connect().await? else { return Ok(()) };
    let (_, org) = fresh_org(&db, "lease").await?;
    let mut j = job("billing");
    j.max_attempts = 2;
    queue::enqueue(&org, j).await?;

    let first = queue::claim(&org, "dead", &[], 1, 1).await?;
    assert_eq!(first.len(), 1);
    assert!(queue::claim(&org, "other", &[], 1, 60).await?.is_empty(), "held while the lease lasts");
    tokio::time::sleep(Duration::from_millis(1300)).await;
    let second = queue::claim(&org, "other", &[], 1, 1).await?;
    assert_eq!(second.len(), 1, "the lease ran out");
    assert_eq!(second[0].key, first[0].key);
    assert_eq!(second[0].attempts, 2);
    // The first worker comes back and reports: it no longer holds the job.
    assert!(!queue::complete(&org, "dead", &first[0].key).await?);
    assert_eq!(queue::fail(&org, "dead", &first[0], "late", false).await?, None, "the late failure changes nothing");

    // The second worker also vanishes; no attempts are left, so the job is closed.
    tokio::time::sleep(Duration::from_millis(1300)).await;
    assert_eq!(queue::fail_abandoned(&org).await?, 1);
    Ok(())
}

#[tokio::test]
#[ignore = "needs SurrealDB (AETHER_TEST_DB)"]
async fn the_same_unique_key_is_queued_once_while_it_waits() -> TestResult {
    let Some(db) = connect().await? else { return Ok(()) };
    let (_, org) = fresh_org(&db, "unique").await?;
    let mut j = job("billing");
    j.unique_key = Some("invoice-7".into());
    let first = queue::enqueue(&org, j.clone()).await.map_err(|e| format!("first: {e}"))?;
    let second = queue::enqueue(&org, j.clone()).await.map_err(|e| format!("second: {e}"))?;
    assert!(first.created && !second.created);
    assert_eq!(first.id, second.id);
    let mut other = j.clone();
    other.plugin = Some("other".into());
    assert!(queue::enqueue(&org, other).await.map_err(|e| format!("other: {e}"))?.created, "another plugin's key is its own");

    // Once it has finished, the key can be used again.
    let taken = queue::claim(&org, "n", &[], 5, 60).await?;
    for job in &taken {
        queue::complete(&org, "n", &job.key).await?;
    }
    assert!(queue::enqueue(&org, j).await.map_err(|e| format!("again: {e}"))?.created);
    Ok(())
}

#[tokio::test]
#[ignore = "needs SurrealDB (AETHER_TEST_DB)"]
async fn queues_are_served_separately_and_cancelling_works() -> TestResult {
    let Some(db) = connect().await? else { return Ok(()) };
    let (_, org) = fresh_org(&db, "queues").await?;
    let mut mail = NewJob::new("communication", "email");
    mail.plugin = Some("crm".into());
    mail.payload = Some(json!({ "type": "email" }));
    queue::enqueue(&org, mail).await?;
    let plain = queue::enqueue(&org, job("crm")).await?;
    // A worker for the default queue ignores email.
    let taken = queue::claim(&org, "n", &["default".to_string()], 10, 60).await?;
    assert_eq!(taken.len(), 1);
    assert_eq!(taken[0].kind, "plugin");
    assert_eq!(queue::claim(&org, "n", &["email".to_string()], 10, 60).await?.len(), 1);

    // Only a job that has not started can be cancelled, and only by its own plugin.
    let waiting = queue::enqueue(&org, job("crm")).await?;
    assert!(!queue::cancel(&org, "other", &waiting.id).await?);
    assert!(queue::cancel(&org, "crm", &waiting.id).await?);
    assert!(!queue::cancel(&org, "crm", &plain.id).await?, "already running");
    assert!(queue::claim(&org, "n", &[], 10, 60).await?.is_empty());
    Ok(())
}

fn task(name: &str, every: &str) -> TaskDef {
    TaskDef {
        name: name.into(),
        function: "run".into(),
        cron: None,
        every: Some(every.into()),
        timezone: None,
        payload: Some(json!({ "why": name })),
        queue: "default".into(),
        max_attempts: 3,
        catch_up: CatchUp::Once,
    }
}

#[tokio::test]
#[ignore = "needs SurrealDB (AETHER_TEST_DB)"]
async fn a_due_task_queues_one_job_per_slot_and_moves_on() -> TestResult {
    let Some(db) = connect().await? else { return Ok(()) };
    let (_, org) = fresh_org(&db, "tasks").await?;
    tasks::sync_manifest(&org, "reports", &[task("hourly", "1h"), task("daily", "1d")]).await?;

    // Nothing is due yet.
    assert!(tasks::run_due(&org, Utc::now(), 30).await?.is_empty());

    // An hour and a bit later: the hourly task fires, once, with its payload.
    let later = Utc::now() + chrono::Duration::minutes(70);
    let fired = tasks::run_due(&org, later, 30).await?;
    assert_eq!(fired.len(), 1);
    assert_eq!((fired[0].plugin.as_str(), fired[0].task.as_str()), ("reports", "hourly"));
    let jobs = queue::claim(&org, "n", &[], 10, 60).await?;
    assert_eq!(jobs.len(), 1);
    assert_eq!(jobs[0].payload, Some(json!({ "why": "hourly" })));
    assert_eq!(jobs[0].enqueued_by.as_deref(), Some("system:scheduler"));
    // Running it again at the same moment fires nothing: the task moved to its next slot.
    assert!(tasks::run_due(&org, later, 30).await?.is_empty());

    // A long outage: the missed hours run once, not once each.
    let much_later = Utc::now() + chrono::Duration::hours(30);
    let fired = tasks::run_due(&org, much_later, 30).await?;
    assert_eq!(fired.len(), 2, "hourly and daily each run once");
    assert!(tasks::run_due(&org, much_later, 30).await?.is_empty());
    Ok(())
}

#[tokio::test]
#[ignore = "needs SurrealDB (AETHER_TEST_DB)"]
async fn skip_drops_a_run_that_is_far_behind_and_manifest_tasks_follow_the_manifest() -> TestResult {
    let Some(db) = connect().await? else { return Ok(()) };
    let (_, org) = fresh_org(&db, "skip").await?;
    let mut skipper = task("report", "10m");
    skipper.catch_up = CatchUp::Skip;
    tasks::sync_manifest(&org, "reports", &[skipper.clone(), task("old", "1h")]).await?;
    let fired = tasks::run_due(&org, Utc::now() + chrono::Duration::hours(5), 30).await?;
    let report = fired.iter().find(|f| f.task == "report").ok_or("report did not fire")?;
    assert!(report.skipped, "five hours late is too late");
    assert!(!fired.iter().find(|f| f.task == "old").ok_or("old did not fire")?.skipped);

    // A new version of the plugin drops `old`; the runtime-registered task stays.
    tasks::register_runtime(&org, "reports", &task("dynamic", "15m")).await?;
    tasks::sync_manifest(&org, "reports", &[skipper]).await?;
    let mut response = org.query("SELECT VALUE name FROM scheduled_tasks WHERE plugin = 'reports' ORDER BY name;").await?.check()?;
    let names: Vec<String> = response.take(0)?;
    assert_eq!(names, ["dynamic", "report"]);

    // A running plugin cannot replace or remove a manifest task, but can manage its own.
    assert!(tasks::register_runtime(&org, "reports", &task("report", "5m")).await.is_err());
    assert!(!tasks::cancel_runtime(&org, "reports", "report").await?);
    assert!(tasks::cancel_runtime(&org, "reports", "dynamic").await?);
    Ok(())
}

// ---- messages through the kernel command ------------------------------------------------

struct FakeScheduler {
    woken: std::sync::Mutex<Vec<String>>,
    unconfigured: Option<String>,
}

#[async_trait::async_trait]
impl SchedulerHandle for FakeScheduler {
    fn wake(&self, org: &str) {
        self.woken.lock().unwrap().push(org.to_string());
    }
    fn reload(&self) {}
    async fn defaults(&self, _: &str) -> JobDefaults {
        JobDefaults { max_attempts: 4, backoff_secs: 5 }
    }
    async fn check_messaging(&self, _: &str, kind: &str) -> Result<(), String> {
        match &self.unconfigured {
            Some(reason) => Err(format!("{kind}: {reason}")),
            None => Ok(()),
        }
    }
}

async fn plugin_context(db: &Surreal<Client>, org: &str, caps: &[&str], handle: Arc<FakeScheduler>) -> Result<PluginHostContext, Box<dyn std::error::Error>> {
    let cache = aether_core::cache::Cache::from_config(&aether_core::cache::CacheConfig {
        backend: aether_core::cache::CacheBackendKind::Moka,
        default_ttl_secs: None,
        max_entries: 10,
        max_value_bytes: 1024,
        redis: None,
    })?;
    let media: Arc<dyn aether_storage::MediaBackend> =
        Arc::new(aether_storage::ObjectStoreBackend::new(object_store::memory::InMemory::new()));
    Ok(PluginHostContext::new(
        "crm",
        caps.iter().map(|c| c.to_string()).collect(),
        Default::default(),
        session(db, org).await?,
        DbScope::new(NAMESPACE, org),
        NotificationHub::default(),
        CallInfo::new(
            AuditContext { actor: Actor::User("users:u1".into()), request_id: "req-1".into(), ip: None, user_agent: None },
            "send_welcome",
        ),
    )
    .with_services(HostServices { cache, media, scheduler: Some(handle) }))
}

#[tokio::test]
#[ignore = "needs SurrealDB (AETHER_TEST_DB)"]
async fn one_command_sends_any_type_of_message_one_job_per_recipient() -> TestResult {
    let Some(db) = connect().await? else { return Ok(()) };
    let (org, session) = fresh_org(&db, "comm").await?;
    let handle = Arc::new(FakeScheduler { woken: Default::default(), unconfigured: None });
    let caps = ["communication::send", "email::send", "sms::send"];
    let ctx = plugin_context(&db, &org, &caps, handle.clone()).await?;

    let sent = kernel_command(&ctx, "communication::send", json!({
        "type": "email", "to": ["ann@example.com", "bob@example.com"], "subject": "Welcome", "text": "Hello"
    })).await?;
    assert_eq!(sent["data"]["jobs"].as_array().map(Vec::len), Some(2));
    let sms = kernel_command(&ctx, "communication::send", json!({ "type": "sms", "to": "+2348012345678", "text": "Code 1234" })).await?;
    assert_eq!(sms["data"]["jobs"].as_array().map(Vec::len), Some(1));
    assert_eq!(handle.woken.lock().unwrap().as_slice(), [org.clone(), org.clone()], "the scheduler is woken after each send");

    // Each job is for one person, on the queue of its type, with the settings' retry defaults.
    let mut response = session.query("SELECT queue, kind, plugin, max_attempts, backoff_secs, payload.to AS to, enqueued_by FROM jobs ORDER BY queue, payload.to;").await?.check()?;
    let rows: Vec<Value> = response.take(0)?;
    assert_eq!(rows.len(), 3);
    assert_eq!(rows[0]["queue"], "email");
    assert_eq!(rows[0]["to"], "ann@example.com");
    assert_eq!(rows[0]["max_attempts"], 4);
    assert_eq!(rows[0]["backoff_secs"], 5);
    assert_eq!(rows[0]["plugin"], "crm");
    assert_eq!(rows[0]["enqueued_by"], "users:u1");
    assert_eq!(rows[2]["queue"], "sms");

    // Sending an email needs the email capability, not just `communication::send`.
    let only_sms = plugin_context(&db, &org, &["communication::send", "sms::send"], handle.clone()).await?;
    let refused = kernel_command(&only_sms, "communication::send", json!({ "type": "email", "to": "a@x.com", "subject": "S", "text": "T" })).await;
    assert!(matches!(refused, Err(HostError::Capability(_))), "{refused:?}");
    let none = plugin_context(&db, &org, &["email::send"], handle.clone()).await?;
    assert!(matches!(
        kernel_command(&none, "communication::send", json!({ "type": "email", "to": "a@x.com", "subject": "S", "text": "T" })).await,
        Err(HostError::Capability(_))
    ));

    // A plugin cannot choose the sender, and bad messages are refused before anything is queued.
    for bad in [
        json!({ "type": "email", "to": "a@x.com", "subject": "S", "text": "T", "from": "ceo@bank.com" }),
        json!({ "type": "email", "to": "not-an-address", "subject": "S", "text": "T" }),
        json!({ "type": "pigeon", "to": "x", "text": "T" }),
        json!({ "type": "sms", "to": "+2348012345678" }),
    ] {
        let result = kernel_command(&ctx, "communication::send", bad.clone()).await;
        assert!(matches!(result, Err(HostError::InvalidPayload(_))), "{bad}: {result:?}");
    }
    let mut response = session.query("SELECT count() AS n FROM jobs GROUP ALL;").await?.check()?;
    let count: Option<Value> = response.take(0)?;
    assert_eq!(count.ok_or("no count")?["n"], 3, "refused messages queued nothing");

    // Nothing is queued for a type that is not set up.
    let unconfigured = Arc::new(FakeScheduler { woken: Default::default(), unconfigured: Some("no provider is chosen".into()) });
    let ctx = plugin_context(&db, &org, &caps, unconfigured).await?;
    let result = kernel_command(&ctx, "communication::send", json!({ "type": "sms", "to": "+2348012345678", "text": "T" })).await;
    assert!(matches!(&result, Err(HostError::Message(m)) if m.contains("no provider")), "{result:?}");
    Ok(())
}

#[tokio::test]
#[ignore = "needs SurrealDB (AETHER_TEST_DB)"]
async fn plugin_jobs_and_tasks_through_the_scheduler_commands() -> TestResult {
    let Some(db) = connect().await? else { return Ok(()) };
    let (org, session) = fresh_org(&db, "cmds").await?;
    let handle = Arc::new(FakeScheduler { woken: Default::default(), unconfigured: None });
    let ctx = plugin_context(&db, &org, &["scheduler::enqueue", "scheduler::register", "scheduler::cancel"], handle.clone()).await?;

    let made = kernel_command(&ctx, "scheduler::enqueue", json!({ "function": "export", "payload": { "n": 1 }, "unique_key": "export-1", "delay_secs": 0 })).await?;
    let id = made["data"]["id"].as_str().ok_or("no id")?.to_string();
    let again = kernel_command(&ctx, "scheduler::enqueue", json!({ "function": "export", "unique_key": "export-1" })).await?;
    assert_eq!(again["data"]["created"], false);
    assert_eq!(handle.woken.lock().unwrap().len(), 1, "a duplicate does not wake anyone");
    let status = kernel_command(&ctx, "scheduler::job", json!({ "id": id })).await?;
    assert_eq!(status["data"]["state"], "queued");
    assert_eq!(status["data"]["function"], "export");
    let cancelled = kernel_command(&ctx, "scheduler::cancel_job", json!({ "id": id })).await?;
    assert_eq!(cancelled["data"]["cancelled"], true);

    for bad in [json!({ "function": "" }), json!({ "function": "f", "delay_secs": -1 }), json!({ "function": "f", "queue": "Bad Queue" }), json!({ "function": "f", "payload": [1] })] {
        assert!(matches!(kernel_command(&ctx, "scheduler::enqueue", bad.clone()).await, Err(HostError::InvalidPayload(_))), "{bad}");
    }

    kernel_command(&ctx, "scheduler::register", json!({ "name": "sweep", "function": "sweep", "every": "30m" })).await?;
    let mut response = session.query("SELECT name, source, every_secs FROM scheduled_tasks;").await?.check()?;
    let rows: Vec<Value> = response.take(0)?;
    assert_eq!(rows, [json!({ "name": "sweep", "source": "runtime", "every_secs": 1800 })]);
    assert!(kernel_command(&ctx, "scheduler::register", json!({ "name": "x", "function": "f", "cron": "bad" })).await.is_err());
    let removed = kernel_command(&ctx, "scheduler::cancel", json!({ "name": "sweep" })).await?;
    assert_eq!(removed["data"]["removed"], true);

    let no_cap = plugin_context(&db, &org, &[], handle).await?;
    assert!(matches!(kernel_command(&no_cap, "scheduler::enqueue", json!({ "function": "f" })).await, Err(HostError::Capability(_))));
    Ok(())
}

// ---- secret settings and credentials ----------------------------------------------------

async fn app_state(db: &Surreal<Client>, label: &str) -> Result<(AppState, tempfile::TempDir), Box<dyn std::error::Error>> {
    aether_orm::migrate_core(db, NAMESPACE, "core").await?;
    let dir = tempfile::tempdir()?;
    let mut config = AetherConfig::default();
    config.app_dir = dir.path().to_path_buf();
    config.media = aether_core::config_manager::models::MediaConfig::default_for(dir.path());
    config.security.secret_key = Some(format!("a long test secret key for {label} 123456"));
    let state = AppState::new(db.clone(), config, NAMESPACE, "core").await?;
    Ok((state, dir))
}

/// A global setting with this key, as the seed makes them.
async fn global_item(state: &AppState, key: &str, label: &str, value: Value, secret: bool) -> TestResult {
    let core = state.core().await?;
    core.query("CREATE gl_settings_items SET label = $label, s_key = $key, s_value = $value, is_secret = $secret;")
        .bind(("label", label.to_string()))
        .bind(("key", key.to_string()))
        .bind(("value", value))
        .bind(("secret", secret))
        .await?
        .check()?;
    Ok(())
}

#[tokio::test]
#[ignore = "needs SurrealDB (AETHER_TEST_DB)"]
async fn secret_settings_are_encrypted_masked_and_fall_back_to_the_global_one() -> TestResult {
    let Some(db) = connect().await? else { return Ok(()) };
    let (state, _dir) = app_state(&db, "secrets").await?;
    let (org_db, _) = fresh_org(&db, "secrets").await?;
    let org = OrgRef { slug: org_db.clone(), db_name: org_db.clone() };
    let key = unique("test.secret.key").replace('_', "");
    let plain_key = unique("test.plain.key").replace('_', "");
    global_item(&state, &key, &unique("Secret"), json!(""), true).await?;
    global_item(&state, &plain_key, &unique("Plain"), json!("default"), false).await?;

    // Saving a global secret: stored encrypted, shown blank, readable by the kernel.
    let saved = set_setting(&state, &key, json!("sk_live_global"), None).await?;
    assert!(saved.secret && saved.has_value);
    assert_eq!(saved.value, json!(""), "the answer never repeats a secret");
    let mut response = state.core().await?.query("SELECT VALUE s_value FROM gl_settings_items WHERE s_key = $key;").bind(("key", key.clone())).await?.check()?;
    let raw: Option<String> = response.take(0)?;
    let raw = raw.ok_or("no row")?;
    assert!(raw.starts_with("enc:v1:") && !raw.contains("sk_live_global"), "{raw}");
    let shown = get_setting(&state, &key, None).await?.ok_or("missing")?;
    assert!(shown.secret && shown.has_value && shown.value == json!(""));
    assert_eq!(get_setting_plain(&state, &key, None).await?, Some(json!("sk_live_global")));

    // An organization that sets nothing uses the global value; its own value wins and is also encrypted.
    assert_eq!(get_setting_plain(&state, &key, Some(&org)).await?, Some(json!("sk_live_global")));
    assert!(get_setting_plain_in(&state, &key, Scope::Org(&org)).await?.is_none());
    set_setting(&state, &key, json!("sk_live_own"), Some(&org)).await?;
    assert_eq!(get_setting_plain(&state, &key, Some(&org)).await?, Some(json!("sk_live_own")));
    assert_eq!(get_setting_plain_in(&state, &key, Scope::Org(&org)).await?, Some(json!("sk_live_own")));
    assert_eq!(get_setting_plain_in(&state, &key, Scope::Global).await?, Some(json!("sk_live_global")));
    let mut response = _org_session(&db, &org_db).await?.query("SELECT VALUE s_value FROM settings_items WHERE s_key = $key;").bind(("key", key.clone())).await?.check()?;
    let raw: Option<String> = response.take(0)?;
    assert!(raw.ok_or("no org row")?.starts_with("enc:v1:"));

    // Clearing the organization's secret returns it to the global one.
    let cleared = set_setting(&state, &key, json!(""), Some(&org)).await?;
    assert_eq!(cleared.source, "global");
    assert_eq!(get_setting_plain(&state, &key, Some(&org)).await?, Some(json!("sk_live_global")));

    // A secret must be text; ordinary settings are untouched.
    assert!(set_setting(&state, &key, json!(5), None).await.is_err());
    set_setting(&state, &plain_key, json!("custom"), None).await?;
    let plain = get_setting(&state, &plain_key, None).await?.ok_or("missing")?;
    assert!(!plain.secret && plain.value == json!("custom"));
    Ok(())
}

async fn _org_session(db: &Surreal<Client>, org: &str) -> Result<Arc<Surreal<Client>>, Box<dyn std::error::Error>> {
    session(db, org).await
}

#[tokio::test]
#[ignore = "needs SurrealDB (AETHER_TEST_DB)"]
async fn an_organization_uses_its_own_account_or_the_global_one_never_a_mix() -> TestResult {
    let Some(db) = connect().await? else { return Ok(()) };
    let (state, _dir) = app_state(&db, "bridges").await?;
    let (org_db, _) = fresh_org(&db, "bridges").await?;
    let org = OrgRef { slug: org_db.clone(), db_name: org_db };
    // Twilio's settings, as the seed makes them (the core database is shared by these tests,
    // so tolerate rows left by an earlier run).
    for spec_field in messaging::SMS_BRIDGES.iter().filter(|s| s.key == "twilio").flat_map(|s| s.fields()) {
        let key = messaging::bridge_setting("twilio", spec_field.name);
        let core = state.core().await?;
        core.query("UPSERT type::record('gl_settings_items', $id) SET label = $label, s_key = $key, s_value = '', is_secret = $secret;")
            .bind(("id", key.replace('.', "_")))
            .bind(("label", format!("Twilio {}", spec_field.name)))
            .bind(("key", key))
            .bind(("secret", spec_field.secret))
            .await?
            .check()?;
    }
    let twilio = messaging::SMS_BRIDGES.iter().copied().find(|s| s.key == "twilio").ok_or("no twilio")?;
    let set = |key: &str, value: &str, org: Option<&OrgRef>| {
        let (state, key, value, org) = (state.clone(), key.to_string(), value.to_string(), org.cloned());
        async move { set_setting(&state, &messaging::bridge_setting("twilio", &key), json!(value), org.as_ref()).await }
    };
    // Nothing configured: a clear message.
    let error = messaging::resolve_values(&state, &org, twilio).await.unwrap_err();
    assert!(error.message.contains("incomplete") && error.message.contains("global"), "{}", error.message);

    // The global account.
    set("account_sid", "AC_GLOBAL", None).await?;
    set("auth_token", "token_global", None).await?;
    set("from", "+15550000001", None).await?;
    let values = messaging::resolve_values(&state, &org, twilio).await.unwrap();
    assert_eq!((values["account_sid"].as_str(), values["auth_token"].as_str()), ("AC_GLOBAL", "token_global"));

    // The organization fills in only its account SID: that is a different account, so the global
    // token and number are NOT used with it, and the missing ones are named.
    set("account_sid", "AC_OWN", Some(&org)).await?;
    let error = messaging::resolve_values(&state, &org, twilio).await.unwrap_err();
    assert!(error.message.contains("Auth token") && error.message.contains("this organization's own"), "{}", error.message);

    // Complete, it uses all of its own.
    set("auth_token", "token_own", Some(&org)).await?;
    set("from", "+15550000002", Some(&org)).await?;
    let values = messaging::resolve_values(&state, &org, twilio).await.unwrap();
    assert_eq!(
        (values["account_sid"].as_str(), values["auth_token"].as_str(), values["from"].as_str()),
        ("AC_OWN", "token_own", "+15550000002")
    );

    // Clearing its secret and the other fields hands it back to the global account.
    set("auth_token", "", Some(&org)).await?;
    let core_org = session(&db, &org.db_name).await?;
    core_org.query("DELETE settings_items WHERE s_key CONTAINS 'bridge.twilio';").await?.check()?;
    let values = messaging::resolve_values(&state, &org, twilio).await.unwrap();
    assert_eq!(values["account_sid"], "AC_GLOBAL");
    Ok(())
}
