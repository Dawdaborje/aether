//! The `chatter_messages` and `chatter_followers` tables of one organization database.

use serde::Serialize;
use serde_json::Value;
use surrealdb::{Surreal, engine::remote::ws::Client, types::SurrealValue};

/// What a line in a thread is.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum Kind {
    /// Notifies the record's followers.
    Message,
    /// Internal: notifies nobody except people mentioned.
    Note,
    /// Written by the kernel or a plugin ("created").
    System,
    /// Tracked fields that changed.
    Change,
}

impl Kind {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Message => "message",
            Self::Note => "note",
            Self::System => "system",
            Self::Change => "change",
        }
    }

    pub fn parse(text: &str) -> Option<Self> {
        match text {
            "message" => Some(Self::Message),
            "note" => Some(Self::Note),
            "system" => Some(Self::System),
            "change" => Some(Self::Change),
            _ => None,
        }
    }
}

/// A line to add to a thread.
#[derive(Debug, Clone)]
pub struct NewMessage {
    pub model_id: String,
    pub record_key: String,
    pub kind: Kind,
    pub author: String,
    pub author_name: Option<String>,
    pub body: Option<String>,
    pub mentions: Vec<String>,
    pub link: Option<String>,
    pub plugin: Option<String>,
}

/// A line of a thread as stored.
#[derive(Debug, Clone, Serialize)]
pub struct Message {
    pub id: String,
    pub kind: Kind,
    pub author: String,
    pub author_name: Option<String>,
    pub body: Option<String>,
    pub mentions: Vec<String>,
    pub link: Option<String>,
    pub plugin: Option<String>,
    pub before: Option<Value>,
    pub after: Option<Value>,
    pub created_at: String,
    pub edited_at: Option<String>,
    pub edited: bool,
    pub deleted_at: Option<String>,
    pub deleted_by: Option<String>,
    pub record_deleted_at: Option<String>,
}

const COLUMNS: &str = "ulid, model_id, record_key, kind, author, author_name, body, mentions, link, plugin, \
     before, after, <string> created_at AS created_at, \
     IF edited_at = NONE { NONE } ELSE { <string> edited_at } AS edited_at, \
     array::len(edits) > 0 AS edited, \
     IF deleted_at = NONE { NONE } ELSE { <string> deleted_at } AS deleted_at, deleted_by, \
     IF record_deleted_at = NONE { NONE } ELSE { <string> record_deleted_at } AS record_deleted_at";

#[derive(Debug, SurrealValue)]
struct Row {
    ulid: String,
    kind: String,
    author: String,
    author_name: Option<String>,
    body: Option<String>,
    mentions: Vec<String>,
    link: Option<String>,
    plugin: Option<String>,
    before: Option<Value>,
    after: Option<Value>,
    created_at: String,
    edited_at: Option<String>,
    edited: bool,
    deleted_at: Option<String>,
    deleted_by: Option<String>,
    record_deleted_at: Option<String>,
}

impl Row {
    fn into_message(self) -> Message {
        Message {
            id: self.ulid,
            kind: Kind::parse(&self.kind).unwrap_or(Kind::System),
            author: self.author,
            author_name: self.author_name,
            body: self.body,
            mentions: self.mentions,
            link: self.link,
            plugin: self.plugin,
            before: self.before,
            after: self.after,
            created_at: self.created_at,
            edited_at: self.edited_at,
            edited: self.edited,
            deleted_at: self.deleted_at,
            deleted_by: self.deleted_by,
            record_deleted_at: self.record_deleted_at,
        }
    }
}

#[derive(Debug, SurrealValue)]
struct Counted {
    count: u64,
}

fn one(rows: Vec<Row>) -> Option<Message> {
    rows.into_iter().next().map(Row::into_message)
}

/// Add a line to a thread.
pub async fn insert(db: &Surreal<Client>, new: &NewMessage) -> Result<Message, surrealdb::Error> {
    let mut response = db
        .query(format!(
            "CREATE chatter_messages SET ulid = rand::ulid(), model_id = $model, record_key = $key, \
                kind = $kind, author = $author, author_name = $author_name, body = $body, \
                mentions = $mentions, link = $link, plugin = $plugin RETURN {COLUMNS};"
        ))
        .bind(("model", new.model_id.clone()))
        .bind(("key", new.record_key.clone()))
        .bind(("kind", new.kind.as_str().to_string()))
        .bind(("author", new.author.clone()))
        .bind(("author_name", new.author_name.clone()))
        .bind(("body", new.body.clone()))
        .bind(("mentions", new.mentions.clone()))
        .bind(("link", new.link.clone()))
        .bind(("plugin", new.plugin.clone()))
        .await?
        .check()?;
    let rows: Vec<Row> = response.take(0)?;
    one(rows).ok_or_else(|| surrealdb::Error::internal("chatter message was not created".to_string()))
}

/// A record's thread, oldest first. The trash is left out unless `with_trash`; kinds a viewer
/// may not read (notes, for visitors) are left out through `kinds`.
pub async fn thread(
    db: &Surreal<Client>,
    model_id: &str,
    record_key: &str,
    kinds: &[Kind],
    with_trash: bool,
    limit: u32,
) -> Result<Vec<Message>, surrealdb::Error> {
    let trash = if with_trash { "" } else { " AND deleted_at = NONE AND record_deleted_at = NONE" };
    let mut response = db
        .query(format!(
            "SELECT {COLUMNS} FROM chatter_messages \
             WHERE model_id = $model AND record_key = $key AND kind IN $kinds{trash} \
             ORDER BY ulid LIMIT $limit;"
        ))
        .bind(("model", model_id.to_string()))
        .bind(("key", record_key.to_string()))
        .bind(("kinds", kinds.iter().map(|k| k.as_str().to_string()).collect::<Vec<_>>()))
        .bind(("limit", u64::from(limit)))
        .await?
        .check()?;
    let rows: Vec<Row> = response.take(0)?;
    Ok(rows.into_iter().map(Row::into_message).collect())
}

/// One line, trash included.
pub async fn get(
    db: &Surreal<Client>,
    model_id: &str,
    record_key: &str,
    id: &str,
) -> Result<Option<Message>, surrealdb::Error> {
    let mut response = db
        .query(format!(
            "SELECT {COLUMNS} FROM chatter_messages \
             WHERE ulid = $id AND model_id = $model AND record_key = $key LIMIT 1;"
        ))
        .bind(("id", id.to_string()))
        .bind(("model", model_id.to_string()))
        .bind(("key", record_key.to_string()))
        .await?
        .check()?;
    let rows: Vec<Row> = response.take(0)?;
    Ok(one(rows))
}

/// Replace the text of a message, keeping the earlier text in its history.
pub async fn edit(db: &Surreal<Client>, id: &str, body: &str) -> Result<Option<Message>, surrealdb::Error> {
    let mut response = db
        .query(format!(
            "UPDATE chatter_messages SET \
                edits = array::append(edits, {{ body: body, at: time::now() }}), \
                body = $body, edited_at = time::now() \
             WHERE ulid = $id AND deleted_at = NONE RETURN {COLUMNS};"
        ))
        .bind(("id", id.to_string()))
        .bind(("body", body.to_string()))
        .await?
        .check()?;
    let rows: Vec<Row> = response.take(0)?;
    Ok(one(rows))
}

/// Put a message in the trash. Nothing is removed until cleanup.
pub async fn trash(db: &Surreal<Client>, id: &str, by: &str) -> Result<Option<Message>, surrealdb::Error> {
    let mut response = db
        .query(format!(
            "UPDATE chatter_messages SET deleted_at = time::now(), deleted_by = $by \
             WHERE ulid = $id AND deleted_at = NONE RETURN {COLUMNS};"
        ))
        .bind(("id", id.to_string()))
        .bind(("by", by.to_string()))
        .await?
        .check()?;
    let rows: Vec<Row> = response.take(0)?;
    Ok(one(rows))
}

/// Take a message out of the trash.
pub async fn restore(db: &Surreal<Client>, id: &str) -> Result<Option<Message>, surrealdb::Error> {
    let mut response = db
        .query(format!(
            "UPDATE chatter_messages SET deleted_at = NONE, deleted_by = NONE \
             WHERE ulid = $id AND deleted_at != NONE RETURN {COLUMNS};"
        ))
        .bind(("id", id.to_string()))
        .await?
        .check()?;
    let rows: Vec<Row> = response.take(0)?;
    Ok(one(rows))
}

/// The record was deleted: its whole thread waits in the trash.
pub async fn trash_thread(
    db: &Surreal<Client>,
    model_id: &str,
    record_key: &str,
) -> Result<(), surrealdb::Error> {
    db.query(
        "UPDATE chatter_messages SET record_deleted_at = time::now() \
         WHERE model_id = $model AND record_key = $key AND record_deleted_at = NONE;",
    )
    .bind(("model", model_id.to_string()))
    .bind(("key", record_key.to_string()))
    .await?
    .check()?;
    Ok(())
}

/// A follower of a record.
#[derive(Debug, Clone, Serialize, SurrealValue)]
pub struct Follower {
    pub actor: String,
    pub reason: String,
    pub muted: bool,
}

/// Follow a record. An existing follow keeps its reason and mute state.
pub async fn follow(
    db: &Surreal<Client>,
    model_id: &str,
    record_key: &str,
    actor: &str,
    reason: &str,
) -> Result<(), surrealdb::Error> {
    db.query(
        "IF array::len(SELECT id FROM chatter_followers \
              WHERE model_id = $model AND record_key = $key AND actor = $actor) = 0 { \
            CREATE chatter_followers SET model_id = $model, record_key = $key, actor = $actor, reason = $reason; \
         };",
    )
    .bind(("model", model_id.to_string()))
    .bind(("key", record_key.to_string()))
    .bind(("actor", actor.to_string()))
    .bind(("reason", reason.to_string()))
    .await?
    .check()?;
    Ok(())
}

pub async fn unfollow(
    db: &Surreal<Client>,
    model_id: &str,
    record_key: &str,
    actor: &str,
) -> Result<(), surrealdb::Error> {
    db.query("DELETE chatter_followers WHERE model_id = $model AND record_key = $key AND actor = $actor;")
        .bind(("model", model_id.to_string()))
        .bind(("key", record_key.to_string()))
        .bind(("actor", actor.to_string()))
        .await?
        .check()?;
    Ok(())
}

/// Mute or unmute a follow (a mute without a follow does nothing).
pub async fn set_muted(
    db: &Surreal<Client>,
    model_id: &str,
    record_key: &str,
    actor: &str,
    muted: bool,
) -> Result<(), surrealdb::Error> {
    db.query(
        "UPDATE chatter_followers SET muted = $muted \
         WHERE model_id = $model AND record_key = $key AND actor = $actor;",
    )
    .bind(("model", model_id.to_string()))
    .bind(("key", record_key.to_string()))
    .bind(("actor", actor.to_string()))
    .bind(("muted", muted))
    .await?
    .check()?;
    Ok(())
}

pub async fn followers(
    db: &Surreal<Client>,
    model_id: &str,
    record_key: &str,
) -> Result<Vec<Follower>, surrealdb::Error> {
    let mut response = db
        .query(
            "SELECT actor, reason, muted, created_at FROM chatter_followers \
             WHERE model_id = $model AND record_key = $key ORDER BY created_at;",
        )
        .bind(("model", model_id.to_string()))
        .bind(("key", record_key.to_string()))
        .await?
        .check()?;
    response.take(0)
}

/// Remove trash older than `retention_days`: messages deleted that long ago, and the threads
/// of records deleted that long ago, with their followers. Returns how many messages went.
pub async fn purge(db: &Surreal<Client>, retention_days: u32) -> Result<u64, surrealdb::Error> {
    let mut response = db
        .query(
            "LET $cutoff = time::now() - <duration> string::concat($days, 'd'); \
             LET $gone = SELECT VALUE ulid FROM chatter_messages \
                WHERE (deleted_at != NONE AND deleted_at < $cutoff) \
                   OR (record_deleted_at != NONE AND record_deleted_at < $cutoff); \
             DELETE chatter_messages WHERE ulid IN $gone; \
             RETURN array::len($gone);",
        )
        .bind(("days", retention_days))
        .await?
        .check()?;
    let removed: Option<u64> = response.take(3)?;
    Ok(removed.unwrap_or(0))
}

/// How many lines a thread has in the trash (shown to those who may restore).
pub async fn trashed_count(
    db: &Surreal<Client>,
    model_id: &str,
    record_key: &str,
) -> Result<u64, surrealdb::Error> {
    let mut response = db
        .query(
            "SELECT count() AS count FROM chatter_messages \
             WHERE model_id = $model AND record_key = $key AND deleted_at != NONE GROUP ALL;",
        )
        .bind(("model", model_id.to_string()))
        .bind(("key", record_key.to_string()))
        .await?
        .check()?;
    let rows: Vec<Counted> = response.take(0)?;
    Ok(rows.first().map_or(0, |c| c.count))
}
