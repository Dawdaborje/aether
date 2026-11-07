//! Webhooks: telling outside systems that something happened (see `docs/architecture/api_and_webhooks.md`).
//!
//! A plugin says which of the events it emits may leave, and which fields of their payload
//! (`webhook = [..]` in `[[events]]`); nothing else is ever sent. An administrator subscribes an
//! address to events of installed plugins. When a plugin emits such an event ([`enqueue`], from
//! `plugin_events::dispatch`), each matching subscription gets a delivery and a background job
//! ([`KIND`]) that sends it ([`run`]):
//!
//! * a JSON envelope `{ id, event, created_at, data }`, signed with the subscription's secret:
//!   `X-Aether-Signature: t=<unix>,v1=<hex HMAC-SHA256 of "<t>.<body>">`, one `v1` per secret
//!   in force (two during the day after a rotation);
//! * only to `https` addresses (unless `webhooks.allow_http`) that resolve to public addresses
//!   (unless the host is in `webhooks.allowed_hosts`), connecting to the address that was checked;
//!   no redirects, a 10 s timeout, a body of at most 256 KiB, no proxy;
//! * at least once: a failure is tried again with growing waits (the scheduler's retries) up to
//!   `webhooks.max_attempts`, then the delivery is `dead` and can be sent again by hand. A subscription
//!   whose deliveries keep dying is switched off after `webhooks.disable_after` in a row.
//!
//! Every attempt is kept (status, time, the start of the answer) for `webhooks.log_days`.

use std::net::SocketAddr;
use std::time::{Duration, Instant};

use hmac::{Hmac, KeyInit, Mac};
use serde_json::{Map, Value, json};
use sha2::Sha256;
use surrealdb::{Surreal, engine::remote::ws::Client};

use crate::kernel::host::context::PluginHostContext;
use crate::scheduler::queue::{self, NewJob};

/// The scheduler job kind that sends one delivery.
pub const KIND: &str = "webhook";
pub const QUEUE: &str = "webhooks";
/// Largest body sent.
pub const MAX_BODY_BYTES: usize = 256 * 1024;
pub const TIMEOUT: Duration = Duration::from_secs(10);
pub const RESPONSE_HEAD_BYTES: usize = 1024;
/// How long the old secret is still signed with after a rotation.
pub const ROTATION_OVERLAP_HOURS: i64 = 24;
pub const MAX_SUBSCRIPTIONS: usize = 100;
pub const MAX_EVENTS_PER_SUBSCRIPTION: usize = 50;
pub const DEFAULT_MAX_ATTEMPTS: i64 = 8;
pub const DEFAULT_RETRY_BASE_SECS: u64 = 60;
pub const DEFAULT_LOG_DAYS: u64 = 14;
pub const DEFAULT_DISABLE_AFTER: i64 = 20;

/// The text a signature covers: the time, a dot, and the body exactly as sent.
pub fn signed_text(timestamp: i64, body: &[u8]) -> Vec<u8> {
    let mut text = format!("{timestamp}.").into_bytes();
    text.extend_from_slice(body);
    text
}

/// Lower-case hex HMAC-SHA256 of `timestamp.body` under `secret`.
pub fn sign(secret: &str, timestamp: i64, body: &[u8]) -> String {
    let Ok(mut mac) = Hmac::<Sha256>::new_from_slice(secret.as_bytes()) else {
        return String::new();
    };
    mac.update(&signed_text(timestamp, body));
    mac.finalize().into_bytes().iter().map(|b| format!("{b:02x}")).collect()
}

/// The `X-Aether-Signature` header for the secrets in force.
pub fn signature_header(timestamp: i64, secrets: &[String], body: &[u8]) -> String {
    let mut parts = vec![format!("t={timestamp}")];
    parts.extend(secrets.iter().map(|secret| format!("v1={}", sign(secret, timestamp, body))));
    parts.join(",")
}

/// A new signing secret: `whsec_` and 48 hex characters.
pub fn generate_secret() -> String {
    let mut bytes = [0u8; 24];
    rand::RngExt::fill(&mut rand::rng(), &mut bytes);
    format!("whsec_{}", bytes.iter().map(|b| format!("{b:02x}")).collect::<String>())
}

/// The payload cut down to the fields the event allows: nothing else leaves.
pub fn project(payload: &Value, fields: &[String]) -> Map<String, Value> {
    let mut out = Map::new();
    if let Some(object) = payload.as_object() {
        for field in fields {
            if let Some(value) = object.get(field) {
                out.insert(field.clone(), value.clone());
            }
        }
    }
    out
}

/// Whether `data` meets a subscription's conditions (every listed field equals its value).
pub fn meets(data: &Map<String, Value>, conditions: Option<&Value>) -> bool {
    match conditions.and_then(Value::as_object) {
        None => true,
        Some(wanted) => wanted.iter().all(|(field, value)| data.get(field) == Some(value)),
    }
}

/// What is wrong with a subscription address, if anything (the network is checked when sending).
pub fn url_problem(text: &str, allow_http: bool) -> Option<String> {
    let url = match url::Url::parse(text) {
        Ok(url) => url,
        Err(error) => return Some(format!("not an address: {error}")),
    };
    match url.scheme() {
        "https" => {}
        "http" if allow_http => {}
        "http" => return Some("only https:// addresses are allowed here".into()),
        _ => return Some("use an https:// address".into()),
    }
    if !url.username().is_empty() || url.password().is_some() {
        return Some("an address must not carry a username or password".into());
    }
    if url.host_str().is_none_or(str::is_empty) {
        return Some("the address has no host".into());
    }
    None
}

#[derive(Debug, thiserror::Error)]
pub enum WebhookError {
    #[error("{0}")]
    Invalid(String),
    #[error("database error: {0}")]
    Database(#[from] surrealdb::Error),
}

pub async fn setting(db: &crate::databases::Db, core: Option<&crate::databases::Db>, key: &str) -> Option<Value> {
    crate::kernel::host::settings_read::effective_in(db, core, &[key]).await.remove(key)
}

pub async fn setting_u64(db: &crate::databases::Db, core: Option<&crate::databases::Db>, key: &str, default: u64) -> u64 {
    setting(db, core, key).await.and_then(|v| v.as_u64()).unwrap_or(default)
}

/// Queue a delivery for every active subscription to `full` (`<plugin>.<event>`), if the plugin lets
/// this event leave (`webhook` in `[[events]]`). Returns how many were queued. Never fails an emit.
pub async fn enqueue(
    ctx: &PluginHostContext,
    short: &str,
    full: &str,
    payload: &Value,
) -> Result<usize, surrealdb::Error> {
    let Some(fields) = ctx.webhook_events.get(short) else {
        return Ok(0);
    };
    let mut response = ctx
        .db
        .query("SELECT <string> record::id(id) AS id, conditions FROM webhook_subscriptions WHERE active = true AND $event IN events;")
        .bind(("event", full.to_string()))
        .await?
        .check()?;
    let subscriptions: Vec<Value> = response.take(0)?;
    if subscriptions.is_empty() {
        return Ok(0);
    }
    let data = project(payload, fields);
    let core = ctx.services.as_ref().and_then(|services| services.core.clone());
    let attempts = setting_u64(&ctx.db, core.as_ref(), "webhooks.max_attempts", DEFAULT_MAX_ATTEMPTS as u64).await;
    let backoff = setting_u64(&ctx.db, core.as_ref(), "webhooks.retry_base_secs", DEFAULT_RETRY_BASE_SECS).await as i64;
    let mut queued = 0;
    for subscription in subscriptions {
        let (Some(id), conditions) = (subscription.get("id").and_then(Value::as_str), subscription.get("conditions")) else {
            continue;
        };
        if !meets(&data, conditions) {
            continue;
        }
        if queue_delivery(&ctx.db, id, full, &data, None, Retry { attempts: attempts as i64, backoff_secs: backoff }, Some(&ctx.audit.request_id)).await? {
            queued += 1;
        }
    }
    if queued > 0
        && let Some(scheduler) = ctx.services.as_ref().and_then(|services| services.scheduler.clone())
    {
        scheduler.wake(&ctx.database);
    }
    Ok(queued)
}

/// How a delivery is tried again: how many times in all, and the wait before the second try (it doubles).
#[derive(Debug, Clone, Copy)]
pub struct Retry {
    pub attempts: i64,
    pub backoff_secs: i64,
}

/// The retry settings of an organization.
pub async fn retry_of(db: &crate::databases::Db, core: Option<&crate::databases::Db>) -> Retry {
    Retry {
        attempts: setting_u64(db, core, "webhooks.max_attempts", DEFAULT_MAX_ATTEMPTS as u64).await as i64,
        backoff_secs: setting_u64(db, core, "webhooks.retry_base_secs", DEFAULT_RETRY_BASE_SECS).await as i64,
    }
}

/// Make a delivery row and its job. `false` when the data is too large to send (kept as `dead`).
pub async fn queue_delivery(
    db: &Surreal<Client>,
    subscription: &str,
    event: &str,
    data: &Map<String, Value>,
    replay_of: Option<&str>,
    retry: Retry,
    request_id: Option<&str>,
) -> Result<bool, surrealdb::Error> {
    let too_large = serde_json::to_vec(data).map_or(true, |bytes| bytes.len() > MAX_BODY_BYTES);
    let mut response = db
        .query(
            "CREATE webhook_deliveries SET ulid = rand::ulid(), subscription = $sub, event = $event, data = $data, \
             status = $status, error = $error, replay_of = $replay RETURN ulid;",
        )
        .bind(("sub", subscription.to_string()))
        .bind(("event", event.to_string()))
        .bind(("data", Value::Object(data.clone())))
        .bind(("status", if too_large { "dead" } else { "pending" }))
        .bind(("error", too_large.then(|| format!("the data is larger than {MAX_BODY_BYTES} bytes"))))
        .bind(("replay", replay_of.map(str::to_string)))
        .await?
        .check()?;
    let made: Vec<Value> = response.take(0)?;
    let Some(ulid) = made.first().and_then(|row| row.get("ulid")).and_then(Value::as_str) else {
        return Ok(false);
    };
    if too_large {
        return Ok(false);
    }
    let mut job = NewJob::new(KIND, QUEUE);
    job.payload = Some(json!({ "delivery": ulid }));
    job.max_attempts = retry.attempts.max(1);
    job.backoff_secs = retry.backoff_secs.max(1);
    job.unique_key = Some(format!("webhook-{ulid}"));
    job.enqueued_by = Some("system:webhooks".into());
    job.request_id = request_id.map(str::to_string);
    if let Err(error) = queue::enqueue(db, job).await {
        log::warn!("a webhook delivery could not be queued: {error}");
        return Ok(false);
    }
    Ok(true)
}

/// What sending one delivery came to.
pub struct Sent {
    pub status: Option<u16>,
    pub head: String,
    pub error: Option<String>,
    pub millis: i64,
}

/// Send `body` to `url`, to an address that was checked to be public (or listed in `allowed_hosts`).
pub async fn post(
    url: &str,
    body: Vec<u8>,
    headers: &[(&str, String)],
    allow_http: bool,
    allowed_hosts: &[String],
) -> Sent {
    let started = Instant::now();
    let fail = |error: String| Sent {
        status: None,
        head: String::new(),
        error: Some(error),
        millis: started.elapsed().as_millis() as i64,
    };
    if let Some(problem) = url_problem(url, allow_http) {
        return fail(problem);
    }
    let Ok(parsed) = url::Url::parse(url) else {
        return fail("not an address".into());
    };
    let host = parsed.host_str().unwrap_or_default().to_string();
    let port = parsed.port_or_known_default().unwrap_or(443);
    let listed = crate::kernel::host::http::host_allowed(allowed_hosts, &host);
    let addresses: Vec<SocketAddr> = match tokio::time::timeout(
        Duration::from_secs(5),
        tokio::net::lookup_host((host.as_str(), port)),
    )
    .await
    {
        Ok(Ok(found)) => found.collect(),
        Ok(Err(_)) => return fail(format!("`{host}` could not be resolved")),
        Err(_) => return fail(format!("`{host}` did not resolve in time")),
    };
    if addresses.is_empty() {
        return fail(format!("`{host}` has no address"));
    }
    if !listed && addresses.iter().any(|a| !crate::kernel::host::http::is_public_address(a.ip())) {
        return fail(format!(
            "`{host}` points at a private or reserved address (list it in `webhooks.allowed_hosts` to allow it)"
        ));
    }
    let client = match reqwest::Client::builder()
        .redirect(reqwest::redirect::Policy::none())
        .no_proxy()
        .timeout(TIMEOUT)
        .user_agent("Aether-Webhook")
        .resolve_to_addrs(&host, &addresses)
        .build()
    {
        Ok(client) => client,
        Err(error) => return fail(format!("could not prepare the request: {error}")),
    };
    let mut request = client.post(parsed).body(body);
    for (name, value) in headers {
        request = request.header(*name, value);
    }
    let mut response = match request.send().await {
        Ok(response) => response,
        Err(error) => {
            return fail(if error.is_timeout() {
                format!("`{host}` did not answer within {}s", TIMEOUT.as_secs())
            } else if error.is_connect() {
                format!("could not connect to `{host}`")
            } else {
                format!("the request to `{host}` failed")
            });
        }
    };
    let status = response.status().as_u16();
    // Only the start of the answer is kept; the rest is not read.
    let mut head = Vec::new();
    while head.len() < RESPONSE_HEAD_BYTES {
        match response.chunk().await {
            Ok(Some(chunk)) => head.extend_from_slice(&chunk[..chunk.len().min(RESPONSE_HEAD_BYTES - head.len())]),
            _ => break,
        }
    }
    Sent {
        status: Some(status),
        head: String::from_utf8_lossy(&head).into_owned(),
        error: None,
        millis: started.elapsed().as_millis() as i64,
    }
}

