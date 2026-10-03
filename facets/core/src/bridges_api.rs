//! `GET /api/ui/bridges`: the bridge catalog, for developers.
//!
//! Bridges are the first-party integrations (payments, messaging, identity, storage, …)
//! seeded into the core database by `aether --init` / `--seed`. This lists them with
//! whether each is enabled globally.

use axum::{
    Json, Router,
    extract::State,
    http::HeaderMap,
    response::{IntoResponse, Response},
    routing::get,
};
use serde::{Deserialize, Serialize};
use serde_json::json;
use surrealdb::types::SurrealValue;

use crate::application::settings_api::require_session;
use crate::state::AppState;

pub fn routes() -> Router<AppState> {
    Router::new().route("/api/ui/bridges", get(list_bridges))
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
            (
                axum::http::StatusCode::INTERNAL_SERVER_ERROR,
                Json(json!({ "error": "database error" })),
            )
                .into_response()
        }
    }
}
