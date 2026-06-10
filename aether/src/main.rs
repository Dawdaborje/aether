use aether::{cli::initialization::initialize_system, server::serve::run_server};
use aether_core::config_manager::{
    models::{AetherConfig, DatabaseConfig},
    services::generate_aether_config,
};
use clap::Parser;
use log::LevelFilter;
use std::{str::FromStr, sync::LazyLock};
use surrealdb::{
    Surreal,
    engine::remote::ws::{Client as SurrealClient, Ws},
    opt::auth::Root,
};

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

    // database
    #[arg(long)]
    // Surreal db namespace
    db_namespace: Option<String>,

    #[arg(long)]
    // Surreal db namespace
    db_name: Option<String>,

    #[arg(long)]
    // Surreal db user
    db_user: Option<String>,

    #[arg(long)]
    // Surreal db user password
    db_password: Option<String>,

    #[arg(long)]
    // Surreal db host
    db_host: Option<String>,

    #[arg(long, default_value = "8000")]
    // Surreal db port
    db_port: Option<u16>,
}

static DB: LazyLock<Surreal<SurrealClient>> = LazyLock::new(Surreal::init);

async fn build_db_conn(
    db_config: &DatabaseConfig,
    db_host: Option<String>,
    db_user: Option<String>,
    db_password: Option<String>,
    db_port: Option<u16>,
) -> &'static Surreal<SurrealClient> {
    // Log every resolved value so you can see exactly what's happening
    let host = db_host.filter(|s| !s.is_empty()).unwrap_or_else(|| {
        if db_config.host.is_empty() {
            "127.0.0.1".to_string()
        } else {
            db_config.host.clone()
        }
    });
    let port = db_port.unwrap_or_else(|| {
        if db_config.port != 0 {
            db_config.port
        } else {
            8000
        }
    });
    let user = db_user.filter(|s| !s.is_empty()).unwrap_or_else(|| {
        if db_config.user.is_empty() {
            "root".to_string()
        } else {
            db_config.user.clone()
        }
    });
    let password = db_password.filter(|s| !s.is_empty()).unwrap_or_else(|| {
        if db_config.password.is_empty() {
            "root".to_string()
        } else {
            db_config.password.clone()
        }
    });

    let addr = format!("{}:{}", host, port);
    log::info!("Connecting to SurrealDB: ws://{}", addr);

    DB.connect::<Ws>(addr)
        .await
        .expect("Failed to connect to SurrealDB");
    DB.signin(Root {
        username: user,
        password,
    })
    .await
    .expect("Failed to sign in to SurrealDB");
    log::info!("Connected to SurrealDB");

    &*DB
}

async fn get_prerequisites(args: &Args) -> (AetherConfig, &'static Surreal<SurrealClient>) {
    let configuration: AetherConfig = generate_aether_config(args.config_file.clone());
    let db_conn = build_db_conn(
        configuration.database.as_ref().expect("Database config"),
        args.db_host.clone(),
        args.db_user.clone(),
        args.db_password.clone(),
        args.db_port,
    )
    .await;

    (configuration, db_conn)
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

    if let Some(_host) = &args.serve {
        let (configuration, db_conn) = get_prerequisites(&args).await;
        if let Err(err) = run_server(configuration, args.http_port, db_conn).await {
            log::info!("Server failed: {}", err);
            std::process::exit(1);
        };
    };

    if args.initialize {
        let (_configuration, db_conn) = get_prerequisites(&args).await;
        initialize_system(db_conn).await;
    }
}
