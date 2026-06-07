use aether_core::config_manager::models;
use axum::Router;
use std::error::Error;

pub async fn run_server(
    server: &str,
    conf_file: &str,
    config: models::AetherConfig,
) -> Result<(), Box<dyn Error + Send + Sync>> {
    let app = Router::new().merge(aether_core::routes::routes());

    // Prefer server configuration from `config` when provided; fall back to the
    // `server` argument otherwise.
    let bind_addr = if let Some(srv_cfg) = config.server {
        format!("{}:{}", srv_cfg.host, srv_cfg.port)
    } else {
        server.to_string()
    };

    println!(
        "Starting server on {} and using configuration file: {:?}",
        bind_addr, conf_file
    );

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
