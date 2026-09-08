use std::collections::HashMap;

use include_dir::{Dir, include_dir};
use serde::Deserialize;
use sha2::{Digest, Sha256};
use surrealdb::{Surreal, engine::remote::ws::Client, types::SurrealValue};
use thiserror::Error;

static CORE_MIGRATIONS: Dir<'_> = include_dir!("$CARGO_MANIFEST_DIR/../../migrations/core");
static ORG_MIGRATIONS: Dir<'_> = include_dir!("$CARGO_MANIFEST_DIR/../../migrations/org");

#[derive(Debug, Error)]
pub enum MigrationError {
    #[error("surrealdb error: {0}")]
    Surreal(#[from] surrealdb::Error),

    #[error("migration file `{0}` is not valid UTF-8")]
    InvalidUtf8(String),

    #[error("migration `{version}` failed: {message}")]
    ApplyFailed { version: String, message: String },

    #[error("migration `{version}` has changed since it was applied")]
    ChecksumMismatch { version: String },
}

#[derive(Debug, Deserialize, SurrealValue)]
struct AppliedMigration {
    version: String,
    filename: String,
    checksum: Option<String>,
}

/// Apply all pending core migrations to `namespace` / `database`
/// (typically `aether` / `core`).
pub async fn migrate_core(
    db: &Surreal<Client>,
    namespace: &str,
    database: &str,
) -> Result<Vec<String>, MigrationError> {
    apply_migrations(db, namespace, database, &CORE_MIGRATIONS).await
}

/// Apply all pending org migrations to a tenant database
/// (e.g. `aether` / `org_acme`).
pub async fn migrate_org(
    db: &Surreal<Client>,
    namespace: &str,
    org_database: &str,
) -> Result<Vec<String>, MigrationError> {
    apply_migrations(db, namespace, org_database, &ORG_MIGRATIONS).await
}

/// Convenience: run core migrations against the default `aether` / `core` pair.
pub async fn run_migrations(db: &Surreal<Client>) -> Result<Vec<String>, MigrationError> {
    migrate_core(db, "aether", "core").await
}

async fn apply_migrations(
    db: &Surreal<Client>,
    namespace: &str,
    database: &str,
    dir: &Dir<'_>,
) -> Result<Vec<String>, MigrationError> {
    db.use_ns(namespace).await?;
    db.use_db(database).await?;

    ensure_tracking_table(db).await?;

    let applied = load_applied_versions(db).await?;
    let mut newly_applied = Vec::new();

    let mut files: Vec<_> = dir.files().collect();
    files.sort_by_key(|file| migration_number(file.path()));

    for file in files {
        let filename = file
            .path()
            .file_name()
            .and_then(|n| n.to_str())
            .unwrap_or("unknown.surql")
            .to_string();

        if !filename.ends_with(".surql") {
            continue;
        }

        let version = filename.trim_end_matches(".surql").to_string();

        let content = file
            .contents_utf8()
            .ok_or_else(|| MigrationError::InvalidUtf8(filename.clone()))?;
        let checksum = sha256_hex(content);

        if let Some(applied_migration) = applied.get(&version) {
            if applied_migration
                .checksum
                .as_deref()
                .is_some_and(|stored| stored != checksum)
            {
                return Err(MigrationError::ChecksumMismatch { version });
            }
            log::debug!("Skipping already-applied migration {version}");
            continue;
        }

        if content.trim().is_empty() {
            log::warn!("Migration {version} is empty — recording without SQL");
            record_migration(db, &version, &filename, "").await?;
            newly_applied.push(version);
            continue;
        }

        log::info!("Applying migration {version}…");

        let response = db.query(content).await?;
        if let Err(err) = response.check() {
            return Err(MigrationError::ApplyFailed {
                version: version.clone(),
                message: err.to_string(),
            });
        }

        record_migration(db, &version, &filename, &checksum).await?;
        newly_applied.push(version);
    }

    Ok(newly_applied)
}

async fn ensure_tracking_table(db: &Surreal<Client>) -> Result<(), MigrationError> {
    let sql = r#"
DEFINE TABLE IF NOT EXISTS schema_migrations;
DEFINE FIELD IF NOT EXISTS version ON TABLE schema_migrations TYPE string;
DEFINE FIELD IF NOT EXISTS filename ON TABLE schema_migrations TYPE string;
DEFINE FIELD IF NOT EXISTS applied_at ON TABLE schema_migrations TYPE datetime DEFAULT time::now();
DEFINE FIELD IF NOT EXISTS checksum ON TABLE schema_migrations TYPE option<string | NONE>;
DEFINE INDEX IF NOT EXISTS schema_migrations_version ON TABLE schema_migrations COLUMNS version UNIQUE;
DEFINE INDEX IF NOT EXISTS schema_migrations_filename ON TABLE schema_migrations COLUMNS filename UNIQUE;
"#;

    db.query(sql).await?.check()?;
    Ok(())
}

async fn load_applied_versions(
    db: &Surreal<Client>,
) -> Result<HashMap<String, AppliedMigration>, MigrationError> {
    let mut response = db
        .query("SELECT version, filename, checksum FROM schema_migrations;")
        .await?
        .check()?;

    let rows: Vec<AppliedMigration> = response.take(0)?;
    Ok(rows
        .into_iter()
        .map(|migration| (migration.version.clone(), migration))
        .collect())
}

async fn record_migration(
    db: &Surreal<Client>,
    version: &str,
    filename: &str,
    checksum: &str,
) -> Result<(), MigrationError> {
    let checksum_value = if checksum.is_empty() {
        None
    } else {
        Some(checksum.to_string())
    };

    db.query(
        r#"
            CREATE schema_migrations SET
                version = $version,
                filename = $filename,
                checksum = $checksum,
                applied_at = time::now();
            "#,
    )
    .bind(("version", version.to_string()))
    .bind(("filename", filename.to_string()))
    .bind(("checksum", checksum_value))
    .await?
    .check()?;
    Ok(())
}

fn sha256_hex(content: &str) -> String {
    let digest = Sha256::digest(content.as_bytes());
    digest.iter().map(|b| format!("{b:02x}")).collect()
}

fn migration_number(path: &std::path::Path) -> u64 {
    path.file_stem()
        .and_then(|name| name.to_str())
        .map(|name| {
            name.chars()
                .take_while(char::is_ascii_digit)
                .collect::<String>()
        })
        .and_then(|prefix| prefix.parse().ok())
        .unwrap_or(u64::MAX)
}
