use serde_json::Value as JsonValue;

use super::context::PluginHostContext;
use super::db;
use super::error::HostError;
use super::{bridge_call, communication, files, graph, http, plugin_call, scheduling, storage, store};

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
        "db::relate" => graph::db_relate(ctx, &payload).await,
        "db::unrelate" => graph::db_unrelate(ctx, &payload).await,
        "db::related" => graph::db_related(ctx, &payload).await,
        "db::transitions" => super::transitions::db_transitions(ctx, &payload).await,
        "db::tree" => graph::db_tree(ctx, &payload).await,
        "db::count" => db::db_count(ctx, &payload).await,
        "db::aggregate" => db::db_aggregate(ctx, &payload).await,
        "db::create" => db::db_create(ctx, &payload).await,
        "db::update" => db::db_update(ctx, &payload).await,
        "db::delete" => db::db_delete(ctx, &payload).await,
        "db::increment" => db::db_increment(ctx, &payload).await,
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
        "cache::get" => store::cache_get(ctx, &payload),
        "cache::set" => store::cache_set(ctx, &payload),
        "cache::invalidate" => store::cache_invalidate(ctx, &payload),
        "cache::clear" => store::cache_clear(ctx, &payload),
        "fs::read" => files::read(ctx, &payload).await,
        "fs::write" => files::write(ctx, &payload).await,
        "fs::list" => files::list(ctx, &payload).await,
        "fs::stat" => files::stat(ctx, &payload).await,
        "fs::rename" => files::rename(ctx, &payload).await,
        "fs::delete" => files::delete(ctx, &payload).await,
        "storage::read" => storage::storage_read(ctx, &payload).await,
        "storage::write" => storage::storage_write(ctx, &payload).await,
        "storage::delete" => storage::storage_delete(ctx, &payload).await,
        "storage::list" => storage::storage_list(ctx, &payload).await,
        "communication::send" => communication::send(ctx, &payload).await,
        "events::emit" => {
            ctx.require_cap(command)?;
            let answer = emit_ui_event(ctx, &payload)?;
            // The browsers have been told; now the plugins that listen. A failure here does not
            // undo the emit.
            if let Some(event) = payload.get("event").and_then(|event| event.as_str()) {
                let data = payload.get("payload").cloned().unwrap_or(JsonValue::Null);
                if let Err(error) = crate::plugin_events::dispatch(ctx, event, &data).await {
                    log::warn!("{}: event `{event}` could not be passed to plugins: {error}", ctx.database);
                }
            }
            Ok(answer)
        }
        // Who is calling and where. Needs no capability: it only tells the plugin about
        // the request it is already serving.
        "context::get" => {
            // The roles an administrator gave this person. Visitors and anonymous callers have none.
            let roles = match ctx.audit.actor.id() {
                Some(user) if ctx.audit.actor.kind() == "user" => {
                    crate::roles::roles_of(&ctx.db, user).await.map_err(HostError::Db)?
                }
                _ => Vec::new(),
            };
            Ok(serde_json::json!({
            "ok": true,
            "roles": roles,
            "now": surrealdb::types::Datetime::now().to_string(),
            "plugin": ctx.plugin_name,
            "function": ctx.function,
            "organization": ctx.database,
            "request_id": ctx.audit.request_id,
            "actor": { "kind": ctx.audit.actor.kind(), "id": ctx.audit.actor.id() },
            }))
        }
        "notify::send" => {
            ctx.require_cap(command)?;
            notify_send(ctx, &payload).await
        }
        "events::subscribe" => {
            ctx.require_cap(command)?;
            let event = payload.get("event").and_then(|v| v.as_str()).unwrap_or("");
            let function = payload.get("function").and_then(|v| v.as_str()).unwrap_or("");
            crate::plugin_events::subscribe(ctx, event, function).await.map_err(event_error)?;
            Ok(serde_json::json!({ "ok": true, "data": null }))
        }
        "events::unsubscribe" => {
            ctx.require_cap("events::subscribe")?;
            let event = payload.get("event").and_then(|v| v.as_str()).unwrap_or("");
            let removed = crate::plugin_events::unsubscribe(ctx, event).await.map_err(event_error)?;
            Ok(serde_json::json!({ "ok": true, "data": { "removed": removed } }))
        }
        "http::request" => http::http_request(ctx, &payload).await,
        "plugins::call" => plugin_call::plugins_call(ctx, &payload).await,
        "bridge::call" => bridge_call::bridge_call(ctx, &payload).await,
        "scheduler::enqueue" => scheduling::enqueue(ctx, &payload).await,
        "scheduler::job" => scheduling::job(ctx, &payload).await,
        "scheduler::cancel_job" => scheduling::cancel_job(ctx, &payload).await,
        "scheduler::register" => scheduling::register(ctx, &payload).await,
        "scheduler::cancel" => scheduling::cancel(ctx, &payload).await,
        "db::transaction" => db::db_transaction(ctx, &payload).await,
        other => Err(HostError::UnknownCommand(other.into())),
    }
}

fn event_error(error: crate::plugin_events::EventError) -> HostError {
    match error {
        crate::plugin_events::EventError::Db(error) => HostError::Db(error),
        crate::plugin_events::EventError::Refused(message) => HostError::InvalidPayload(message),
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
    use crate::kernel::host::test_support::{dummy_ctx, in_memory_media, with_services};
    use crate::kernel::host::context::{ModelGrant, PluginHostContext};
    use std::collections::{HashMap, HashSet};
    use surrealdb::Surreal;
    use surrealdb::engine::remote::ws::Client;

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
            "db::get", "db::find", "db::query", "db::count", "db::aggregate", "db::relate", "db::unrelate", "db::related", "db::transitions", "db::tree", "db::create", "db::update", "db::delete", "db::increment",
            "db::mutate", "db::transaction", "cache::get", "cache::set", "cache::invalidate",
            "cache::clear", "storage::read", "storage::write", "storage::delete",
            "storage::list", "fs::read", "fs::write", "fs::list", "fs::stat", "fs::rename", "fs::delete", "communication::send", "events::emit", "events::subscribe", "events::unsubscribe",
            "http::request", "plugins::call", "bridge::call", "scheduler::register",
            "scheduler::cancel", "scheduler::enqueue", "scheduler::job", "scheduler::cancel_job", "notify::send",
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

    #[tokio::test]
    async fn cache_round_trips_json_and_is_private_to_each_plugin() {
        let caps = ["cache::get", "cache::set", "cache::invalidate", "cache::clear"];
        let first = with_services(dummy_ctx(&caps), in_memory_media());
        let mut second = first.clone();
        second.plugin_name = "other".into();

        let value = serde_json::json!({ "n": 1, "list": [1, 2] });
        kernel_command(&first, "cache::set", serde_json::json!({ "key": "k", "value": value })).await.unwrap();
        let got = kernel_command(&first, "cache::get", serde_json::json!({ "key": "k" })).await.unwrap();
        assert_eq!(got["data"], value);
        let other = kernel_command(&second, "cache::get", serde_json::json!({ "key": "k" })).await.unwrap();
        assert!(other["data"].is_null(), "another plugin must not see the entry");

        kernel_command(&second, "cache::clear", serde_json::json!({})).await.unwrap();
        let still = kernel_command(&first, "cache::get", serde_json::json!({ "key": "k" })).await.unwrap();
        assert_eq!(still["data"], value, "clearing is per plugin");
        let gone = kernel_command(&first, "cache::invalidate", serde_json::json!({ "key": "k" })).await.unwrap();
        assert_eq!(gone["data"]["removed"], 1);
    }

    #[tokio::test]
    async fn cache_refuses_bad_requests() {
        let ctx = with_services(dummy_ctx(&["cache::get", "cache::set"]), in_memory_media());
        let zero = kernel_command(&ctx, "cache::set", serde_json::json!({ "key": "k", "value": 1, "ttl_secs": 0 })).await;
        assert!(matches!(zero, Err(HostError::InvalidPayload(_))));
        let empty = kernel_command(&ctx, "cache::get", serde_json::json!({ "key": "" })).await;
        assert!(matches!(empty, Err(HostError::InvalidPayload(_))));
        let big = "x".repeat(2000);
        let too_big = kernel_command(&ctx, "cache::set", serde_json::json!({ "key": "k", "value": big })).await;
        assert!(matches!(too_big, Err(HostError::Message(_))));
    }

    #[tokio::test]
    async fn storage_keeps_each_plugin_in_its_own_folder() {
        let caps = ["storage::read", "storage::write", "storage::delete", "storage::list"];
        let media = in_memory_media();
        let first = with_services(dummy_ctx(&caps), media.clone());
        let mut second = first.clone();
        second.plugin_name = "other".into();

        kernel_command(&first, "storage::write", serde_json::json!({ "key": "a/b.txt", "text": "hello" })).await.unwrap();
        let read = kernel_command(&first, "storage::read", serde_json::json!({ "key": "a/b.txt" })).await.unwrap();
        assert_eq!(read["data"]["text"], "hello");
        // Stored under the plugin's folder, invisible to another plugin.
        assert!(media.exists(&aether_storage::MediaKey::parse("plugins/test/a/b.txt").unwrap()).await.unwrap());
        let hidden = kernel_command(&second, "storage::read", serde_json::json!({ "key": "a/b.txt" })).await.unwrap();
        assert!(hidden["data"].is_null(), "another plugin's file is simply not there");
        let theirs = kernel_command(&second, "storage::list", serde_json::json!({})).await.unwrap();
        assert_eq!(theirs["data"], serde_json::json!([]));
        let mine = kernel_command(&first, "storage::list", serde_json::json!({})).await.unwrap();
        assert_eq!(mine["data"], serde_json::json!([{ "key": "a/b.txt", "size": 5 }]));

        for bad in ["../other/x", "a/../../x", "/x", ""] {
            let refused = kernel_command(&first, "storage::write", serde_json::json!({ "key": bad, "text": "x" })).await;
            assert!(matches!(refused, Err(HostError::InvalidPayload(_))), "{bad}: {refused:?}");
        }

        kernel_command(&first, "storage::write", serde_json::json!({ "key": "bin", "base64": "AAEC" })).await.unwrap();
        let bin = kernel_command(&first, "storage::read", serde_json::json!({ "key": "bin", "encoding": "base64" })).await.unwrap();
        assert_eq!(bin["data"]["base64"], "AAEC");
        kernel_command(&first, "storage::delete", serde_json::json!({ "key": "bin" })).await.unwrap();
        let gone = kernel_command(&first, "storage::read", serde_json::json!({ "key": "bin" })).await.unwrap();
        assert!(gone["data"].is_null());
    }

    #[tokio::test]
    async fn storage_and_cache_say_so_when_the_call_has_no_services() {
        let ctx = dummy_ctx(&["storage::list", "cache::get"]);
        assert!(matches!(kernel_command(&ctx, "storage::list", serde_json::json!({})).await, Err(HostError::Message(_))));
        assert!(matches!(kernel_command(&ctx, "cache::get", serde_json::json!({ "key": "k" })).await, Err(HostError::Message(_))));
    }
}
