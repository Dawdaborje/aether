//! Generic single-use magic links for auth and plugins.
//!
//! Tokens are stored hashed; the raw token appears once in the returned URL.
//! Revoke and supersede set `revoked_at` — rows are never hard-deleted.

use chrono::{Duration, Utc};
use rand::RngExt;
use serde::{Deserialize, Serialize};
use serde_json::Value;
use sha2::{Digest, Sha256};
use surrealdb::{
    Surreal,
    engine::remote::ws::Client,
    types::{Datetime, SurrealValue, ToSql},
};
use thiserror::Error;

pub const PURPOSE_PASSWORD_RESET: &str = "auth.password_reset";
pub const PURPOSE_LOGIN: &str = "auth.login";
/// Host-issued link a person uses to choose a password and join an organization.
pub const PURPOSE_ACCOUNT_INVITE: &str = "auth.account_invite";

const MAX_TTL_SECS: i64 = 30 * 24 * 3600;
const DEFAULT_PLUGIN_TTL_SECS: i64 = 24 * 3600;
const DEFAULT_RESET_TTL_SECS: i64 = 60 * 60;
const DEFAULT_LOGIN_TTL_SECS: i64 = 15 * 60;
const DEFAULT_INVITE_TTL_SECS: i64 = 7 * 24 * 3600;

#[derive(Debug, Error)]
pub enum MagicLinkError {
    #[error("database error: {0}")]
    Db(#[from] surrealdb::Error),
    #[error("{0}")]
    Refused(String),
    #[error("this magic link is invalid, expired, or already used")]
    Invalid,
}

#[derive(Debug, Clone)]
pub struct CreateMagicLink {
    pub purpose: String,
    pub user_id: Option<String>,
    pub email: Option<String>,
    pub payload: Option<Value>,
    pub plugin: Option<String>,
    pub org_db: Option<String>,
    pub path: Option<String>,
    pub ttl_secs: Option<i64>,
    pub single_use: bool,
    pub supersede: bool,
    pub created_by: Option<String>,
    /// Public site origin used to build the absolute URL (e.g. `http://localhost:5173`).
    pub public_base: String,
}

#[derive(Debug, Clone, Serialize)]
pub struct CreatedMagicLink {
    pub id: String,
    pub token: String,
    pub url: String,
    pub expires_at: String,
    pub purpose: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, SurrealValue)]
pub struct MagicLinkRow {
    pub id: String,
    pub purpose: String,
    pub user_id: Option<String>,
    pub email: Option<String>,
    pub payload: Option<Value>,
    pub plugin: Option<String>,
    pub org_db: Option<String>,
    pub path: String,
    pub single_use: bool,
    pub expires_at: String,
    pub used_at: Option<String>,
    pub revoked_at: Option<String>,
    pub created_by: Option<String>,
}

fn hash_token(raw: &str) -> String {
    let mut hasher = Sha256::new();
    hasher.update(raw.as_bytes());
    hasher
        .finalize()
        .iter()
        .map(|b| format!("{b:02x}"))
        .collect()
}

fn generate_token() -> String {
    let mut bytes = [0u8; 32];
    rand::rng().fill(&mut bytes);
    bytes.iter().map(|b| format!("{b:02x}")).collect()
}

fn default_ttl(purpose: &str) -> i64 {
    match purpose {
        PURPOSE_PASSWORD_RESET => DEFAULT_RESET_TTL_SECS,
        PURPOSE_LOGIN => DEFAULT_LOGIN_TTL_SECS,
        PURPOSE_ACCOUNT_INVITE => DEFAULT_INVITE_TTL_SECS,
        _ => DEFAULT_PLUGIN_TTL_SECS,
    }
}

fn normalize_path(path: &str) -> Result<String, MagicLinkError> {
    let path = path.trim();
    if path.is_empty() {
        return Ok("/magic".into());
    }
    if !path.starts_with('/') || path.contains("://") || path.contains("..") {
        return Err(MagicLinkError::Refused(
            "path must be an absolute site path starting with /".into(),
        ));
    }
    Ok(path.to_string())
}

/// Plugins may only create purposes under `<plugin>.…`; host may create `auth.*`.
pub fn purpose_allowed_for_plugin(
    purpose: &str,
    plugin: Option<&str>,
) -> Result<(), MagicLinkError> {
    if purpose.is_empty() || purpose.len() > 128 || purpose.contains(' ') {
        return Err(MagicLinkError::Refused(
            "purpose must be a non-empty namespaced key (at most 128 characters)".into(),
        ));
    }
    if purpose.starts_with("auth.") {
        if plugin.is_some() {
            return Err(MagicLinkError::Refused(
                "only the host may create auth.* magic links".into(),
            ));
        }
        return Ok(());
    }
    let Some(plugin) = plugin else {
        return Ok(());
    };
    let prefix = format!("{plugin}.");
    if !purpose.starts_with(&prefix) {
        return Err(MagicLinkError::Refused(format!(
            "plugin `{plugin}` may only create purposes starting with `{prefix}`"
        )));
    }
    Ok(())
}

pub async fn create(
    db: &Surreal<Client>,
    namespace: &str,
    request: CreateMagicLink,
) -> Result<CreatedMagicLink, MagicLinkError> {
    purpose_allowed_for_plugin(&request.purpose, request.plugin.as_deref())?;
    let path = normalize_path(request.path.as_deref().unwrap_or("/magic"))?;
    let ttl = request
        .ttl_secs
        .unwrap_or_else(|| default_ttl(&request.purpose));
    if !(60..=MAX_TTL_SECS).contains(&ttl) {
        return Err(MagicLinkError::Refused(format!(
            "ttl_secs must be between 60 and {MAX_TTL_SECS}"
        )));
    }
    if request.user_id.is_none() && request.email.is_none() {
        return Err(MagicLinkError::Refused(
            "a magic link needs a user_id or an email".into(),
        ));
    }

    db.use_ns(namespace).await?;
    db.use_db("core").await?;

    if request.supersede {
        if let Some(user_id) = &request.user_id {
            db.query(
                "UPDATE magic_links SET revoked_at = time::now() \
                 WHERE purpose = $purpose AND user_id = $user \
                   AND used_at = NONE AND revoked_at = NONE;",
            )
            .bind(("purpose", request.purpose.clone()))
            .bind(("user", user_id.clone()))
            .await?
            .check()?;
        }
    }

    let raw = generate_token();
    let token_hash = hash_token(&raw);
    let expires = Utc::now() + Duration::seconds(ttl);
    let expires_at = Datetime::from(expires);

    let mut response = db
        .query(
            "CREATE magic_links SET \
                token_hash = $token_hash, \
                purpose = $purpose, \
                user_id = $user_id, \
                email = $email, \
                payload = $payload, \
                plugin = $plugin, \
                org_db = $org_db, \
                path = $path, \
                single_use = $single_use, \
                expires_at = $expires_at, \
                created_by = $created_by \
             RETURN AFTER;",
        )
        .bind(("token_hash", token_hash))
        .bind(("purpose", request.purpose.clone()))
        .bind(("user_id", request.user_id.clone()))
        .bind(("email", request.email.clone()))
        .bind(("payload", request.payload.clone()))
        .bind(("plugin", request.plugin.clone()))
        .bind(("org_db", request.org_db.clone()))
        .bind(("path", path.clone()))
        .bind(("single_use", request.single_use))
        .bind(("expires_at", expires_at))
        .bind(("created_by", request.created_by.clone()))
        .await?
        .check()?;

    #[derive(Debug, Deserialize, SurrealValue)]
    struct Created {
        id: surrealdb::types::RecordId,
        expires_at: Datetime,
    }
    let rows: Vec<Created> = response.take(0)?;
    let row = rows.into_iter().next().ok_or(MagicLinkError::Invalid)?;
    let id = format!("{}:{}", row.id.table, row.id.key.to_sql());
    let base = request.public_base.trim_end_matches('/');
    let sep = if path.contains('?') { '&' } else { '?' };
    let url = format!("{base}{path}{sep}token={raw}");

    Ok(CreatedMagicLink {
        id,
        token: raw,
        url,
        expires_at: row.expires_at.to_string(),
        purpose: request.purpose,
    })
}

pub async fn revoke(
    db: &Surreal<Client>,
    namespace: &str,
    id: Option<&str>,
    purpose: Option<&str>,
    user_id: Option<&str>,
) -> Result<u64, MagicLinkError> {
    db.use_ns(namespace).await?;
    db.use_db("core").await?;
    let mut response = if let Some(id) = id {
        db.query(
            "UPDATE type::record('magic_links', $key) SET revoked_at = time::now() \
             WHERE revoked_at = NONE AND used_at = NONE RETURN AFTER;",
        )
        .bind((
            "key",
            id.strip_prefix("magic_links:").unwrap_or(id).to_string(),
        ))
        .await?
        .check()?
    } else if let (Some(purpose), Some(user_id)) = (purpose, user_id) {
        db.query(
            "UPDATE magic_links SET revoked_at = time::now() \
             WHERE purpose = $purpose AND user_id = $user \
               AND used_at = NONE AND revoked_at = NONE RETURN AFTER;",
        )
        .bind(("purpose", purpose.to_string()))
        .bind(("user", user_id.to_string()))
        .await?
        .check()?
    } else {
        return Err(MagicLinkError::Refused(
            "revoke needs an id, or purpose and user_id".into(),
        ));
    };
    let rows: Vec<Value> = response.take(0)?;
    Ok(rows.len() as u64)
}

pub async fn inspect(
    db: &Surreal<Client>,
    namespace: &str,
    token: &str,
) -> Result<MagicLinkRow, MagicLinkError> {
    let row = lookup(db, namespace, token).await?;
    if row.revoked_at.is_some() {
        return Err(MagicLinkError::Invalid);
    }
    Ok(row)
}

async fn lookup(
    db: &Surreal<Client>,
    namespace: &str,
    token: &str,
) -> Result<MagicLinkRow, MagicLinkError> {
    db.use_ns(namespace).await?;
    db.use_db("core").await?;
    let token_hash = hash_token(token);
    let mut response = db
        .query(
            "SELECT \
                <string> id AS id, purpose, user_id, email, payload, plugin, org_db, path, \
                single_use, <string> expires_at AS expires_at, \
                IF used_at = NONE { NONE } ELSE { <string> used_at } AS used_at, \
                IF revoked_at = NONE { NONE } ELSE { <string> revoked_at } AS revoked_at, \
                created_by \
             FROM magic_links WHERE token_hash = $hash LIMIT 1;",
        )
        .bind(("hash", token_hash))
        .await?
        .check()?;
    let rows: Vec<MagicLinkRow> = response.take(0)?;
    rows.into_iter().next().ok_or(MagicLinkError::Invalid)
}

/// Validate and mark used (when single_use). Caller handles purpose-specific side effects.
pub async fn consume(
    db: &Surreal<Client>,
    namespace: &str,
    token: &str,
) -> Result<MagicLinkRow, MagicLinkError> {
    db.use_ns(namespace).await?;
    db.use_db("core").await?;
    let token_hash = hash_token(token);
    let mut response = db
        .query(
            "SELECT \
                <string> id AS id, purpose, user_id, email, payload, plugin, org_db, path, \
                single_use, <string> expires_at AS expires_at, \
                IF used_at = NONE { NONE } ELSE { <string> used_at } AS used_at, \
                IF revoked_at = NONE { NONE } ELSE { <string> revoked_at } AS revoked_at, \
                created_by \
             FROM magic_links \
             WHERE token_hash = $hash AND expires_at > time::now() \
               AND used_at = NONE AND revoked_at = NONE \
             LIMIT 1;",
        )
        .bind(("hash", token_hash))
        .await?
        .check()?;
    let rows: Vec<MagicLinkRow> = response.take(0)?;
    let row = rows.into_iter().next().ok_or(MagicLinkError::Invalid)?;
    if row.single_use {
        let key = row
            .id
            .strip_prefix("magic_links:")
            .unwrap_or(&row.id)
            .to_string();
        let mut marked = db
            .query(
                "UPDATE type::record('magic_links', $key) SET used_at = time::now() \
                 WHERE used_at = NONE AND revoked_at = NONE RETURN AFTER;",
            )
            .bind(("key", key))
            .await?
            .check()?;
        let updated: Vec<Value> = marked.take(0)?;
        if updated.is_empty() {
            return Err(MagicLinkError::Invalid);
        }
    }
    Ok(row)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn plugins_may_only_use_their_prefix() {
        assert!(purpose_allowed_for_plugin("approvals.decide", Some("approvals")).is_ok());
        assert!(purpose_allowed_for_plugin("other.decide", Some("approvals")).is_err());
        assert!(purpose_allowed_for_plugin("auth.password_reset", Some("approvals")).is_err());
        assert!(purpose_allowed_for_plugin("auth.account_invite", Some("hr")).is_err());
        assert!(purpose_allowed_for_plugin("auth.password_reset", None).is_ok());
        assert!(purpose_allowed_for_plugin("auth.account_invite", None).is_ok());
    }

    #[test]
    fn path_must_be_site_relative() {
        assert_eq!(normalize_path("/login/magic").unwrap(), "/login/magic");
        assert!(normalize_path("https://evil.example").is_err());
        assert!(normalize_path("../x").is_err());
    }
}
