//! Desk HTTP API for the system health page (`GET /api/desk/health`).
//!
//! One call gathers the facts an operator wants when something feels wrong: is the database
//! answering, is a scheduler running, are plugins waiting to compile, is the cache full. Each
//! area gets a status (`ok`, `warn`, `error`) and a sentence, so the page does not have to
//! decide what is worrying.

use std::sync::LazyLock;
use std::time::Instant;

use axum::{
    Json, Router,
    extract::State,
    http::HeaderMap,
    response::{IntoResponse, Response},
    routing::get,
};
use serde::Serialize;
use serde_json::json;

use aether_core::application::settings_api::require_session;
use aether_core::scheduler::nodes;
use aether_core::state::AppState;

/// When this process started answering requests; set the first time the routes are built.
static STARTED: LazyLock<Instant> = LazyLock::new(Instant::now);

pub fn routes() -> Router<AppState> {
    LazyLock::force(&STARTED);
    Router::new().route("/api/desk/health", get(health))
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "lowercase")]
pub enum Level {
    Ok,
    Warn,
    Error,
}

#[derive(Debug, Clone, Serialize)]
pub struct Check {
    /// Stable name for the area: `database`, `scheduler`, `plugins`, `cache`, `mode`.
    pub id: &'static str,
    pub label: &'static str,
    pub level: Level,
    pub detail: String,
}

/// What was measured; [`evaluate`] turns it into checks.
#[derive(Debug, Clone, Default)]
pub struct Facts {
    pub database_error: Option<String>,
    pub database_millis: u64,
    pub scheduler_nodes: usize,
    pub scheduler_paused: bool,
    pub plugins_waiting: u32,
    pub plugins_timeouts: u64,
    pub plugins_queue_full: u64,
    pub plugins_refused: u64,
    pub cache_used: u64,
    pub cache_max: u64,
    pub development_mode: bool,
}

/// A database that answers slower than this is worth a look.
const SLOW_DATABASE_MILLIS: u64 = 250;
/// The cache is "nearly full" above this share of its budget.
const CACHE_FULL_PERCENT: u64 = 90;

pub fn evaluate(facts: &Facts) -> Vec<Check> {
    let mut checks = Vec::new();

    checks.push(match &facts.database_error {
        Some(error) => Check {
            id: "database",
            label: "Database",
            level: Level::Error,
            detail: format!("The database did not answer: {error}"),
        },
        None if facts.database_millis >= SLOW_DATABASE_MILLIS => Check {
            id: "database",
            label: "Database",
            level: Level::Warn,
            detail: format!(
                "Answering slowly: {} ms for a simple query (over {SLOW_DATABASE_MILLIS} ms).",
                facts.database_millis
            ),
        },
        None => Check {
            id: "database",
            label: "Database",
            level: Level::Ok,
            detail: format!("Answering in {} ms.", facts.database_millis),
        },
    });

    checks.push(if facts.scheduler_paused {
        Check {
            id: "scheduler",
            label: "Scheduler",
            level: Level::Warn,
            detail: "The scheduler is paused: schedules and queued jobs are not running.".into(),
        }
    } else if facts.scheduler_nodes == 0 {
        Check {
            id: "scheduler",
            label: "Scheduler",
            level: Level::Error,
            detail: "No scheduler has reported in during the last 30 seconds, so schedules and \
                     queued jobs are not running."
                .into(),
        }
    } else {
        Check {
            id: "scheduler",
            label: "Scheduler",
            level: Level::Ok,
            detail: format!(
                "{} scheduler node{} reporting in.",
                facts.scheduler_nodes,
                if facts.scheduler_nodes == 1 { "" } else { "s" }
            ),
        }
    });

    let trouble = facts.plugins_timeouts + facts.plugins_queue_full + facts.plugins_refused;
    checks.push(if trouble > 0 {
        Check {
            id: "plugins",
            label: "Plugin runtime",
            level: Level::Warn,
            detail: format!(
                "Since start-up: {} timed out waiting to compile, {} turned away because the queue was full, \
                 {} refused. {} waiting now.",
                facts.plugins_timeouts,
                facts.plugins_queue_full,
                facts.plugins_refused,
                facts.plugins_waiting
            ),
        }
    } else if facts.plugins_waiting > 0 {
        Check {
            id: "plugins",
            label: "Plugin runtime",
            level: Level::Ok,
            detail: format!("{} plugin(s) waiting to compile.", facts.plugins_waiting),
        }
    } else {
        Check {
            id: "plugins",
            label: "Plugin runtime",
            level: Level::Ok,
            detail: "No plugins waiting to compile.".into(),
        }
    });

    let percent = if facts.cache_max == 0 {
        0
    } else {
        facts.cache_used.saturating_mul(100) / facts.cache_max
    };
    checks.push(if percent >= CACHE_FULL_PERCENT {
        Check {
            id: "cache",
            label: "Cache (Mantle)",
            level: Level::Warn,
            detail: format!("{percent}% of the cache budget is in use; entries are being evicted."),
        }
    } else {
        Check {
            id: "cache",
            label: "Cache (Mantle)",
            level: Level::Ok,
            detail: format!("{percent}% of the cache budget in use."),
        }
    });

    checks.push(if facts.development_mode {
        Check {
            id: "mode",
            label: "Mode",
            level: Level::Warn,
            detail: "Development mode is on: session cookies are sent without the Secure flag. \
                     Turn it off outside your own machine."
                .into(),
        }
    } else {
        Check {
            id: "mode",
            label: "Mode",
            level: Level::Ok,
            detail: "Production mode.".into(),
        }
    });

    checks
}

/// The worst level among the checks.
pub fn overall(checks: &[Check]) -> Level {
    if checks.iter().any(|c| c.level == Level::Error) {
        Level::Error
    } else if checks.iter().any(|c| c.level == Level::Warn) {
        Level::Warn
    } else {
        Level::Ok
    }
}

async fn health(State(state): State<AppState>, headers: HeaderMap) -> Response {
    if let Err((status, body)) = require_session(&state, &headers).await {
        return (status, body).into_response();
    }

    // Database: a trivial query on the core database, timed.
    let started = Instant::now();
    let mut database_error = None;
    let mut nodes_alive = Vec::new();
    match state.databases().core().await {
        Ok(core) => {
            match core.query("RETURN 1;").await.and_then(|r| r.check()) {
                Ok(_) => {}
                Err(error) => database_error = Some(error.to_string()),
            }
            if database_error.is_none() {
                nodes_alive = nodes::alive(&core).await.unwrap_or_default();
            }
        }
        Err(error) => database_error = Some(error.to_string()),
    }
    let database_millis = started.elapsed().as_millis() as u64;
    let organizations = aether_core::access::org_databases::list_cached(&state)
        .await
        .map(|names| names.len())
        .ok();

    let runtime = state.plugin_runtime.stats().await;
    let cache = state.cache.mantle_snapshot();
    let development_mode = state
        .config
        .configuration
        .as_ref()
        .is_some_and(|core| core.is_development_mode);

    let facts = Facts {
        database_error: database_error.clone(),
        database_millis,
        scheduler_nodes: nodes_alive.len(),
        scheduler_paused: state.scheduler.is_paused(),
        plugins_waiting: runtime.waiting_to_compile,
        plugins_timeouts: runtime.timeouts,
        plugins_queue_full: runtime.queue_full,
        plugins_refused: runtime.refused,
        cache_used: cache.bytes_used,
        cache_max: cache.bytes_max,
        development_mode,
    };
    let checks = evaluate(&facts);

    Json(json!({
        "status": overall(&checks),
        "checks": checks,
        "server": {
            "version": env!("CARGO_PKG_VERSION"),
            "uptime_secs": STARTED.elapsed().as_secs(),
            "development_mode": development_mode,
        },
        "database": {
            "ok": database_error.is_none(),
            "latency_ms": database_millis,
            "organizations": organizations,
        },
        "scheduler": {
            "paused": facts.scheduler_paused,
            "nodes": nodes_alive,
        },
        "plugins": runtime,
        "cache": {
            "bytes_used": cache.bytes_used,
            "bytes_max": cache.bytes_max,
            "entries": cache.entries,
            "hits": cache.hits,
            "misses": cache.misses,
            "evictions": cache.evictions,
            "backend": cache.backend,
            "l2_redis": cache.l2_redis,
        },
    }))
    .into_response()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn level(checks: &[Check], id: &str) -> Level {
        checks.iter().find(|c| c.id == id).map(|c| c.level).unwrap()
    }

    fn healthy() -> Facts {
        Facts { database_millis: 3, scheduler_nodes: 1, cache_used: 10, cache_max: 100, ..Facts::default() }
    }

    #[test]
    fn a_healthy_server_is_ok() {
        let checks = evaluate(&healthy());
        assert_eq!(overall(&checks), Level::Ok);
    }

    #[test]
    fn a_dead_database_is_an_error_and_a_slow_one_a_warning() {
        let down = Facts { database_error: Some("refused".into()), ..healthy() };
        assert_eq!(level(&evaluate(&down), "database"), Level::Error);
        assert_eq!(overall(&evaluate(&down)), Level::Error);
        let slow = Facts { database_millis: 400, ..healthy() };
        assert_eq!(level(&evaluate(&slow), "database"), Level::Warn);
    }

    #[test]
    fn no_scheduler_is_an_error_and_a_paused_one_a_warning() {
        let none = Facts { scheduler_nodes: 0, ..healthy() };
        assert_eq!(level(&evaluate(&none), "scheduler"), Level::Error);
        let paused = Facts { scheduler_paused: true, ..healthy() };
        assert_eq!(level(&evaluate(&paused), "scheduler"), Level::Warn);
    }

    #[test]
    fn plugin_trouble_and_a_full_cache_warn() {
        let timeouts = Facts { plugins_timeouts: 2, ..healthy() };
        assert_eq!(level(&evaluate(&timeouts), "plugins"), Level::Warn);
        let full = Facts { cache_used: 95, ..healthy() };
        assert_eq!(level(&evaluate(&full), "cache"), Level::Warn);
        let empty_budget = Facts { cache_max: 0, ..healthy() };
        assert_eq!(level(&evaluate(&empty_budget), "cache"), Level::Ok);
    }

    #[test]
    fn development_mode_warns() {
        let dev = Facts { development_mode: true, ..healthy() };
        assert_eq!(level(&evaluate(&dev), "mode"), Level::Warn);
    }
}
