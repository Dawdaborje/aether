use aether_core::state::AppState;
use aether_orm::revoke_session_by_token;
use axum::{Json, extract::State, http::StatusCode};
use axum_extra::extract::CookieJar;
use serde_json::{Value as JsonValue, json};

use crate::csrf::{cookies_secure, rotate_csrf_cookie};
use crate::session_cookie::{clear_session_cookie, read_session_token};

pub async fn logout(
    State(state): State<AppState>,
    jar: CookieJar,
) -> Result<(CookieJar, Json<JsonValue>), (StatusCode, Json<JsonValue>)> {
    if let Some(token) = read_session_token(&jar) {
        if let Ok(core) = state.core().await {
            let _ = revoke_session_by_token(&core, &token).await;
        }
    }
    let secure = cookies_secure(&state);
    let jar = clear_session_cookie(jar, secure);
    let jar = rotate_csrf_cookie(jar, secure);
    Ok((jar, Json(json!({ "ok": true }))))
}
