use aether_core::routes::routes as core_routes;
use aether_core::state::AppState;
use aether_core::config_manager::models::{self, ServerConfig};
use aether_authentication::routes::routes as auth_routes;
use axum::Router;
use std::error::Error;
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
        .map(|d| d.name.clone())
        .filter(|s| !s.is_empty())
        .unwrap_or_else(|| "aether".into());

    let state = match AppState::new(db_conn.clone(), config.clone(), namespace, "core") {
        Ok(state) => state,
        Err(err) => {
            log::error!("Failed to initialize application cache: {err}");
            return Err(Box::new(err));
        }
    };

    if let Err(err) = state.use_core().await {
        log::error!("Failed to select core database: {err}");
        return Err(Box::new(err));
    }

    let app = Router::new()
        .merge(core_routes())
        .nest("/api/auth", auth_routes())
        .with_state(state);

    let bind_addr = get_server_host(config.server, http_port);
    log::info!("Starting server: http://{bind_addr}");

    let listener = match tokio::net::TcpListener::bind(&bind_addr).await {
        Ok(l) => l,
        Err(e) => {
            log::error!("Failed to bind TCP listener: {e}");
            return Err(Box::new(e));
        }
    };

    if let Err(e) = axum::serve(listener, app).await {
        log::error!("Server error: {e}");
        return Err(Box::new(e));
    }

    Ok(())
}
