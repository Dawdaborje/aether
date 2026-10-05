//! Shared helpers for tests that need a plugin host context without a database.

use std::collections::{HashMap, HashSet};

use surrealdb::Surreal;
use surrealdb::engine::remote::ws::Client;

use super::context::{ModelGrant, PluginHostContext};

pub fn dummy_ctx(caps: &[&str]) -> PluginHostContext {
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
            schema: None,
            rules: None,
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

pub fn with_services(mut ctx: PluginHostContext, media: std::sync::Arc<dyn aether_storage::MediaBackend>) -> PluginHostContext {
    let built = crate::cache::Cache::from_config(&crate::cache::CacheConfig {
        backend: crate::cache::CacheBackendKind::Moka,
        default_ttl_secs: None,
        max_entries: 100,
        max_value_bytes: 1024,
        redis: None,
    });
    // Without a cache the context has no services, and the tests that need them fail saying so.
    match built {
        Ok(cache) => ctx.services = Some(crate::kernel::HostServices { cache, media, scheduler: None, files_root: None, bridges: None }),
        Err(error) => log::error!("test cache could not be built: {error}"),
    }
    ctx
}

pub fn in_memory_media() -> std::sync::Arc<dyn aether_storage::MediaBackend> {
    std::sync::Arc::new(aether_storage::ObjectStoreBackend::new(object_store::memory::InMemory::new()))
}

