use aether_core::config_manager::{
    models::{AetherConfig, DatabaseConfig},
    services::generate_aether_config,
};
use std::sync::LazyLock;
use surrealdb::{
    Surreal,
    engine::remote::ws::{Client as SurrealClient, Ws},
    opt::auth::Root,
};

use super::args::Args;

static DB: LazyLock<Surreal<SurrealClient>> = LazyLock::new(Surreal::init);

pub struct DbContext {
    pub config: AetherConfig,
    pub db: &'static Surreal<SurrealClient>,
    pub namespace: String,
    /// Core database name (default `core`).
    pub database: String,
}

pub async fn get_prerequisites(args: &Args) -> DbContext {
    let configuration: AetherConfig = match generate_aether_config(args.config_file.clone()) {
        Ok(config) => config,
        Err(err) => {
            log::error!("Failed to load configuration: {}", err);
            std::process::exit(1);
        }
    };

    let db_config = configuration.database.as_ref().cloned().unwrap_or_default();

    let db = build_db_conn(
        &db_config,
        args.db_host.clone(),
        args.db_user.clone(),
        args.db_password.clone(),
        args.db_port,
    )
    .await;

    let namespace = resolve_namespace(args, &db_config);
    let database = resolve_database(args);
    db.use_ns(namespace.clone())
        .await
        .expect("Failed to set namespace");
    db.use_db(database.clone())
        .await
        .expect("Failed to set database");

    DbContext {
        config: configuration,
        db,
        namespace,
        database,
    }
}

/// Config `database.namespace` is stored on [`DatabaseConfig::namespace`].
fn resolve_namespace(args: &Args, db_config: &DatabaseConfig) -> String {
    args.db_namespace
        .clone()
        .filter(|s| !s.is_empty())
        .unwrap_or_else(|| {
            if db_config.namespace.is_empty() {
                "aether".to_string()
            } else {
                db_config.namespace.clone()
            }
        })
}

fn resolve_database(args: &Args) -> String {
    args.db_name
        .clone()
        .filter(|s| !s.is_empty())
        .unwrap_or_else(|| "core".to_string())
}

async fn build_db_conn(
    db_config: &DatabaseConfig,
    db_host: Option<String>,
    db_user: Option<String>,
    db_password: Option<String>,
    db_port: Option<u16>,
) -> &'static Surreal<SurrealClient> {
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

    let addr = format!("{host}:{port}");
    log::info!("Connecting to SurrealDB: ws://{addr}");

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
