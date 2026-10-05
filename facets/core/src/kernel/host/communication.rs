//! `communication::send`: one command for every kind of message.
//!
//! The plugin says what *type* of message it is sending and what is in it; it never names a
//! provider. The kernel checks the message, puts one job per recipient on the queue, and the
//! scheduler delivers each through the bridge the administrator configured for that type (SMTP or
//! Resend for `email`; Twilio, Termii or Africa's Talking for `sms`).
//!
//! ```json
//! { "type": "email", "to": ["ann@example.com"], "subject": "Your invoice", "text": "Total: 500" }
//! { "type": "sms",   "to": "+2348012345678", "text": "Your code is 1234" }
//! ```
//!
//! Needs `communication::send` and the capability of the type (`email::send`, `sms::send`). The
//! answer is `{ jobs: [ids] }`: the message is queued, not yet delivered. The sender is set in the
//! settings, not by the plugin.

use serde_json::Value as JsonValue;

use crate::messaging::{Message, message};
use crate::scheduler::queue::{self, NewJob, QueueError};

use super::context::PluginHostContext;
use super::error::HostError;

/// Most messages one plugin may queue in an organization per hour.
pub const MAX_PER_PLUGIN_PER_HOUR: i64 = 1000;

pub async fn send(ctx: &PluginHostContext, payload: &JsonValue) -> Result<JsonValue, HostError> {
    ctx.require_cap("communication::send")?;
    let request = payload
        .as_object()
        .ok_or_else(|| HostError::InvalidPayload("a message is a JSON object".into()))?;
    let kind = request.get("type").and_then(JsonValue::as_str).unwrap_or("");
    match kind {
        "email" | "sms" => ctx.require_cap(&format!("{kind}::send"))?,
        _ => {}
    }
    message::reject_unknown_fields(request).map_err(|e| HostError::InvalidPayload(e.0))?;
    let messages = Message::from_request(payload).map_err(|e| HostError::InvalidPayload(e.0))?;

    let scheduler = ctx.services()?.scheduler.clone().ok_or_else(|| HostError::Message("sending messages is not available here".into()))?;
    scheduler.check_messaging(&ctx.database, kind).await.map_err(HostError::Message)?;

    let mut response = ctx
        .db
        .query("SELECT count() AS n FROM jobs WHERE plugin = $plugin AND kind = 'communication' AND date_created > time::now() - 1h GROUP ALL;")
        .bind(("plugin", ctx.plugin_name.clone()))
        .await?
        .check()?;
    let recent: Option<JsonValue> = response.take(0)?;
    let recent = recent.and_then(|row| row.get("n").and_then(JsonValue::as_i64)).unwrap_or(0);
    if recent + messages.len() as i64 > MAX_PER_PLUGIN_PER_HOUR {
        return Err(HostError::Message(format!(
            "this plugin has already queued {recent} messages in the last hour; the limit is {MAX_PER_PLUGIN_PER_HOUR}"
        )));
    }

    let defaults = scheduler.defaults(&ctx.database).await;
    let mut jobs = Vec::with_capacity(messages.len());
    for message in &messages {
        let mut job = NewJob::new("communication", message.queue());
        job.plugin = Some(ctx.plugin_name.clone());
        job.payload = Some(message.to_payload());
        job.max_attempts = defaults.max_attempts;
        job.backoff_secs = defaults.backoff_secs;
        job.enqueued_by = ctx.audit.actor.id().map(str::to_string).or_else(|| Some("anonymous".into()));
        job.request_id = Some(ctx.audit.request_id.clone());
        match queue::enqueue(&ctx.db, job).await {
            Ok(enqueued) => jobs.push(enqueued.id),
            Err(QueueError::Refused(message)) => return Err(HostError::InvalidPayload(message)),
            Err(QueueError::Db(error)) => return Err(HostError::Db(error)),
        }
    }
    scheduler.wake(&ctx.database);
    Ok(serde_json::json!({ "ok": true, "data": { "jobs": jobs } }))
}
