use aether_core::config_manager::models;
use axum::Router;

pub async fn run_server(server: &str, conf_file: &str, config: models::AetherConfig) {
    let app = Router::new();
    println!(
        "Starting server on {server} and using configuration file: {:?}",
        conf_file
    );

    if config.server.is_none() {
        eprintln!("Server configuration is missing in the config file.");
        std::process::exit(1);
    }
    let listener = tokio::net::TcpListener::bind(server).await.unwrap();
    axum::serve(listener, app).await.unwrap();
}
