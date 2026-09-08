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
    organization::{assign_user, create_organization},
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

    if let Some(username) = &args.change_password {
        let Some(new_password) = &args.password else {
            log::error!("--password must be provided when changing a user's password");
            return;
        };
        let ctx = get_prerequisites(&args).await;
        match change_user_password(username, new_password, ctx.db).await {
            Ok(()) => println!("Password changed for user '{username}'."),
            Err(err) => log::error!("Failed to change password for '{username}': {err}"),
        }
        return;
    }

    if let Some(organization_name) = &args.create_org {
        let Some(company_name) = &args.company_name else {
            log::error!("--company-name must be provided when creating an organization");
            return;
        };
        let Some(username) = &args.username else {
            log::error!("--username must be provided when creating an organization");
            return;
        };
        let Some(email) = &args.email else {
            log::error!("--email must be provided when creating an organization");
            return;
        };
        let Some(password) = &args.password else {
            log::error!("--password must be provided when creating an organization");
            return;
        };

        let ctx = get_prerequisites(&args).await;
        match create_organization(
            ctx.db,
            &ctx.namespace,
            organization_name,
            args.org_db_name.as_deref(),
            company_name,
            args.company_email.as_deref(),
            username,
            email,
            password,
        )
        .await
        {
            Ok(db_name) => println!(
                "Created organization '{organization_name}', company '{company_name}', user '{username}' in database '{db_name}'."
            ),
            Err(err) => log::error!("Failed to create organization: {err}"),
        }
        return;
    }

    if let Some(user_login) = &args.assign_user {
        let Some(org_db_name) = &args.org_db_name else {
            log::error!("--org-db-name must be provided when assigning a user");
            return;
        };
        let Some(company_name) = &args.company_name else {
            log::error!("--company-name must be provided when assigning a user");
            return;
        };

        let ctx = get_prerequisites(&args).await;
        match assign_user(
            ctx.db,
            &ctx.namespace,
            user_login,
            org_db_name,
            company_name,
        )
        .await
        {
            Ok(()) => println!(
                "Assigned user '{user_login}' to organization '{org_db_name}' and company '{company_name}'."
            ),
            Err(err) => log::error!("Failed to assign user '{user_login}': {err}"),
        }
        return;
    }

    if args.init {
        let ctx = get_prerequisites(&args).await;
        match initialize_system(
            ctx.db,
            &ctx.namespace,
            &ctx.database,
            args.admin_username.as_deref(),
            args.admin_email.as_deref(),
            args.admin_password.as_deref(),
        )
        .await
        {
            Ok(result) => print_bootstrap_summary(&result),
            Err(err) => {
                log::error!("Init failed: {err}");
                std::process::exit(1);
            }
        }
    }

    if args.seed {
        let ctx = get_prerequisites(&args).await;
        if let Err(err) = seed_system(ctx.db, &ctx.namespace, &ctx.database).await {
            log::error!("Seeding failed: {err}");
            std::process::exit(1);
        }
    }

    if let Some(_host) = &args.serve {
        let ctx = get_prerequisites(&args).await;
        if let Err(err) = run_server(ctx.config, args.http_port, ctx.db).await {
            log::error!("Server failed: {err}");
            std::process::exit(1);
        }
    }
}
