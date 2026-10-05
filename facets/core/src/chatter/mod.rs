//! Chatter: the conversation and history of a record.
//!
//! A model switches it on in its definition (`"chatter": { "enabled": true }`); otherwise it
//! does not exist for that model: the panel is not drawn, the endpoints answer that it is off,
//! nothing is tracked or stored. When it is on, a record's thread holds
//!
//! * **messages**, which notify the record's followers,
//! * **notes**, internal, which notify nobody except people mentioned,
//! * **changes** to the fields the model marks `track`, written by the kernel in the same
//!   transaction as the write itself, so they cannot be skipped or forged,
//! * **system** lines (the record was created, a plugin posted something).
//!
//! Nothing is ever deleted by a user. A deleted message goes to the trash and can be restored; a
//! deleted record's whole thread waits there too. Cleanup removes trash older than the
//! `chatter.trash_retention_days` setting.

pub mod api;
pub mod store;

pub use store::{Kind, Message, NewMessage};

use std::time::Duration;

use surrealdb::types::SurrealValue;

use crate::state::AppState;
use crate::tenancy::OrgRef;

/// How often the trash is cleaned.
const CLEANUP_INTERVAL: Duration = Duration::from_secs(60 * 60);

/// Setting: days a deleted message stays in the trash. Organizations can override it.
pub const RETENTION_SETTING: &str = "chatter.trash_retention_days";
/// Setting: whether chatter works at all in an organization.
pub const ENABLED_SETTING: &str = "chatter.enabled";
/// Used when the setting is missing or not a number; the seeded value is the same.
pub const DEFAULT_RETENTION_DAYS: u32 = 30;

#[derive(Debug, SurrealValue)]
struct OrgName {
    db_name: String,
}

/// An organization named by its database, which is not always `org_<slug>`.
pub fn org_ref(db_name: &str) -> OrgRef {
    OrgRef { slug: db_name.to_string(), db_name: db_name.to_string() }
}

/// The trash retention for an organization, from settings.
pub async fn retention_days(state: &AppState, org: &OrgRef) -> u32 {
    match crate::application::settings::get_setting(state, RETENTION_SETTING, Some(org)).await {
        Ok(Some(setting)) => setting
            .value
            .as_u64()
            .and_then(|days| u32::try_from(days).ok())
            .unwrap_or_else(|| {
                log::warn!("`{RETENTION_SETTING}` is not a whole number of days; using {DEFAULT_RETENTION_DAYS}");
                DEFAULT_RETENTION_DAYS
            }),
        Ok(None) => DEFAULT_RETENTION_DAYS,
        Err(error) => {
            log::warn!("could not read `{RETENTION_SETTING}`: {error}; using {DEFAULT_RETENTION_DAYS}");
            DEFAULT_RETENTION_DAYS
        }
    }
}

/// Whether chatter is allowed in an organization: on unless the setting says `false`.
pub async fn enabled_in(state: &AppState, org: &OrgRef) -> bool {
    match crate::application::settings::get_setting(state, ENABLED_SETTING, Some(org)).await {
        Ok(Some(setting)) => setting.value != serde_json::Value::Bool(false),
        Ok(None) => true,
        Err(error) => {
            log::warn!("could not read `{ENABLED_SETTING}`: {error}; chatter stays on");
            true
        }
    }
}

/// Remove trash older than each organization's retention, now and then every hour. Returns the
/// task so shutdown can stop it.
pub fn spawn_cleanup(state: &AppState) -> tokio::task::JoinHandle<()> {
    let state = state.clone();
    tokio::spawn(async move {
        loop {
            match cleanup(&state).await {
                Ok(0) => {}
                Ok(removed) => log::info!("Chatter cleanup removed {removed} message(s) from the trash"),
                Err(error) => log::warn!("Chatter cleanup failed: {error}"),
            }
            tokio::time::sleep(CLEANUP_INTERVAL).await;
        }
    })
}

async fn cleanup(state: &AppState) -> Result<u64, surrealdb::Error> {
    let core = state.core().await?;
    let mut response = core.query("SELECT db_name FROM org_databases;").await?.check()?;
    let organizations: Vec<OrgName> = response.take(0)?;
    let mut removed = 0;
    for organization in organizations {
        let org = org_ref(&organization.db_name);
        let days = retention_days(state, &org).await;
        let db = state.org(&organization.db_name).await?;
        removed += store::purge(&db, days).await?;
    }
    Ok(removed)
}
