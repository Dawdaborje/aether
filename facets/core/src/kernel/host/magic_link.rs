//! `magic_link::*` kernel commands for plugins and host callers.

use serde_json::{Value as JsonValue, json};

use crate::magic_link::{self, CreateMagicLink, MagicLinkError};

use super::context::PluginHostContext;
use super::error::HostError;

fn map_error(error: MagicLinkError) -> HostError {
    match error {
        MagicLinkError::Db(error) => HostError::Db(error),
        MagicLinkError::Refused(message) => HostError::InvalidPayload(message),
        MagicLinkError::Invalid => {
            HostError::InvalidPayload("this magic link is invalid, expired, or already used".into())
        }
    }
}

fn text<'a>(payload: &'a JsonValue, key: &str) -> Option<&'a str> {
    payload
        .get(key)
        .and_then(JsonValue::as_str)
        .map(str::trim)
        .filter(|s| !s.is_empty())
}

/// Public base URL for links. Prefer payload `public_base`, else the host origin.
fn public_base(ctx: &PluginHostContext, payload: &JsonValue) -> String {
    if let Some(given) = text(payload, "public_base") {
        return given.to_string();
    }
    ctx.services
        .as_ref()
        .map(|services| {
            services
                .public_base
                .trim()
                .trim_end_matches('/')
                .to_string()
        })
        .filter(|base| !base.is_empty())
        .unwrap_or_else(|| "http://127.0.0.1:7890".into())
}

fn core_db(ctx: &PluginHostContext) -> Result<&crate::state::Db, HostError> {
    ctx.services()?
        .core
        .as_ref()
        .ok_or_else(|| HostError::Message("magic links need the core database".into()))
}

pub async fn create(ctx: &PluginHostContext, payload: &JsonValue) -> Result<JsonValue, HostError> {
    ctx.require_cap("magic_link::create")?;
    let purpose = text(payload, "purpose")
        .ok_or_else(|| HostError::InvalidPayload("magic_link::create needs `purpose`".into()))?;
    let plugin = Some(ctx.plugin_name.as_str());
    magic_link::purpose_allowed_for_plugin(purpose, plugin).map_err(map_error)?;
    let core = core_db(ctx)?;

    let created = magic_link::create(
        core,
        &ctx.namespace,
        CreateMagicLink {
            purpose: purpose.to_string(),
            user_id: text(payload, "user_id").map(str::to_string),
            email: text(payload, "email").map(str::to_string),
            payload: payload.get("payload").cloned(),
            plugin: Some(ctx.plugin_name.clone()),
            org_db: Some(ctx.database.clone()),
            path: text(payload, "path").map(str::to_string),
            ttl_secs: payload.get("ttl_secs").and_then(JsonValue::as_i64),
            single_use: payload
                .get("single_use")
                .and_then(JsonValue::as_bool)
                .unwrap_or(true),
            supersede: payload
                .get("supersede")
                .and_then(JsonValue::as_bool)
                .unwrap_or(true),
            created_by: ctx.audit.actor.id().map(str::to_string),
            public_base: public_base(ctx, payload),
        },
    )
    .await
    .map_err(map_error)?;

    Ok(json!({
        "ok": true,
        "data": {
            "id": created.id,
            "url": created.url,
            "token": created.token,
            "expires_at": created.expires_at,
            "purpose": created.purpose,
        }
    }))
}

pub async fn revoke(ctx: &PluginHostContext, payload: &JsonValue) -> Result<JsonValue, HostError> {
    ctx.require_cap("magic_link::revoke")?;
    let core = core_db(ctx)?;
    let n = magic_link::revoke(
        core,
        &ctx.namespace,
        text(payload, "id"),
        text(payload, "purpose"),
        text(payload, "user_id"),
    )
    .await
    .map_err(map_error)?;
    Ok(json!({ "ok": true, "data": { "revoked": n } }))
}

pub async fn inspect(ctx: &PluginHostContext, payload: &JsonValue) -> Result<JsonValue, HostError> {
    ctx.require_cap("magic_link::inspect")?;
    let token = text(payload, "token")
        .ok_or_else(|| HostError::InvalidPayload("magic_link::inspect needs `token`".into()))?;
    let core = core_db(ctx)?;
    let row = magic_link::inspect(core, &ctx.namespace, token)
        .await
        .map_err(map_error)?;
    Ok(json!({ "ok": true, "data": row }))
}

#[cfg(test)]
mod tests {
    use super::super::test_support::dummy_ctx;
    use super::*;

    #[tokio::test]
    async fn a_plugin_cannot_mint_an_account_invite() {
        let ctx = dummy_ctx(&["magic_link::create"]);
        let error = create(
            &ctx,
            &json!({ "purpose": "auth.account_invite", "email": "ada@acme.io" }),
        )
        .await
        .unwrap_err();
        let HostError::InvalidPayload(message) = error else {
            panic!("expected a refusal");
        };
        assert!(message.contains("auth."), "{message}");
    }
}
