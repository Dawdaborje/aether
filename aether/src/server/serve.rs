use aether_core::config_manager::models::{self, ServerConfig};
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
        return format!("0.0.0.0:{}", port);
    }
    if let Some(server_config) = conf {
        return format!("{}:{}", server_config.host, server_config.port);
    }
    // fallback
    "0.0.0.0:3000".to_string()
}

// State holds a reference to the static DB handle
#[derive(Clone)]
pub struct AetherAppState {
    pub db: Surreal<SurrealClient>,
}

pub async fn run_server(
    config: models::AetherConfig,
    http_port: Option<u16>,
    db_conn: &'static Surreal<SurrealClient>,
) -> Result<(), Box<dyn Error + Send + Sync + '_>> {
    // Build and connect state first
    let state = AetherAppState {
        db: db_conn.clone(),
    };

    let app = Router::new()
        // .route("/your/routes", get(handler))
        .with_state(state);

    let bind_addr = get_server_host(config.server, http_port);
    log::info!("Starting server: http://{}", bind_addr);

    let listener = match tokio::net::TcpListener::bind(&bind_addr).await {
        Ok(l) => l,
        Err(e) => {
            log::error!("Failed to bind TCP listener: {}", e);
            return Err(Box::new(e));
        }
    };

    if let Err(e) = axum::serve(listener, app).await {
        log::error!("Server error: {}", e);
        return Err(Box::new(e));
    }

    Ok(())
}
