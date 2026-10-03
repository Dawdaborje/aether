//! Resolving a request to an organization and an actor.
//!
//! A request is either from a logged-in user (an `aether_session` cookie that
//! maps to an active session) or from an anonymous visitor. Either way the
//! organization comes from the user's membership or from tenancy
//! (subdomain, path prefix or header), and is checked against
//! `org_databases`, so a made-up slug never reaches a database.

use std::net::IpAddr;

use aether_orm::{ValidSession, find_session_by_token};
use axum::http::{HeaderMap, Request, header::USER_AGENT};
use axum_extra::extract::CookieJar;
use surrealdb::types::SurrealValue;
use thiserror::Error;

use super::audit::{Actor, AuditContext, AuditError, clip_user_agent, new_request_id};
use crate::request_log::REQUEST_ID_HEADER;
use super::ip::client_ip;
use super::organizations::membership_db_names;
use super::visitor::{self, VISITOR_COOKIE, record_id_string};
use crate::state::AppState;
use crate::tenancy::resolve_org_slug;

pub const SESSION_COOKIE: &str = "aether_session";

#[derive(Debug, Error)]
pub enum IdentityError {
    /// No organization could be determined, or it does not exist. `logged_in`
    /// is whether the caller has an active session.
    #[error("organization not found")]
    OrgNotFound { logged_in: bool },

    /// A logged-in user belongs to several organizations and nothing says which
    /// one this request is for: the app must ask them (see `organizations`).
    #[error("organization selection required")]
    OrgSelectionRequired,

    #[error("too many requests from this address")]
    RateLimited,

    #[error("database error: {0}")]
    Database(#[from] surrealdb::Error),

    #[error(transparent)]
    Audit(#[from] AuditError),
}

impl IdentityError {
    /// HTTP status and a message that is safe to show any client.
    pub fn http(&self) -> (axum::http::StatusCode, &'static str) {
        use axum::http::StatusCode;
        match self {
            // Someone not logged in is asked to log in rather than told what exists.
            Self::OrgNotFound { logged_in: true } => (StatusCode::NOT_FOUND, "not found"),
            Self::OrgNotFound { logged_in: false } => (StatusCode::UNAUTHORIZED, "not authenticated"),
            Self::OrgSelectionRequired => (StatusCode::CONFLICT, "organization selection required"),
            Self::RateLimited => (StatusCode::TOO_MANY_REQUESTS, "too many requests"),
            Self::Database(_) | Self::Audit(_) => {
                (StatusCode::INTERNAL_SERVER_ERROR, "database error")
            }
        }
    }
}

/// The outcome of [`identify`].
#[derive(Debug, Clone)]
pub struct Identity {
    /// Database name of the organization the request is for.
    pub org_db: String,
    /// The user's session, when logged in.
    pub session: Option<ValidSession>,
    pub actor: Actor,
    /// A logged-in user who is not a member of this organization. They are
    /// treated as an anonymous visitor (public pages still work), but private
    /// content answers 403 rather than asking them to log in again.
    pub foreign_user: bool,
    pub client_ip: Option<IpAddr>,
    pub audit: AuditContext,
    /// Raw token of a visitor created during this request; the caller must
    /// set it as the `aether_visitor` cookie.
    pub new_visitor_token: Option<String>,
}

#[derive(Debug, SurrealValue)]
struct DbNameRow {
    db_name: String,
}

#[derive(Debug, SurrealValue)]
struct OrganizationIdRow {
    organization_id: String,
}

/// Identify who is making a request and for which organization. On success
/// `state.db` is left on the organization's database.
///
/// Anonymous callers get [`Actor::Anonymous`] (or their existing visitor);
/// call [`ensure_visitor`] once the request is known to be allowed to issue one.
pub async fn identify(
    state: &AppState,
    headers: &HeaderMap,
    peer: Option<IpAddr>,
    uri: &axum::http::Uri,
) -> Result<Identity, IdentityError> {
    let started = std::time::Instant::now();
    let result = identify_inner(state, headers, peer, uri).await;
    log::debug!(
        "{} identify finished in {}ms ({})",
        headers
            .get(REQUEST_ID_HEADER)
            .and_then(|value| value.to_str().ok())
            .unwrap_or("-"),
        started.elapsed().as_millis(),
        if result.is_ok() { "ok" } else { "refused" }
    );
    result
}

async fn identify_inner(
    state: &AppState,
    headers: &HeaderMap,
    peer: Option<IpAddr>,
    uri: &axum::http::Uri,
) -> Result<Identity, IdentityError> {
    let trusted = state
        .config
        .server
        .as_ref()
        .map(|server| server.trusted_proxies.as_slice())
        .unwrap_or_default();
    let ip = client_ip(headers, peer, trusted);
    let audit_ip = state.ip_policy.apply(ip);
    let user_agent = headers
        .get(USER_AGENT)
        .and_then(|value| value.to_str().ok())
        .map(clip_user_agent);

    // The request-logging middleware assigns the id; fall back to a fresh one
    // when this runs without it (tests).
    let request_id = headers
        .get(REQUEST_ID_HEADER)
        .and_then(|value| value.to_str().ok())
        .map(str::to_string)
        .unwrap_or_else(new_request_id);
    let jar = CookieJar::from_headers(headers);

    // Anyone who is not a logged-in user counts against the per-address
    // request budget. A request with no session cookie can be refused before
    // any database work; one with a cookie is counted once it turns out not to
    // belong to an active session or a member.
    let has_session_cookie = jar
        .get(SESSION_COOKIE)
        .is_some_and(|cookie| !cookie.value().is_empty());
    let mut throttled = false;
    if !has_session_cookie {
        throttle(state, ip)?;
        throttled = true;
    }

    let core = state.core().await?;
    let session = match jar
        .get(SESSION_COOKIE)
        .map(|cookie| cookie.value())
        .filter(|value| !value.is_empty())
    {
        Some(token) => find_session_by_token(&core, token)
            .await
            .map_err(|error| match error {
                aether_orm::SessionError::Surreal(error) => IdentityError::Database(error),
                aether_orm::SessionError::Invalid => IdentityError::OrgNotFound { logged_in: false },
            })?
            .filter(|session| session.user.is_active),
        None => None,
    };

    // Tenancy-derived organization (header, subdomain or path modes only).
    let mut request = Request::builder().uri(uri.clone());
    for (name, value) in headers {
        request = request.header(name, value);
    }
    let from_request = request
        .body(())
        .ok()
        .and_then(|request| resolve_org_slug(&request, &state.config.tenancy, None));

    log::debug!(
        "{request_id} identify: {}",
        match (&session, has_session_cookie) {
            (Some(session), _) => format!("logged in as {}", record_id_string(&session.user.id)),
            (None, true) => "session cookie present but not an active session".to_string(),
            (None, false) => "no session cookie".to_string(),
        }
    );
    if session.is_none() && !throttled {
        throttle(state, ip)?;
        throttled = true;
    }

    let mut candidates: Vec<String> = Vec::new();
    if let Some(org) = &from_request {
        candidates.push(org.slug.clone());
        candidates.push(org.db_name.clone());
    }
    if let Some(org) = session.as_ref().and_then(|s| s.org_database_id.clone()) {
        candidates.push(org);
    }

    let mut org_db = existing_org(state, &candidates).await?;
    if org_db.is_none()
        && let Some(session) = &session
    {
        let memberships =
            membership_db_names(state, &record_id_string(&session.user.id)).await?;
        // A developer who left their organization stays out of every one.
        let left = super::organizations::session_left_organization(session);
        match memberships.as_slice() {
            _ if left => {}
            [only] => org_db = Some(only.clone()),
            [] => {}
            // Several organizations and no hint: the user has to choose.
            _ => {
                log::info!(
                    "{request_id} user belongs to {} organizations and nothing selects one; asking the app to let them choose",
                    memberships.len()
                );
                return Err(IdentityError::OrgSelectionRequired);
            }
        }
    }
    let Some(org_db) = org_db else {
        log::info!(
            "{request_id} no organization for this request: tenancy mode {:?}, candidates tried {candidates:?}, logged in: {}. \
             Anonymous callers need `[tenancy] org_resolution` of subdomain, path or header, and the organization must be registered.",
            state.config.tenancy.org_resolution,
            session.is_some()
        );
        return Err(IdentityError::OrgNotFound {
            logged_in: session.is_some(),
        });
    };

    let mut actor = Actor::Anonymous;
    let mut foreign_user = false;
    let session = match session {
        Some(session) => {
            let user_id = record_id_string(&session.user.id);
            if session.user.is_super_user || is_member(state, &user_id, &org_db).await? {
                actor = Actor::User(user_id);
                Some(session)
            } else {
                foreign_user = true;
                if !throttled {
                    throttle(state, ip)?;
                }
                None
            }
        }
        None => None,
    };

    log::debug!(
        "{request_id} organization `{org_db}`; actor {}{}",
        actor.kind(),
        if foreign_user { " (logged in, but not a member: treated as a visitor)" } else { "" }
    );
    let org = state.org(&org_db).await?;
    if let Some(token) = jar
        .get(VISITOR_COOKIE)
        .map(|cookie| cookie.value())
        .filter(|value| !value.is_empty())
        && let Some(found) = visitor::find(&org, token).await?
    {
        match &actor {
            Actor::User(user_id) => {
                // The browser that browsed anonymously has now logged in:
                // tie its earlier history to the user.
                let link = found.linked_user.is_none().then_some(user_id.as_str());
                visitor::touch(&org, &found.id, link).await?;
            }
            _ => {
                visitor::touch(&org, &found.id, None).await?;
                actor = Actor::Visitor(found.id);
            }
        }
    }

    Ok(Identity {
        org_db,
        session,
        audit: AuditContext {
            actor: actor.clone(),
            request_id: request_id.clone(),
            ip: audit_ip,
            user_agent,
        },
        actor,
        foreign_user,
        client_ip: ip,
        new_visitor_token: None,
    })
}

fn throttle(state: &AppState, ip: Option<IpAddr>) -> Result<(), IdentityError> {
    if state.request_limiter.allow(ip) {
        Ok(())
    } else {
        log::debug!("rate limit: refusing request from {ip:?}");
        Err(IdentityError::RateLimited)
    }
}

/// Add `Retry-After` to a `429` response so well-behaved clients back off.
pub fn with_retry_after(mut response: axum::response::Response) -> axum::response::Response {
    if response.status() == axum::http::StatusCode::TOO_MANY_REQUESTS {
        response.headers_mut().insert(
            axum::http::header::RETRY_AFTER,
            axum::http::HeaderValue::from_static("60"),
        );
    }
    response
}

/// Give an anonymous caller a visitor identity (subject to the per-address
/// rate limit) and make it the actor. Leaves `org` on the organization.
pub async fn ensure_visitor(state: &AppState, identity: &mut Identity) -> Result<(), IdentityError> {
    if identity.actor != Actor::Anonymous {
        return Ok(());
    }
    if !state.visitor_limiter.allow(identity.client_ip) {
        return Err(IdentityError::RateLimited);
    }
    let org = state.org(&identity.org_db).await?;
    let (created, raw_token) = visitor::create(
        &org,
        identity.audit.ip.clone(),
        identity.audit.user_agent.clone(),
    )
    .await?;
    identity.actor = Actor::Visitor(created.id);
    identity.audit.actor = identity.actor.clone();
    identity.new_visitor_token = Some(raw_token);
    Ok(())
}

/// Add the `aether_visitor` cookie to `jar` when a visitor was just issued.
pub fn with_visitor_cookie(state: &AppState, identity: &Identity, jar: CookieJar) -> CookieJar {
    match &identity.new_visitor_token {
        Some(token) => {
            let development = state
                .config
                .configuration
                .as_ref()
                .is_some_and(|core| core.is_development_mode);
            jar.add(visitor::build_cookie(token.clone(), !development))
        }
        None => jar,
    }
}

/// The first candidate that is a registered organization database. Tenancy
/// slugs can be written with or without an `org_` prefix, so both are tried.
/// Leaves `org` on the core database.
async fn existing_org(state: &AppState, candidates: &[String]) -> Result<Option<String>, surrealdb::Error> {
    if candidates.is_empty() {
        return Ok(None);
    }
    let core = state.core().await?;
    let mut response = core
        .query("SELECT db_name FROM org_databases WHERE db_name IN $candidates;")
        .bind(("candidates", candidates.to_vec()))
        .await?
        .check()?;
    let rows: Vec<DbNameRow> = response.take(0)?;
    Ok(candidates
        .iter()
        .find(|candidate| rows.iter().any(|row| &row.db_name == *candidate))
        .cloned())
}

async fn is_member(state: &AppState, user_id: &str, org_db: &str) -> Result<bool, surrealdb::Error> {
    let core = state.core().await?;
    let mut response = core
        .query("SELECT organization_id FROM organization_users WHERE user_id = $user AND organization_id = $org LIMIT 1;")
        .bind(("user", user_id.to_string()))
        .bind(("org", format!("organizations:{org_db}")))
        .await?
        .check()?;
    let rows: Vec<OrganizationIdRow> = response.take(0)?;
    Ok(!rows.is_empty())
}

