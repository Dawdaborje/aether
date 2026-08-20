use std::collections::HashMap;
use std::sync::{Arc, OnceLock};

use aether_auth_bridge::{AuthBridge, BridgeError};
use aether_core::application::settings::get_setting;
use aether_core::state::AppState;
use aether_orm::create_session;
use axum::{
    Json,
    extract::{Path, Query, State},
    http::{HeaderMap, StatusCode},
    response::{IntoResponse, Redirect},
};
use axum_extra::extract::CookieJar;
use serde::Deserialize;
use serde_json::{Value as JsonValue, json};
use surrealdb::types::{RecordId, SurrealValue};

use super::login::{methods_as_strings, org_from_headers};
use crate::bridges::{default_registry, resolve_bridge};
use crate::session_cookie::set_session_cookie;

fn registry() -> &'static HashMap<&'static str, Arc<dyn AuthBridge>> {
    static REG: OnceLock<HashMap<&'static str, Arc<dyn AuthBridge>>> = OnceLock::new();
    REG.get_or_init(default_registry)
}

#[derive(Debug, Deserialize)]
pub struct OAuthStartQuery {
    pub redirect_uri: Option<String>,
}

pub async fn oauth_start(
    State(state): State<AppState>,
    Path(provider): Path<String>,
    Query(query): Query<OAuthStartQuery>,
    headers: HeaderMap,
) -> Result<impl IntoResponse, (StatusCode, Json<JsonValue>)> {
    let org = org_from_headers(&state, &headers, None);
    ensure_method_enabled(&state, &provider, org.as_ref()).await?;

    let _ = state.use_core().await;
    let bridge = resolve_bridge(&state.db, registry(), &provider)
        .await
        .ok_or_else(|| {
            (
                StatusCode::NOT_IMPLEMENTED,
                Json(json!({ "error": format!("provider `{provider}` is not configured") })),
            )
        })?;

    let redirect_uri = query.redirect_uri.unwrap_or_else(|| {
        default_callback_uri(&headers, &provider)
    });
    let oauth_state = format!("aether_{}", rand_hex(16));

    match bridge.authorize_url(&oauth_state, &redirect_uri).await {
        Ok(url) => Ok(Redirect::temporary(url.as_str())),
        Err(BridgeError::NotConfigured(name)) => Err((
            StatusCode::NOT_IMPLEMENTED,
            Json(json!({ "error": format!("provider `{name}` is not configured") })),
        )),
        Err(err) => Err((
            StatusCode::BAD_GATEWAY,
            Json(json!({ "error": err.to_string() })),
        )),
    }
}

#[derive(Debug, Deserialize)]
pub struct OAuthCallbackQuery {
    pub code: Option<String>,
    pub state: Option<String>,
    pub error: Option<String>,
    pub redirect_uri: Option<String>,
}

pub async fn oauth_callback(
    State(state): State<AppState>,
    Path(provider): Path<String>,
    Query(query): Query<OAuthCallbackQuery>,
    jar: CookieJar,
    headers: HeaderMap,
) -> Result<(CookieJar, Json<JsonValue>), (StatusCode, Json<JsonValue>)> {
    if let Some(err) = query.error {
        return Err((
            StatusCode::BAD_REQUEST,
            Json(json!({ "error": err })),
        ));
    }
    let code = query.code.ok_or_else(|| {
        (
            StatusCode::BAD_REQUEST,
            Json(json!({ "error": "missing code" })),
        )
    })?;

    let org = org_from_headers(&state, &headers, None);
    ensure_method_enabled(&state, &provider, org.as_ref()).await?;

    let _ = state.use_core().await;
    let bridge = resolve_bridge(&state.db, registry(), &provider)
        .await
        .ok_or_else(|| {
            (
                StatusCode::NOT_IMPLEMENTED,
                Json(json!({ "error": format!("provider `{provider}` is not configured") })),
            )
        })?;

    let redirect_uri = query
        .redirect_uri
        .unwrap_or_else(|| default_callback_uri(&headers, &provider));

    let identity = bridge
        .exchange_code(&code, &redirect_uri)
        .await
        .map_err(|err| match err {
            BridgeError::NotConfigured(name) => (
                StatusCode::NOT_IMPLEMENTED,
                Json(json!({ "error": format!("provider `{name}` is not configured") })),
            ),
            other => (
                StatusCode::BAD_GATEWAY,
                Json(json!({ "error": other.to_string() })),
            ),
        })?;

    let user_id = upsert_oauth_user(&state, &identity.provider, &identity.provider_user_id, &identity)
        .await?;

    let org_db = org.as_ref().map(|o| o.db_name.clone());
    let session = create_session(&state.db, user_id, &provider, org_db, 24)
        .await
        .map_err(|err| {
            log::error!("oauth session: {err}");
            (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(json!({ "error": "failed to create session" })),
            )
        })?;

    let jar = set_session_cookie(jar, &session.raw_token);

    Ok((
        jar,
        Json(json!({
            "ok": true,
            "provider": provider,
            "state": query.state,
        })),
    ))
}

async fn ensure_method_enabled(
    state: &AppState,
    provider: &str,
    org: Option<&aether_core::tenancy::OrgRef>,
) -> Result<(), (StatusCode, Json<JsonValue>)> {
    let enabled = get_setting(state, "auth.enabled_methods", org)
        .await
        .ok()
        .flatten()
        .map(|s| methods_as_strings(&s.value))
        .unwrap_or_else(|| vec!["local".into()]);
    if enabled.iter().any(|m| m == provider) {
        Ok(())
    } else {
        Err((
            StatusCode::FORBIDDEN,
            Json(json!({ "error": format!("auth method `{provider}` is not enabled") })),
        ))
    }
}

fn default_callback_uri(headers: &HeaderMap, provider: &str) -> String {
    let host = headers
        .get(axum::http::header::HOST)
        .and_then(|v| v.to_str().ok())
        .unwrap_or("localhost:7890");
    let scheme = if host.contains("localhost") {
        "http"
    } else {
        "https"
    };
    format!("{scheme}://{host}/api/auth/oauth/{provider}/callback")
}

fn rand_hex(n: usize) -> String {
    use rand::RngExt;
    let mut bytes = vec![0u8; n];
    rand::rng().fill(&mut bytes);
    bytes.iter().map(|b| format!("{b:02x}")).collect()
}

#[derive(Debug, Deserialize, SurrealValue)]
struct IdRow {
    id: RecordId,
}

async fn upsert_oauth_user(
    state: &AppState,
    provider: &str,
    provider_user_id: &str,
    identity: &aether_auth_bridge::ExternalIdentity,
) -> Result<RecordId, (StatusCode, Json<JsonValue>)> {
    let mut existing = state
        .db
        .query(
            r#"
            SELECT user AS id FROM user_identities
            WHERE provider = $provider AND provider_user_id = $provider_user_id
            LIMIT 1;
            "#,
        )
        .bind(("provider", provider.to_string()))
        .bind(("provider_user_id", provider_user_id.to_string()))
        .await
        .map_err(|e| {
            log::error!("{e}");
            (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(json!({ "error": "database error" })),
            )
        })?
        .check()
        .map_err(|e| {
            log::error!("{e}");
            (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(json!({ "error": "database error" })),
            )
        })?;

    let rows: Vec<IdRow> = existing.take(0).unwrap_or_default();
    if let Some(row) = rows.into_iter().next() {
        return Ok(row.id);
    }

    // Create user + identity
    let email = identity.email.clone();
    let display = identity
        .display_name
        .clone()
        .or_else(|| email.clone())
        .unwrap_or_else(|| format!("{provider}:{provider_user_id}"));

    let created = state
        .db
        .query(
            r#"
            LET $u = (CREATE users SET
                email = $email,
                display_name = $display,
                is_active = true,
                is_email_verified = true
            RETURN AFTER)[0].id;
            CREATE user_identities SET
                user = $u,
                provider = $provider,
                provider_user_id = $provider_user_id,
                email = $email;
            "#,
        )
        .bind(("email", email))
        .bind(("display", display))
        .bind(("provider", provider.to_string()))
        .bind(("provider_user_id", provider_user_id.to_string()))
        .await
        .map_err(|e| {
            log::error!("{e}");
            (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(json!({ "error": "failed to create user" })),
            )
        })?;

    let _: Result<(), _> = created.check().map(|_| ());

    let mut lookup = state
        .db
        .query(
            r#"
            SELECT user AS id FROM user_identities
            WHERE provider = $provider AND provider_user_id = $provider_user_id
            LIMIT 1;
            "#,
        )
        .bind(("provider", provider.to_string()))
        .bind(("provider_user_id", provider_user_id.to_string()))
        .await
        .map_err(|e| {
            log::error!("{e}");
            (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(json!({ "error": "database error" })),
            )
        })?
        .check()
        .map_err(|e| {
            log::error!("{e}");
            (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(json!({ "error": "database error" })),
            )
        })?;

    let found: Vec<IdRow> = lookup.take(0).unwrap_or_default();
    found
        .into_iter()
        .next()
        .map(|r| r.id)
        .ok_or_else(|| {
            (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(json!({ "error": "failed to resolve oauth user" })),
            )
        })
}
