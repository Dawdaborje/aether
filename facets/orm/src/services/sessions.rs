use rand::RngCore;
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
}

#[derive(Debug, Clone)]
pub struct CreatedSession {
    pub raw_token: String,
    pub record: SessionRecord,
}

pub fn hash_token(raw: &str) -> String {
    let mut hasher = Sha256::new();
    hasher.update(raw.as_bytes());
    format!("{:x}", hasher.finalize())
}

pub fn generate_session_token() -> String {
    let mut bytes = [0u8; 32];
    rand::thread_rng().fill_bytes(&mut bytes);
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
            RETURN AFTER;
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
    pub is_super_user: bool,
    pub is_active: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ValidSession {
    pub session_id: RecordId,
    pub org_database_id: Option<String>,
    pub provider: Option<String>,
    pub user: SessionUserRow,
}

#[derive(Debug, Deserialize, SurrealValue)]
struct SessionLookupRow {
    id: RecordId,
    user: RecordId,
    provider: Option<String>,
    org_database_id: Option<String>,
}

pub async fn find_session_by_token(
    db: &Surreal<Client>,
    raw_token: &str,
) -> Result<Option<ValidSession>, SessionError> {
    let token_hash = hash_token(raw_token);
    let mut response = db
        .query(
            r#"
            SELECT id, user, provider, org_database_id
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
            SELECT id, username, email, display_name, is_super_user, is_active
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
