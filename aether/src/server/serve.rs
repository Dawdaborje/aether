use aether_authentication::routes::routes as auth_routes;
use aether_core::config_manager::models::{self, ServerConfig};
use aether_core::routes::routes as core_routes;
use aether_core::state::AppState;
use axum::Router;
use std::error::Error;
use std::net::UdpSocket;
use surrealdb::Surreal;
use surrealdb::engine::remote::ws::Client as SurrealClient;

mod error {
    use axum::Json;
    use axum::http::StatusCode;
    use axum::response::IntoResponse;
    use axum::response::Response;
    use thiserror::Error;

    #[derive(Error, Debug)]
    #[allow(dead_code)]
    pub enum Error {
        #[error("database error")]
        Db,
    }

    impl IntoResponse for Error {
        fn into_response(self) -> Response {
            (StatusCode::INTERNAL_SERVER_ERROR, Json(self.to_string())).into_response()
        }
    }

    impl From<surrealdb::Error> for Error {
        fn from(error: surrealdb::Error) -> Self {
            log::error!("{error}");
            Self::Db
        }
    }
}

fn get_local_ip() -> String {
    UdpSocket::bind("0.0.0.0:0")
        .ok()
        .and_then(|s| s.connect("8.8.8.8:80").ok().map(|_| s))
        .and_then(|s| s.local_addr().ok())
        .map(|a| a.ip().to_string())
        .unwrap_or_else(|| "127.0.0.1".to_string())
}

pub fn get_server_host(conf: Option<ServerConfig>, port: Option<u16>) -> String {
    if let Some(port) = port {
        return format!("0.0.0.0:{port}");
    }
    if let Some(server_config) = conf {
        return format!("{}:{}", server_config.host, server_config.port);
    }
    "0.0.0.0:7890".to_string()
}

pub async fn run_server(
    config: models::AetherConfig,
    http_port: Option<u16>,
    db_conn: &'static Surreal<SurrealClient>,
) -> Result<(), Box<dyn Error + Send + Sync + '_>> {
    let namespace = config
        .database
        .as_ref()
        .map(|d| d.namespace.clone())
        .filter(|s| !s.is_empty())
        .unwrap_or_else(|| "aether".into());

    let server_config = config.server.clone();

    let state = match AppState::new(db_conn.clone(), config.clone(), namespace, "core").await {
        Ok(state) => state,
        Err(err) => {
            log::error!("Failed to initialize application state: {err}");
            return Err(Box::new(err));
        }
    };

    let core = match state.core().await {
        Ok(core) => core,
        Err(err) => {
            log::error!("Failed to select core database: {err}");
            return Err(Box::new(err));
        }
    };

    if let Err(err) = state.plugin_runtime.load_catalog(&core).await {
        log::error!("Failed to load and compile active plugins: {err}");
        return Err(Box::new(err));
    }

    let retention_task = aether_core::access::audit::spawn_retention_task(&state);

    let app = Router::new()
        .merge(core_routes())
        .nest("/api/auth", auth_routes())
        .fallback(aether_core::error_pages::not_found)
        .layer(axum::middleware::from_fn_with_state(
            state.clone(),
            aether_core::request_log::log_requests,
        ))
        .with_state(state);

    let bind_addr = get_server_host(server_config, http_port);

    let listener = match tokio::net::TcpListener::bind(&bind_addr).await {
        Ok(l) => l,
        Err(e) => {
            log::error!("Failed to bind TCP listener: {e}");
            return Err(Box::new(e));
        }
    };

    let actual_addr = listener.local_addr()?;
    let port = actual_addr.port();

    if actual_addr.ip().is_unspecified() {
        let local_ip = get_local_ip();
        log::info!("Starting server: http://localhost:{port}");
        log::info!("Starting server: http://{local_ip}:{port}");
        log::info!("Starting server: http://localhost:{port}/web");
        log::info!("Starting server: http://{local_ip}:{port}/web");
    } else {
        log::info!("Starting server: http://{actual_addr}");
        log::info!("Starting server: http://{actual_addr}/web");
    }

    // Ctrl+C or SIGTERM starts a graceful shutdown: stop accepting connections
    // and let in-flight requests finish, but only for `SHUTDOWN_GRACE`; open
    // connections such as the notifications websocket would otherwise keep the
    // server alive forever.
    let (shutdown_tx, mut shutdown_rx) = tokio::sync::watch::channel(false);
    tokio::spawn(async move {
        wait_for_shutdown_signal().await;
        log::info!("Shutdown requested: no longer accepting connections, draining in-flight requests");
        // Ignored: there is no receiver left only if the server already ended.
        let _ = shutdown_tx.send(true);
    });

    let mut drain_rx = shutdown_rx.clone();
    let server = axum::serve(
        listener,
        app.into_make_service_with_connect_info::<std::net::SocketAddr>(),
    )
    .with_graceful_shutdown(async move {
        // Resolves when the flag flips, or when the sender is gone.
        let _ = drain_rx.wait_for(|stopping| *stopping).await;
    });

    let outcome = tokio::select! {
        result = server => result.map_err(|error| {
            log::error!("Server error: {error}");
            error
        }),
        () = async {
            let _ = shutdown_rx.wait_for(|stopping| *stopping).await;
            tokio::time::sleep(SHUTDOWN_GRACE).await;
        } => {
            log::warn!(
                "Some connections were still open after {}s; closing them",
                SHUTDOWN_GRACE.as_secs()
            );
            Ok(())
        }
    };

    if let Some(task) = retention_task {
        task.abort();
    }
    // End the database session cleanly. The connection itself closes when the
    // process exits, which follows immediately.
    match db_conn.invalidate().await {
        Ok(()) => log::info!("Database session closed"),
        Err(error) => log::warn!("Could not sign out of the database cleanly: {error}"),
    }
    log::info!("Aether stopped");

    outcome.map_err(|error| Box::new(error) as Box<dyn Error + Send + Sync>)
}

/// How long in-flight requests get to finish after a shutdown request.
const SHUTDOWN_GRACE: std::time::Duration = std::time::Duration::from_secs(10);

/// Resolves on Ctrl+C, or on SIGTERM on Unix (what service managers send).
async fn wait_for_shutdown_signal() {
    let ctrl_c = async {
        if let Err(error) = tokio::signal::ctrl_c().await {
            log::error!("Could not listen for Ctrl+C: {error}");
            std::future::pending::<()>().await;
        }
    };

    #[cfg(unix)]
    let terminate = async {
        match tokio::signal::unix::signal(tokio::signal::unix::SignalKind::terminate()) {
            Ok(mut signal) => {
                signal.recv().await;
            }
            Err(error) => {
                log::error!("Could not listen for SIGTERM: {error}");
                std::future::pending::<()>().await;
            }
        }
    };
    #[cfg(not(unix))]
    let terminate = std::future::pending::<()>();

    tokio::select! {
        () = ctrl_c => {}
        () = terminate => {}
    }
}
