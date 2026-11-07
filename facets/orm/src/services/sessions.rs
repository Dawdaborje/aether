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

/// Idle window: a session that is not used for this long expires.
pub const SESSION_IDLE_HOURS: i64 = 24;
/// Absolute cap: a session ends this long after login, however active it is.
pub const SESSION_MAX_DAYS: i64 = 14;

fn unix_now() -> i64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_secs() as i64)
        .unwrap_or(0)
}

/// Sliding expiry: the new expiry for a session seen at `now`, or `None` when it needs no
/// extension (more than half the idle window left, or already at the absolute cap).
fn renewed_expiry(now: i64, expires: i64, created: i64, idle_secs: i64, max_secs: i64) -> Option<i64> {
    if expires - now > idle_secs / 2 {
        return None;
    }
    let new = (now + idle_secs).min(created + max_secs);
    (new > expires).then_some(new)
}

pub async fn create_session(
    db: &Surreal<Client>,
    user_id: RecordId,
    provider: &str,
    org_database_id: Option<String>,
    ttl_hours: i64,
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
                created_at = time::now(),
                expires_at = time::now() + <duration>$ttl
            RETURN id, user, token_hash, provider, org_database_id,
                   time::unix(expires_at) AS expires_unix;
            "#,
        )
        .bind(("user", user_id))
        .bind(("token_hash", token_hash))
        .bind(("provider", provider.to_string()))
        .bind(("org_database_id", org_database_id))
        .bind(("ttl", format!("{ttl_hours}h")))
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
    /// Login time. Sessions created before the cap existed have none; see the query.
    created_unix: Option<i64>,
}

pub async fn find_session_by_token(
    db: &Surreal<Client>,
    raw_token: &str,
) -> Result<Option<ValidSession>, SessionError> {
    let token_hash = hash_token(raw_token);
    let mut response = db
        .query(
            r#"
            SELECT id, user, provider, org_database_id, time::unix(expires_at) AS expires_unix,
                   time::unix(created_at ?? (expires_at - 24h)) AS created_unix
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

    // Sliding renewal, bounded by the absolute cap.
    let mut expires_unix = session.expires_unix;
    if let (Some(expires), Some(created)) = (session.expires_unix, session.created_unix) {
        if let Some(new) = renewed_expiry(
            unix_now(),
            expires,
            created,
            SESSION_IDLE_HOURS * 3600,
            SESSION_MAX_DAYS * 86400,
        ) {
            // Best effort: a failed renewal must not reject a session that is still valid.
            let renewed = db
                .query("UPDATE $target SET expires_at = time::from_unix($new);")
                .bind(("target", session.id.clone()))
                .bind(("new", new))
                .await
                .and_then(|r| r.check());
            match renewed {
                Ok(_) => expires_unix = Some(new),
                Err(error) => log::warn!("session renewal failed: {error}"),
            }
        }
    }

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
        expires_unix,
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

#[cfg(test)]
mod renewal_tests {
    use super::renewed_expiry;
    const IDLE: i64 = 24 * 3600;
    const MAX: i64 = 14 * 86400;

    #[test]
    fn fresh_session_is_not_renewed() {
        assert_eq!(renewed_expiry(100, 100 + IDLE, 100, IDLE, MAX), None);
    }

    #[test]
    fn past_half_window_extends_to_full_window() {
        let now = 20 * 3600;
        assert_eq!(renewed_expiry(now, IDLE, 0, IDLE, MAX), Some(now + IDLE));
    }

    #[test]
    fn extension_is_clamped_to_the_cap() {
        let now = MAX - 3600;
        assert_eq!(renewed_expiry(now, now + 1800, 0, IDLE, MAX), Some(MAX));
    }

    #[test]
    fn at_the_cap_nothing_more_is_granted() {
        assert_eq!(renewed_expiry(MAX - 60, MAX, 0, IDLE, MAX), None);
    }
}
