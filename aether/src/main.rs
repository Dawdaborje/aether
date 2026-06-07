use aether::server::serve::run_server;
use aether_core::config_manager::models::gen_config_file;
use clap::Parser;
use log::LevelFilter;
use std::str::FromStr;

#[derive(Parser, Debug)]
struct Args {
    #[arg(short, long)]
    verbose: bool,

    #[arg(short, long, default_missing_value = "aether.conf.toml")]
    config_file: Option<String>,

    #[arg(short, long, default_missing_value = "0.0.0.0:7890", num_args = 0..=1)]
    serve: Option<String>,

    #[arg(short = 'l', long = "log", default_value = "debug")]
    /// Logging level (error, warn, info, debug, trace)
    log: String,

    #[arg(short = 'e', long = "environment", default_value = "dev")]
    /// Environment mode: `dev` or `prod`
    environment: String,
}

#[tokio::main]
async fn main() {
    let args = Args::parse();

    // Initialize logging based on CLI arg (default: debug).
    // If `--log` is `debug`/`trace` or `--verbose` is set, also enable debug
    // logging for axum, tower_http and hyper so you get framework debug info.
    let level = match LevelFilter::from_str(&args.log) {
        Ok(l) => l,
        Err(_) => {
            eprintln!("Invalid log level '{}', defaulting to 'debug'", args.log);
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

    println!("Starting in '{}' environment", args.environment);

    if let Some(host) = args.serve {
        let config_file = args
            .config_file
            .unwrap_or_else(|| "aether.conf.toml".to_string());
        let config = gen_config_file(&config_file).unwrap_or_else(|err| {
            eprintln!("Failed to generate config file: {err}");
            std::process::exit(1);
        });
        if let Err(err) = run_server(&host, &config_file, config).await {
            eprintln!("Server failed: {}", err);
            std::process::exit(1);
        }
    }
}
