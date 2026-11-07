//! Builds the kernel's media backend from `[media]` in `aether.toml`.
//!
//! When the main backend is a remote one (S3), it is wrapped in a [`FallbackBackend`]: an upload
//! that cannot reach it is kept on local disk, written down in the core database (`media_pending`)
//! and moved by [`spawn_drain`] once the backend answers again.

use std::{
    sync::{Arc, OnceLock},
    time::Duration,
};

use aether_storage::{
    FallbackBackend, MediaBackend, PendingEntry, PendingJournal, StorageError,
};
use async_trait::async_trait;
use serde::Deserialize;
use surrealdb::types::SurrealValue;
use thiserror::Error;

use crate::{
    config_manager::models::{AetherConfig, MediaBackendKind, MediaConfig},
    databases::Databases,
};

#[derive(Debug, Error)]
pub enum MediaError {
    #[error("media backend `{0}` is selected but its settings are missing")]
    MissingSettings(&'static str),

    #[error(transparent)]
    Storage(#[from] StorageError),
}

/// The main backend alone, as `[media]` names it.
pub async fn build_media_backend(
    config: &MediaConfig,
) -> Result<Arc<dyn MediaBackend>, MediaError> {
    match config.backend {
        MediaBackendKind::Local => {
            let local = config
                .local
                .as_ref()
                .ok_or(MediaError::MissingSettings("local"))?;
            Ok(Arc::new(local_storage::build(local).await?))
        }
        MediaBackendKind::S3 => {
            let s3 = config
                .s3
                .as_ref()
                .ok_or(MediaError::MissingSettings("s3"))?;
            Ok(Arc::new(s3_storage::build(s3)?))
        }
    }
}

/// The pending files, kept in the core database. The database is not open yet when the backend is
/// built, so it is handed over afterwards with [`DbJournal::attach`].
#[derive(Default)]
pub struct DbJournal {
    databases: OnceLock<Databases>,
}

impl DbJournal {
    pub fn attach(&self, databases: Databases) {
        let _ = self.databases.set(databases);
    }

    async fn core(&self) -> Result<crate::databases::Db, StorageError> {
        let databases = self
            .databases
            .get()
            .ok_or_else(|| StorageError::Config("the database is not open yet".into()))?;
        databases
            .core()
            .await
            .map_err(|error| StorageError::Backend(Box::new(error)))
    }
}

fn db_error(error: surrealdb::Error) -> StorageError {
    StorageError::Backend(Box::new(error))
}

#[derive(Debug, Deserialize, SurrealValue)]
struct PendingRow {
    file_key: String,
    size: i64,
    attempts: i64,
}

#[async_trait]
impl PendingJournal for DbJournal {
    async fn record(&self, key: &str, size: u64, reason: &str) -> Result<(), StorageError> {
        let db = self.core().await?;
        db.query(
            "UPSERT type::record('media_pending', crypto::md5($key)) SET \
             file_key = $key, size = $size, last_error = $reason, date_updated = time::now();",
        )
        .bind(("key", key.to_string()))
        .bind(("size", size as i64))
        .bind(("reason", reason.to_string()))
        .await
        .map_err(db_error)?
        .check()
        .map_err(db_error)?;
        Ok(())
    }

    async fn forget(&self, key: &str) -> Result<(), StorageError> {
        let db = self.core().await?;
        db.query("DELETE type::record('media_pending', crypto::md5($key));")
            .bind(("key", key.to_string()))
            .await
            .map_err(db_error)?
            .check()
            .map_err(db_error)?;
        Ok(())
    }

    async fn pending(&self) -> Result<Vec<PendingEntry>, StorageError> {
        let db = self.core().await?;
        let mut response = db
            .query("SELECT file_key, size, attempts FROM media_pending ORDER BY date_created ASC LIMIT 200;")
            .await
            .map_err(db_error)?
            .check()
            .map_err(db_error)?;
        let rows: Vec<PendingRow> = response.take(0).map_err(db_error)?;
        Ok(rows
            .into_iter()
            .map(|row| PendingEntry {
                key: row.file_key,
                size: row.size.max(0) as u64,
                attempts: row.attempts.max(0) as u32,
            })
            .collect())
    }

    async fn failed(&self, key: &str, reason: &str) -> Result<(), StorageError> {
        let db = self.core().await?;
        db.query(
            "UPDATE type::record('media_pending', crypto::md5($key)) SET \
             attempts += 1, last_error = $reason, date_updated = time::now();",
        )
        .bind(("key", key.to_string()))
        .bind(("reason", reason.to_string()))
        .await
        .map_err(db_error)?
        .check()
        .map_err(db_error)?;
        Ok(())
    }
}

/// What [`build_media`] made.
pub struct BuiltMedia {
    /// What the kernel stores files through.
    pub backend: Arc<dyn MediaBackend>,
    /// Set when uploads can fall back to local disk.
    pub fallback: Option<Arc<FallbackBackend>>,
    pub journal: Arc<DbJournal>,
}

/// The backend the kernel uses: the main one, with the local fallback when `[media.fallback]` says so.
pub async fn build_media(config: &AetherConfig) -> Result<BuiltMedia, MediaError> {
    let primary = build_media_backend(&config.media).await?;
    let journal = Arc::new(DbJournal::default());
    if !config.media.fallback_enabled() {
        return Ok(BuiltMedia { backend: primary, fallback: None, journal });
    }
    let settings = &config.media.fallback;
    let base_path = settings
        .base_path
        .clone()
        .unwrap_or_else(|| config.app_dir.join("media_pending"));
    let local = local_storage::build(&local_storage::LocalStorageConfig { base_path }).await?;
    let fallback = Arc::new(FallbackBackend::new(
        primary,
        Arc::new(local),
        journal.clone(),
        Duration::from_secs(settings.write_timeout_seconds.max(1)),
    ));
    Ok(BuiltMedia { backend: fallback.clone(), fallback: Some(fallback), journal })
}

/// Move waiting files to the main backend every `retry_seconds`, for as long as the process runs.
pub fn spawn_drain(fallback: Arc<FallbackBackend>, retry_seconds: u64) {
    tokio::spawn(async move {
        let mut ticker = tokio::time::interval(Duration::from_secs(retry_seconds.max(5)));
        loop {
            ticker.tick().await;
            match fallback.drain().await {
                Ok(report) if report.uploaded > 0 => log::info!(
                    "media: moved {} waiting file(s) to the main storage, {} left",
                    report.uploaded,
                    report.remaining
                ),
                Ok(_) => {}
                Err(error) => log::debug!("media: moving waiting files: {error}"),
            }
        }
    });
}
