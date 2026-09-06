use std::{env, path::Path, str::FromStr};

use aether_core::application::services::change_user_password;
use clap::Parser;
use log::LevelFilter;

use crate::server::serve::run_server;

use super::{
    args::Args,
    db::get_prerequisites,
    generation::{
        generate_default_config_template, generate_plugin_config, generate_plugin_workspace,
    },
    initialization::{initialize_system, print_bootstrap_summary},
    seed::seed_system,
};

/// Parse CLI args, configure logging, and dispatch commands.
pub async fn run() {
    let args = Args::parse();
    init_logger(&args);

    log::warn!("Starting in '{}' environment", args.environment);

    dispatch(args).await;
}

fn init_logger(args: &Args) {
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
}

async fn dispatch(args: Args) {
    let current_path = env::current_dir().expect("Failed to get current working directory");

    if let Some(plugins_to_upgrade) = &args.upgrade_plugin {
        for plugin_name in plugins_to_upgrade {
            log::info!("{plugin_name}");
        }
    }

    if let Some(config_file_name) = &args.generate_config_file {
        let full_path = Path::new(&current_path).join(config_file_name);
        generate_default_config_template(full_path).await;
    }

    if let Some(gen_target) = &args.generate {
        let current_dir = current_path.to_string_lossy().to_string();
        match gen_target.as_str() {
            "workspace" => {
                generate_plugin_workspace(current_dir).await;
            }
            "plugin" => {
                let plugin_name = current_path
                    .file_name()
                    .and_then(|name| name.to_str())
                    .unwrap_or("my_plugin");
                generate_plugin_config(current_dir, plugin_name).await;
            }
            "aether_config" => {
                generate_default_config_template(current_path.join("aether.toml")).await;
            }
            other => {
                log::error!(
                    "Unknown generation target '{other}'. Supported: workspace, plugin, aether_config"
                );
            }
        }
    }

    if args.change_password {
        let username = &args
            .password
            .as_ref()
            .expect("Password must be provided when changing password");
        let new_password = &args
            .password
            .as_ref()
            .expect("Password must be provided when changing password");

        let ctx = get_prerequisites(&args).await;
        change_user_password(username, new_password, &ctx.db).await;
    }

    if args.init {
        let ctx = get_prerequisites(&args).await;
        match initialize_system(ctx.db, &ctx.namespace, &ctx.database).await {
            Ok(result) => print_bootstrap_summary(&result),
            Err(err) => {
                log::error!("Init failed: {err}");
                std::process::exit(1);
            }
        }
    }

    if args.seed {
        let ctx = get_prerequisites(&args).await;
        seed_system(ctx.db, &ctx.namespace, &ctx.database).await;
    }

    if let Some(_host) = &args.serve {
        let ctx = get_prerequisites(&args).await;
        if let Err(err) = run_server(ctx.config, args.http_port, ctx.db).await {
            log::error!("Server failed: {err}");
            std::process::exit(1);
        }
    }
}
