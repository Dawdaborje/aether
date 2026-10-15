use rand::RngExt;
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use surrealdb::{Surreal, engine::remote::ws::Client, types::RecordId, types::SurrealValue};
use thiserror::Error;

#[derive(Debug, Error)]
pub enum SessionError {
    #[error("surrealdb error: {0}")]
    Surreal(#[from] surrealdb::Error),
    #[error("session not found or expired")]
    Invalid,
}

#[derive(Debug, Clone, Serialize, Deserialize, SurrealValue)]
pub struct SessionRecord {
    pub id: RecordId,
    pub user: RecordId,
    pub token_hash: String,
    pub provider: Option<String>,
    pub org_database_id: Option<String>,
    /// Expiry as unix seconds, so a cached copy can be refused once it has passed.
    pub expires_unix: Option<i64>,
}

#[derive(Debug, Clone)]
pub struct CreatedSession {
    pub raw_token: String,
    pub record: SessionRecord,
}

pub fn hash_token(raw: &str) -> String {
    let mut hasher = Sha256::new();
    hasher.update(raw.as_bytes());
    hasher.finalize().iter().map(|b| format!("{b:02x}")).collect()
}

pub fn generate_session_token() -> String {
    let mut bytes = [0u8; 32];
    rand::rng().fill(&mut bytes);
    bytes.iter().map(|b| format!("{b:02x}")).collect()
}

pub async fn create_session(
    db: &Surreal<Client>,
    user_id: RecordId,
    provider: &str,
    org_database_id: Option<String>,
    _ttl_hours: i64,
) -> Result<CreatedSession, SessionError> {
    let raw_token = generate_session_token();
    let token_hash = hash_token(&raw_token);

    let mut response = db
        .query(
            r#"
            CREATE sessions SET
                user = $user,
                token_hash = $token_hash,
                provider = $provider,
                org_database_id = $org_database_id,
                expires_at = time::now() + 24h
            RETURN id, user, token_hash, provider, org_database_id,
                   time::unix(expires_at) AS expires_unix;
            "#,
        )
        .bind(("user", user_id))
        .bind(("token_hash", token_hash))
        .bind(("provider", provider.to_string()))
        .bind(("org_database_id", org_database_id))
        .await?
        .check()?;

    let rows: Vec<SessionRecord> = response.take(0)?;
    let record = rows.into_iter().next().ok_or(SessionError::Invalid)?;
    Ok(CreatedSession { raw_token, record })
}

#[derive(Debug, Clone, Serialize, Deserialize, SurrealValue)]
pub struct SessionUserRow {
    pub id: RecordId,
    pub username: Option<String>,
    pub email: Option<String>,
    pub display_name: Option<String>,
    /// Filename under `app_dir/users/<id>/` when a profile photo is set.
    #[serde(default)]
    pub avatar_file: Option<String>,
    pub is_super_user: bool,
    pub is_active: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ValidSession {
    pub session_id: RecordId,
    pub org_database_id: Option<String>,
    pub provider: Option<String>,
    pub user: SessionUserRow,
    /// Unix seconds. A cached session past this moment is not honoured.
    #[serde(default)]
    pub expires_unix: Option<i64>,
}

#[derive(Debug, Deserialize, SurrealValue)]
struct SessionLookupRow {
    id: RecordId,
    user: RecordId,
    provider: Option<String>,
    org_database_id: Option<String>,
    expires_unix: Option<i64>,
}

pub async fn find_session_by_token(
    db: &Surreal<Client>,
    raw_token: &str,
) -> Result<Option<ValidSession>, SessionError> {
    let token_hash = hash_token(raw_token);
    let mut response = db
        .query(
            r#"
            SELECT id, user, provider, org_database_id, time::unix(expires_at) AS expires_unix
            FROM sessions
            WHERE token_hash = $token_hash
              AND revoked_at = NONE
              AND expires_at > time::now()
            LIMIT 1;
            "#,
        )
        .bind(("token_hash", token_hash))
        .await?
        .check()?;

    let sessions: Vec<SessionLookupRow> = response.take(0)?;
    let Some(session) = sessions.into_iter().next() else {
        return Ok(None);
    };

    let mut user_response = db
        .query(
            r#"
            SELECT id, username, email, display_name, avatar_file, is_super_user, is_active
            FROM $user_id;
            "#,
        )
        .bind(("user_id", session.user.clone()))
        .await?
        .check()?;
    let users: Vec<SessionUserRow> = user_response.take(0)?;
    let Some(user) = users.into_iter().next() else {
        return Ok(None);
    };

    Ok(Some(ValidSession {
        session_id: session.id,
        org_database_id: session.org_database_id,
        provider: session.provider,
        user,
        expires_unix: session.expires_unix,
    }))
}

pub async fn revoke_session_by_token(
    db: &Surreal<Client>,
    raw_token: &str,
) -> Result<(), SessionError> {
    let token_hash = hash_token(raw_token);
    db.query(
        r#"
        UPDATE sessions SET revoked_at = time::now()
        WHERE token_hash = $token_hash AND revoked_at = NONE;
        "#,
    )
    .bind(("token_hash", token_hash))
    .await?
    .check()?;
    Ok(())
}

/// Revoke every active session for a user. Returns token hashes so Mantle can bust them.
pub async fn revoke_sessions_for_user(
    db: &Surreal<Client>,
    user_id: &RecordId,
) -> Result<Vec<String>, SessionError> {
    let mut response = db
        .query(
            r#"
            SELECT VALUE token_hash FROM sessions
            WHERE user = $user AND revoked_at = NONE AND expires_at > time::now();
            UPDATE sessions SET revoked_at = time::now()
            WHERE user = $user AND revoked_at = NONE;
            "#,
        )
        .bind(("user", user_id.clone()))
        .await?
        .check()?;
    let hashes: Vec<String> = response.take(0)?;
    Ok(hashes)
}

/// Remember which organization a session is working in.
/// The `org_database_id` of a session whose developer deliberately left their organization:
/// not "unset" (which means nothing was chosen yet), so it is not filled in again from the
/// developer's only membership.
pub const LEFT_ORGANIZATION: &str = "";

pub async fn set_session_org(
    db: &Surreal<Client>,
    session_id: &RecordId,
    org_database_id: &str,
) -> Result<(), SessionError> {
    // `$session` is a reserved variable in SurrealQL, hence `$target`.
    db.query("UPDATE $target SET org_database_id = $org;")
        .bind(("target", session_id.clone()))
        .bind(("org", org_database_id.to_string()))
        .await?
        .check()?;
    Ok(())
}
