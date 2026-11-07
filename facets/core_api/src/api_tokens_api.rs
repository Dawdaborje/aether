//! Managing API tokens: a person makes, lists and revokes their own (see `access::api_token` and
//! `docs/architecture/api_and_webhooks.md`).
//!
//! `POST   /api/ui/tokens { name, scopes, expires_days? }` → `{ id, token, ... }`; the token is in this
//! answer only and cannot be shown again.
//! `GET    /api/ui/tokens` → the caller's tokens in this organization (never the token itself).
//! `DELETE /api/ui/tokens/{id}` stops one from working.
//!
//! Only a person signed in with a session can do this; a token cannot make more tokens.

use std::net::SocketAddr;

use axum::{
    Extension, Json, Router,
    extract::{ConnectInfo, Path, State},
    http::{HeaderMap, StatusCode, Uri},
    response::{IntoResponse, Response},
    routing::{delete, get},
};
use serde::Deserialize;
use serde_json::{Value, json};

use aether_core::access::api_token::{self, NewToken, Scopes, TokenError};
use aether_core::access::audit::Actor;
use aether_core::access::identity::{Identity, identify};
use aether_core::state::AppState;

/// Days a token lasts when the person does not say.
const DEFAULT_DAYS: u32 = 90;
/// The longest a token may last unless the organization sets another (`api.token_max_days`; 0: no limit).
const DEFAULT_MAX_DAYS: u64 = 365;

pub fn routes() -> Router<AppState> {
    Router::new()
        .route("/api/ui/tokens", get(list).post(create))
        .route("/api/ui/tokens/{id}", delete(revoke))
}

fn failure(status: StatusCode, message: &str) -> Response {
    (status, Json(json!({ "error": message }))).into_response()
}

/// The signed-in person, and the organization the request is for.
async fn person(
    state: &AppState,
    peer: Option<Extension<ConnectInfo<SocketAddr>>>,
    headers: &HeaderMap,
    uri: &Uri,
) -> Result<(Identity, String), Response> {
    let peer_ip = peer.map(|Extension(ConnectInfo(address))| address.ip());
    let identity = identify(state, headers, peer_ip, uri)
        .await
        .map_err(|error| {
            let (status, message) = error.http();
            failure(status, message)
        })?;
    match (&identity.actor, &identity.session) {
        (Actor::User(user), Some(_)) => {
            let user = user.clone();
            Ok((identity, user))
        }
        _ => Err(failure(
            StatusCode::UNAUTHORIZED,
            "sign in to manage API tokens",
        )),
    }
}

async fn setting(state: &AppState, org_db: &str, key: &str) -> Option<Value> {
    aether_core::application::settings::get_setting(
        state,
        key,
        Some(&aether_core::chatter::org_ref(org_db)),
    )
    .await
    .ok()
    .flatten()
    .map(|setting| setting.value)
}

fn token_failure(error: TokenError) -> Response {
    match error {
        TokenError::Invalid(message) => failure(StatusCode::UNPROCESSABLE_ENTITY, &message),
        TokenError::Database(error) => {
            log::error!("api tokens: {error}");
            failure(StatusCode::INTERNAL_SERVER_ERROR, "database error")
        }
    }
}

async fn list(
    State(state): State<AppState>,
    peer: Option<Extension<ConnectInfo<SocketAddr>>>,
    headers: HeaderMap,
    uri: Uri,
) -> Response {
    let (identity, user) = match person(&state, peer, &headers, &uri).await {
        Ok(found) => found,
        Err(response) => return response,
    };
    let core = match state.core().await {
        Ok(core) => core,
        Err(error) => {
            log::error!("api tokens: {error}");
            return failure(StatusCode::INTERNAL_SERVER_ERROR, "database error");
        }
    };
    match api_token::list(&core, &user, &identity.org_db).await {
        Ok(tokens) => Json(json!({ "tokens": tokens })).into_response(),
        Err(error) => token_failure(error),
    }
}

#[derive(Debug, Deserialize)]
struct CreateBody {
    name: String,
    scopes: Scopes,
    /// Days until it stops working; 0 for never (only where the organization allows it).
    #[serde(default)]
    expires_days: Option<u32>,
}

async fn create(
    State(state): State<AppState>,
    peer: Option<Extension<ConnectInfo<SocketAddr>>>,
    headers: HeaderMap,
    uri: Uri,
    Json(body): Json<CreateBody>,
) -> Response {
    let (identity, user) = match person(&state, peer, &headers, &uri).await {
        Ok(found) => found,
        Err(response) => return response,
    };
    if setting(&state, &identity.org_db, "api.enabled").await == Some(Value::Bool(false)) {
        return failure(StatusCode::FORBIDDEN, "API access is switched off");
    }
    let max = setting(&state, &identity.org_db, "api.token_max_days")
        .await
        .and_then(|v| v.as_u64())
        .unwrap_or(DEFAULT_MAX_DAYS);
    let days = match body.expires_days {
        None => Some(if max == 0 {
            DEFAULT_DAYS
        } else {
            DEFAULT_DAYS.min(u32::try_from(max).unwrap_or(u32::MAX))
        }),
        Some(0) if max == 0 => None,
        Some(0) => {
            return failure(
                StatusCode::UNPROCESSABLE_ENTITY,
                &format!("a token lasts at most {max} days here"),
            );
        }
        Some(days) if max > 0 && u64::from(days) > max => {
            return failure(
                StatusCode::UNPROCESSABLE_ENTITY,
                &format!("a token lasts at most {max} days here"),
            );
        }
        Some(days) => Some(days),
    };
    let core = match state.core().await {
        Ok(core) => core,
        Err(error) => {
            log::error!("api tokens: {error}");
            return failure(StatusCode::INTERNAL_SERVER_ERROR, "database error");
        }
    };
    match api_token::create(
        &core,
        NewToken {
            user: &user,
            org_db: &identity.org_db,
            name: &body.name,
            scopes: &body.scopes,
            expires_days: days,
        },
    )
    .await
    {
        Ok((id, token)) => (
            StatusCode::CREATED,
            Json(json!({
                "id": id,
                "token": token,
                "expires_days": days,
                "note": "This is the only time the token is shown. Keep it secret.",
            })),
        )
            .into_response(),
        Err(error) => token_failure(error),
    }
}

async fn revoke(
    State(state): State<AppState>,
    peer: Option<Extension<ConnectInfo<SocketAddr>>>,
    Path(id): Path<String>,
    headers: HeaderMap,
    uri: Uri,
) -> Response {
    let (identity, user) = match person(&state, peer, &headers, &uri).await {
        Ok(found) => found,
        Err(response) => return response,
    };
    let core = match state.core().await {
        Ok(core) => core,
        Err(error) => {
            log::error!("api tokens: {error}");
            return failure(StatusCode::INTERNAL_SERVER_ERROR, "database error");
        }
    };
    match api_token::revoke(&core, &id, &user, &identity.org_db).await {
        Ok(true) => Json(json!({ "revoked": id })).into_response(),
        Ok(false) => failure(StatusCode::NOT_FOUND, "no such token"),
        Err(error) => token_failure(error),
    }
}
