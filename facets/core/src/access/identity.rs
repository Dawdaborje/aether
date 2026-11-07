//! Resolving a request to an organization and an actor.
//!
//! A request is either from a logged-in user (an `aether_session` cookie that
//! maps to an active session) or from an anonymous visitor. Either way the
//! organization comes from the user's membership or from tenancy
//! (subdomain, path prefix or header), and is checked against
//! `org_databases`, so a made-up slug never reaches a database.

use std::net::IpAddr;

use aether_orm::ValidSession;
use axum::http::{HeaderMap, header::USER_AGENT};
use axum_extra::extract::CookieJar;
use thiserror::Error;

use super::audit::{Actor, AuditContext, AuditError, clip_user_agent, new_request_id};
use super::ip::client_ip;
use super::organizations::membership_db_names;
use super::visitor::{self, VISITOR_COOKIE, record_id_string};
use crate::request_log::REQUEST_ID_HEADER;
use crate::state::AppState;

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

    /// The organization exists but a developer has disabled it.
    #[error("organization disabled")]
    OrgDisabled,

    #[error("too many requests from this address")]
    RateLimited,

    /// A bearer token that is unknown, revoked, expired, or whose owner is gone.
    #[error("invalid or expired token")]
    InvalidToken,

    /// API tokens are switched off for the organization.
    #[error("API access is switched off")]
    ApiDisabled,

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
            Self::OrgNotFound { logged_in: false } => {
                (StatusCode::UNAUTHORIZED, "not authenticated")
            }
            Self::OrgSelectionRequired => (StatusCode::CONFLICT, "organization selection required"),
            Self::OrgDisabled => (StatusCode::FORBIDDEN, "This organization is disabled."),
            Self::RateLimited => (StatusCode::TOO_MANY_REQUESTS, "too many requests"),
            Self::InvalidToken => (StatusCode::UNAUTHORIZED, "invalid or expired token"),
            Self::ApiDisabled => (StatusCode::FORBIDDEN, "API access is switched off"),
            Self::Database(_) | Self::Audit(_) => {
                (StatusCode::INTERNAL_SERVER_ERROR, "database error")
            }
        }
    }
}

pub use aether_core_base::access::RuleCache;

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
    /// Answers to rule variables for this request.
    pub rule_cache: RuleCache,
    /// The API token the request came with, when it was let in by one (see [`identify_api`]).
    pub token: Option<super::api_token::TokenUse>,
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
    _uri: &axum::http::Uri,
) -> Result<Identity, IdentityError> {
    let trusted = state
        .config
        .server
        .as_ref()
        .map(|server| server.trusted_proxies.as_slice())
        .unwrap_or_default();
    let ip = client_ip(headers, peer, trusted);
    let audit_ip = state.limits.ip_policy.apply(ip);
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
        throttle(&state.limits.request_limiter, ip)?;
        throttled = true;
    }

    let core = state.core().await?;
    let session = match jar
        .get(SESSION_COOKIE)
        .map(|cookie| cookie.value())
        .filter(|value| !value.is_empty())
    {
        Some(token) => crate::session_cache::session_from_token(&state.cache, &core, token)
            .await
            .map_err(|error| match error {
                aether_orm::SessionError::Surreal(error) => IdentityError::Database(error),
                aether_orm::SessionError::Invalid => {
                    IdentityError::OrgNotFound { logged_in: false }
                }
            })?
            .filter(|session| session.user.is_active),
        None => None,
    };

    log::debug!(
        "{request_id} identify: {}",
        match (&session, has_session_cookie) {
            (Some(session), _) => format!("logged in as {}", record_id_string(&session.user.id)),
            (None, true) => "session cookie present but not an active session".to_string(),
            (None, false) => "no session cookie".to_string(),
        }
    );
    if session.is_none() && !throttled {
        throttle(&state.limits.request_limiter, ip)?;
        throttled = true;
    }

    // Single-organization installs have exactly one candidate: nothing in the request or the
    // session can name another, and nobody is asked to choose.
    let candidates: Vec<String> = vec![state.config.tenancy.single_org_db_name.clone()];

    let org_db = existing_org(state, &candidates).await?;
    if org_db.is_none() && is_disabled(state, &candidates).await? {
        return Err(IdentityError::OrgDisabled);
    }
    let Some(org_db) = org_db else {
        log::info!(
            "{request_id} no organization for this request: tenancy mode {:?}, candidates tried {candidates:?}, logged in: {}. \
             The installation's organization must be registered (`aether --init`).",
            "single",
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
                    throttle(&state.limits.request_limiter, ip)?;
                }
                None
            }
        }
        None => None,
    };

    log::debug!(
        "{request_id} organization `{org_db}`; actor {}{}",
        actor.kind(),
        if foreign_user {
            " (logged in, but not a member: treated as a visitor)"
        } else {
            ""
        }
    );
    let org = state.org(&org_db).await?;
    if let Some(token) = jar
        .get(VISITOR_COOKIE)
        .map(|cookie| cookie.value())
        .filter(|value| !value.is_empty())
        && let Some(found) = find_visitor(state, &org_db, &org, token).await?
    {
        match &actor {
            Actor::User(user_id) => {
                // The browser that browsed anonymously has now logged in:
                // tie its earlier history to the user.
                if found.linked_user.is_none() {
                    visitor::touch(&org, &found.id, Some(user_id.as_str())).await?;
                    let _ = state
                        .cache
                        .kernel_org_invalidate(&org_db, &visitor_key(token));
                } else {
                    touch_visitor(state, &org_db, &org, &found.id).await?;
                }
            }
            _ => {
                touch_visitor(state, &org_db, &org, &found.id).await?;
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
        rule_cache: RuleCache::default(),
        token: None,
    })
}

/// Like [`identify`], but a request with `Authorization: Bearer aet_…` and no session cookie is
/// let in by that API token: the actor is the token's owner, in the token's organization, and
/// `Identity::token` carries its scopes for the endpoint to enforce. A request with a session
/// cookie is the browser's, whatever else it sends.
///
/// Only the endpoints meant for machines call this (`/api/plugins/…`, `/api/v1/…`); every other
/// endpoint uses [`identify`] and does not know tokens, so a token cannot reach them.
pub async fn identify_api(
    state: &AppState,
    headers: &HeaderMap,
    peer: Option<IpAddr>,
    uri: &axum::http::Uri,
) -> Result<Identity, IdentityError> {
    let jar = CookieJar::from_headers(headers);
    let has_session_cookie = jar
        .get(SESSION_COOKIE)
        .is_some_and(|cookie| !cookie.value().is_empty());
    let Some(raw) = super::api_token::bearer(headers).filter(|_| !has_session_cookie) else {
        return identify(state, headers, peer, uri).await;
    };

    let trusted = state
        .config
        .server
        .as_ref()
        .map(|server| server.trusted_proxies.as_slice())
        .unwrap_or_default();
    let ip = client_ip(headers, peer, trusted);
    let core = state.core().await?;
    let row = match super::api_token::lookup(&core, raw).await {
        Ok(row) => row,
        Err(super::api_token::TokenError::Database(error)) => return Err(IdentityError::Database(error)),
        Err(_) => None,
    };
    // Guessing tokens counts against the address like any other failed attempt.
    let Some(row) = row.filter(|row| row.user_active) else {
        throttle(&state.limits.request_limiter, ip)?;
        return Err(IdentityError::InvalidToken);
    };
    let org_ok = existing_org(state, std::slice::from_ref(&row.org_db)).await?.is_some();
    if !org_ok || !(row.user_is_super || is_member(state, &row.user, &row.org_db).await?) {
        throttle(&state.limits.request_limiter, ip)?;
        return Err(IdentityError::InvalidToken);
    }
    let org_ref = crate::chatter::org_ref(&row.org_db);
    let setting = |key: &'static str| {
        let org_ref = org_ref.clone();
        async move {
            crate::application::settings::get_setting(state, key, Some(&org_ref))
                .await
                .ok()
                .flatten()
                .map(|setting| setting.value)
        }
    };
    if setting("api.enabled").await == Some(serde_json::Value::Bool(false)) {
        return Err(IdentityError::ApiDisabled);
    }
    let per_minute = setting("api.rate_per_minute")
        .await
        .and_then(|v| v.as_u64())
        .map_or(600, |n| u32::try_from(n).unwrap_or(u32::MAX));
    if !super::api_token::allow(&row.id, per_minute) {
        return Err(IdentityError::RateLimited);
    }
    let audit_ip = state.limits.ip_policy.apply(ip);
    super::api_token::touch(&core, &row.id, audit_ip.as_deref()).await;

    // The audit trail says which token made the call: the user agent column carries it, and every
    // data access of the request is tied to the call by the request id.
    let agent = headers
        .get(USER_AGENT)
        .and_then(|value| value.to_str().ok())
        .unwrap_or("");
    let request_id = headers
        .get(REQUEST_ID_HEADER)
        .and_then(|value| value.to_str().ok())
        .map(str::to_string)
        .unwrap_or_else(new_request_id);
    let actor = Actor::User(row.user.clone());
    Ok(Identity {
        org_db: row.org_db.clone(),
        session: None,
        audit: AuditContext {
            actor: actor.clone(),
            request_id,
            ip: audit_ip,
            user_agent: Some(clip_user_agent(&format!("token:{}; {agent}", row.id))),
        },
        actor,
        foreign_user: false,
        client_ip: ip,
        new_visitor_token: None,
        rule_cache: RuleCache::default(),
        token: Some(super::api_token::TokenUse {
            id: row.id,
            name: row.name,
            scopes: row.scopes,
        }),
    })
}

/// How long a visitor lookup, and the "seen" write that follows it, are reused.
const VISITOR_WARM: std::time::Duration = std::time::Duration::from_secs(300);
const VISITOR_SEEN_EVERY: std::time::Duration = std::time::Duration::from_secs(60);

fn visitor_key(raw_token: &str) -> String {
    format!("visitor:{}", visitor::hash_token(raw_token))
}

/// `visitor::find` behind the warm cache: a returning browser costs no query per request.
async fn find_visitor(
    state: &AppState,
    org_db: &str,
    org: &surrealdb::Surreal<surrealdb::engine::remote::ws::Client>,
    raw_token: &str,
) -> Result<Option<visitor::Visitor>, surrealdb::Error> {
    let key = visitor_key(raw_token);
    if let Ok(Some(value)) = state.cache.kernel_org_get(org_db, &key)
        && let Ok(json) = serde_json::from_slice::<serde_json::Value>(value.as_slice())
        && let Some(id) = json.get("id").and_then(|v| v.as_str())
    {
        return Ok(Some(visitor::Visitor {
            id: id.to_string(),
            linked_user: json
                .get("linked_user")
                .and_then(|v| v.as_str())
                .map(str::to_string),
        }));
    }
    let found = visitor::find(org, raw_token).await?;
    if let Some(found) = &found {
        let json = serde_json::json!({ "id": found.id, "linked_user": found.linked_user });
        if let Ok(bytes) = serde_json::to_vec(&json) {
            let _ = state
                .cache
                .kernel_org_set(org_db, &key, bytes, Some(VISITOR_WARM));
        }
    }
    Ok(found)
}

/// Record "seen now" at most once a minute per visitor instead of once per request.
async fn touch_visitor(
    state: &AppState,
    org_db: &str,
    org: &surrealdb::Surreal<surrealdb::engine::remote::ws::Client>,
    visitor_id: &str,
) -> Result<(), surrealdb::Error> {
    let key = format!("visitor_seen:{visitor_id}");
    if matches!(state.cache.kernel_org_get(org_db, &key), Ok(Some(_))) {
        return Ok(());
    }
    visitor::touch(org, visitor_id, None).await?;
    let _ = state
        .cache
        .kernel_org_set(org_db, &key, b"1".as_slice(), Some(VISITOR_SEEN_EVERY));
    Ok(())
}

fn throttle(request_limiter: &crate::access::ratelimit::RateLimiter, ip: Option<IpAddr>) -> Result<(), IdentityError> {
    if request_limiter.allow(ip) {
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
pub async fn ensure_visitor(
    state: &AppState,
    identity: &mut Identity,
) -> Result<(), IdentityError> {
    if identity.actor != Actor::Anonymous {
        return Ok(());
    }
    if !state.limits.visitor_limiter.allow(identity.client_ip) {
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
async fn existing_org(
    state: &AppState,
    candidates: &[String],
) -> Result<Option<String>, surrealdb::Error> {
    if candidates.is_empty() {
        return Ok(None);
    }
    let pick = |names: &[String]| {
        candidates
            .iter()
            .find(|candidate| names.iter().any(|name| name == *candidate))
            .cloned()
    };
    if let Ok(Some(names)) = state.cache.warm_get_org_databases() {
        if let Some(hit) = pick(&names) {
            return Ok(Some(hit));
        }
        // Warm list may lag a CLI create: reload from core and try again.
    }
    let names = super::org_databases::refresh(state).await?;
    Ok(pick(&names))
}

/// Whether any of `names` is an organization a developer has disabled. Only asked on the way to
/// refusing a request, so it reads core directly.
async fn is_disabled(state: &AppState, names: &[String]) -> Result<bool, surrealdb::Error> {
    if names.is_empty() {
        return Ok(false);
    }
    let core = state.core().await?;
    let disabled = crate::org_admin::disabled_db_names(&core).await?;
    Ok(names.iter().any(|name| disabled.contains(name)))
}

async fn is_member(
    state: &AppState,
    user_id: &str,
    org_db: &str,
) -> Result<bool, surrealdb::Error> {
    // Warm-cached memberships; invalidated whenever a member is invited, removed or restored.
    Ok(membership_db_names(state, user_id)
        .await?
        .iter()
        .any(|name| name == org_db))
}
