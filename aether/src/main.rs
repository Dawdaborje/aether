use aether::{
    cli::{
        args::Args,
        generation::{generate_default_config_template, generate_plugin_workspace},
        initialization::initialize_system,
        seed::seed_system,
    },
    server::serve::run_server,
};
use clap::Parser;
use aether_core::config_manager::{
    models::{AetherConfig, DatabaseConfig},
    services::generate_aether_config,
};
use log::LevelFilter;
use std::{env, path::Path, str::FromStr, sync::LazyLock};
use surrealdb::{
    Surreal,
    engine::remote::ws::{Client as SurrealClient, Ws},
    opt::auth::Root,
};


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
    let configuration: AetherConfig = match generate_aether_config(args.config_file.clone()) {
        Ok(config) => config,
        Err(err) => {
            log::error!("Failed to load configuration: {}", err);
            std::process::exit(1);
        }
    };
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

    let current_path = env::current_dir().expect("Failed to get current working directory");

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

    if let Some(plugins_to_upgrade) = &args.upgrade {
        // let (_configuration, db_conn) = get_prerequisites(&args).await;
        for plugin_name in plugins_to_upgrade {
            log::info!("{plugin_name}")
        }
    }

    if let Some(config_file_name) = &args.generate_config_file {
        let full_path = Path::new(&current_path).join(config_file_name);

        generate_default_config_template(full_path).await;
    }

    if let Some(gen_target) = &args.generate {
        match gen_target.as_str() {
            "workspace" => {
                generate_plugin_workspace(current_path.to_string_lossy().to_string()).await;
            }
            other => {
                log::error!("Unknown generation target '{}'. Supported: workspace", other);
            }
        }
    }

    if args.initialize {
        let (_configuration, db_conn) = get_prerequisites(&args).await;
        initialize_system(db_conn).await;
    }

    if args.seed {
        let (_configuration, db_conn) = get_prerequisites(&args).await;
        seed_system(db_conn).await;
    }

    if let Some(_host) = &args.serve {
        let (configuration, db_conn) = get_prerequisites(&args).await;
        if let Err(err) = run_server(configuration, args.http_port, db_conn).await {
            log::info!("Server failed: {}", err);
            std::process::exit(1);
        };
    };
}
