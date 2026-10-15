//! Public consume endpoint for magic links (auth purposes + plugin hand-off).

use aether_core::magic_link::{
    self, PURPOSE_ACCOUNT_INVITE, PURPOSE_LOGIN, PURPOSE_PASSWORD_RESET,
};
use aether_core::org_admin::{self, OrganizationError};
use aether_core::scheduler::queue::{self, NewJob};
use aether_core::state::AppState;
use aether_orm::{create_session, hash_password};
use axum::{Json, extract::State, http::StatusCode};
use axum_extra::extract::CookieJar;
use serde::Deserialize;
use serde_json::{Value as JsonValue, json};
use surrealdb::types::{RecordId, SurrealValue};

use crate::csrf::{cookies_secure, rotate_csrf_cookie};
use crate::session_cookie::set_session_cookie;

const MIN_PASSWORD: usize = 8;
const MAX_PASSWORD: usize = 128;

#[derive(Debug, Deserialize)]
pub struct ConsumeBody {
    pub token: String,
    pub new_password: Option<String>,
}

pub async fn consume(
    State(state): State<AppState>,
    jar: CookieJar,
    Json(body): Json<ConsumeBody>,
) -> Result<(CookieJar, Json<JsonValue>), (StatusCode, Json<JsonValue>)> {
    let token = body.token.trim();
    if token.is_empty() {
        return Err((
            StatusCode::BAD_REQUEST,
            Json(json!({ "error": "token is required" })),
        ));
    }
    let core = state.core().await.map_err(|err| {
        log::error!("magic-link consume core: {err}");
        (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(json!({ "error": "database error" })),
        )
    })?;

    let preview = magic_link::inspect(&core, &state.namespace, token)
        .await
        .map_err(|err| match err {
            magic_link::MagicLinkError::Invalid | magic_link::MagicLinkError::Refused(_) => (
                StatusCode::BAD_REQUEST,
                Json(json!({ "error": "This link is invalid, expired, or already used." })),
            ),
            magic_link::MagicLinkError::Db(error) => {
                log::error!("magic-link inspect: {error}");
                (
                    StatusCode::INTERNAL_SERVER_ERROR,
                    Json(json!({ "error": "database error" })),
                )
            }
        })?;
    if preview.purpose == PURPOSE_ACCOUNT_INVITE
        && preview.used_at.is_none()
        && preview.revoked_at.is_none()
    {
        let password = body.new_password.as_deref().unwrap_or("").trim();
        let len = password.chars().count();
        if !(MIN_PASSWORD..=MAX_PASSWORD).contains(&len) {
            return Err((
                StatusCode::BAD_REQUEST,
                Json(json!({
                    "error": format!("The password must be {MIN_PASSWORD} to {MAX_PASSWORD} characters.")
                })),
            ));
        }
        let email = preview.email.as_deref().unwrap_or("").trim();
        if email.is_empty() {
            return Err((
                StatusCode::BAD_REQUEST,
                Json(json!({ "error": "This invite has no email address." })),
            ));
        }
        let session = state.fresh_session();
        match org_admin::user_by_email(&session, &state.namespace, email).await {
            Ok(Some(_)) => {
                return Err((
                    StatusCode::BAD_REQUEST,
                    Json(
                        json!({ "error": "An account with this email already exists. Sign in instead." }),
                    ),
                ));
            }
            Ok(None) => {}
            Err(error) => {
                log::error!("magic-link invite lookup: {error}");
                return Err((
                    StatusCode::INTERNAL_SERVER_ERROR,
                    Json(json!({ "error": "database error" })),
                ));
            }
        }
    }

    let row = magic_link::consume(&core, &state.namespace, token)
        .await
        .map_err(|err| match err {
            magic_link::MagicLinkError::Invalid | magic_link::MagicLinkError::Refused(_) => (
                StatusCode::BAD_REQUEST,
                Json(json!({ "error": "This link is invalid, expired, or already used." })),
            ),
            magic_link::MagicLinkError::Db(error) => {
                log::error!("magic-link consume: {error}");
                (
                    StatusCode::INTERNAL_SERVER_ERROR,
                    Json(json!({ "error": "database error" })),
                )
            }
        })?;

    match row.purpose.as_str() {
        PURPOSE_PASSWORD_RESET => {
            let password = body.new_password.as_deref().unwrap_or("").trim();
            let len = password.chars().count();
            if !(MIN_PASSWORD..=MAX_PASSWORD).contains(&len) {
                return Err((
                    StatusCode::BAD_REQUEST,
                    Json(json!({
                        "error": format!("The password must be {MIN_PASSWORD} to {MAX_PASSWORD} characters.")
                    })),
                ));
            }
            let Some(user_id) = row.user_id.as_deref() else {
                return Err((
                    StatusCode::BAD_REQUEST,
                    Json(json!({ "error": "This reset link has no user." })),
                ));
            };
            let hashed = hash_password(password).map_err(|err| {
                log::error!("magic-link hash: {err}");
                (
                    StatusCode::INTERNAL_SERVER_ERROR,
                    Json(json!({ "error": "could not set password" })),
                )
            })?;
            let key = user_id.strip_prefix("users:").unwrap_or(user_id);
            core.query("UPDATE type::record('users', $key) SET hashed_password = $hashed;")
                .bind(("key", key.to_string()))
                .bind(("hashed", hashed))
                .await
                .and_then(|r| r.check())
                .map_err(|err| {
                    log::error!("magic-link set password: {err}");
                    (
                        StatusCode::INTERNAL_SERVER_ERROR,
                        Json(json!({ "error": "could not set password" })),
                    )
                })?;

            let record = RecordId::new("users", key);
            // Password change: drop every other session so stolen cookies die with the reset.
            let _ =
                aether_core::session_cache::revoke_all_for_user(&state.cache, &core, &record).await;
            let session = create_session(&core, record, "magic_link", row.org_db.clone(), 24)
                .await
                .map_err(|err| {
                    log::error!("magic-link session: {err}");
                    (
                        StatusCode::INTERNAL_SERVER_ERROR,
                        Json(json!({ "error": "failed to create session" })),
                    )
                })?;
            let _ = aether_core::session_cache::session_from_token(
                &state.cache,
                &core,
                &session.raw_token,
            )
            .await;
            let secure = cookies_secure(&state);
            let jar = set_session_cookie(jar, &session.raw_token, secure);
            let jar = rotate_csrf_cookie(jar, secure);
            Ok((
                jar,
                Json(json!({
                    "ok": true,
                    "purpose": row.purpose,
                    "path": row.path,
                })),
            ))
        }
        PURPOSE_ACCOUNT_INVITE => {
            let password = body.new_password.as_deref().unwrap_or("").trim();
            let email = row.email.as_deref().unwrap_or("").trim();
            let Some(org_db) = row
                .org_db
                .as_deref()
                .map(str::trim)
                .filter(|name| !name.is_empty())
            else {
                return Err((
                    StatusCode::BAD_REQUEST,
                    Json(json!({ "error": "This invite is not for an organization." })),
                ));
            };
            if email.is_empty() {
                return Err((
                    StatusCode::BAD_REQUEST,
                    Json(json!({ "error": "This invite has no email address." })),
                ));
            }
            let username = payload_text(row.payload.as_ref(), "username");
            let display_name = payload_text(row.payload.as_ref(), "display_name");
            let session = state.fresh_session();
            let user_id = match org_admin::accept_account_invite(
                &session,
                &state.namespace,
                org_db,
                email,
                username.as_deref(),
                display_name.as_deref(),
                password,
            )
            .await
            {
                Ok(user_id) => user_id,
                Err(OrganizationError::AccountAlreadyExists) => {
                    return Err((
                        StatusCode::BAD_REQUEST,
                        Json(
                            json!({ "error": "An account with this email already exists. Sign in instead." }),
                        ),
                    ));
                }
                Err(OrganizationError::MissingUserCredentials) => {
                    return Err((
                        StatusCode::BAD_REQUEST,
                        Json(json!({
                            "error": format!("The password must be {MIN_PASSWORD} to {MAX_PASSWORD} characters.")
                        })),
                    ));
                }
                Err(error) => {
                    log::error!("magic-link invite accept: {error}");
                    return Err((
                        StatusCode::INTERNAL_SERVER_ERROR,
                        Json(json!({ "error": "could not create the account" })),
                    ));
                }
            };
            let _ = state.cache.warm_invalidate_memberships(&user_id);
            let _ = state.cache.warm_invalidate_roles(org_db, &user_id);
            let key = user_id.strip_prefix("users:").unwrap_or(&user_id);
            let record = RecordId::new("users", key);
            let session = create_session(&core, record, "magic_link", Some(org_db.to_string()), 24)
                .await
                .map_err(|err| {
                    log::error!("magic-link session: {err}");
                    (
                        StatusCode::INTERNAL_SERVER_ERROR,
                        Json(json!({ "error": "failed to create session" })),
                    )
                })?;
            let _ = aether_core::session_cache::session_from_token(
                &state.cache,
                &core,
                &session.raw_token,
            )
            .await;
            let secure = cookies_secure(&state);
            let jar = set_session_cookie(jar, &session.raw_token, secure);
            let jar = rotate_csrf_cookie(jar, secure);
            Ok((
                jar,
                Json(json!({
                    "ok": true,
                    "purpose": row.purpose,
                    "path": row.path,
                })),
            ))
        }
        PURPOSE_LOGIN => {
            let Some(user_id) = row.user_id.as_deref() else {
                return Err((
                    StatusCode::BAD_REQUEST,
                    Json(json!({ "error": "This login link has no user." })),
                ));
            };
            let key = user_id.strip_prefix("users:").unwrap_or(user_id);
            let record = RecordId::new("users", key);
            let session = create_session(&core, record, "magic_link", row.org_db.clone(), 24)
                .await
                .map_err(|err| {
                    log::error!("magic-link session: {err}");
                    (
                        StatusCode::INTERNAL_SERVER_ERROR,
                        Json(json!({ "error": "failed to create session" })),
                    )
                })?;
            let _ = aether_core::session_cache::session_from_token(
                &state.cache,
                &core,
                &session.raw_token,
            )
            .await;
            let secure = cookies_secure(&state);
            let jar = set_session_cookie(jar, &session.raw_token, secure);
            let jar = rotate_csrf_cookie(jar, secure);
            Ok((
                jar,
                Json(json!({
                    "ok": true,
                    "purpose": row.purpose,
                    "path": row.path,
                })),
            ))
        }
        _ => {
            // Plugin purposes: queue jobs for subscribers of `<plugin>.magic_link_consumed`.
            if let (Some(org_db), Some(plugin)) = (row.org_db.as_deref(), row.plugin.as_deref()) {
                if let Ok(org) = state.org(org_db).await {
                    let full = format!("{plugin}.magic_link_consumed");
                    #[derive(Debug, Deserialize, SurrealValue)]
                    struct Subscriber {
                        plugin: String,
                        function: String,
                        queue: String,
                        max_attempts: i64,
                    }
                    if let Ok(mut response) = org
                        .query(
                            "SELECT plugin, function_name AS function, queue, max_attempts \
                             FROM event_subscriptions WHERE event = $event AND enabled = true;",
                        )
                        .bind(("event", full.clone()))
                        .await
                        .and_then(|r| r.check())
                    {
                        let subscribers: Vec<Subscriber> = response.take(0).unwrap_or_default();
                        for subscriber in subscribers {
                            let mut job = NewJob::new("plugin", subscriber.queue);
                            job.plugin = Some(subscriber.plugin);
                            job.function = Some(subscriber.function);
                            job.payload = Some(json!({
                                "event": full,
                                "source": plugin,
                                "payload": {
                                    "purpose": row.purpose,
                                    "payload": row.payload,
                                    "user_id": row.user_id,
                                    "email": row.email,
                                    "org_db": row.org_db,
                                },
                                "emitted_by": null,
                                "depth": 1,
                            }));
                            job.max_attempts = subscriber.max_attempts;
                            job.enqueued_by = Some("system:magic_link".into());
                            let _ = queue::enqueue(&org, job).await;
                        }
                        state.scheduler.wake(org_db);
                    }
                }
            }
            Ok((
                jar,
                Json(json!({
                    "ok": true,
                    "purpose": row.purpose,
                    "path": row.path,
                    "payload": row.payload,
                    "plugin": row.plugin,
                })),
            ))
        }
    }
}

fn payload_text(payload: Option<&JsonValue>, key: &str) -> Option<String> {
    payload
        .and_then(|value| value.get(key))
        .and_then(JsonValue::as_str)
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .map(str::to_string)
}
