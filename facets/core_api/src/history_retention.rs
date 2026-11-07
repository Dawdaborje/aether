//! Folding away record history that has outlived its keep time (see `docs/architecture/history.md`).
//!
//! A version is kept for `history.keep_days` of its model, or for the organization setting of the
//! same name when the model does not say (`0` keeps it forever). Old versions are not simply
//! deleted: the newest one older than the cut-off becomes a full snapshot and the ones before it go,
//! so the record can still be read as it was from the cut-off on. A record that was deleted longer
//! ago than its keep time loses its whole history. Nothing here touches the records themselves.

use std::time::Duration;

use serde_json::{Value, json};
use surrealdb::{Surreal, engine::remote::ws::Client};

use aether_core::data_model::history::{Version, state_at};
use aether_core::state::AppState;

/// How often the history is trimmed.
const INTERVAL: Duration = Duration::from_secs(60 * 60);
/// Wait this long after start-up before the first pass, so it never competes with the boot.
const FIRST_PASS_DELAY: Duration = Duration::from_secs(90);
/// Setting: days history is kept for models that do not say; `0` is forever.
pub const KEEP_SETTING: &str = "history.keep_days";
/// Records folded in one pass for one keep time, so a long backlog is worked off a bit at a time.
const BATCH: u32 = 200;

/// Run the trim now and then. Returns the task so shutdown can stop it.
pub fn spawn_cleanup(state: &AppState) -> tokio::task::JoinHandle<()> {
    let state = state.clone();
    tokio::spawn(async move {
        tokio::time::sleep(FIRST_PASS_DELAY).await;
        loop {
            match cleanup(&state).await {
                Ok(0) => {}
                Ok(removed) => log::info!("History cleanup removed {removed} old version(s)"),
                Err(error) => log::warn!("History cleanup failed: {error}"),
            }
            tokio::time::sleep(INTERVAL).await;
        }
    })
}

async fn cleanup(state: &AppState) -> Result<u64, surrealdb::Error> {
    let organizations = aether_core::access::org_databases::list_cached(state).await?;
    let mut removed = 0;
    for db_name in organizations {
        let org = aether_core::chatter::org_ref(&db_name);
        let days =
            match aether_core::application::settings::get_setting(state, KEEP_SETTING, Some(&org))
                .await
            {
                Ok(Some(setting)) => setting.value.as_u64().unwrap_or(0),
                _ => 0,
            };
        let db = state.org(&db_name).await?;
        // One organization's trouble does not stop the others.
        match purge(&db, u32::try_from(days).unwrap_or(0)).await {
            Ok(count) => removed += count,
            Err(error) => log::warn!("History cleanup skipped `{db_name}`: {error}"),
        }
    }
    Ok(removed)
}

/// Trim one organization's versions. `default_days` is the organization's setting. Returns how many
/// versions were removed.
pub async fn purge(db: &Surreal<Client>, default_days: u32) -> Result<u64, surrealdb::Error> {
    let mut removed = 0;
    for keep in keep_times(db).await? {
        // `Some(0)` keeps forever; no keep time of its own means the organization's.
        let days = keep.unwrap_or(i64::from(default_days));
        if days <= 0 {
            continue;
        }
        removed += purge_deleted(db, keep, days).await?;
        removed += fold(db, keep, days).await?;
    }
    Ok(removed)
}

/// The distinct keep times in use (`None`: the version has none of its own).
async fn keep_times(db: &Surreal<Client>) -> Result<Vec<Option<i64>>, surrealdb::Error> {
    let mut response = db
        .query("SELECT keep_days FROM record_versions GROUP BY keep_days;")
        .await?
        .check()?;
    let rows: Vec<Value> = response.take(0)?;
    Ok(rows
        .iter()
        .map(|row| row.get("keep_days").and_then(Value::as_i64))
        .collect())
}

/// `keep_days = $k` for a model's own keep time, `keep_days = NONE` for the organization's.
fn group(keep: Option<i64>) -> &'static str {
    if keep.is_some() {
        "keep_days = $k"
    } else {
        "keep_days = NONE"
    }
}

/// A record deleted longer ago than its keep time loses its whole history.
async fn purge_deleted(
    db: &Surreal<Client>,
    keep: Option<i64>,
    days: i64,
) -> Result<u64, surrealdb::Error> {
    let mut response = db
        .query(format!(
            "LET $cutoff = time::now() - <duration> string::concat($days, 'd'); \
             SELECT model_id, record_key, seq FROM record_versions \
             WHERE {} AND op = 'delete' AND at < $cutoff LIMIT $batch;",
            group(keep)
        ))
        .bind(("k", keep))
        .bind(("days", days))
        .bind(("batch", BATCH))
        .await?
        .check()?;
    let deletes: Vec<Value> = response.take(1)?;
    let mut removed = 0;
    for row in deletes {
        let (Some(model), Some(key), Some(seq)) = (
            row.get("model_id").and_then(Value::as_str),
            row.get("record_key").and_then(Value::as_str),
            row.get("seq").and_then(Value::as_u64),
        ) else {
            continue;
        };
        removed += delete_upto(db, model, key, seq, true).await?;
    }
    Ok(removed)
}

/// Remove a record's versions up to `seq` (`inclusive`, or before it); the number removed.
async fn delete_upto(
    db: &Surreal<Client>,
    model: &str,
    key: &str,
    seq: u64,
    inclusive: bool,
) -> Result<u64, surrealdb::Error> {
    let comparison = if inclusive { "<=" } else { "<" };
    let mut response = db
        .query(format!(
            "LET $gone = (DELETE record_versions WHERE model_id = $m AND record_key = $k \
             AND seq {comparison} $seq RETURN BEFORE); RETURN array::len($gone);"
        ))
        .bind(("m", model.to_string()))
        .bind(("k", key.to_string()))
        .bind(("seq", seq))
        .await?
        .check()?;
    let count: Option<u64> = response.take(1)?;
    Ok(count.unwrap_or(0))
}

/// Make the newest too-old version of each record a full snapshot and remove the ones before it.
async fn fold(db: &Surreal<Client>, keep: Option<i64>, days: i64) -> Result<u64, surrealdb::Error> {
    let mut response = db
        .query(format!(
            "LET $cutoff = time::now() - <duration> string::concat($days, 'd'); \
             SELECT * FROM (SELECT model_id, record_key, math::min(seq) AS first, math::max(seq) AS cut \
             FROM record_versions WHERE {} AND at < $cutoff GROUP BY model_id, record_key) \
             WHERE first < cut LIMIT $batch;",
            group(keep)
        ))
        .bind(("k", keep))
        .bind(("days", days))
        .bind(("batch", BATCH))
        .await?
        .check()?;
    let candidates: Vec<Value> = response.take(1)?;
    let mut removed = 0;
    for row in candidates {
        let (Some(model), Some(key), Some(cut)) = (
            row.get("model_id").and_then(Value::as_str),
            row.get("record_key").and_then(Value::as_str),
            row.get("cut").and_then(Value::as_u64),
        ) else {
            continue;
        };
        // The versions from the last snapshot up to the cut-off are all the replay needs.
        let mut response = db
            .query(
                "LET $s = (SELECT VALUE seq FROM record_versions WHERE model_id = $m AND record_key = $k \
                 AND seq <= $cut AND snapshot != NONE ORDER BY seq DESC LIMIT 1)[0] ?? 0; \
                 SELECT * FROM record_versions WHERE model_id = $m AND record_key = $k \
                 AND seq >= $s AND seq <= $cut ORDER BY seq;",
            )
            .bind(("m", model.to_string()))
            .bind(("k", key.to_string()))
            .bind(("cut", cut))
            .await?
            .check()?;
        let rows: Vec<Value> = response.take(1)?;
        let versions: Vec<Version> = rows.iter().filter_map(Version::from_row).collect();
        let Some(state) = state_at(&versions, cut) else {
            // Cannot be replayed from here (no snapshot reaches it): leave this record alone.
            log::warn!(
                "History cleanup left `{model}:{key}` alone: no snapshot reaches version {cut}"
            );
            continue;
        };
        db.query(
            "UPDATE record_versions SET snapshot = $state \
             WHERE model_id = $m AND record_key = $k AND seq = $cut;",
        )
        .bind(("m", model.to_string()))
        .bind(("k", key.to_string()))
        .bind(("cut", cut))
        .bind(("state", json!(state)))
        .await?
        .check()?;
        removed += delete_upto(db, model, key, cut, false).await?;
    }
    Ok(removed)
}
