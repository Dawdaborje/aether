use aether_core::state::AppState;
use aether_orm::{ValidSession, find_session_by_token};
use axum::{
    extract::FromRequestParts,
    http::{StatusCode, request::Parts},
};
use axum_extra::extract::CookieJar;

use crate::session_cookie::read_session_token;

#[derive(Debug, Clone)]
pub struct AuthSession(pub ValidSession);

#[derive(Debug, Clone)]
pub struct OptionalAuthSession(pub Option<ValidSession>);

impl FromRequestParts<AppState> for AuthSession {
    type Rejection = (StatusCode, &'static str);

    async fn from_request_parts(
        parts: &mut Parts,
        state: &AppState,
    ) -> Result<Self, Self::Rejection> {
        let jar = CookieJar::from_headers(&parts.headers);
        let Some(token) = read_session_token(&jar) else {
            return Err((StatusCode::UNAUTHORIZED, "not authenticated"));
        };

        let core = state.core().await.map_err(|err| {
            log::error!("auth session core database: {err}");
            (StatusCode::INTERNAL_SERVER_ERROR, "database error")
        })?;

        match find_session_by_token(&core, &token).await {
            Ok(Some(session)) if session.user.is_active => Ok(AuthSession(session)),
            Ok(Some(_)) => Err((StatusCode::FORBIDDEN, "user inactive")),
            Ok(None) => Err((StatusCode::UNAUTHORIZED, "invalid session")),
            Err(err) => {
                log::error!("session lookup failed: {err}");
                Err((StatusCode::INTERNAL_SERVER_ERROR, "database error"))
            }
        }
    }
}

impl FromRequestParts<AppState> for OptionalAuthSession {
    type Rejection = (StatusCode, &'static str);

    async fn from_request_parts(
        parts: &mut Parts,
        state: &AppState,
    ) -> Result<Self, Self::Rejection> {
        match AuthSession::from_request_parts(parts, state).await {
            Ok(AuthSession(s)) => Ok(OptionalAuthSession(Some(s))),
            Err((StatusCode::UNAUTHORIZED, _)) => Ok(OptionalAuthSession(None)),
            Err(other) => Err(other),
        }
    }
}

pub fn require_superuser(session: &ValidSession) -> Result<(), (StatusCode, &'static str)> {
    if session.user.is_super_user {
        Ok(())
    } else {
        Err((StatusCode::FORBIDDEN, "developer access required"))
    }
}

/// Platform permission check. Developers (superusers) always pass.
/// Full RBAC graph resolution is deferred — non-developers are denied for now
/// unless we later load role→permission edges.
pub async fn require_permission(
    _state: &AppState,
    session: &ValidSession,
    _permission_key: &str,
) -> Result<(), (StatusCode, &'static str)> {
    if session.user.is_super_user {
        return Ok(());
    }
    // Stub: allow authenticated users for read-ish paths later; deny privileged keys.
    Err((StatusCode::FORBIDDEN, "permission denied"))
}
