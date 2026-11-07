//! What becomes of a message once its sender has been verified (the HTTP side is in `api.rs`).
//!
//! In order: mail no person wrote is set aside; a sender who sends too much is throttled; a **reply** (its address
//! or a header carries a thread token) is appended to the record's chatter; mail to a plugin's **alias** is queued as
//! a job that calls the plugin's function; anything else goes to the **unmatched queue** for an administrator. Every
//! message is stored with its outcome, never silently dropped, and a provider's retry creates nothing twice.

use aether_storage::MediaKey;
use rand::RngExt;
use serde_json::{Value, json};

use super::message::{Inbound, RawAttachment};
use super::thread;
use crate::chatter::store::{self, Kind, NewMessage};
use crate::data_model::media::{self, MAX_MEDIA_BYTES};
use crate::plugin_manager::models::plugin_def::PluginMailRouteDef;
use crate::scheduler::queue::{self, NewJob};
use crate::state::{AppState, Databases, Db};

/// What a plugin function can be handed in one job.
const MAX_TEXT_CHARS: usize = 20_000;
const MAX_HTML_CHARS: usize = 20_000;

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Outcome {
    Duplicate,
    Ignored(String),
    Replied { model_id: String, record_key: String },
    Routed { plugin: String, function: String },
    Unmatched(String),
}

impl Outcome {
    pub fn word(&self) -> &'static str {
        match self {
            Self::Duplicate => "duplicate",
            Self::Ignored(_) => "ignored",
            Self::Replied { .. } => "replied",
            Self::Routed { .. } => "routed",
            Self::Unmatched(_) => "unmatched",
        }
    }
}

pub struct Limits {
    pub domain: Option<String>,
    pub max_per_hour: i64,
}

pub async fn limits(state: &AppState, org: &str) -> Limits {
    let org_ref = crate::chatter::org_ref(org);
    let get = |key: &'static str| {
        let org_ref = org_ref.clone();
        async move { crate::application::settings::get_setting_plain(state, key, Some(&org_ref)).await.ok().flatten() }
    };
    Limits {
        domain: get("mail.inbound.domain").await.and_then(|v| v.as_str().map(|s| s.trim().to_ascii_lowercase())).filter(|s| !s.is_empty()),
        max_per_hour: get("mail.max_per_sender_hour").await.and_then(|v| v.as_i64()).unwrap_or(30).max(1),
    }
}

/// The routes plugins declare, with the plugin's name, for the plugins installed and enabled in `org`.
pub async fn routes(state: &AppState, org: &str, db: &Db) -> Vec<(String, PluginMailRouteDef)> {
    let Ok(core) = state.core().await else { return Vec::new() };
    let Ok(installed) = crate::plugin_manager::installed::list_cached(&state.cache, org, db).await else {
        return Vec::new();
    };
    let mut out = Vec::new();
    for row in installed.into_iter().filter(|row| row.is_enabled) {
        if let Ok(loaded) = state.plugin_runtime.ensure_loaded(&core, &row.plugin_name, &row.version).await {
            out.extend(loaded.manifest.mail_routes.iter().cloned().map(|route| (row.plugin_name.clone(), route)));
        }
    }
    out
}

fn passes(message: &Inbound, require: &[String]) -> bool {
    require.iter().all(|what| match what.as_str() {
        "spf" => message.auth.spf == "pass",
        "dkim" => message.auth.dkim == "pass",
        _ => false,
    })
}

/// The first recipient address that is one of ours (right domain, when one is set), by alias.
fn alias_of<'a>(message: &'a Inbound, domain: &Option<String>) -> impl Iterator<Item = &'a str> + 'a {
    let domain = domain.clone();
    message.recipients().filter_map(move |address| {
        if let Some(wanted) = &domain
            && thread::domain_of(address) != wanted
        {
            return None;
        }
        Some(thread::split_local(address).0)
    })
}

async fn store_attachments(media: &crate::state::Media, org: &str, attachments: Vec<RawAttachment>) -> Vec<Value> {
    let Ok(backend) = media.org(org) else {
        return attachments.iter().map(|a| json!({ "name": a.name, "rejected": "storage is unavailable" })).collect();
    };
    let mut stored = Vec::new();
    for attachment in attachments {
        let name = media::clean_name(&attachment.name);
        if attachment.bytes.len() > MAX_MEDIA_BYTES {
            stored.push(json!({ "name": name, "rejected": "the file is too large" }));
            continue;
        }
        let Some(kind) = media::sniff(&attachment.bytes) else {
            stored.push(json!({ "name": name, "rejected": "that kind of file is not allowed" }));
            continue;
        };
        let (bytes, kind) = if kind.is_image() {
            let source = attachment.bytes.clone();
            match tokio::task::spawn_blocking(move || crate::media_image::process(&source, kind)).await {
                Ok(Ok(done)) => (done.bytes, done.kind),
                _ => {
                    stored.push(json!({ "name": name, "rejected": "the picture could not be read" }));
                    continue;
                }
            }
        } else {
            (attachment.bytes, kind)
        };
        let mut random = [0u8; 16];
        rand::rng().fill(&mut random);
        let now = chrono::Utc::now();
        let key = media::new_key(chrono::Datelike::year(&now), chrono::Datelike::month(&now), random, kind);
        let size = bytes.len();
        match MediaKey::parse(&key) {
            Ok(media_key) if backend.put(&media_key, bytes.into()).await.is_ok() => {
                stored.push(json!({ "media_key": key, "name": name, "size": size, "type": kind.mime() }));
            }
            _ => stored.push(json!({ "name": name, "rejected": "storage is unavailable" })),
        }
    }
    stored
}

/// The person in this organization whose account has `address`, if any.
async fn member_with_email(state: &Databases, db: &Db, address: &str) -> Option<String> {
    let core = state.core().await.ok()?;
    let mut found = core
        .query("SELECT VALUE <string> id FROM users WHERE email = $e LIMIT 1;")
        .bind(("e", address.to_string()))
        .await
        .ok()?;
    let id: String = found.take::<Vec<String>>(0).ok()?.into_iter().next()?;
    let mut member = db
        .query("SELECT VALUE core_user_id FROM org_users WHERE core_user_id = $u AND is_active = true AND deleted_at = NONE LIMIT 1;")
        .bind(("u", id.clone()))
        .await
        .ok()?;
    member.take::<Vec<String>>(0).ok()?.into_iter().next().map(|_| id)
}

struct Row<'a> {
    alias: &'a str,
    status: &'a str,
    reason: Option<String>,
    plugin: Option<&'a str>,
    function: Option<&'a str>,
    model_id: Option<&'a str>,
    record_key: Option<&'a str>,
}

/// Store the message with its outcome. `Ok(false)` when this message was stored before.
async fn record(db: &Db, message: &Inbound, stored: &[Value], row: Row<'_>) -> Result<bool, surrealdb::Error> {
    let result = db
        .query(
            "CREATE inbound_messages SET message_id = $id, alias = $alias, sender = $sender, subject = $subject, status = $status, \
             reason = $reason, plugin = $plugin, route_function = $function, model_id = $model, record_key = $key, message = $message, \
             handled_at = IF $status IN ['received', 'unmatched'] THEN NONE ELSE time::now() END;",
        )
        .bind(("id", message.message_id.clone()))
        .bind(("alias", row.alias.to_string()))
        .bind(("sender", message.from.address.clone()))
        .bind(("subject", message.subject.chars().take(300).collect::<String>()))
        .bind(("status", row.status.to_string()))
        .bind(("reason", row.reason))
        .bind(("plugin", row.plugin.map(str::to_string)))
        .bind(("function", row.function.map(str::to_string)))
        .bind(("model", row.model_id.map(str::to_string)))
        .bind(("key", row.record_key.map(str::to_string)))
        .bind(("message", message.to_json(stored.to_vec())))
        .await
        .and_then(|r| r.check());
    match result {
        Ok(_) => Ok(true),
        // The unique index on (message_id, alias): a provider's retry.
        Err(error) if error.to_string().contains("already contains") => Ok(false),
        Err(error) => Err(error),
    }
}

async fn already_stored(db: &Db, message_id: &str, alias: &str) -> Result<bool, surrealdb::Error> {
    let mut response = db
        .query("SELECT count() AS n FROM inbound_messages WHERE message_id = $id AND alias = $alias GROUP ALL;")
        .bind(("id", message_id.to_string()))
        .bind(("alias", alias.to_string()))
        .await?
        .check()?;
    let rows: Vec<Value> = response.take(0)?;
    Ok(rows.first().and_then(|r| r["n"].as_i64()).unwrap_or(0) > 0)
}

async fn sent_in_the_last_hour(db: &Db, sender: &str) -> Result<i64, surrealdb::Error> {
    let mut response = db
        .query("SELECT count() AS n FROM inbound_messages WHERE sender = $s AND received_at > time::now() - 1h GROUP ALL;")
        .bind(("s", sender.to_string()))
        .await?
        .check()?;
    let rows: Vec<Value> = response.take(0)?;
    Ok(rows.first().and_then(|r| r["n"].as_i64()).unwrap_or(0))
}

fn reply_token(message: &Inbound) -> Option<String> {
    message
        .recipients()
        .find_map(|address| thread::split_local(address).1.and_then(thread::token_in))
        .or_else(|| {
            std::iter::once(&message.in_reply_to)
                .chain(message.references.iter().rev())
                .find_map(|id| thread::token_in(id))
        })
}

pub async fn accept(state: &AppState, org: &str, db: &Db, message: Inbound) -> Result<Outcome, String> {
    let fail = |e: surrealdb::Error| format!("database error: {e}");
    let limits = limits(state, org).await;

    let mut ignore = message.automatic.clone().map(|why| format!("not written by a person ({why})"));
    if ignore.is_none() && sent_in_the_last_hour(db, &message.from.address).await.map_err(fail)? >= limits.max_per_hour {
        ignore = Some(format!("{} sent more than {} messages in an hour", message.from.address, limits.max_per_hour));
    }
    if let Some(reason) = ignore {
        if already_stored(db, &message.message_id, "").await.map_err(fail)? {
            return Ok(Outcome::Duplicate);
        }
        let stored = record(db, &message, &[], Row { alias: "", status: "ignored", reason: Some(reason.clone()), plugin: None, function: None, model_id: None, record_key: None })
            .await
            .map_err(fail)?;
        return Ok(if stored { Outcome::Ignored(reason) } else { Outcome::Duplicate });
    }

    // 1. A reply to a record.
    if let Some(token) = reply_token(&message)
        && let Some(found) = thread::find(db, &token).await.map_err(fail)?
    {
        if already_stored(db, &message.message_id, "").await.map_err(fail)? {
            return Ok(Outcome::Duplicate);
        }
        if message.auth.spf == "fail" && message.auth.dkim == "fail" {
            let reason = "the sender's domain failed both SPF and DKIM".to_string();
            let attachments = Vec::new();
            return finish_unmatched(db, &message, &attachments, reason).await;
        }
        let user = member_with_email(state.databases(), db, &message.from.address).await;
        let stored = store_attachments(&state.media, org, message.attachments.clone()).await;
        let mut body = message.text.trim().to_string();
        if body.is_empty() {
            body = "(no text)".into();
        }
        if !stored.is_empty() {
            body.push_str(&format!("\n\n{} attachment(s): {}", stored.len(), stored.iter().filter_map(|a| a["name"].as_str()).collect::<Vec<_>>().join(", ")));
        }
        let new = NewMessage {
            model_id: found.model_id.clone(),
            record_key: found.record_key.clone(),
            kind: Kind::Message,
            author: user.clone().unwrap_or_else(|| format!("mail:{}", message.from.address)),
            author_name: Some(if message.from.name.is_empty() { message.from.address.clone() } else { message.from.name.clone() }),
            body: Some(body),
            mentions: Vec::new(),
            link: None,
            plugin: Some(found.plugin.clone()),
        };
        store::insert(db, &new).await.map_err(fail)?;
        let fresh = record(
            db,
            &message,
            &stored,
            Row { alias: "", status: "replied", reason: None, plugin: Some(&found.plugin), function: None, model_id: Some(&found.model_id), record_key: Some(&found.record_key) },
        )
        .await
        .map_err(fail)?;
        return Ok(if fresh { Outcome::Replied { model_id: found.model_id, record_key: found.record_key } } else { Outcome::Duplicate });
    }

    // 2. An alias a plugin declares.
    let declared = routes(state, org, db).await;
    let chosen = alias_of(&message, &limits.domain)
        .find_map(|alias| declared.iter().find(|(_, route)| route.alias == alias).map(|(plugin, route)| (alias.to_string(), plugin.clone(), route.clone())));
    let Some((alias, plugin, route)) = chosen else {
        let reason = "no plugin has an alias for any recipient".to_string();
        if already_stored(db, &message.message_id, "").await.map_err(fail)? {
            return Ok(Outcome::Duplicate);
        }
        let stored = store_attachments(&state.media, org, message.attachments.clone()).await;
        return finish_unmatched(db, &message, &stored, reason).await;
    };
    if already_stored(db, &message.message_id, &alias).await.map_err(fail)? {
        return Ok(Outcome::Duplicate);
    }
    if !passes(&message, &route.require) {
        let reason = format!("`{alias}` requires {} to pass", route.require.join(" and "));
        let stored = store_attachments(&state.media, org, message.attachments.clone()).await;
        return finish_unmatched(db, &message, &stored, reason).await;
    }
    let stored = store_attachments(&state.media, org, message.attachments.clone()).await;
    if !record(
        db,
        &message,
        &stored,
        Row { alias: &alias, status: "routed", reason: None, plugin: Some(&plugin), function: Some(&route.function), model_id: None, record_key: None },
    )
    .await
    .map_err(fail)?
    {
        return Ok(Outcome::Duplicate);
    }
    enqueue_route(state, org, db, &message, &stored, &alias, &plugin, &route).await?;
    Ok(Outcome::Routed { plugin, function: route.function })
}

async fn finish_unmatched(db: &Db, message: &Inbound, stored: &[Value], reason: String) -> Result<Outcome, String> {
    let fail = |e: surrealdb::Error| format!("database error: {e}");
    let fresh = record(db, message, stored, Row { alias: "", status: "unmatched", reason: Some(reason.clone()), plugin: None, function: None, model_id: None, record_key: None })
        .await
        .map_err(fail)?;
    Ok(if fresh { Outcome::Unmatched(reason) } else { Outcome::Duplicate })
}

/// The payload of the job that calls a route's function.
fn payload(message: &Inbound, stored: &[Value], alias: &str, route: &PluginMailRouteDef, sender_user: Option<String>) -> Value {
    let mut shown = message.clone();
    shown.text = shown.text.chars().take(MAX_TEXT_CHARS).collect();
    shown.html = shown.html.chars().take(MAX_HTML_CHARS).collect();
    let mut value = shown.to_json(stored.to_vec());
    value["event"] = json!("mail.received");
    value["route"] = json!({ "alias": alias, "model": route.model });
    value["sender_user"] = json!(sender_user);
    value
}

#[allow(clippy::too_many_arguments)]
async fn enqueue_route(
    state: &AppState,
    org: &str,
    db: &Db,
    message: &Inbound,
    stored: &[Value],
    alias: &str,
    plugin: &str,
    route: &PluginMailRouteDef,
) -> Result<(), String> {
    let sender_user = member_with_email(state.databases(), db, &message.from.address).await;
    let mut job = NewJob::new("plugin", "default");
    job.plugin = Some(plugin.to_string());
    job.function = Some(route.function.clone());
    job.payload = Some(payload(message, stored, alias, route, sender_user));
    job.unique_key = Some(format!("mail:{alias}:{}", message.message_id));
    job.enqueued_by = Some("system:mail".into());
    queue::enqueue(db, job).await.map_err(|e| format!("could not queue the message: {e}"))?;
    state.scheduler.wake(org);
    Ok(())
}

/// Hand an unmatched message to a plugin's route by hand (an administrator's decision).
pub async fn assign(state: &AppState, org: &str, db: &Db, key: &str, plugin: &str, alias: &str) -> Result<(), String> {
    let fail = |e: surrealdb::Error| format!("database error: {e}");
    let mut found = db
        .query("SELECT * FROM inbound_messages WHERE id = type::record('inbound_messages', $k) AND status = 'unmatched' LIMIT 1;")
        .bind(("k", key.to_string()))
        .await
        .map_err(fail)?
        .check()
        .map_err(fail)?;
    let rows: Vec<Value> = found.take(0).map_err(fail)?;
    let Some(row) = rows.into_iter().next() else {
        return Err("there is no unmatched message with that id".into());
    };
    let declared = routes(state, org, db).await;
    let Some((_, route)) = declared.iter().find(|(p, r)| p == plugin && r.alias == alias) else {
        return Err("that plugin has no such alias".into());
    };
    let message = row.get("message").cloned().unwrap_or(Value::Null);
    let sender = message["from"]["address"].as_str().unwrap_or_default().to_string();
    let sender_user = member_with_email(state.databases(), db, &sender).await;
    let mut value = message.clone();
    value["event"] = json!("mail.received");
    value["route"] = json!({ "alias": alias, "model": route.model });
    value["sender_user"] = json!(sender_user);
    value["assigned_by_hand"] = json!(true);
    let mut job = NewJob::new("plugin", "default");
    job.plugin = Some(plugin.to_string());
    job.function = Some(route.function.clone());
    job.payload = Some(value);
    job.enqueued_by = Some("system:mail".into());
    queue::enqueue(db, job).await.map_err(|e| format!("could not queue the message: {e}"))?;
    state.scheduler.wake(org);
    db.query("UPDATE type::record('inbound_messages', $k) SET status = 'routed', plugin = $p, route_function = $f, reason = NONE, handled_at = time::now();")
        .bind(("k", key.to_string()))
        .bind(("p", plugin.to_string()))
        .bind(("f", route.function.clone()))
        .await
        .map_err(fail)?
        .check()
        .map_err(fail)?;
    Ok(())
}

/// Remove old messages: handled ones after `keep_days`, unmatched ones after `unmatched_days` (0 keeps them forever).
pub async fn purge(db: &Db, keep_days: u64, unmatched_days: u64) -> Result<u64, surrealdb::Error> {
    let mut removed = 0u64;
    for (days, unmatched) in [(keep_days, false), (unmatched_days, true)] {
        if days == 0 {
            continue;
        }
        let mut response = db
            .query(format!(
                "LET $gone = (DELETE inbound_messages WHERE received_at < time::now() - <duration> string::concat($days, 'd') \
                 AND status {} 'unmatched' RETURN BEFORE); RETURN array::len($gone);",
                if unmatched { "=" } else { "!=" }
            ))
            .bind(("days", days as i64))
            .await?
            .check()?;
        removed += response.take::<Option<u64>>(1)?.unwrap_or(0);
    }
    Ok(removed)
}

/// Trim old messages now and then. Returns the task so shutdown can stop it.
pub fn spawn_cleanup(state: &AppState) -> tokio::task::JoinHandle<()> {
    let state = state.clone();
    tokio::spawn(async move {
        tokio::time::sleep(std::time::Duration::from_secs(150)).await;
        loop {
            if let Ok(organizations) = crate::access::org_databases::list_cached(&state).await {
                for db_name in organizations {
                    let Ok(db) = state.org(&db_name).await else { continue };
                    let org_ref = crate::chatter::org_ref(&db_name);
                    let days = |key: &'static str, default: u64| {
                        let org_ref = org_ref.clone();
                        let state = state.clone();
                        async move {
                            crate::application::settings::get_setting_plain(&state, key, Some(&org_ref))
                                .await
                                .ok()
                                .flatten()
                                .and_then(|v| v.as_u64())
                                .unwrap_or(default)
                        }
                    };
                    let (keep, unmatched) = (days("mail.keep_days", 90).await, days("mail.unmatched_keep_days", 90).await);
                    match purge(&db, keep, unmatched).await {
                        Ok(0) => {}
                        Ok(removed) => log::info!("Inbound mail cleanup removed {removed} message(s) from `{db_name}`"),
                        Err(error) => log::debug!("Inbound mail cleanup skipped `{db_name}`: {error}"),
                    }
                }
            }
            tokio::time::sleep(std::time::Duration::from_secs(60 * 60)).await;
        }
    })
}
