use aether_core::config_manager::{
    models::{AetherConfig, DatabaseConfig},
    services::{ConfigOverrides, load_config},
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
    /// Core database name.
    pub database: String,
}

/// Load the configuration and connect to SurrealDB.
///
/// Nothing is assumed: the connection details and `app_dir` must come from the
/// config file or from flags. Anything missing is reported together, naming the
/// key to set and the flag that can supply it.
pub async fn get_prerequisites(args: &Args) -> Result<DbContext, String> {
    let config_file = resolve_config_file(args)?;
    let overrides = ConfigOverrides {
        app_dir: args
            .app_dir
            .as_deref()
            .map(|raw| absolute_from_cwd(raw, "--app-dir"))
            .transpose()?,
        db_host: args.db_host.clone(),
        db_port: args.db_port,
        db_user: args.db_user.clone(),
        db_password: args.db_password.clone(),
        db_namespace: args.db_namespace.clone(),
    };
    let mut configuration = load_config(config_file.as_deref(), &overrides)
        .map_err(|err| format!("Failed to load configuration: {err}"))?;
    if let Some(media_dir) = &args.media_dir {
        configuration
            .media
            .override_local_dir(absolute_from_cwd(media_dir, "--media-dir")?)?;
    }

    let db_config = configuration
        .database
        .clone()
        .ok_or_else(|| "Failed to load configuration: the [database] section is missing".to_string())?;
    let db = connect(&db_config).await?;

    let namespace = db_config.namespace.clone();
    let database = CORE_DATABASE.to_string();

    db.use_ns(&namespace)
        .await
        .map_err(|err| format!("Failed to set database namespace: {err}"))?;
    db.use_db(&database)
        .await
        .map_err(|err| format!("Failed to set database name: {err}"))?;

    log::info!("Namespace: {}, Database: {}", namespace, database);

    Ok(DbContext {
        config: configuration,
        db,
        namespace,
        database,
    })
}

/// The kernel's own database inside the configured namespace.
const CORE_DATABASE: &str = "core";

/// Resolve a CLI path flag against the current directory.
fn absolute_from_cwd(raw: &str, flag: &str) -> Result<PathBuf, String> {
    let path = PathBuf::from(raw);
    if path.is_absolute() {
        return Ok(path);
    }
    env::current_dir()
        .map(|cwd| cwd.join(path))
        .map_err(|err| format!("Failed to resolve {flag}: {err}"))
}

/// The config file to use: `--config-file`, else `./aether.toml` when it exists.
/// `None` means there is no file, which the loader reports unless flags cover
/// every required setting.
fn resolve_config_file(args: &Args) -> Result<Option<String>, String> {
    let current_dir =
        env::current_dir().map_err(|err| format!("Failed to get current directory: {err}"))?;
    let configured_path = args.config_file.as_ref().map(PathBuf::from);
    let path = configured_path.or_else(|| {
        let default_path = current_dir.join("aether.toml");
        default_path.is_file().then_some(default_path)
    });
    let Some(path) = path else {
        return Ok(None);
    };
    let absolute_path = if path.is_absolute() {
        path
    } else {
        current_dir.join(path)
    };
    let full_path = absolute_path.canonicalize().unwrap_or(absolute_path);

    log::info!("Using configuration file: {}", full_path.display());
    Ok(Some(full_path.to_string_lossy().into_owned()))
}

/// Connect and sign in with exactly the configured details.
async fn connect(db_config: &DatabaseConfig) -> Result<&'static Surreal<SurrealClient>, String> {
    let addr = format!("{}:{}", db_config.host, db_config.port);
    log::info!("Connecting to SurrealDB: ws://{addr}");

    DB.connect::<Ws>(addr.clone()).await.map_err(|err| {
        format!(
            "Failed to connect to SurrealDB at ws://{addr}: {err}\nIs SurrealDB running there? The address comes from `host` and `port` under [database] (or --db-host / --db-port)."
        )
    })?;
    DB.signin(Root {
        username: db_config.user.clone(),
        password: db_config.password.clone(),
    })
    .await
    .map_err(|err| {
        format!(
            "Failed to sign in to SurrealDB at ws://{addr} as `{}`: {err}\nCheck `user` and `password` under [database] (or --db-user / --db-password).",
            db_config.user
        )
    })?;
    log::info!("Connected to SurrealDB");

    Ok(&*DB)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_core_database_is_always_core() {
        assert_eq!(CORE_DATABASE, "core");
    }
}
