//! Desk HTTP API for the Mantle inspector (`GET /api/desk/mantle/stats`).

use axum::{
    Json, Router,
    extract::{Path, Query, State},
    http::HeaderMap,
    response::{IntoResponse, Response},
    routing::get,
};
use serde::{Deserialize, Serialize};
use serde_json::json;

use aether_core::application::settings_api::require_session;
use aether_core::cache::{HeatModel, HeatPlugin};
use aether_core::state::AppState;

pub fn routes() -> Router<AppState> {
    Router::new()
        .route("/api/desk/mantle/stats", get(stats))
        .route("/api/desk/mantle/orgs", get(org_list))
        .route("/api/desk/mantle/orgs/{org}", get(org_detail))
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
struct OrgSummary {
    org: String,
    bytes: u64,
    entries: u64,
}

#[derive(Debug, Deserialize)]
struct OrgListQuery {
    #[serde(default)]
    q: String,
    /// `bytes` (default), `entries` or `name`.
    #[serde(default)]
    sort: String,
    #[serde(default)]
    offset: usize,
    limit: Option<usize>,
}

const DEFAULT_PAGE: usize = 25;
const MAX_PAGE: usize = 200;

/// Every organization the cache knows (heat ledger plus the warm org list), sorted.
fn known_orgs(state: &AppState) -> Vec<String> {
    let mut names = state.cache.heat().known_orgs();
    if let Ok(Some(warm)) = state.cache.warm_get_org_databases() {
        names.extend(warm);
    }
    names.sort();
    names.dedup();
    names
}

/// One page of organizations with their cache size, filtered by `q` and sorted. Only the
/// totals are computed here (one pass over the rollups); the heat of an org is loaded when
/// its row is opened, so a platform with thousands of orgs stays cheap to list.
async fn org_list(
    State(state): State<AppState>,
    headers: HeaderMap,
    Query(query): Query<OrgListQuery>,
) -> Response {
    if let Err(response) = require_developer(&state, &headers).await {
        return response;
    }
    let needle = query.q.trim().to_lowercase();
    let totals = state.cache.stats().org_totals();
    let mut rows: Vec<OrgSummary> = known_orgs(&state)
        .into_iter()
        .filter(|org| needle.is_empty() || org.to_lowercase().contains(&needle))
        .map(|org| {
            let (bytes, entries) = totals.get(&org).copied().unwrap_or_default();
            OrgSummary { org, bytes, entries }
        })
        .collect();
    match query.sort.as_str() {
        "name" => {}
        "entries" => rows.sort_by(|a, b| b.entries.cmp(&a.entries).then_with(|| a.org.cmp(&b.org))),
        _ => rows.sort_by(|a, b| b.bytes.cmp(&a.bytes).then_with(|| a.org.cmp(&b.org))),
    }
    let total = rows.len();
    let limit = query.limit.unwrap_or(DEFAULT_PAGE).clamp(1, MAX_PAGE);
    let page: Vec<OrgSummary> = rows.into_iter().skip(query.offset).take(limit).collect();
    Json(json!({ "total": total, "offset": query.offset, "limit": limit, "orgs": page }))
        .into_response()
}

/// Heat of one organization: its hot plugins and models.
async fn org_detail(
    State(state): State<AppState>,
    headers: HeaderMap,
    Path(org): Path<String>,
) -> Response {
    if let Err(response) = require_developer(&state, &headers).await {
        return response;
    }
    state.cache.heat().ensure_loaded(&org);
    let (bytes, entries) = state.cache.stats().org_bytes(&org);
    let heat = state.cache.heat().snapshot(&org);
    let heat_updated_at = heat.as_ref().map(|h| h.updated_at.clone());
    let hot_plugins: Vec<HotPluginRow> = heat
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
    let hot_models: Vec<HotModelRow> = heat
        .as_ref()
        .map(|h| {
            h.models
                .iter()
                .take(25)
                .map(|m: &HeatModel| {
                    let kind = match m.kind.as_str() {
                        "row" => Some(aether_core::cache::CacheKind::Row),
                        "list" => Some(aether_core::cache::CacheKind::List),
                        "agg" => Some(aether_core::cache::CacheKind::Agg),
                        "meta" => Some(aether_core::cache::CacheKind::Meta),
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
    Json(json!({
        "org": org,
        "bytes": bytes,
        "entries": entries,
        "heat_updated_at": heat_updated_at,
        "hot_plugins": hot_plugins,
        "hot_models": hot_models,
    }))
    .into_response()
}

async fn stats(State(state): State<AppState>, headers: HeaderMap) -> Response {
    if let Err(response) = require_developer(&state, &headers).await {
        return response;
    }

    let snapshot = state.cache.mantle_snapshot();
    let tiers = state.cache.tier_presence();
    let org_count = known_orgs(&state).len();

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
        "org_count": org_count,
    }))
    .into_response()
}
