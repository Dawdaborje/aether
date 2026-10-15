//! Bridge catalog API for developers (`/api/ui/bridges`).
//!
//! Bridges are first-party integrations seeded into the core database. This
//! lists them and lets a developer enable or disable each one globally.

use axum::{
    Json, Router,
    extract::{Path, State},
    http::HeaderMap,
    response::{IntoResponse, Response},
    routing::{get, patch},
};
use serde::{Deserialize, Serialize};
use serde_json::json;
use surrealdb::types::SurrealValue;

use crate::application::settings_api::require_session;
use crate::state::AppState;

pub fn routes() -> Router<AppState> {
    Router::new()
        .route("/api/ui/bridges", get(list_bridges))
        .route("/api/ui/bridges/{feature_key}", patch(set_bridge_enabled))
}

#[derive(Debug, Deserialize, Serialize, SurrealValue)]
struct BridgeRow {
    feature_key: String,
    name: String,
    label: String,
    description: Option<String>,
    category: String,
    version: Option<String>,
    is_builtin: bool,
    enabled_globally: bool,
}

#[derive(Debug, Deserialize)]
struct EnableBody {
    enabled: bool,
}

fn failure(status: axum::http::StatusCode, message: &str) -> Response {
    (status, Json(json!({ "error": message }))).into_response()
}

async fn list_bridges(State(state): State<AppState>, headers: HeaderMap) -> Response {
    if let Err((status, body)) = require_session(&state, &headers).await {
        return (status, body).into_response();
    }
    let rows = async {
        let core = state.core().await?;
        let mut response = core
            .query(
                "SELECT feature_key, name, label, description, category, version, is_builtin, enabled_globally \
                 FROM bridges ORDER BY category, name;",
            )
            .await?
            .check()?;
        response.take::<Vec<BridgeRow>>(0)
    }
    .await;
    match rows {
        Ok(bridges) => Json(json!({ "bridges": bridges })).into_response(),
        Err(error) => {
            log::error!("bridge catalog: {error}");
            failure(
                axum::http::StatusCode::INTERNAL_SERVER_ERROR,
                "database error",
            )
        }
    }
}

async fn set_bridge_enabled(
    State(state): State<AppState>,
    headers: HeaderMap,
    Path(feature_key): Path<String>,
    Json(body): Json<EnableBody>,
) -> Response {
    if let Err((status, body)) = require_session(&state, &headers).await {
        return (status, body).into_response();
    }
    if feature_key.trim().is_empty() {
        return failure(axum::http::StatusCode::BAD_REQUEST, "missing feature key");
    }

    let updated = async {
        let core = state.core().await?;
        let mut response = core
            .query(
                "UPDATE bridges SET enabled_globally = $enabled, date_updated = time::now() \
                 WHERE feature_key = $feature_key \
                 RETURN AFTER;",
            )
            .bind(("feature_key", feature_key.clone()))
            .bind(("enabled", body.enabled))
            .await?
            .check()?;
        response.take::<Vec<BridgeRow>>(0)
    }
    .await;

    match updated {
        Ok(rows) => match rows.into_iter().next() {
            Some(bridge) => {
                if bridge.enabled_globally {
                    let core = match state.core().await {
                        Ok(core) => core,
                        Err(error) => {
                            log::error!("bridge settings for `{}`: {error}", feature_key);
                            return failure(
                                axum::http::StatusCode::INTERNAL_SERVER_ERROR,
                                "database error",
                            );
                        }
                    };
                    if let Err(error) = crate::application::bridge_settings::ensure_bridge_settings(
                        &core,
                        &bridge.feature_key,
                    )
                    .await
                    {
                        log::error!("bridge settings for `{}`: {error}", bridge.feature_key);
                        return failure(
                            axum::http::StatusCode::INTERNAL_SERVER_ERROR,
                            "could not create bridge settings",
                        );
                    }
                }
                crate::application::bridge_settings::invalidate_settings_catalog(&state).await;
                log::info!(
                    "bridge `{}` globally {}",
                    bridge.feature_key,
                    if bridge.enabled_globally {
                        "enabled"
                    } else {
                        "disabled"
                    }
                );
                Json(json!({ "bridge": bridge })).into_response()
            }
            None => failure(axum::http::StatusCode::NOT_FOUND, "bridge not found"),
        },
        Err(error) => {
            log::error!("bridge enable `{}`: {error}", feature_key);
            failure(
                axum::http::StatusCode::INTERNAL_SERVER_ERROR,
                "database error",
            )
        }
    }
}
