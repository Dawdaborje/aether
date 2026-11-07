use std::{path::PathBuf, sync::Arc};

use aether_storage::MediaBackend;
use thiserror::Error;

use super::cache::Cache;
use surrealdb::{Surreal, engine::remote::ws::Client as SurrealClient};

use crate::access::{ip::IpPolicy, ratelimit::RateLimiter};
use crate::config_manager::models::AetherConfig;
use crate::media::{MediaError, build_media, spawn_drain};
use crate::notifications::NotificationHub;
use crate::plugin_manager::runtime::PluginRuntime;
use crate::secrets::{SecretBox, SecretError};

#[derive(Debug, Error)]
pub enum AppStateError {
    #[error("failed to initialize application cache: {0}")]
    Cache(#[from] super::cache::CacheError),

    #[error("failed to initialize media storage: {0}")]
    Media(#[from] MediaError),

    #[error("failed to initialize the plugin runtime: {0}")]
    PluginRuntime(#[from] crate::plugin_manager::runtime::PluginRuntimeError),
}

pub use crate::databases::{Databases, Db};

impl axum::extract::FromRef<AppState> for Databases {
    fn from_ref(state: &AppState) -> Self {
        state.databases.clone()
    }
}

/// Who may call, and how often: the address policy and the limits on callers who are not
/// logged-in users. A sub-state of [`AppState`].
#[derive(Clone)]
pub struct Limits {
    pub ip_policy: IpPolicy,
    /// Caps new visitor identities per client address.
    pub visitor_limiter: RateLimiter,
    /// Caps requests from callers who are not logged-in users, per client address.
    pub request_limiter: RateLimiter,
    /// Caps sign-in attempts per client address (password, magic link, OAuth).
    pub login_limiter: RateLimiter,
    /// Blocks a username that has collected too many failed passwords, from any address.
    pub failed_login_limiter: crate::access::ratelimit::FailureLimiter,
}

/// File storage. A sub-state of [`AppState`].
#[derive(Clone)]
pub struct Media {
    backend: Arc<dyn MediaBackend>,
}

impl Media {
    /// The whole backend, for code that provisions storage across organizations.
    pub fn backend(&self) -> Arc<dyn MediaBackend> {
        self.backend.clone()
    }

    /// The backend as one organization sees it: every key lives under
    /// `orgs/<organization>/`, so it cannot reach another organization's files.
    pub fn org(
        &self,
        organization: &str,
    ) -> Result<Arc<dyn MediaBackend>, aether_storage::StorageError> {
        Ok(Arc::new(aether_storage::PrefixedBackend::for_organization(
            self.backend.clone(),
            organization,
        )?))
    }
}

/// Encrypts and decrypts secret settings. A sub-state of [`AppState`]; the key is found the
/// first time it is needed.
#[derive(Clone)]
pub struct Secrets {
    configured_key: Option<String>,
    app_dir: PathBuf,
    loaded: Arc<std::sync::OnceLock<Result<SecretBox, String>>>,
}

impl Secrets {
    /// The box that encrypts and decrypts secret settings. Fails (with the reason) when no key
    /// can be found or created; everything else keeps working.
    pub fn open(&self) -> Result<&SecretBox, SecretError> {
        let loaded = self.loaded.get_or_init(|| {
            SecretBox::load(self.configured_key.as_deref(), &self.app_dir).map_err(|e| e.to_string())
        });
        match loaded {
            Ok(secrets) => Ok(secrets),
            Err(reason) => Err(SecretError::KeyUnavailable(reason.clone())),
        }
    }
}

/// Each part of the state a handler may ask for on its own, by its type:
/// `State<Databases>`, `State<Cache>`, `State<Limits>`, and so on.
macro_rules! sub_state {
    ($($ty:ty => $field:ident),* $(,)?) => {$(
        impl axum::extract::FromRef<AppState> for $ty {
            fn from_ref(state: &AppState) -> Self {
                state.$field.clone()
            }
        }
    )*};
}

sub_state! {
    Cache => cache,
    Arc<AetherConfig> => config,
    NotificationHub => notifications,
    PluginRuntime => plugin_runtime,
    crate::scheduler::SchedulerLink => scheduler,
    Media => media,
    Limits => limits,
    Secrets => secret_store,
}

/// Shared kernel state for HTTP handlers (core, auth, settings, …).
#[derive(Clone)]
pub struct AppState {
    databases: Databases,
    pub config: Arc<AetherConfig>,
    /// Absolute path of the `aether.toml` this process was started with, when known.
    /// Used to re-read `plugin_paths` for Desk Apps catalog reload.
    pub config_path: Option<PathBuf>,
    pub cache: Cache,
    pub notifications: NotificationHub,
    pub plugin_runtime: PluginRuntime,
    pub media: Media,
    pub limits: Limits,
    pub namespace: String,
    pub core_database: String,
    /// Encrypts and decrypts secret settings.
    secret_store: Secrets,
    /// How this process reaches the job scheduler.
    pub scheduler: crate::scheduler::SchedulerLink,
}

impl AppState {
    pub async fn new(
        db: Surreal<SurrealClient>,
        config: AetherConfig,
        namespace: impl Into<String>,
        core_database: impl Into<String>,
        config_path: Option<PathBuf>,
    ) -> Result<Self, AppStateError> {
        let namespace = namespace.into();
        let cache = config.build_cache()?;
        // Tier A: capability catalog is compile-time; mark it warm at boot.
        let _ = cache.warm_set_cap_catalog_present();
        if let Err(error) = aether_security::capabilities::CapabilityCatalog::builtin() {
            log::warn!("capability catalog warm load: {error}");
        }
        let built = build_media(&config).await?;
        let media = Media { backend: built.backend };
        let limits = Limits {
            ip_policy: IpPolicy::from_config(&config.audit),
            visitor_limiter: RateLimiter::new(config.public.max_new_visitors_per_ip_per_minute),
            request_limiter: RateLimiter::new(config.public.max_requests_per_ip_per_minute),
            login_limiter: RateLimiter::new(config.public.max_login_attempts_per_ip_per_minute),
            failed_login_limiter: crate::access::ratelimit::FailureLimiter::new(
                config.public.max_failed_logins_per_user_per_minute,
            ),
        };
        let secret_store = Secrets {
            configured_key: config.security.secret_key.clone(),
            app_dir: config.app_dir.clone(),
            loaded: Arc::default(),
        };
        let plugin_runtime =
            PluginRuntime::new(config.app_dir.clone(), config.plugin_runtime.clone())?;
        let core_database = core_database.into();
        let databases = Databases::new(db, namespace.clone(), core_database.clone());
        built.journal.attach(databases.clone());
        if let Some(fallback) = built.fallback {
            spawn_drain(fallback, config.media.fallback.retry_seconds);
        }
        let heat = cache.heat();
        tokio::spawn(async move {
            let mut ticker = tokio::time::interval(crate::cache::HeatLedger::flush_interval());
            loop {
                ticker.tick().await;
                if let Err(error) = heat.flush_dirty() {
                    log::debug!("mantle heat flush: {error}");
                }
            }
        });
        let state = Self {
            databases,
            config: Arc::new(config),
            config_path,
            cache,
            notifications: NotificationHub::default(),
            plugin_runtime,
            media,
            limits,
            namespace,
            core_database,
            secret_store,
            scheduler: crate::scheduler::SchedulerLink::default(),
        };
        // Tier A: org registry for identify(); CLI create still refreshes on miss.
        crate::access::org_databases::warm_at_boot(&state).await;
        // CLI `--load-plugins` / `--upgrade-plugin` write signals; LIVE SELECT applies them.
        let _ = crate::plugin_manager::mantle_signals::spawn_listener(state.databases().clone(), state.cache.clone());
        // Role, role-holder and installed-plugin writes bust the role caches, whoever makes them.
        let _ = crate::plugin_manager::mantle_signals::spawn_role_watcher(state.databases().clone(), state.cache.clone());
        Ok(state)
    }

    /// The box that encrypts and decrypts secret settings. Fails (with the reason) when no key
    /// can be found or created; everything else keeps working.
    pub fn secrets(&self) -> Result<&SecretBox, SecretError> {
        self.secret_store.open()
    }

    /// The media backend as one organization sees it. See [`Media::org`].
    pub fn org_media(
        &self,
        organization: &str,
    ) -> Result<Arc<dyn MediaBackend>, aether_storage::StorageError> {
        self.media.org(organization)
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
        self.databases.fresh_session()
    }

    /// The core database's session.
    pub async fn core(&self) -> Result<Db, surrealdb::Error> {
        self.databases.core().await
    }

    /// An organization database's session.
    pub async fn org(&self, org_db: &str) -> Result<Db, surrealdb::Error> {
        self.databases.org(org_db).await
    }

    /// The database access alone, for helpers that need nothing else.
    pub fn databases(&self) -> &Databases {
        &self.databases
    }
}
