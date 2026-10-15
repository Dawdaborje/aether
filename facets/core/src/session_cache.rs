//! Mantle Tier A session-by-token cache.
//!
//! Keys are `kernel::session:{token_hash}`. Invalidate synchronously on logout/revoke
//! and on org switch. Soft TTL is a safety net only.

use std::time::Duration;

use aether_orm::{
    AuthUser, CreatedSession, SessionError, SessionUserRow, ValidSession, find_session_by_token,
    hash_token, revoke_session_by_token, revoke_sessions_for_user, set_session_org,
};
use surrealdb::{Surreal, engine::remote::ws::Client, types::RecordId};

use crate::cache::Cache;

/// Soft ceiling so a revoked or expired session cannot outlive a missed hook for long. The
/// cached entry does not carry `expires_at`, so this also bounds how far past its expiry a
/// session can still be honoured.
const SESSION_SOFT_TTL: Duration = Duration::from_secs(300);

fn put_valid(cache: &Cache, token_hash: &str, session: &ValidSession) {
    if let Ok(bytes) = serde_json::to_vec(session) {
        let _ = cache.session_put(token_hash, bytes, Some(SESSION_SOFT_TTL));
    }
}

/// Pre-warm Mantle right after login so the next request skips a DB round-trip.
pub fn warm_created(cache: &Cache, created: &CreatedSession, user: &AuthUser) {
    let session = ValidSession {
        session_id: created.record.id.clone(),
        org_database_id: created.record.org_database_id.clone(),
        provider: created.record.provider.clone(),
        user: SessionUserRow {
            id: user.id.clone(),
            username: user.username.clone(),
            email: user.email.clone(),
            display_name: user.display_name.clone(),
            avatar_file: user.avatar_file.clone(),
            is_super_user: user.is_super_user,
            is_active: user.is_active,
        },
        expires_unix: created.record.expires_unix,
    };
    put_valid(cache, &created.record.token_hash, &session);
}

pub async fn session_from_token(
    cache: &Cache,
    db: &Surreal<Client>,
    raw_token: &str,
) -> Result<Option<ValidSession>, SessionError> {
    let token_hash = hash_token(raw_token);
    if let Ok(Some(value)) = cache.session_get(&token_hash) {
        if let Ok(session) = serde_json::from_slice::<ValidSession>(value.as_slice()) {
            if session.user.is_active && !session_expired(&session) {
                return Ok(Some(session));
            }
        }
        let _ = cache.session_invalidate(&token_hash);
    }

    let session = find_session_by_token(db, raw_token).await?;
    if let Some(ref session) = session {
        put_valid(cache, &token_hash, session);
    }
    Ok(session)
}

/// A cached session is only as good as the expiry it was read with.
fn session_expired(session: &ValidSession) -> bool {
    let Some(expires) = session.expires_unix else {
        return false;
    };
    let now = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_secs() as i64)
        .unwrap_or(0);
    now >= expires
}

/// Drop the cached session so the next request reloads the user (photo, display name).
pub fn invalidate_token(cache: &Cache, raw_token: &str) {
    let token_hash = hash_token(raw_token);
    let _ = cache.session_invalidate(&token_hash);
}

pub async fn revoke_and_bust(
    cache: &Cache,
    db: &Surreal<Client>,
    raw_token: &str,
) -> Result<(), SessionError> {
    let token_hash = hash_token(raw_token);
    revoke_session_by_token(db, raw_token).await?;
    let _ = cache.session_invalidate(&token_hash);
    Ok(())
}

pub async fn set_org_and_refresh(
    cache: &Cache,
    db: &Surreal<Client>,
    session_id: &RecordId,
    org_database_id: &str,
    raw_token: Option<&str>,
) -> Result<(), SessionError> {
    set_session_org(db, session_id, org_database_id).await?;
    if let Some(raw_token) = raw_token {
        let token_hash = hash_token(raw_token);
        // Prefer invalidate so the next request reloads from DB with the new org.
        let _ = cache.session_invalidate(&token_hash);
    }
    Ok(())
}

/// Force-logout: revoke every session for a user and drop Mantle entries.
pub async fn revoke_all_for_user(
    cache: &Cache,
    db: &Surreal<Client>,
    user_id: &RecordId,
) -> Result<usize, SessionError> {
    let hashes = revoke_sessions_for_user(db, user_id).await?;
    let n = hashes.len();
    for hash in hashes {
        let _ = cache.session_invalidate(&hash);
    }
    Ok(n)
}
