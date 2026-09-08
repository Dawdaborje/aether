use aether_core::config_manager::{
    models::{AetherConfig, DatabaseConfig},
    services::generate_aether_config,
};
use std::{env, path::PathBuf, sync::LazyLock};
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
    let config_file = resolve_config_file(args);
    let configuration: AetherConfig = match generate_aether_config(config_file) {
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

    let namespace = resolve_namespace(args.db_namespace.as_deref(), &db_config);
    let database = resolve_database();

    db.use_ns(&namespace)
        .await
        .expect("Failed to set database namespace");
    db.use_db(&database)
        .await
        .expect("Failed to set database name");

    log::info!("Namespace: {}, Database: {}", namespace, database);

    DbContext {
        config: configuration,
        db,
        namespace,
        database,
    }
}

fn resolve_config_file(args: &Args) -> Option<String> {
    let current_dir = env::current_dir().expect("Failed to get current working directory");
    let configured_path = args.config_file.as_ref().map(PathBuf::from);
    let path = configured_path.or_else(|| {
        let default_path = current_dir.join("aether.toml");
        default_path.is_file().then_some(default_path)
    })?;
    let absolute_path = if path.is_absolute() {
        path
    } else {
        current_dir.join(path)
    };
    let full_path = absolute_path.canonicalize().unwrap_or(absolute_path);

    log::info!("Using configuration file: {}", full_path.display());
    Some(full_path.to_string_lossy().into_owned())
}

/// Config `database.namespace` is stored on [`DatabaseConfig::namespace`].
fn resolve_namespace(cli_namespace: Option<&str>, db_config: &DatabaseConfig) -> String {
    cli_namespace
        .map(String::from)
        .filter(|namespace| !namespace.is_empty())
        .or_else(|| (!db_config.namespace.is_empty()).then(|| db_config.namespace.clone()))
        .unwrap_or_else(|| "aether".to_string())
}

fn resolve_database() -> String {
    "core".to_string()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn configured_namespace_is_used_without_cli_override() {
        let config = DatabaseConfig {
            namespace: "configured_namespace".to_string(),
            ..DatabaseConfig::default()
        };

        assert_eq!(resolve_namespace(None, &config), "configured_namespace");
    }

    #[test]
    fn database_is_always_core() {
        let config = DatabaseConfig::default();

        assert_eq!(
            resolve_namespace(Some("cli_namespace"), &config),
            "cli_namespace"
        );
        assert_eq!(resolve_database(), "core");
    }
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
