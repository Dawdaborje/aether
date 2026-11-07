//! The background work of the audit chain: sealing new rows soon after they are written, making
//! checkpoints, and checking the chains once a day (see [`super::chain`] and
//! `docs/architecture/audit_chain.md`).

use std::time::Duration;

use serde_json::{Value, json};

use super::chain::{AuditKey, Chain, ChainReport, SealOptions, seal, verify};
use crate::state::AppState;

/// How often the organizations are looked at for new rows.
const SEAL_INTERVAL: Duration = Duration::from_secs(5);
/// Every this many passes every chain is visited, whether or not it has new rows, so a checkpoint
/// that is due by time is made even on a quiet organization.
const FULL_PASS_EVERY: u32 = 60;
/// Wait this long after start-up before the first pass.
const FIRST_PASS_DELAY: Duration = Duration::from_secs(15);
/// Checking every chain end to end takes a while on a large trail, so it happens daily.
const VERIFY_INTERVAL: Duration = Duration::from_secs(24 * 60 * 60);
const FIRST_VERIFY_DELAY: Duration = Duration::from_secs(10 * 60);
/// Rows one pass seals per chain, at most, so one busy organization cannot hold up the others.
const MAX_BATCHES_PER_PASS: u32 = 20;

pub const ENABLED_SETTING: &str = "audit.chain";
pub const CHECKPOINT_ROWS_SETTING: &str = "audit.checkpoint_rows";
pub const CHECKPOINT_SECS_SETTING: &str = "audit.checkpoint_secs";

/// Where the installation's audit key lives.
pub fn key_path(config: &crate::config_manager::models::AetherConfig) -> std::path::PathBuf {
    crate::app_dir::AppDir::new(&config.app_dir)
        .conf_dir()
        .join("audit.key")
}

async fn setting(state: &AppState, org: &crate::tenancy::OrgRef, key: &str) -> Option<Value> {
    crate::application::settings::get_setting(state, key, Some(org))
        .await
        .ok()
        .flatten()
        .map(|setting| setting.value)
}

async fn enabled(state: &AppState, org: &crate::tenancy::OrgRef) -> bool {
    setting(state, org, ENABLED_SETTING).await != Some(Value::Bool(false))
}

async fn options(state: &AppState, org: &crate::tenancy::OrgRef) -> SealOptions {
    let defaults = SealOptions::default();
    SealOptions {
        batch: defaults.batch,
        checkpoint_rows: setting(state, org, CHECKPOINT_ROWS_SETTING)
            .await
            .and_then(|v| v.as_u64())
            .filter(|n| *n > 0)
            .unwrap_or(defaults.checkpoint_rows),
        checkpoint_secs: setting(state, org, CHECKPOINT_SECS_SETTING)
            .await
            .and_then(|v| v.as_u64())
            .filter(|n| *n > 0)
            .unwrap_or(defaults.checkpoint_secs),
    }
}

/// The chains of an organization that have rows waiting to be sealed (one cheap query).
pub async fn pending(db: &surrealdb::Surreal<surrealdb::engine::remote::ws::Client>) -> Result<Vec<Chain>, surrealdb::Error> {
    let mut response = db
        .query(
            "RETURN array::len(SELECT VALUE id FROM data_access WHERE chain_seq = NONE LIMIT 1); \
             RETURN array::len(SELECT VALUE id FROM plugin_calls WHERE chain_seq = NONE LIMIT 1); \
             RETURN array::len(SELECT VALUE id FROM page_visits WHERE chain_seq = NONE LIMIT 1);",
        )
        .await?
        .check()?;
    let mut waiting = Vec::new();
    for (index, chain) in Chain::ALL.into_iter().enumerate() {
        let count: Option<u64> = response.take(index)?;
        if count.unwrap_or(0) > 0 {
            waiting.push(chain);
        }
    }
    Ok(waiting)
}

/// One pass over every organization. `full`: visit every chain, not only those with new rows.
async fn pass(state: &AppState, key: &AuditKey, full: bool) -> Result<u64, surrealdb::Error> {
    let organizations = crate::access::org_databases::list_cached(state).await?;
    let mut sealed = 0;
    for db_name in organizations {
        let org = crate::chatter::org_ref(&db_name);
        if !enabled(state, &org).await {
            continue;
        }
        let db = state.org(&db_name).await?;
        let chains = if full {
            Chain::ALL.to_vec()
        } else {
            match pending(&db).await {
                Ok(chains) => chains,
                // An organization not migrated yet has no chain tables: nothing to seal.
                Err(error) => {
                    log::debug!("Audit chain skipped `{db_name}`: {error}");
                    continue;
                }
            }
        };
        if chains.is_empty() {
            continue;
        }
        let options = options(state, &org).await;
        for chain in chains {
            for _ in 0..MAX_BATCHES_PER_PASS {
                let started = std::time::Instant::now();
                match seal(&db, &db_name, chain, key, &options).await {
                    Ok(done) => {
                        sealed += done.sealed;
                        if done.sealed < u64::from(options.batch) {
                            break;
                        }
                        // More is waiting: rest as long as the batch took, so a backlog is sealed
                        // in the gaps between requests instead of in one burst that slows them.
                        tokio::time::sleep(started.elapsed()).await;
                    }
                    Err(error) => {
                        log::warn!("Audit chain: sealing {} of `{db_name}` failed: {error}", chain.name());
                        break;
                    }
                }
            }
        }
    }
    Ok(sealed)
}

/// Seal new audit rows now and then. Returns the task so shutdown can stop it; `None` when the
/// audit key cannot be read or made.
pub fn spawn_sealer(state: &AppState) -> Option<tokio::task::JoinHandle<()>> {
    let path = key_path(&state.config);
    let key = match AuditKey::load_or_create(&path) {
        Ok(key) => key,
        Err(error) => {
            log::error!("The audit chain is off: {error}");
            return None;
        }
    };
    let state = state.clone();
    Some(tokio::spawn(async move {
        tokio::time::sleep(FIRST_PASS_DELAY).await;
        let mut round = 0u32;
        loop {
            let full = round % FULL_PASS_EVERY == 0;
            round = round.wrapping_add(1);
            match pass(&state, &key, full).await {
                Ok(0) => {}
                Ok(sealed) => log::debug!("Audit chain sealed {sealed} row(s)"),
                Err(error) => log::warn!("Audit chain pass failed: {error}"),
            }
            tokio::time::sleep(SEAL_INTERVAL).await;
        }
    }))
}

/// Verify every chain of an organization and keep the result in `audit_verifications`.
pub async fn verify_organization(
    db: &surrealdb::Surreal<surrealdb::engine::remote::ws::Client>,
) -> Result<Vec<ChainReport>, super::chain::ChainError> {
    let mut reports = Vec::new();
    for chain in Chain::ALL {
        reports.push(verify(db, chain, None).await?);
    }
    let ok = reports.iter().all(|report| report.ok);
    db.query("CREATE audit_verifications SET ok = $ok, report = $report;")
        .bind(("ok", ok))
        .bind(("report", json!({ "chains": reports })))
        .await?
        .check()?;
    Ok(reports)
}

/// Check the chains once a day and log what is wrong, loudly. Returns the task so shutdown can
/// stop it.
pub fn spawn_verifier(state: &AppState) -> tokio::task::JoinHandle<()> {
    let state = state.clone();
    tokio::spawn(async move {
        tokio::time::sleep(FIRST_VERIFY_DELAY).await;
        loop {
            match crate::access::org_databases::list_cached(&state).await {
                Ok(organizations) => {
                    for db_name in organizations {
                        let org = crate::chatter::org_ref(&db_name);
                        if !enabled(&state, &org).await {
                            continue;
                        }
                        let Ok(db) = state.org(&db_name).await else {
                            continue;
                        };
                        match verify_organization(&db).await {
                            Ok(reports) => {
                                for report in reports.iter().filter(|report| !report.ok) {
                                    log::error!(
                                        "AUDIT CHAIN BROKEN in `{db_name}` ({}): {}",
                                        report.chain,
                                        report
                                            .failures
                                            .first()
                                            .map(|f| format!("{} at row {:?}: {}", f.rule, f.seq, f.detail))
                                            .unwrap_or_default()
                                    );
                                }
                            }
                            Err(error) => log::warn!("Audit verification skipped `{db_name}`: {error}"),
                        }
                    }
                }
                Err(error) => log::warn!("Audit verification could not list organizations: {error}"),
            }
            tokio::time::sleep(VERIFY_INTERVAL).await;
        }
    })
}
