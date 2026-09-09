use aether_authentication::routes::routes as auth_routes;
use aether_core::config_manager::models::{self, ServerConfig};
use aether_core::plugin_manager::models::plugin_def::PluginDefinition;
use aether_core::plugin_manager::services::load_plugin_manifest;
use aether_core::routes::routes as core_routes;
use aether_core::state::AppState;
use axum::Router;
use std::net::UdpSocket;
use std::{error::Error, path::PathBuf};
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

async fn load_plugins(plugin_paths: Option<&Vec<String>>) -> Vec<PluginDefinition> {
    let mut plugins = Vec::new();

    let Some(plugin_paths) = plugin_paths else {
        log::info!("No plugin paths configured");
        return plugins;
    };

    for configured_path in plugin_paths {
        let configured_path = PathBuf::from(configured_path);
        let manifest_path = if configured_path.is_dir() {
            configured_path.join("plugin.toml")
        } else {
            configured_path.clone()
        };

        log::info!("Loading plugin manifest: {:?}", manifest_path);
        match load_plugin_manifest(manifest_path).await {
            Ok(manifest) => {
                log::info!("Loaded plugin: {}", manifest.plugin.name);
                plugins.push(manifest.plugin);
            }
            Err(err) => log::error!("Failed to load plugin from {:?}: {err}", configured_path),
        }
    }

    plugins
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

    let _plugin_defs = load_plugins(config.plugin_paths.as_ref()).await;

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

    if let Err(e) = axum::serve(listener, app).await {
        log::error!("Server error: {e}");
        return Err(Box::new(e));
    }

    Ok(())
}
