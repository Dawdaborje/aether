use aether::server::serve::run_server;
use aether_core::config_manager::{models::AetherConfig, services::generate_aether_config};
use clap::Parser;
use log::LevelFilter;
use std::str::FromStr;

#[derive(Parser, Debug)]
struct Args {
    #[arg(short, long)]
    verbose: bool,

    #[arg(short, long, default_missing_value = "aether.toml")]
    // aether configuration
    config_file: Option<String>,

    #[arg(long, default_missing_value = "7890")]
    // port
    http_port: Option<u16>,

    #[arg(short, long, default_missing_value = "0.0.0.0:7890", num_args = 0..=1)]
    // Server address to bind to (e.g., 0.0.0.0:7890)
    serve: Option<String>,

    #[arg(short = 'l', long = "log", default_value = "debug")]
    /// Logging level (error, warn, info, debug, trace)
    log: String,

    #[arg(short = 'e', long = "environment", default_value = "dev")]
    /// Environment mode: `dev` or `prod`
    environment: String,

    #[arg(short, long, action = clap::ArgAction::SetTrue)]
    // Generate a default configuration for the system and exit
    initialize: bool,
}

#[tokio::main]
async fn main() {
    let args = Args::parse();

    let level = match LevelFilter::from_str(&args.log) {
        Ok(l) => l,
        Err(_) => {
            log::error!("Invalid log level '{}', defaulting to 'debug'", args.log);
            LevelFilter::Debug
        }
    };

    let mut builder = env_logger::Builder::new();
    builder.filter_level(level);

    let enable_framework_debug =
        args.verbose || matches!(level, LevelFilter::Debug | LevelFilter::Trace);
    if enable_framework_debug {
        builder
            .filter_module("axum", LevelFilter::Debug)
            .filter_module("tower_http", LevelFilter::Debug)
            .filter_module("hyper", LevelFilter::Debug);
    }

    builder.init();

    log::warn!("Starting in '{}' environment", args.environment);

    if let Some(_host) = args.serve {
        let configuration: AetherConfig = generate_aether_config(args.config_file);
        if let Err(err) = run_server(configuration, args.http_port).await {
            log::info!("Server failed: {}", err);
            std::process::exit(1);
        };
    };
}
