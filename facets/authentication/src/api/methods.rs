use aether_core::application::settings::get_setting;
use aether_core::state::AppState;
use aether_core::tenancy::OrgRef;
use axum::{
    Json,
    extract::State,
    http::{HeaderMap, StatusCode},
};
use serde_json::{Value as JsonValue, json};

use super::login::{methods_as_strings, org_from_headers};
use crate::authz::OptionalAuthSession;

pub async fn methods(
    State(state): State<AppState>,
    headers: HeaderMap,
    OptionalAuthSession(session): OptionalAuthSession,
) -> Result<Json<JsonValue>, (StatusCode, Json<JsonValue>)> {
    let session_org = session
        .as_ref()
        .and_then(|s| s.org_database_id.as_ref())
        .map(|db| OrgRef {
            slug: db.trim_start_matches("org_").to_string(),
            db_name: db.clone(),
        });
    let org = org_from_headers(&state, &headers, session_org.as_ref());

    let primary = get_setting(&state, "auth.primary_method", org.as_ref())
        .await
        .ok()
        .flatten()
        .map(|s| s.value)
        .unwrap_or(json!("local"));

    let enabled = get_setting(&state, "auth.enabled_methods", org.as_ref())
        .await
        .ok()
        .flatten()
        .map(|s| methods_as_strings(&s.value))
        .unwrap_or_else(|| vec!["local".into()]);

    let allow_registration = get_setting(&state, "auth.allow_registration", org.as_ref())
        .await
        .ok()
        .flatten()
        .and_then(|s| s.value.as_bool())
        .unwrap_or(false);

    Ok(Json(json!({
        "primary_method": primary,
        "enabled_methods": enabled,
        "allow_registration": allow_registration,
        "org": org.map(|o| o.slug),
    })))
}
