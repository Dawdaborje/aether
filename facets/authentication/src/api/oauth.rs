use std::collections::HashMap;
use std::sync::{Arc, OnceLock};

use aether_auth_bridge::{AuthBridge, BridgeError};
use aether_core::application::settings::get_setting;
use aether_core::state::{AppState, Databases};
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
use crate::csrf::{
    clear_oauth_state_cookie, cookies_secure, new_token, read_oauth_state, rotate_csrf_cookie,
    set_oauth_state_cookie, tokens_equal,
};
use crate::session_cookie::set_session_cookie;

fn registry() -> &'static HashMap<&'static str, Arc<dyn AuthBridge>> {
    static REG: OnceLock<HashMap<&'static str, Arc<dyn AuthBridge>>> = OnceLock::new();
    REG.get_or_init(default_registry)
}

/// Where the provider sends the person back: this server's own callback unless the caller names
/// another address, which must be on this server's origin or on one it trusts (`csrf_trusted_origins`,
/// where a separately served web app lives). Anything else is refused, so a crafted sign-in link cannot
/// make the provider hand the authorization code to someone else's address.
fn chosen_redirect(
    state: &AppState,
    headers: &HeaderMap,
    provider: &str,
    wanted: Option<String>,
) -> Result<String, (StatusCode, Json<JsonValue>)> {
    let Some(uri) = wanted else {
        return Ok(default_callback_uri(headers, provider));
    };
    let trusted = state
        .config
        .server
        .as_ref()
        .map(|server| server.csrf_trusted_origins.as_slice())
        .unwrap_or_default();
    if redirect_allowed(headers, &uri, trusted) {
        Ok(uri)
    } else {
        Err((
            StatusCode::BAD_REQUEST,
            Json(json!({ "error": "redirect_uri is not an address this server trusts" })),
        ))
    }
}

fn redirect_allowed(headers: &HeaderMap, uri: &str, trusted: &[String]) -> bool {
    let own = crate::csrf::request_origin(headers, None, &[]);
    crate::csrf::origin_from_referer(uri)
        .is_some_and(|origin| crate::csrf::origin_allowed(&origin, own.as_deref(), trusted))
}

#[cfg(test)]
mod redirect_tests {
    use super::*;

    fn host(name: &str) -> HeaderMap {
        let mut headers = HeaderMap::new();
        headers.insert("host", name.parse().unwrap());
        headers
    }

    #[test]
    fn only_this_server_or_a_trusted_origin_may_receive_the_code() {
        let trusted = vec!["http://localhost:5173".to_string()];
        let headers = host("aether.example.com");
        assert!(redirect_allowed(&headers, "https://aether.example.com/api/auth/oauth/google/callback", &trusted));
        assert!(redirect_allowed(&headers, "http://localhost:5173/login/callback", &trusted));
        assert!(!redirect_allowed(&headers, "https://evil.example.net/steal", &trusted));
        assert!(!redirect_allowed(&headers, "https://aether.example.com.evil.net/x", &trusted));
        assert!(!redirect_allowed(&headers, "javascript:alert(1)", &trusted));
        assert!(!redirect_allowed(&headers, "not a url", &trusted));
    }
}

#[derive(Debug, Deserialize)]
pub struct OAuthStartQuery {
    pub redirect_uri: Option<String>,
}

pub async fn oauth_start(
    State(state): State<AppState>,
    Path(provider): Path<String>,
    Query(query): Query<OAuthStartQuery>,
    jar: CookieJar,
    headers: HeaderMap,
) -> Result<impl IntoResponse, (StatusCode, Json<JsonValue>)> {
    let org = org_from_headers(&state.config, &headers, None);
    ensure_method_enabled(&state, &provider, org.as_ref()).await?;

    let core = state.core().await.map_err(|err| {
        log::error!("oauth core database: {err}");
        (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(json!({ "error": "database error" })),
        )
    })?;
    let bridge = resolve_bridge(&core, registry(), &provider)
        .await
        .ok_or_else(|| {
            (
                StatusCode::NOT_IMPLEMENTED,
                Json(json!({ "error": format!("provider `{provider}` is not configured") })),
            )
        })?;

    let redirect_uri = chosen_redirect(&state, &headers, &provider, query.redirect_uri)?;
    let oauth_state = new_token();
    let jar = set_oauth_state_cookie(jar, &oauth_state, cookies_secure(&state));

    match bridge.authorize_url(&oauth_state, &redirect_uri).await {
        Ok(url) => Ok((jar, Redirect::temporary(url.as_str()))),
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
    let secure = cookies_secure(&state);

    if let Some(err) = query.error {
        return Err((StatusCode::BAD_REQUEST, Json(json!({ "error": err }))));
    }
    let code = query.code.ok_or_else(|| {
        (
            StatusCode::BAD_REQUEST,
            Json(json!({ "error": "missing code" })),
        )
    })?;
    let query_state = query.state.as_deref().unwrap_or("");
    let cookie_state = read_oauth_state(&jar).unwrap_or_default();
    if query_state.is_empty()
        || cookie_state.is_empty()
        || !tokens_equal(query_state, &cookie_state)
    {
        return Err((
            StatusCode::BAD_REQUEST,
            Json(json!({ "error": "invalid oauth state" })),
        ));
    }

    let org = org_from_headers(&state.config, &headers, None);
    ensure_method_enabled(&state, &provider, org.as_ref()).await?;

    let core = state.core().await.map_err(|err| {
        log::error!("oauth core database: {err}");
        (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(json!({ "error": "database error" })),
        )
    })?;
    let bridge = resolve_bridge(&core, registry(), &provider)
        .await
        .ok_or_else(|| {
            (
                StatusCode::NOT_IMPLEMENTED,
                Json(json!({ "error": format!("provider `{provider}` is not configured") })),
            )
        })?;

    let redirect_uri = chosen_redirect(&state, &headers, &provider, query.redirect_uri)?;

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

    let (user_id, created) = upsert_oauth_user(
        state.databases(),
        &identity.provider,
        &identity.provider_user_id,
        &identity,
    )
    .await?;

    // A single-organization installation can let a first-time sign-in join the organization.
    // Only on creation: a member an administrator removed must not be put back at each login.
    if created && state.config.tenancy.oauth_auto_join {
        let label = identity
            .display_name
            .clone()
            .or_else(|| identity.email.clone())
            .unwrap_or_else(|| identity.provider_user_id.clone());
        if let Err(error) = aether_core::org_admin::assign_user_by_id(
            &state.fresh_session(),
            &state.namespace,
            &aether_core::access::visitor::record_id_string(&user_id),
            &label,
            &state.config.tenancy.single_org_db_name,
        )
        .await
        {
            log::error!("oauth: could not add the new account to the organization: {error}");
        }
    }

    let org_db = org.as_ref().map(|o| o.db_name.clone());
    let session = create_session(&core, user_id, &provider, org_db, aether_orm::SESSION_IDLE_HOURS)
        .await
        .map_err(|err| {
            log::error!("oauth session: {err}");
            (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(json!({ "error": "failed to create session" })),
            )
        })?;
    let _ = aether_core::session_cache::session_from_token(&state.cache, &core, &session.raw_token)
        .await;

    let jar = clear_oauth_state_cookie(jar, secure);
    let jar = set_session_cookie(jar, &session.raw_token, secure);
    let jar = rotate_csrf_cookie(jar, secure);

    Ok((
        jar,
        Json(json!({
            "ok": true,
            "provider": provider,
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

#[derive(Debug, Deserialize, SurrealValue)]
struct IdRow {
    id: RecordId,
}

async fn upsert_oauth_user(
    state: &Databases,
    provider: &str,
    provider_user_id: &str,
    identity: &aether_auth_bridge::ExternalIdentity,
) -> Result<(RecordId, bool), (StatusCode, Json<JsonValue>)> {
    let core = state.core().await.map_err(|e| {
        log::error!("oauth core database: {e}");
        (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(json!({ "error": "database error" })),
        )
    })?;
    let mut existing = core
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
        return Ok((row.id, false));
    }

    // Create user + identity
    let email = identity.email.clone();
    let display = identity
        .display_name
        .clone()
        .or_else(|| email.clone())
        .unwrap_or_else(|| format!("{provider}:{provider_user_id}"));

    let created = core
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

    let mut lookup = core
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
    found.into_iter().next().map(|r| (r.id, true)).ok_or_else(|| {
        (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(json!({ "error": "failed to resolve oauth user" })),
        )
    })
}
