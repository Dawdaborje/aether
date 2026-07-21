use aether_core::state::AppState;
use aether_orm::revoke_session_by_token;
use axum::{Json, extract::State, http::StatusCode};
use axum_extra::extract::CookieJar;
use serde_json::{Value as JsonValue, json};

use crate::session_cookie::{clear_session_cookie, read_session_token};

pub async fn logout(
    State(state): State<AppState>,
    jar: CookieJar,
) -> Result<(CookieJar, Json<JsonValue>), (StatusCode, Json<JsonValue>)> {
    if let Some(token) = read_session_token(&jar) {
        let _ = state.use_core().await;
        let _ = revoke_session_by_token(&state.db, &token).await;
    }
    let jar = clear_session_cookie(jar);
    Ok((jar, Json(json!({ "ok": true }))))
}
