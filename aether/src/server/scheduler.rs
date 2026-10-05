//! `aether --start-scheduler`: the job scheduler as a process of its own.
//!
//! It runs exactly what the scheduler inside `--serve` runs, so it needs the same database and
//! the same `app_dir` (plugin files, and the secret key if none is configured elsewhere). It adds
//! a control API: the HTTP server finds the address in the core database and uses it to wake the
//! scheduler the moment a job is enqueued or plugins change. Several of these can run at once;
//! they share jobs safely.

use std::{error::Error, time::Duration};

use aether_core::{
    config_manager::models::AetherConfig,
    scheduler::{
        control,
        nodes::NodeKind,
        worker::{self, WorkerOptions},
    },
    state::AppState,
};
use surrealdb::{Surreal, engine::remote::ws::Client as SurrealClient};
use tokio::sync::watch;

/// The address other machines should use to reach a control API bound to `bind`.
fn advertised(bind: &str) -> String {
    match bind.parse::<std::net::SocketAddr>() {
        Ok(address) if address.ip().is_unspecified() => format!("{}:{}", super::serve::get_local_ip(), address.port()),
        _ => bind.to_string(),
    }
}

pub async fn run_scheduler(
    config: AetherConfig,
    db_conn: &'static Surreal<SurrealClient>,
    bind: Option<&str>,
) -> Result<(), Box<dyn Error + Send + Sync + 'static>> {
    let namespace = config
        .database
        .as_ref()
        .map(|d| d.namespace.clone())
        .filter(|s| !s.is_empty())
        .unwrap_or_else(|| "aether".into());
    let scheduler = config.scheduler.clone();
    let bind = bind.map_or_else(|| scheduler.bind.clone(), str::to_string);

    let state = AppState::new(db_conn.clone(), config, namespace, "core").await?;
    let core = state.core().await?;
    state.plugin_runtime.load_catalog(&core).await?;

    let options = WorkerOptions {
        node_id: worker::new_node_id(NodeKind::Standalone),
        kind: NodeKind::Standalone,
        address: Some(advertised(&bind)),
        queues: scheduler.queues.clone(),
        concurrency: scheduler.concurrency,
        poll: Duration::from_secs(scheduler.poll_secs),
        lease_secs: scheduler.lease_secs,
    };

    let (stop_tx, stop_rx) = watch::channel(false);
    tokio::spawn(async move {
        super::serve::wait_for_shutdown_signal().await;
        log::info!("Shutdown requested; finishing the jobs that are running");
        let _ = stop_tx.send(true);
    });

    let control_state = state.clone();
    let control_options = options.clone();
    let control_stop = stop_rx.clone();
    let control_task = tokio::spawn(async move {
        if let Err(error) = control::serve(control_state, &bind, &control_options, control_stop).await {
            log::error!("The scheduler's control API stopped: {error}");
        }
    });

    worker::run(state, options, stop_rx).await;
    let _ = control_task.await;
    log::info!("Scheduler stopped");
    Ok(())
}
