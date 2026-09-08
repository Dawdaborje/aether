use std::sync::Arc;

use super::cache::Cache;
use surrealdb::{Surreal, engine::remote::ws::Client as SurrealClient};

use crate::config_manager::models::AetherConfig;

/// Shared kernel state for HTTP handlers (core, auth, settings, …).
#[derive(Clone)]
pub struct AppState {
    pub db: Surreal<SurrealClient>,
    pub config: Arc<AetherConfig>,
    pub cache: Cache,
    pub namespace: String,
    pub core_database: String,
}

impl AppState {
    pub fn new(
        db: Surreal<SurrealClient>,
        config: AetherConfig,
        namespace: impl Into<String>,
        core_database: impl Into<String>,
    ) -> Result<Self, super::cache::CacheError> {
        let cache = config.build_cache()?;
        Ok(Self {
            db,
            config: Arc::new(config),
            cache,
            namespace: namespace.into(),
            core_database: core_database.into(),
        })
    }

    pub async fn use_core(&self) -> Result<(), surrealdb::Error> {
        self.db.use_ns(&self.namespace).await?;
        self.db.use_db(&self.core_database).await?;
        Ok(())
    }

    pub async fn use_org(&self, org_db: &str) -> Result<(), surrealdb::Error> {
        self.db.use_ns(&self.namespace).await?;
        self.db.use_db(org_db).await?;
        Ok(())
    }
}
