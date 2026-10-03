use std::sync::Arc;

use surrealdb::{Surreal, engine::remote::ws::Client, types::SurrealValue};
use thiserror::Error;

use super::{
    hub::{HubMessage, NotificationHub},
    model::{Audience, NewNotification, Notification},
    store,
};

const MAX_TITLE: usize = 200;
const MAX_BODY: usize = 2_000;
const MAX_LINK: usize = 500;
const MAX_PAYLOAD_BYTES: usize = 16_384;
const MAX_ACTORS: usize = 500;

#[derive(Debug, Error)]
pub enum NotifyError {
    #[error("invalid notification: {0}")]
    Invalid(String),
    #[error("notification database error: {0}")]
    Database(#[from] surrealdb::Error),
}

fn invalid(message: &str) -> NotifyError {
    NotifyError::Invalid(message.to_string())
}

/// Check what a sender gave, before anything is stored.
pub fn validate(new: &NewNotification) -> Result<(), NotifyError> {
    if new.title.trim().is_empty() || new.title.len() > MAX_TITLE {
        return Err(invalid("title must be 1 to 200 bytes"));
    }
    if new.body.as_ref().is_some_and(|body| body.len() > MAX_BODY) {
        return Err(invalid("body must be at most 2000 bytes"));
    }
    if let Some(link) = &new.link
        && (!link.starts_with('/') || link.starts_with("//") || link.len() > MAX_LINK)
    {
        // An app path only: a notification must not send people to another site.
        return Err(invalid("link must be an app path starting with a single `/`"));
    }
    if let Some(payload) = &new.payload {
        let size = serde_json::to_vec(payload).map_or(usize::MAX, |bytes| bytes.len());
        if size > MAX_PAYLOAD_BYTES || !payload.is_object() {
            return Err(invalid("payload must be an object of at most 16384 bytes"));
        }
    }
    if let Audience::Actors { actors } = &new.audience {
        if actors.is_empty() || actors.len() > MAX_ACTORS {
            return Err(invalid("actors must name 1 to 500 users or visitors"));
        }
        if actors
            .iter()
            .any(|actor| !(actor.starts_with("users:") || actor.starts_with("visitors:")))
        {
            return Err(invalid("actors must be `users:…` or `visitors:…` ids"));
        }
    }
    Ok(())
}

/// Store a notification in the organization's database, then wake whoever is connected.
///
/// The stored row is what counts: if waking fails nobody is hurt, the notification is
/// replayed when they next connect or list.
pub async fn send(
    db: &Surreal<Client>,
    hub: &NotificationHub,
    org_database: &str,
    new: NewNotification,
) -> Result<Notification, NotifyError> {
    validate(&new)?;
    let stored = store::insert(db, &new).await?;
    log::debug!(
        "notification {} from `{}` for {} in `{org_database}`",
        stored.id,
        stored.source,
        stored.audience.kind()
    );
    hub.publish(org_database, HubMessage::Notification(Arc::new(stored.clone())));
    Ok(stored)
}

/// How often expired and old notifications are removed.
const CLEANUP_INTERVAL: std::time::Duration = std::time::Duration::from_secs(60 * 60);

/// Remove expired notifications (and, with `[notifications] retention_days`, old ones)
/// from every organization, now and then every hour. Returns the task so shutdown can
/// stop it.
pub fn spawn_cleanup(state: &crate::state::AppState) -> tokio::task::JoinHandle<()> {
    let state = state.clone();
    tokio::spawn(async move {
        loop {
            match cleanup(&state).await {
                Ok(0) => {}
                Ok(removed) => log::info!("Notification cleanup removed {removed} notification(s)"),
                Err(error) => log::warn!("Notification cleanup failed: {error}"),
            }
            tokio::time::sleep(CLEANUP_INTERVAL).await;
        }
    })
}

#[derive(Debug, SurrealValue)]
struct OrgName {
    db_name: String,
}

async fn cleanup(state: &crate::state::AppState) -> Result<u64, surrealdb::Error> {
    let core = state.core().await?;
    let mut response = core.query("SELECT db_name FROM org_databases;").await?.check()?;
    let organizations: Vec<OrgName> = response.take(0)?;
    let days = state.config.notifications.retention_days;
    let mut removed = 0;
    for organization in organizations {
        let db = state.org(&organization.db_name).await?;
        removed += store::purge(&db, days).await?;
    }
    Ok(removed)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::notifications::Level;

    fn note() -> NewNotification {
        NewNotification::kernel(Level::Info, "Hello", None)
    }

    #[test]
    fn a_plain_notification_is_valid() {
        assert!(validate(&note()).is_ok());
    }

    #[test]
    fn rejects_empty_titles_and_off_site_links() {
        let mut empty = note();
        empty.title = "  ".into();
        assert!(validate(&empty).is_err());
        for link in ["https://evil.example", "//evil.example", "relative"] {
            let mut linked = note();
            linked.link = Some(link.to_string());
            assert!(validate(&linked).is_err(), "{link}");
        }
        let mut ok = note();
        ok.link = Some("/chat/general".into());
        assert!(validate(&ok).is_ok());
    }

    #[test]
    fn actors_must_be_users_or_visitors() {
        let mut bad = note();
        bad.audience = Audience::Actors { actors: vec!["org:acme".into()] };
        assert!(validate(&bad).is_err());
        bad.audience = Audience::Actors { actors: vec![] };
        assert!(validate(&bad).is_err());
        bad.audience = Audience::Actors { actors: vec!["visitors:abc".into(), "users:def".into()] };
        assert!(validate(&bad).is_ok());
    }
}
