//! The `notifications` and `notification_reads` tables of one organization database.

use serde_json::Value;
use surrealdb::{Surreal, engine::remote::ws::Client, types::SurrealValue};

use super::model::{Audience, Level, NewNotification, Notification, Viewer};

/// The columns of a notification, in the shape [`Notification`] needs.
const COLUMNS: &str = "ulid, audience_kind, audience_actors, source, level, title, body, link, payload, \
                       <string> date_created AS created_at";

/// A notification the viewer may see: not expired, and for them.
const VISIBLE: &str = "(expires_at = NONE OR expires_at > time::now()) AND ( \
                        audience_kind = 'everyone' \
                        OR (audience_kind = 'members' AND $member) \
                        OR (audience_kind = 'actors' AND $actor != NONE AND $actor IN audience_actors))";

#[derive(Debug, SurrealValue)]
struct Row {
    ulid: String,
    audience_kind: String,
    audience_actors: Vec<String>,
    source: String,
    level: String,
    title: String,
    body: Option<String>,
    link: Option<String>,
    payload: Option<Value>,
    created_at: String,
}

impl Row {
    fn into_notification(self, read: bool) -> Notification {
        Notification {
            id: self.ulid,
            source: self.source,
            level: Level::parse(&self.level).unwrap_or(Level::Info),
            title: self.title,
            body: self.body,
            link: self.link,
            payload: self.payload,
            created_at: self.created_at,
            read,
            audience: Audience::from_columns(&self.audience_kind, self.audience_actors),
        }
    }
}

#[derive(Debug, SurrealValue)]
struct Counted {
    count: u64,
}

/// Store a notification and return it as stored.
pub async fn insert(
    db: &Surreal<Client>,
    new: &NewNotification,
) -> Result<Notification, surrealdb::Error> {
    let mut response = db
        .query(format!(
            "CREATE notifications SET \
                ulid = rand::ulid(), \
                audience_kind = $kind, audience_actors = $actors, \
                source = $source, level = $level, title = $title, \
                body = $body, link = $link, payload = $payload, \
                expires_at = IF $ttl = NONE {{ NONE }} ELSE {{ time::now() + <duration> string::concat($ttl, 's') }} \
             RETURN {COLUMNS};"
        ))
        .bind(("kind", new.audience.kind().to_string()))
        .bind(("actors", new.audience.actors().to_vec()))
        .bind(("source", new.source.clone()))
        .bind(("level", new.level.as_str().to_string()))
        .bind(("title", new.title.clone()))
        .bind(("body", new.body.clone()))
        .bind(("link", new.link.clone()))
        .bind(("payload", new.payload.clone()))
        .bind(("ttl", new.expires_in_secs))
        .await?
        .check()?;
    let rows: Vec<Row> = response.take(0)?;
    rows.into_iter()
        .next()
        .map(|row| row.into_notification(false))
        .ok_or_else(|| surrealdb::Error::internal("notification was not created".to_string()))
}

/// Newest first; with `unread_only`, those the viewer has not read.
pub async fn list(
    db: &Surreal<Client>,
    viewer: &Viewer,
    unread_only: bool,
    limit: u32,
) -> Result<Vec<Notification>, surrealdb::Error> {
    let unread = if unread_only {
        " AND ulid NOT IN (SELECT VALUE notification FROM notification_reads WHERE actor = $actor)"
    } else {
        ""
    };
    let mut response = db
        .query(format!(
            "SELECT {COLUMNS} FROM notifications WHERE {VISIBLE}{unread} ORDER BY ulid DESC LIMIT $limit; \
             SELECT VALUE notification FROM notification_reads WHERE actor = $actor;"
        ))
        .bind(("member", viewer.member))
        .bind(("actor", viewer.actor.clone()))
        .bind(("limit", u64::from(limit)))
        .await?
        .check()?;
    let rows: Vec<Row> = response.take(0)?;
    let read: Vec<String> = response.take(1)?;
    Ok(rows
        .into_iter()
        .map(|row| {
            let is_read = read.contains(&row.ulid);
            row.into_notification(is_read)
        })
        .collect())
}

/// What a browser missed: everything for the viewer after `after`, oldest first.
pub async fn after(
    db: &Surreal<Client>,
    viewer: &Viewer,
    after: &str,
    limit: u32,
) -> Result<Vec<Notification>, surrealdb::Error> {
    let mut response = db
        .query(format!(
            "SELECT {COLUMNS} FROM notifications WHERE ulid > $after AND {VISIBLE} ORDER BY ulid LIMIT $limit;"
        ))
        .bind(("member", viewer.member))
        .bind(("actor", viewer.actor.clone()))
        .bind(("after", after.to_string()))
        .bind(("limit", u64::from(limit)))
        .await?
        .check()?;
    let rows: Vec<Row> = response.take(0)?;
    Ok(rows.into_iter().map(|row| row.into_notification(false)).collect())
}

pub async fn unread_count(db: &Surreal<Client>, viewer: &Viewer) -> Result<u64, surrealdb::Error> {
    let mut response = db
        .query(format!(
            "SELECT count() AS count FROM notifications \
             WHERE {VISIBLE} AND ulid NOT IN (SELECT VALUE notification FROM notification_reads WHERE actor = $actor) \
             GROUP ALL;"
        ))
        .bind(("member", viewer.member))
        .bind(("actor", viewer.actor.clone()))
        .await?
        .check()?;
    let counted: Vec<Counted> = response.take(0)?;
    Ok(counted.first().map_or(0, |row| row.count))
}

/// Mark notifications as read by `actor`: the ones named, or every one they can see.
/// Notifications the actor may not see are ignored. Returns how many were newly marked.
pub async fn mark_read(
    db: &Surreal<Client>,
    viewer: &Viewer,
    actor: &str,
    ids: Option<Vec<String>>,
) -> Result<u64, surrealdb::Error> {
    let only = if ids.is_some() { " AND ulid IN $ids" } else { "" };
    let mut response = db
        .query(format!(
            "LET $unread = SELECT VALUE ulid FROM notifications \
                WHERE {VISIBLE}{only} \
                AND ulid NOT IN (SELECT VALUE notification FROM notification_reads WHERE actor = $actor); \
             FOR $ulid IN $unread {{ CREATE notification_reads SET notification = $ulid, actor = $actor; }}; \
             RETURN array::len($unread);"
        ))
        .bind(("member", viewer.member))
        .bind(("actor", Some(actor.to_string())))
        .bind(("ids", ids.unwrap_or_default()))
        .await?
        .check()?;
    let marked: Option<u64> = response.take(2)?;
    Ok(marked.unwrap_or(0))
}

/// Delete expired notifications, and (when `retention_days` is set) those older than
/// that, along with read marks for notifications that no longer exist. Returns how many
/// notifications were removed.
pub async fn purge(
    db: &Surreal<Client>,
    retention_days: Option<u32>,
) -> Result<u64, surrealdb::Error> {
    let mut response = db
        .query(
            "LET $cutoff = IF $days = NONE { NONE } ELSE { time::now() - <duration> string::concat($days, 'd') }; \
             LET $gone = SELECT VALUE ulid FROM notifications \
                WHERE (expires_at != NONE AND expires_at < time::now()) \
                   OR ($cutoff != NONE AND date_created < $cutoff); \
             DELETE notifications WHERE ulid IN $gone; \
             DELETE notification_reads WHERE notification NOT IN (SELECT VALUE ulid FROM notifications); \
             RETURN array::len($gone);",
        )
        .bind(("days", retention_days))
        .await?
        .check()?;
    let removed: Option<u64> = response.take(4)?;
    Ok(removed.unwrap_or(0))
}
