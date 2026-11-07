//! Webhook delivery. Signing, queueing and the settings are in `aether_kernel::webhooks`; the worker
//! that sends them, which needs the server state, is here.

pub use aether_kernel::webhooks::*;

use std::time::Duration;
use serde_json::{Value, json};
use surrealdb::{Surreal, engine::remote::ws::Client};
use crate::scheduler::queue::Job;
use crate::state::AppState;

type Failure = (String, bool);

/// Run one `webhook` job: send the delivery it names.
pub async fn run(state: &AppState, org_db: &str, job: &Job) -> Result<(), Failure> {
    let ulid = job
        .payload
        .as_ref()
        .and_then(|p| p.get("delivery"))
        .and_then(Value::as_str)
        .ok_or(("the job names no delivery".to_string(), true))?
        .to_string();
    let db = state.org(org_db).await.map_err(|e| (format!("the database: {e}"), false))?;
    let core = state.core().await.ok();
    let mut response = db
        .query("SELECT * FROM webhook_deliveries WHERE ulid = $ulid LIMIT 1;")
        .bind(("ulid", ulid.clone()))
        .await
        .and_then(|r| r.check())
        .map_err(|e| (format!("the database: {e}"), false))?;
    let rows: Vec<Value> = response.take(0).map_err(|e| (e.to_string(), false))?;
    let Some(delivery) = rows.into_iter().next() else {
        return Ok(()); // purged meanwhile
    };
    let subscription_id = delivery.get("subscription").and_then(Value::as_str).unwrap_or("").to_string();
    let mut response = db
        .query("SELECT * FROM type::record('webhook_subscriptions', $id);")
        .bind(("id", subscription_id.clone()))
        .await
        .and_then(|r| r.check())
        .map_err(|e| (format!("the database: {e}"), false))?;
    let subs: Vec<Value> = response.take(0).map_err(|e| (e.to_string(), false))?;
    let Some(sub) = subs.into_iter().next() else {
        let _ = finish(&db, &ulid, "dead", None, Some("the subscription was removed"), None, None, job.attempts).await;
        return Ok(());
    };
    // A subscription switched off after this was queued does not send; replaying switches nothing on.
    if sub.get("active").and_then(Value::as_bool) != Some(true) {
        let _ = finish(&db, &ulid, "dead", None, Some("the subscription is switched off"), None, None, job.attempts).await;
        return Ok(());
    }

    let secrets = state.secrets().map_err(|e| (format!("secrets: {e}"), true))?;
    let mut keys = Vec::new();
    if let Some(current) = sub.get("secret").and_then(Value::as_str).and_then(|s| secrets.decrypt(s).ok()) {
        keys.push(current);
    }
    let overlap_until = sub
        .get("secret_prev_until")
        .and_then(Value::as_str)
        .and_then(|t| chrono::DateTime::parse_from_rfc3339(t).ok());
    if overlap_until.is_some_and(|until| until > chrono::Utc::now())
        && let Some(previous) = sub.get("secret_prev").and_then(Value::as_str).and_then(|s| secrets.decrypt(s).ok())
    {
        keys.push(previous);
    }

    let event = delivery.get("event").and_then(Value::as_str).unwrap_or("").to_string();
    let created = delivery.get("created_at").cloned().unwrap_or(Value::Null);
    let body = serde_json::to_vec(&json!({
        "id": ulid,
        "event": event,
        "created_at": created,
        "data": delivery.get("data").cloned().unwrap_or(json!({})),
    }))
    .map_err(|e| (e.to_string(), true))?;
    let timestamp = chrono::Utc::now().timestamp();
    let headers = [
        ("content-type", "application/json".to_string()),
        ("x-aether-event", event.clone()),
        ("x-aether-delivery", ulid.clone()),
        ("x-aether-attempt", job.attempts.to_string()),
        ("x-aether-signature", signature_header(timestamp, &keys, &body)),
    ];
    let allow_http = setting(&db, core.as_ref(), "webhooks.allow_http").await == Some(Value::Bool(true));
    let allowed: Vec<String> = setting(&db, core.as_ref(), "webhooks.allowed_hosts")
        .await
        .and_then(|v| v.as_array().map(|list| list.iter().filter_map(|h| h.as_str().map(str::to_string)).collect()))
        .unwrap_or_default();
    let url = sub.get("url").and_then(Value::as_str).unwrap_or("").to_string();

    let sent = post(&url, body, &headers, allow_http, &allowed).await;
    let ok = sent.status.is_some_and(|code| (200..300).contains(&code));
    let final_attempt = job.attempts >= job.max_attempts;
    let message = sent.error.clone().or_else(|| {
        (!ok).then(|| format!("the receiver answered {}", sent.status.map_or(0, i64::from)))
    });
    let state_after = if ok {
        "delivered"
    } else if final_attempt {
        "dead"
    } else {
        "retrying"
    };
    finish(&db, &ulid, state_after, sent.status, message.as_deref(), Some(&sent.head), Some(sent.millis), job.attempts)
        .await
        .map_err(|e| (format!("the delivery could not be recorded: {e}"), false))?;
    if ok {
        let _ = db
            .query("UPDATE type::record('webhook_subscriptions', $id) SET failures = 0;")
            .bind(("id", subscription_id))
            .await;
        return Ok(());
    }
    if final_attempt {
        give_up(&db, core.as_ref(), &subscription_id).await;
        return Ok(()); // the queue has nothing more to try; the log says what happened
    }
    Err((message.unwrap_or_else(|| "it did not go through".into()), false))
}

#[allow(clippy::too_many_arguments)]
async fn finish(
    db: &Surreal<Client>,
    ulid: &str,
    status: &str,
    http_status: Option<u16>,
    error: Option<&str>,
    head: Option<&str>,
    millis: Option<i64>,
    attempts: i64,
) -> Result<(), surrealdb::Error> {
    db.query(
        "UPDATE webhook_deliveries SET status = $status, attempts = $attempts, http_status = $http, error = $error, \
         response_head = $head, duration_ms = $ms, last_attempt_at = time::now() WHERE ulid = $ulid;",
    )
    .bind(("status", status.to_string()))
    .bind(("attempts", attempts))
    .bind(("http", http_status.map(i64::from)))
    .bind(("error", error.map(str::to_string)))
    .bind(("head", head.map(str::to_string)))
    .bind(("ms", millis))
    .bind(("ulid", ulid.to_string()))
    .await?
    .check()?;
    Ok(())
}

/// A delivery gave up: count it against the subscription, and switch the subscription off when too
/// many in a row did.
async fn give_up(db: &crate::databases::Db, core: Option<&crate::databases::Db>, subscription: &str) {
    let limit = setting_u64(db, core, "webhooks.disable_after", DEFAULT_DISABLE_AFTER as u64).await as i64;
    let result = db
        .query(
            "UPDATE type::record('webhook_subscriptions', $id) SET failures += 1, \
             active = IF failures + 1 >= $limit THEN false ELSE active END, \
             disabled_reason = IF failures + 1 >= $limit THEN string::concat($limit, ' deliveries in a row gave up') ELSE disabled_reason END;",
        )
        .bind(("id", subscription.to_string()))
        .bind(("limit", limit.max(1)))
        .await;
    if let Err(error) = result {
        log::warn!("a webhook subscription's failures were not counted: {error}");
    }
}

/// Remove deliveries older than `days` (0 keeps them forever). Returns how many went.
pub async fn purge(db: &Surreal<Client>, days: u64) -> Result<u64, surrealdb::Error> {
    if days == 0 {
        return Ok(0);
    }
    let mut response = db
        .query(
            "LET $cutoff = time::now() - <duration> string::concat($days, 'd'); \
             LET $gone = (DELETE webhook_deliveries WHERE created_at < $cutoff RETURN BEFORE); \
             RETURN array::len($gone);",
        )
        .bind(("days", days as i64))
        .await?
        .check()?;
    let removed: Option<u64> = response.take(2)?;
    Ok(removed.unwrap_or(0))
}

/// Trim old deliveries now and then. Returns the task so shutdown can stop it.
pub fn spawn_cleanup(state: &AppState) -> tokio::task::JoinHandle<()> {
    let state = state.clone();
    tokio::spawn(async move {
        tokio::time::sleep(Duration::from_secs(120)).await;
        loop {
            if let Ok(organizations) = crate::access::org_databases::list_cached(&state).await {
                for db_name in organizations {
                    let Ok(db) = state.org(&db_name).await else {
                        continue;
                    };
                    let core = state.core().await.ok();
                    let days = setting_u64(&db, core.as_ref(), "webhooks.log_days", DEFAULT_LOG_DAYS).await;
                    match purge(&db, days).await {
                        Ok(0) => {}
                        Ok(removed) => log::info!("Webhook log cleanup removed {removed} delivery record(s) from `{db_name}`"),
                        Err(error) => log::debug!("Webhook log cleanup skipped `{db_name}`: {error}"),
                    }
                }
            }
            tokio::time::sleep(Duration::from_secs(60 * 60)).await;
        }
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use hmac::{Hmac, KeyInit, Mac};
    use sha2::Sha256;

    #[test]
    fn a_signature_covers_the_time_and_the_exact_body() {
        let body = br#"{"id":"1"}"#;
        let sig = sign("whsec_x", 1700000000, body);
        assert_eq!(sig.len(), 64);
        assert_eq!(sig, sign("whsec_x", 1700000000, body), "deterministic");
        assert_ne!(sig, sign("whsec_x", 1700000001, body), "the time is signed");
        assert_ne!(sig, sign("whsec_x", 1700000000, br#"{"id":"2"}"#), "the body is signed");
        assert_ne!(sig, sign("whsec_y", 1700000000, body), "the secret is used");
        // The text that is signed, written out: receivers compute exactly this.
        assert_eq!(signed_text(7, b"abc"), b"7.abc".to_vec());
    }

    #[test]
    fn the_header_lists_one_signature_per_secret_in_force() {
        let header = signature_header(5, &["a".into(), "b".into()], b"x");
        let parts: Vec<&str> = header.split(',').collect();
        assert_eq!(parts[0], "t=5");
        assert_eq!(parts.len(), 3);
        assert_eq!(parts[1], format!("v1={}", sign("a", 5, b"x")));
        assert_eq!(parts[2], format!("v1={}", sign("b", 5, b"x")));
        assert_eq!(signature_header(5, &[], b"x"), "t=5");
    }

    #[test]
    fn a_receiver_can_verify_with_hmac_sha256_alone() {
        // What a receiver in any language does: HMAC-SHA256 of "<t>.<body>" with the secret, as hex.
        let mut mac = Hmac::<Sha256>::new_from_slice(b"whsec_k").expect("key");
        mac.update(b"1700000000.{\"a\":1}");
        let expected: String = mac.finalize().into_bytes().iter().map(|b| format!("{b:02x}")).collect();
        assert_eq!(sign("whsec_k", 1700000000, br#"{"a":1}"#), expected);
    }

    #[test]
    fn secrets_are_long_and_recognisable() {
        let secret = generate_secret();
        assert!(secret.starts_with("whsec_") && secret.len() == 6 + 48);
        assert_ne!(secret, generate_secret());
    }

    #[test]
    fn only_the_allowed_fields_of_the_payload_leave() {
        let payload = json!({ "id": 1, "name": "A", "internal_note": "private", "nested": { "a": 1 } });
        let data = project(&payload, &["id".into(), "name".into(), "missing".into()]);
        assert_eq!(Value::Object(data), json!({ "id": 1, "name": "A" }));
        assert!(project(&json!("text"), &["id".into()]).is_empty(), "a payload that is not an object sends nothing");
        assert!(project(&Value::Null, &["id".into()]).is_empty());
    }

    #[test]
    fn conditions_are_equalities_on_the_sent_fields() {
        let data = project(&json!({ "status": "won", "amount": 5 }), &["status".into(), "amount".into()]);
        assert!(meets(&data, None));
        assert!(meets(&data, Some(&json!({}))));
        assert!(meets(&data, Some(&json!({ "status": "won" }))));
        assert!(meets(&data, Some(&json!({ "status": "won", "amount": 5 }))));
        assert!(!meets(&data, Some(&json!({ "status": "lost" }))));
        assert!(!meets(&data, Some(&json!({ "status": "won", "amount": 6 }))));
        assert!(!meets(&data, Some(&json!({ "colour": "red" }))), "a field that was not sent never matches");
    }

    #[test]
    fn addresses_are_https_without_credentials() {
        assert_eq!(url_problem("https://hooks.example.com/in", false), None);
        assert_eq!(url_problem("http://localhost:9000/in", true), None, "plain http only when switched on");
        assert!(url_problem("http://hooks.example.com/in", false).is_some());
        assert!(url_problem("ftp://x.example.com/", true).is_some());
        assert!(url_problem("https://user:pw@hooks.example.com/", false).is_some());
        assert!(url_problem("https://", false).is_some());
        assert!(url_problem("not an address", false).is_some());
    }
}
