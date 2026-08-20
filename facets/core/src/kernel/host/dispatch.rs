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
        // Raw SurQL — explicit high-privilege capability.
        "db::surql" => db::db_surql(ctx, &payload).await,

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
        "events::emit" | "events::subscribe" => {
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

#[cfg(test)]
mod tests {
    use super::*;
    use crate::kernel::host::context::{ModelGrant, PluginHostContext};
    use std::collections::{HashMap, HashSet};
    use surrealdb::Surreal;
    use surrealdb::engine::remote::ws::Client;

    fn dummy_ctx(caps: &[&str]) -> PluginHostContext {
        let granted = caps.iter().map(|s| (*s).to_string()).collect::<HashSet<_>>();
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
        PluginHostContext::new("test", granted, models, db, "aether", "core")
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
    async fn denies_surql_without_surql_cap() {
        let ctx = dummy_ctx(&["db::query"]);
        let err = kernel_command(
            &ctx,
            "db::surql",
            serde_json::json!({ "query": "SELECT * FROM person" }),
        )
        .await
        .unwrap_err();
        assert!(matches!(err, HostError::Capability(_)));
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
        assert!(
            matches!(err, HostError::ModelDenied(_))
                || matches!(err, HostError::Db(_))
        );
    }
}
