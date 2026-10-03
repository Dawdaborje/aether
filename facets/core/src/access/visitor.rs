//! Anonymous visitor identities.
//!
//! A visitor is the anonymous counterpart of a session: an opaque token in an
//! `aether_visitor` cookie maps to a row in the organization's `visitors`
//! table. The row's id is the actor recorded in the audit trail, so everything
//! one browser did can be followed across requests. When that browser later
//! logs in, the row is linked to the user.

use axum_extra::extract::cookie::{Cookie, SameSite};
use rand::RngExt;
use surrealdb::{Surreal, engine::remote::ws::Client, types::{RecordId, SurrealValue, ToSql}};

use crate::plugin_manager::catalog::sha256_hex;

pub const VISITOR_COOKIE: &str = "aether_visitor";

const VISITOR_TTL_DAYS: i64 = 365;

/// `table:key` form used for ids stored as strings.
pub fn record_id_string(id: &RecordId) -> String {
    format!("{}:{}", id.table.as_str(), id.key.to_sql())
}

pub fn new_token() -> String {
    let mut bytes = [0u8; 32];
    rand::rng().fill(&mut bytes);
    bytes.iter().map(|byte| format!("{byte:02x}")).collect()
}

pub fn hash_token(raw: &str) -> String {
    sha256_hex(raw.as_bytes())
}

pub fn build_cookie(raw_token: String, secure: bool) -> Cookie<'static> {
    Cookie::build((VISITOR_COOKIE, raw_token))
        .http_only(true)
        .secure(secure)
        .path("/")
        .same_site(SameSite::Lax)
        .max_age(time::Duration::days(VISITOR_TTL_DAYS))
        .build()
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Visitor {
    /// `visitors:…`
    pub id: String,
    pub linked_user: Option<String>,
}

#[derive(Debug, serde::Deserialize, SurrealValue)]
struct VisitorRow {
    id: RecordId,
    linked_user: Option<String>,
}

impl From<VisitorRow> for Visitor {
    fn from(row: VisitorRow) -> Self {
        Self {
            id: record_id_string(&row.id),
            linked_user: row.linked_user,
        }
    }
}

/// Find the visitor for a raw cookie token. `db` must be on the organization's database.
pub async fn find(db: &Surreal<Client>, raw_token: &str) -> Result<Option<Visitor>, surrealdb::Error> {
    let mut response = db
        .query("SELECT id, linked_user FROM visitors WHERE token_hash = $hash LIMIT 1;")
        .bind(("hash", hash_token(raw_token)))
        .await?
        .check()?;
    let rows: Vec<VisitorRow> = response.take(0)?;
    Ok(rows.into_iter().next().map(Visitor::from))
}

/// Create a visitor; returns the row and the raw token to hand to the browser.
pub async fn create(
    db: &Surreal<Client>,
    ip: Option<String>,
    user_agent: Option<String>,
) -> Result<(Visitor, String), surrealdb::Error> {
    let raw_token = new_token();
    let mut response = db
        .query(
            "CREATE visitors SET token_hash = $hash, ip = $ip, user_agent = $user_agent RETURN id, linked_user;",
        )
        .bind(("hash", hash_token(&raw_token)))
        .bind(("ip", ip))
        .bind(("user_agent", user_agent))
        .await?
        .check()?;
    let rows: Vec<VisitorRow> = response.take(0)?;
    let row = rows.into_iter().next().ok_or_else(|| {
        surrealdb::Error::internal("creating a visitor returned no record".to_string())
    })?;
    Ok((row.into(), raw_token))
}

/// Mark the visitor as seen now, optionally linking it to a user (only if it
/// is not linked yet).
pub async fn touch(
    db: &Surreal<Client>,
    visitor_id: &str,
    link_user: Option<&str>,
) -> Result<(), surrealdb::Error> {
    db.query(
        r#"
        LET $visitor = type::record($visitor_id);
        UPDATE $visitor SET last_seen = time::now();
        IF $user != NONE { UPDATE $visitor SET linked_user = $user WHERE linked_user = NONE; };
        "#,
    )
    .bind(("visitor_id", visitor_id.to_string()))
    .bind(("user", link_user.map(str::to_string)))
    .await?
    .check()?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn tokens_are_random_hex_and_hash_stably() {
        let a = new_token();
        let b = new_token();
        assert_eq!(a.len(), 64);
        assert_ne!(a, b);
        assert_eq!(hash_token(&a), hash_token(&a));
        assert_ne!(hash_token(&a), a);
    }

    #[test]
    fn cookie_is_http_only_and_secure_when_asked() {
        let cookie = build_cookie("t".into(), true);
        assert_eq!(cookie.http_only(), Some(true));
        assert_eq!(cookie.secure(), Some(true));
        assert_eq!(build_cookie("t".into(), false).secure(), Some(false));
    }
}
