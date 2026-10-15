//! Desk HTTP API for the Mantle inspector (`GET /api/desk/mantle/stats`).

use axum::{
    Json, Router,
    extract::State,
    http::HeaderMap,
    response::{IntoResponse, Response},
    routing::get,
};
use serde::Serialize;
use serde_json::json;

use crate::application::settings_api::require_session;
use crate::cache::{HeatModel, HeatPlugin};
use crate::state::AppState;

pub fn routes() -> Router<AppState> {
    Router::new().route("/api/desk/mantle/stats", get(stats))
}

async fn require_developer(state: &AppState, headers: &HeaderMap) -> Result<(), Response> {
    require_session(state, headers)
        .await
        .map_err(|(status, body)| (status, body).into_response())
}

#[derive(Debug, Serialize)]
struct HotPluginRow {
    plugin: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    app: Option<String>,
    score: f64,
    hits: u64,
    bytes: u64,
    entries: u64,
}

#[derive(Debug, Serialize)]
struct HotModelRow {
    plugin: String,
    model: String,
    kind: String,
    score: f64,
    hits: u64,
    bytes: u64,
    entries: u64,
}

#[derive(Debug, Serialize)]
struct OrgMantleRow {
    org: String,
    bytes: u64,
    entries: u64,
    #[serde(skip_serializing_if = "Option::is_none")]
    heat_updated_at: Option<String>,
    hot_plugins: Vec<HotPluginRow>,
    hot_models: Vec<HotModelRow>,
}

async fn stats(State(state): State<AppState>, headers: HeaderMap) -> Response {
    if let Err(response) = require_developer(&state, &headers).await {
        return response;
    }

    let snapshot = state.cache.mantle_snapshot();
    let tiers = state.cache.tier_presence();

    let mut org_names = state.cache.heat().known_orgs();
    if let Ok(Some(warm)) = state.cache.warm_get_org_databases() {
        for name in warm {
            if !org_names.iter().any(|existing| existing == &name) {
                org_names.push(name);
            }
        }
    }
    org_names.sort();
    org_names.dedup();

    let mut orgs = Vec::with_capacity(org_names.len());
    for org in org_names {
        state.cache.ensure_org_heat_loaded(&org);
        let (bytes, entries) = state.cache.stats().org_bytes(&org);
        let heat = state.cache.heat().snapshot(&org);
        let heat_updated_at = heat.as_ref().map(|h| h.updated_at.clone());
        let hot_plugins = heat
            .as_ref()
            .map(|h| {
                h.plugins
                    .iter()
                    .take(15)
                    .map(|p: &HeatPlugin| {
                        let (p_bytes, p_entries) = state
                            .cache
                            .stats()
                            .plugin_model_bytes(&org, &p.plugin, None, None);
                        HotPluginRow {
                            plugin: p.plugin.clone(),
                            app: None,
                            score: p.score,
                            hits: p.hits,
                            bytes: p_bytes,
                            entries: p_entries,
                        }
                    })
                    .collect()
            })
            .unwrap_or_default();
        let hot_models = heat
            .as_ref()
            .map(|h| {
                h.models
                    .iter()
                    .take(25)
                    .map(|m: &HeatModel| {
                        let kind = match m.kind.as_str() {
                            "row" => Some(crate::cache::CacheKind::Row),
                            "list" => Some(crate::cache::CacheKind::List),
                            "agg" => Some(crate::cache::CacheKind::Agg),
                            "meta" => Some(crate::cache::CacheKind::Meta),
                            _ => None,
                        };
                        let (m_bytes, m_entries) = state.cache.stats().plugin_model_bytes(
                            &org,
                            &m.plugin,
                            Some(&m.model),
                            kind,
                        );
                        HotModelRow {
                            plugin: m.plugin.clone(),
                            model: m.model.clone(),
                            kind: m.kind.clone(),
                            score: m.score,
                            hits: m.hits,
                            bytes: m_bytes,
                            entries: m_entries,
                        }
                    })
                    .collect()
            })
            .unwrap_or_default();
        orgs.push(OrgMantleRow {
            org,
            bytes,
            entries,
            heat_updated_at,
            hot_plugins,
            hot_models,
        });
    }

    Json(json!({
        "system": {
            "bytes_used": snapshot.bytes_used,
            "bytes_max": snapshot.bytes_max,
            "bytes_target": snapshot.bytes_target,
            "entries": snapshot.entries,
            "hits": snapshot.hits,
            "misses": snapshot.misses,
            "ghost_hits": snapshot.ghost_hits,
            "evictions": snapshot.evictions,
            "backend": snapshot.backend,
            "l2_redis": snapshot.l2_redis,
            "kernel_bytes": snapshot.kernel_bytes,
            "plugin_bytes": snapshot.plugin_bytes,
            "kind_bytes": snapshot.kind_bytes,
            "ghost_len": snapshot.ghost_len,
            "session_entries": snapshot.session_entries,
            "adaptive_p_millis": snapshot.adaptive_p_millis,
            "warmup": snapshot.warmup,
            "tiers": tiers,
        },
        "orgs": orgs,
    }))
    .into_response()
}
