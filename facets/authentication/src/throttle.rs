//! Limits on sign-in attempts. Password, magic-link and OAuth routes are the ones a stranger can
//! hammer without any credential, so each client address gets a small budget per minute (counted
//! whether the attempt succeeds or not). A username that collects too many failed passwords is
//! also refused for the rest of the minute, from any address (see `api::login`).

use std::net::{IpAddr, SocketAddr};

use aether_core::access::identity::with_retry_after;
use aether_core::access::ip::client_ip;
use aether_core::state::AppState;
use axum::{
    Json,
    extract::{ConnectInfo, Request, State},
    http::StatusCode,
    middleware::Next,
    response::{IntoResponse, Response},
};
use serde_json::json;

/// The address a request counts against: the TCP peer, or the forwarded one when the peer is a
/// trusted proxy.
pub(crate) fn address_of(state: &AppState, request: &Request) -> Option<IpAddr> {
    let peer = request
        .extensions()
        .get::<ConnectInfo<SocketAddr>>()
        .map(|ConnectInfo(address)| address.ip());
    let trusted = state
        .config
        .server
        .as_ref()
        .map(|server| server.trusted_proxies.as_slice())
        .unwrap_or_default();
    client_ip(request.headers(), peer, trusted)
}

pub(crate) fn too_many() -> Response {
    with_retry_after(
        (
            StatusCode::TOO_MANY_REQUESTS,
            Json(json!({ "error": "too many attempts, try again in a minute" })),
        )
            .into_response(),
    )
}

/// Middleware for the sign-in routes: refuse once the address has spent its minute.
pub async fn limit_sign_in(State(state): State<AppState>, request: Request, next: Next) -> Response {
    let ip = address_of(&state, &request);
    if !state.limits.login_limiter.allow(ip) {
        log::warn!("sign-in rate limit: refusing {ip:?}");
        return too_many();
    }
    next.run(request).await
}
