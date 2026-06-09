use aether_core::config_manager::models::{self, ServerConfig};
use axum::Router;
use std::error::Error;

pub fn get_server_host(conf: Option<ServerConfig>, port: Option<u16>) -> String {
    let mut server_host = "".to_string();

    if let Some(port) = &port {
        server_host = format!("0.0.0.0:{}", port);
    } else {
        if let Some(server_config) = &conf {
            server_host = format!("{}:{}", server_config.host, server_config.port);
        }
    }

    server_host
}

pub async fn run_server(
    config: models::AetherConfig,
    http_port: Option<u16>,
) -> Result<(), Box<dyn Error + Send + Sync>> {
    let app = Router::new();

    let bind_addr = get_server_host(config.server, http_port);
    log::info!("Starting server: http://{}", bind_addr);

    let listener = match tokio::net::TcpListener::bind(bind_addr).await {
        Ok(l) => l,
        Err(e) => {
            eprintln!("Failed to bind TCP listener: {}", e);
            return Err(Box::new(e));
        }
    };

    if let Err(e) = axum::serve(listener, app).await {
        eprintln!("Server error: {}", e);
        return Err(Box::new(e));
    }

    Ok(())
}
