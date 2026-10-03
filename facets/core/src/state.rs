use std::{collections::HashMap, sync::Arc};

use aether_storage::MediaBackend;
use thiserror::Error;
use tokio::sync::RwLock;

use super::cache::Cache;
use surrealdb::{Surreal, engine::remote::ws::Client as SurrealClient};

use crate::access::{ip::IpPolicy, ratelimit::RateLimiter};
use crate::config_manager::models::AetherConfig;
use crate::media::{MediaError, build_media_backend};
use crate::plugin_manager::runtime::PluginRuntime;
use crate::notifications::NotificationHub;

#[derive(Debug, Error)]
pub enum AppStateError {
    #[error("failed to initialize application cache: {0}")]
    Cache(#[from] super::cache::CacheError),

    #[error("failed to initialize media storage: {0}")]
    Media(#[from] MediaError),

    #[error("failed to initialize the plugin runtime: {0}")]
    PluginRuntime(#[from] crate::plugin_manager::runtime::PluginRuntimeError),
}

/// A long-lived database session already pointed at one database.
///
/// Cloning a `Surreal` handle opens a new session, whose first operation costs about
/// 25 ms. Sessions are therefore made once per database and shared by reference; they
/// are never switched to another database, so concurrent requests cannot interfere.
pub type Db = Arc<Surreal<SurrealClient>>;

/// One session per database, created on first use.
#[derive(Clone)]
struct SessionPool {
    /// Shared by reference: cloning the `Surreal` handle itself opens a session.
    connection: Arc<Surreal<SurrealClient>>,
    namespace: String,
    sessions: Arc<RwLock<HashMap<String, Db>>>,
}

impl SessionPool {
    async fn session(&self, database: &str) -> Result<Db, surrealdb::Error> {
        if let Some(session) = self.sessions.read().await.get(database) {
            return Ok(session.clone());
        }
        let mut sessions = self.sessions.write().await;
        if let Some(session) = sessions.get(database) {
            return Ok(session.clone());
        }
        let session = Surreal::clone(&self.connection);
        session.use_ns(&self.namespace).use_db(database).await?;
        // Pay the session's first-operation cost now, not in a request.
        session.query("RETURN 1;").await?.check()?;
        let session = Arc::new(session);
        sessions.insert(database.to_string(), session.clone());
        Ok(session)
    }
}

/// Shared kernel state for HTTP handlers (core, auth, settings, …).
#[derive(Clone)]
pub struct AppState {
    pool: SessionPool,
    pub config: Arc<AetherConfig>,
    pub cache: Cache,
    pub notifications: NotificationHub,
    pub plugin_runtime: PluginRuntime,
    pub media: Arc<dyn MediaBackend>,
    pub ip_policy: IpPolicy,
    /// Caps new visitor identities per client address.
    pub visitor_limiter: RateLimiter,
    /// Caps requests from callers who are not logged-in users, per client address.
    pub request_limiter: RateLimiter,
    pub namespace: String,
    pub core_database: String,
}

impl AppState {
    pub async fn new(
        db: Surreal<SurrealClient>,
        config: AetherConfig,
        namespace: impl Into<String>,
        core_database: impl Into<String>,
    ) -> Result<Self, AppStateError> {
        let namespace = namespace.into();
        let cache = config.build_cache()?;
        let media = build_media_backend(&config.media).await?;
        let ip_policy = IpPolicy::from_config(&config.audit);
        let visitor_limiter = RateLimiter::new(config.public.max_new_visitors_per_ip_per_minute);
        let request_limiter = RateLimiter::new(config.public.max_requests_per_ip_per_minute);
        let plugin_runtime =
            PluginRuntime::new(config.app_dir.clone(), config.plugin_runtime.clone())?;
        let pool = SessionPool {
            connection: Arc::new(db),
            namespace: namespace.clone(),
            sessions: Arc::default(),
        };
        Ok(Self {
            pool,
            config: Arc::new(config),
            cache,
            notifications: NotificationHub::default(),
            plugin_runtime,
            media,
            ip_policy,
            visitor_limiter,
            request_limiter,
            namespace,
            core_database: core_database.into(),
        })
    }

    /// The media backend as one organization sees it: every key lives under
    /// `orgs/<organization>/`, so it cannot reach another organization's files.
    pub fn org_media(&self, organization: &str) -> Result<Arc<dyn MediaBackend>, aether_storage::StorageError> {
        Ok(Arc::new(aether_storage::PrefixedBackend::for_organization(
            self.media.clone(),
            organization,
        )?))
    }

    /// Send a notification in an organization: store it, then wake whoever is connected.
    pub async fn notify(
        &self,
        org_database: &str,
        new: crate::notifications::NewNotification,
    ) -> Result<crate::notifications::Notification, crate::notifications::NotifyError> {
        let db = self.org(org_database).await?;
        crate::notifications::send(&db, &self.notifications, org_database, new).await
    }

    /// A new session that may be switched between databases freely. It costs about 25 ms
    /// to first use, so it is for background tasks and for helpers that select the
    /// database themselves, never for request paths that can use [`Self::core`] or
    /// [`Self::org`].
    pub fn fresh_session(&self) -> Surreal<SurrealClient> {
        Surreal::clone(&self.pool.connection)
    }

    /// The core database's session.
    pub async fn core(&self) -> Result<Db, surrealdb::Error> {
        self.pool.session(&self.core_database).await
    }

    /// An organization database's session.
    pub async fn org(&self, org_db: &str) -> Result<Db, surrealdb::Error> {
        self.pool.session(org_db).await
    }
}
