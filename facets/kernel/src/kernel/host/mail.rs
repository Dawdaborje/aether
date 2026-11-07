//! `mail::thread`: the token that ties mail to one record (see `docs/architecture/inbound_mail.md`).
//!
//! A plugin that sends mail about a record puts the address this answers in `Reply-To`; a reply then comes back
//! to the kernel, which finds the record by the token and appends the reply to its chatter.
//!
//! | Command | Does |
//! |---|---|
//! | `mail::thread { model, record, alias? }` | `{ token, reply_to? }`: the record's token, made on first use; `reply_to` when `alias` and the organization's `mail.inbound.domain` are set |
//!
//! Needs `mail::thread` in `capabilities`, and read access to the record (the same checks as `db::get`).

use serde::Deserialize;
use serde_json::{Value as JsonValue, json};

use super::context::PluginHostContext;
use super::db::{db_get, parse_req, require_model};
use super::error::HostError;
use crate::mail::thread;

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct ThreadRequest {
    model: String,
    record: String,
    #[serde(default)]
    alias: Option<String>,
}

pub async fn mail_thread(ctx: &PluginHostContext, payload: &JsonValue) -> Result<JsonValue, HostError> {
    ctx.require_cap("mail::thread")?;
    let req: ThreadRequest = parse_req(payload)?;
    let grant = require_model(ctx, &req.model, false)?;
    // The id as the chatter keeps it: without the table.
    let key = req.record.rsplit(':').next().unwrap_or_default().to_string();
    if key.is_empty() {
        return Err(HostError::InvalidPayload("`record` is the id of a record".into()));
    }
    // Whoever may not read the record gets the same answer as for one that does not exist.
    let found = db_get(ctx, &json!({ "model": req.model, "id": req.record })).await?;
    if found.is_null() || found.get("data").is_some_and(JsonValue::is_null) {
        return Err(HostError::Message("that record does not exist".into()));
    }
    let token = thread::ensure(&ctx.db, &ctx.plugin_name, &grant.table, &key).await?;
    let mut answer = json!({ "token": token });
    if let Some(alias) = req.alias.filter(|a| !a.is_empty()) {
        if !alias.chars().all(|c| c.is_ascii_lowercase() || c.is_ascii_digit() || c == '_' || c == '-' || c == '.') {
            return Err(HostError::InvalidPayload("`alias` is lower-case letters, digits, `.`, `_` and `-`".into()));
        }
        let settings = super::settings_read::effective_settings(ctx, &["mail.inbound.domain"]).await;
        if let Some(domain) = settings.get("mail.inbound.domain").and_then(JsonValue::as_str).map(str::trim).filter(|d| !d.is_empty()) {
            answer["reply_to"] = json!(thread::reply_address(&alias, &token, &domain.to_ascii_lowercase()));
        }
    }
    Ok(answer)
}
