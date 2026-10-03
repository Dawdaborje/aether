use serde_json::Value as JsonValue;

use super::context::PluginHostContext;
use super::db;
use super::error::HostError;

/// Dispatch a kernel host command for a plugin.
///
/// Every command path begins with a capability check inside the handler
/// (or here for stub commands).
pub async fn kernel_command(
    ctx: &PluginHostContext,
    command: &str,
    payload: JsonValue,
) -> Result<JsonValue, HostError> {
    match command {
        // Structured DB — no raw SurQL from the plugin.
        "db::get" => db::db_get(ctx, &payload).await,
        "db::find" | "db::query" => db::db_find(ctx, &payload).await,
        "db::create" => db::db_create(ctx, &payload).await,
        "db::update" => db::db_update(ctx, &payload).await,
        "db::delete" => db::db_delete(ctx, &payload).await,
        "db::mutate" => {
            // Generic mutate entry: expects `{ "op": "create"|"update"|"delete", ... }`
            let op = payload
                .get("op")
                .and_then(|v| v.as_str())
                .unwrap_or("create");
            match op {
                "create" => db::db_create(ctx, &payload).await,
                "update" => db::db_update(ctx, &payload).await,
                "delete" => db::db_delete(ctx, &payload).await,
                other => Err(HostError::InvalidPayload(format!(
                    "unknown db::mutate op `{other}`"
                ))),
            }
        }

        // Other host surfaces — capability-gated stubs until wired to facets.
        "cache::get" | "cache::set" | "cache::invalidate" | "cache::clear" => {
            ctx.require_cap(command)?;
            Err(HostError::NotImplemented(command.into()))
        }
        "storage::read" | "storage::write" | "storage::delete" | "storage::list" => {
            ctx.require_cap(command)?;
            Err(HostError::NotImplemented(command.into()))
        }
        "email::send" | "sms::send" => {
            ctx.require_cap(command)?;
            Err(HostError::NotImplemented(command.into()))
        }
        "events::emit" => {
            ctx.require_cap(command)?;
            emit_ui_event(ctx, &payload)
        }
        // Who is calling and where. Needs no capability: it only tells the plugin about
        // the request it is already serving.
        "context::get" => Ok(serde_json::json!({
            "ok": true,
            "plugin": ctx.plugin_name,
            "function": ctx.function,
            "organization": ctx.database,
            "request_id": ctx.audit.request_id,
            "actor": { "kind": ctx.audit.actor.kind(), "id": ctx.audit.actor.id() },
        })),
        "notify::send" => {
            ctx.require_cap(command)?;
            notify_send(ctx, &payload).await
        }
        "events::subscribe" => {
            ctx.require_cap(command)?;
            Err(HostError::NotImplemented(command.into()))
        }
        "http::request" => {
            ctx.require_cap(command)?;
            Err(HostError::NotImplemented(command.into()))
        }
        "plugins::call" => {
            ctx.require_cap(command)?;
            Err(HostError::NotImplemented(command.into()))
        }
        "bridge::call" => {
            ctx.require_cap("bridge::call")?;
            Err(HostError::NotImplemented(command.into()))
        }
        "scheduler::register" | "scheduler::cancel" => {
            ctx.require_cap(command)?;
            Err(HostError::NotImplemented(command.into()))
        }
        "db::transaction" => {
            ctx.require_cap("db::transaction")?;
            Err(HostError::NotImplemented(
                "db::transaction (batch structured ops)".into(),
            ))
        }
        other => Err(HostError::UnknownCommand(other.into())),
    }
}

/// `notify::send`: store a notification and push it to whoever is connected.
///
/// Payload: `{ "title", "body"?, "link"?, "level"?, "payload"?, "expires_in_secs"?,
/// "audience"? }`. The audience is `"members"` (the default), `"caller"` (whoever made
/// this request), `{ "actors": ["users:…", "visitors:…"] }`, or `"everyone"`, which also
/// needs the `notify::public` capability.
async fn notify_send(ctx: &PluginHostContext, payload: &JsonValue) -> Result<JsonValue, HostError> {
    use crate::notifications::{Audience, Level, NewNotification, NotifyError};

    let text = |key: &str| payload.get(key).and_then(JsonValue::as_str).map(str::to_string);
    let title = text("title")
        .ok_or_else(|| HostError::InvalidPayload("notify::send requires a title".into()))?;
    let level = match text("level") {
        None => Level::Info,
        Some(raw) => Level::parse(&raw)
            .ok_or_else(|| HostError::InvalidPayload(format!("unknown level `{raw}`")))?,
    };
    let audience = match payload.get("audience") {
        None => Audience::Members,
        Some(JsonValue::String(name)) => match name.as_str() {
            "members" => Audience::Members,
            "everyone" => {
                ctx.require_cap("notify::public")?;
                Audience::Everyone
            }
            "caller" => match ctx.audit.actor.id() {
                Some(actor) => Audience::Actors { actors: vec![actor.to_string()] },
                None => {
                    return Err(HostError::InvalidPayload(
                        "this call has no caller to notify".into(),
                    ));
                }
            },
            other => {
                return Err(HostError::InvalidPayload(format!("unknown audience `{other}`")));
            }
        },
        Some(other) => serde_json::from_value::<Audience>(serde_json::json!({
            "kind": "actors",
            "actors": other.get("actors").cloned().unwrap_or(JsonValue::Null),
        }))
        .map_err(|error| HostError::InvalidPayload(format!("invalid audience: {error}")))?,
    };

    let stored = crate::notifications::send(
        &ctx.db,
        &ctx.notifications,
        &ctx.database,
        NewNotification {
            source: ctx.plugin_name.clone(),
            level,
            title,
            body: text("body"),
            link: text("link"),
            payload: payload.get("payload").cloned().filter(|value| !value.is_null()),
            audience,
            expires_in_secs: payload.get("expires_in_secs").and_then(JsonValue::as_u64),
        },
    )
    .await
    .map_err(|error| match error {
        NotifyError::Invalid(message) => HostError::InvalidPayload(message),
        NotifyError::Database(error) => HostError::Db(error),
    })?;
    Ok(serde_json::json!({ "ok": true, "id": stored.id }))
}

fn emit_ui_event(ctx: &PluginHostContext, payload: &JsonValue) -> Result<JsonValue, HostError> {
    let event = payload
        .get("event")
        .and_then(JsonValue::as_str)
        .map(str::trim)
        .filter(|event| !event.is_empty() && event.len() <= 128)
        .ok_or_else(|| {
            HostError::InvalidPayload(
                "events::emit requires a non-empty event (max 128 bytes)".into(),
            )
        })?;
    let data = payload.get("payload").cloned().unwrap_or(JsonValue::Null);
    let serialized = serde_json::to_vec(&data)
        .map_err(|err| HostError::InvalidPayload(format!("invalid event payload: {err}")))?;
    if serialized.len() > 65_536 {
        return Err(HostError::InvalidPayload(
            "event payload exceeds 65536 bytes".into(),
        ));
    }

    ctx.notifications.publish(
        &ctx.database,
        crate::notifications::HubMessage::Event(std::sync::Arc::new(crate::notifications::UiEvent {
            plugin: ctx.plugin_name.clone(),
            event: event.to_string(),
            payload: data,
        })),
    );

    Ok(serde_json::json!({ "ok": true }))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::kernel::host::context::{ModelGrant, PluginHostContext};
    use std::collections::{HashMap, HashSet};
    use surrealdb::Surreal;
    use surrealdb::engine::remote::ws::Client;

    fn dummy_ctx(caps: &[&str]) -> PluginHostContext {
        let granted = caps
            .iter()
            .map(|s| (*s).to_string())
            .collect::<HashSet<_>>();
        let mut models = HashMap::new();
        models.insert(
            "partner".into(),
            ModelGrant {
                name: "partner".into(),
                table: "base_partner".into(),
                can_read: true,
                can_write: true,
            },
        );
        // Surreal::init is fine for constructing context; we only test cap denial paths.
        let db: Surreal<Client> = Surreal::init();
        PluginHostContext::new(
            "test",
            granted,
            models,
            std::sync::Arc::new(db),
            crate::kernel::DbScope::new("aether", "core"),
            crate::notifications::NotificationHub::default(),
            crate::kernel::CallInfo::new(
                crate::access::audit::AuditContext {
                    actor: crate::access::audit::Actor::Anonymous,
                    request_id: "test-request".into(),
                    ip: None,
                    user_agent: None,
                },
                "test_function",
            ),
        )
    }

    #[tokio::test]
    async fn denies_without_capability() {
        let ctx = dummy_ctx(&[]);
        let err = kernel_command(
            &ctx,
            "db::find",
            serde_json::json!({ "model": "partner", "filter": {} }),
        )
        .await
        .unwrap_err();
        assert!(matches!(err, HostError::Capability(_)));
    }

    #[tokio::test]
    async fn emits_events_to_the_plugin_database_scope() {
        let mut ctx = dummy_ctx(&["events::emit"]);
        ctx.database = "org_acme".into();
        let mut receiver = ctx.notifications.subscribe("org_acme");

        let result = kernel_command(
            &ctx,
            "events::emit",
            serde_json::json!({ "event": "toast", "payload": { "message": "Saved" } }),
        )
        .await;
        assert_eq!(result.unwrap(), serde_json::json!({ "ok": true }));

        let crate::notifications::HubMessage::Event(event) = receiver.recv().await.unwrap() else {
            panic!("expected a transient event");
        };
        assert_eq!(event.plugin, "test");
        assert_eq!(event.event, "toast");
        assert_eq!(event.payload["message"], "Saved");
    }

    /// The capability each command asks for is whatever it refuses a plugin without; every
    /// one of them must be in the catalog that manifests are checked against.
    #[tokio::test]
    async fn every_capability_a_command_checks_is_in_the_catalog() {
        let catalog = aether_security::capabilities::CapabilityCatalog::builtin().unwrap();
        let commands = [
            "db::get", "db::find", "db::query", "db::create", "db::update", "db::delete",
            "db::mutate", "db::transaction", "cache::get", "cache::set", "cache::invalidate",
            "cache::clear", "storage::read", "storage::write", "storage::delete",
            "storage::list", "email::send", "sms::send", "events::emit", "events::subscribe",
            "http::request", "plugins::call", "bridge::call", "scheduler::register",
            "scheduler::cancel", "notify::send",
        ];
        let ctx = dummy_ctx(&[]);
        for command in commands {
            let refused = kernel_command(&ctx, command, serde_json::json!({})).await.unwrap_err();
            let HostError::Capability(aether_security::capabilities::CapabilityError::Denied(key)) = refused
            else {
                panic!("{command} should be refused without a capability, got {refused:?}");
            };
            assert!(catalog.contains(&key), "`{key}` (needed by {command}) is not in capabilities/");
        }
        // The one that is only checked for some audiences.
        assert!(catalog.contains("notify::public"));
    }

    #[tokio::test]
    async fn context_tells_a_plugin_who_is_calling_without_any_capability() {
        let mut ctx = dummy_ctx(&[]);
        ctx.database = "org_acme".into();
        let context = kernel_command(&ctx, "context::get", serde_json::json!({})).await.unwrap();
        assert_eq!(context["plugin"], "test");
        assert_eq!(context["organization"], "org_acme");
        assert_eq!(context["actor"]["kind"], "anonymous");
        assert!(context["actor"]["id"].is_null());
    }

    #[tokio::test]
    async fn raw_surql_does_not_exist() {
        // Every plugin database access goes through the structured, model-checked
        // commands; there is no way to send a query of one's own, whatever the capabilities.
        let ctx = dummy_ctx(&["db::query", "db::mutate", "db::surql"]);
        let err = kernel_command(
            &ctx,
            "db::surql",
            serde_json::json!({ "query": "SELECT * FROM person" }),
        )
        .await
        .unwrap_err();
        assert!(matches!(err, HostError::UnknownCommand(_)));
    }

    #[tokio::test]
    async fn denies_unknown_model() {
        let ctx = dummy_ctx(&["db::query"]);
        let err = kernel_command(
            &ctx,
            "db::find",
            serde_json::json!({ "model": "secret", "filter": {} }),
        )
        .await
        .unwrap_err();
        // Will fail at model check before DB — or capability passed then model denied.
        // Without live DB, use_scoped_db may fail first if we get past model — model is checked first.
        assert!(matches!(err, HostError::ModelDenied(_)) || matches!(err, HostError::Db(_)));
    }
}
