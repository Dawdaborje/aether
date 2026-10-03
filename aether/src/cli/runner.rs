use std::{env, path::Path, str::FromStr};

use aether_core::application::services::change_user_password;
use aether_orm::{SchemaStatus, core_schema_status};
use clap::Parser;
use log::LevelFilter;

use crate::server::serve::run_server;

use super::{
    args::Args,
    db::get_prerequisites,
    generation::{generate_default_config_template, generate_plugin_workspace},
    initialization::{initialize_system, print_bootstrap_summary},
    organization::{
        FirstMember, OrganizationRequest, StorageTarget, assign_user, create_organization,
        provision_existing_organization,
    },
    plugin_manager::{
        activate_theme, install_plugins, load_plugins, print_install_summary, print_load_summary,
        print_upgrade_summary, upgrade_plugins,
    },
    scaffold::{Language, create_plugin},
    seed::seed_system,
};

/// Parse CLI args, configure logging, and dispatch commands.
pub async fn run() -> Result<(), Box<dyn std::error::Error>> {
    let args = Args::parse();
    init_logger(&args);

    log::warn!("Starting in '{}' environment", args.environment);

    dispatch(args).await
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

async fn dispatch(args: Args) -> Result<(), Box<dyn std::error::Error>> {
    let current_path = env::current_dir()?;

    if let Some(plugin_paths) = &args.load_plugin {
        let ctx = get_db_context(&args).await;
        let loaded = load_plugins(
            ctx.db,
            &ctx.namespace,
            &ctx.database,
            &ctx.config.app_dir,
            plugin_paths,
        )
        .await?;
        print_load_summary(&loaded);
    }

    if let Some(specs) = &args.install_plugin {
        // clap's `requires = "org"` guarantees this is set.
        let Some(org) = &args.org else {
            return Err("--org must be provided when installing plugins".into());
        };
        let ctx = get_db_context(&args).await;
        let report = install_plugins(ctx.db, &ctx.namespace, &ctx.database, org, specs).await?;
        print_install_summary(org, &report);
    }

    if let Some(specs) = &args.upgrade_plugin {
        let Some(org) = &args.org else {
            return Err("--org must be provided when upgrading plugins".into());
        };
        let ctx = get_db_context(&args).await;
        let report = upgrade_plugins(ctx.db, &ctx.namespace, &ctx.database, org, specs).await?;
        print_upgrade_summary(org, &report);
    }

    if let Some(theme) = &args.activate_theme {
        let Some(org) = &args.org else {
            return Err("--org must be provided when activating a theme".into());
        };
        let ctx = get_db_context(&args).await;
        activate_theme(ctx.db, &ctx.namespace, org, theme).await?;
        println!("Theme '{theme}' is now the active theme for organization '{org}'.");
    }

    if let Some(config_file_name) = &args.generate_config_file {
        let full_path = Path::new(&current_path).join(config_file_name);
        generate_default_config_template(full_path).await?;
    }

    if let Some(gen_target) = &args.generate {
        let current_dir = current_path.to_string_lossy().to_string();
        match gen_target.as_str() {
            "workspace" => {
                generate_plugin_workspace(current_dir).await?;
            }
            "plugin" => {
                let plugin_path = args
                    .plugin_path
                    .as_deref()
                    .map(Path::new)
                    .unwrap_or_else(|| Path::new("my_plugin"));
                let language: Language = args.plugin_language.parse()?;
                let created = create_plugin(plugin_path, language).await?;
                println!(
                    "Created {} plugin '{}' at {}.",
                    created.language.label(),
                    created.name,
                    created.directory.display()
                );
                if let Some(workspace) = created.workspace {
                    println!("Registered it in workspace '{workspace}' under [workspace.plugins].");
                }
            }
            "aether_config" => {
                generate_default_config_template(current_path.join("aether.toml")).await?;
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
            return Ok(());
        };
        let ctx = get_db_context(&args).await;
        match change_user_password(username, new_password, ctx.db).await {
            Ok(()) => println!("Password changed for user '{username}'."),
            Err(err) => log::error!("Failed to change password for '{username}': {err}"),
        }
        return Ok(());
    }

    if let Some(organization_name) = &args.create_org {
        let Some(username) = &args.username else {
            log::error!("--username must be provided when creating an organization");
            return Ok(());
        };
        let Some(email) = &args.email else {
            log::error!("--email must be provided when creating an organization");
            return Ok(());
        };
        let Some(password) = &args.password else {
            log::error!("--password must be provided when creating an organization");
            return Ok(());
        };

        let ctx = get_db_context(&args).await;
        let storage = storage_target(&ctx.config).await?;
        let request = OrganizationRequest {
            name: organization_name,
            db_name: args.org_db_name.as_deref(),
            member: FirstMember::User { username, email, password },
        };
        match create_organization(ctx.db, &ctx.namespace, &request, &storage).await {
            Ok((db_name, org_storage)) => println!(
                "Created organization '{organization_name}' with user '{username}' in database '{db_name}'.\n  files: {}\n  media: {}",
                org_storage.directory.display(),
                describe_media(&ctx.config, &org_storage.media_prefix)
            ),
            Err(err) => log::error!("Failed to create organization: {err}"),
        }
        return Ok(());
    }

    if let Some(organization) = &args.provision_org {
        let ctx = get_db_context(&args).await;
        let storage = storage_target(&ctx.config).await?;
        let provisioned =
            provision_existing_organization(ctx.db, &ctx.namespace, organization, &storage).await?;
        println!(
            "Storage ready for organization '{organization}'.\n  files: {}\n  media: {}",
            provisioned.directory.display(),
            describe_media(&ctx.config, &provisioned.media_prefix)
        );
        return Ok(());
    }

    if let Some(user_login) = &args.assign_user {
        let Some(org_db_name) = &args.org_db_name else {
            log::error!("--org-db-name must be provided when assigning a user");
            return Ok(());
        };

        let ctx = get_db_context(&args).await;
        match assign_user(ctx.db, &ctx.namespace, user_login, org_db_name).await {
            Ok(()) => println!("Assigned user '{user_login}' to organization '{org_db_name}'."),
            Err(err) => log::error!("Failed to assign user '{user_login}': {err}"),
        }
        return Ok(());
    }

    if args.purge_audit {
        let ctx = get_db_context(&args).await;
        let Some(days) = ctx.config.audit.retention_days else {
            return Err("--purge-audit needs `retention_days` under [audit] in aether.toml".into());
        };
        let report =
            aether_core::access::audit::purge_expired(ctx.db, &ctx.namespace, &ctx.database, days)
                .await?;
        println!(
            "Purged audit rows older than {days} days from {} organization(s): {} page visit(s), {} plugin call(s), {} data access row(s), {} visitor(s).",
            report.organizations,
            report.page_visits,
            report.plugin_calls,
            report.data_access,
            report.visitors
        );
        return Ok(());
    }

    if args.init {
        let ctx = connect_context(&args).await;
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
        let ctx = get_db_context(&args).await;
        if let Err(err) = seed_system(ctx.db, &ctx.namespace, &ctx.database).await {
            log::error!("Seeding failed: {err}");
            std::process::exit(1);
        }
    }

    if let Some(_host) = &args.serve {
        let ctx = get_db_context(&args).await;
        if let Err(err) = run_server(ctx.config, args.http_port, ctx.db).await {
            log::error!("Server failed: {err}");
            std::process::exit(1);
        }
    }

    Ok(())
}

/// The folder and media backend new organizations are provisioned in.
async fn storage_target(
    config: &aether_core::config_manager::models::AetherConfig,
) -> Result<StorageTarget, Box<dyn std::error::Error>> {
    let media = aether_core::media::build_media_backend(&config.media).await?;
    Ok(StorageTarget {
        app_dir: aether_core::app_dir::AppDir::new(&config.app_dir),
        media,
    })
}

/// Where an organization's media ends up, for the confirmation message.
fn describe_media(
    config: &aether_core::config_manager::models::AetherConfig,
    prefix: &str,
) -> String {
    use aether_core::config_manager::models::MediaBackendKind;
    match (&config.media.backend, &config.media.local, &config.media.s3) {
        (MediaBackendKind::Local, Some(local), _) => local.base_path.join(prefix).display().to_string(),
        (MediaBackendKind::S3, _, Some(s3)) => {
            let outer = s3.prefix.as_deref().filter(|p| !p.is_empty());
            match outer {
                Some(outer) => format!("s3://{}/{outer}/{prefix}/", s3.bucket),
                None => format!("s3://{}/{prefix}/", s3.bucket),
            }
        }
        _ => prefix.to_string(),
    }
}

/// Connect, and require that `aether --init` has been run. Every command except
/// `--init` itself works on an initialized database, so a fresh one gets a
/// plain instruction instead of a "table does not exist" error.
async fn get_db_context(args: &Args) -> crate::cli::db::DbContext {
    let context = connect_context(args).await;
    match core_schema_status(context.db, &context.namespace, &context.database).await {
        Ok(SchemaStatus::UpToDate) => context,
        Ok(SchemaStatus::Uninitialized) => {
            log::error!(
                "The database `{}/{}` has not been initialized. Run `aether --init` (with the same configuration) to apply the migrations and create the first admin user.",
                context.namespace,
                context.database
            );
            std::process::exit(1);
        }
        Ok(SchemaStatus::Pending(pending)) => {
            log::error!(
                "The database `{}/{}` is missing {} migration(s) shipped with this version of Aether ({}). Run `aether --init` to apply them.",
                context.namespace,
                context.database,
                pending.len(),
                pending.join(", ")
            );
            std::process::exit(1);
        }
        Err(error) => {
            log::error!("Could not check the database schema: {error}");
            std::process::exit(1);
        }
    }
}

async fn connect_context(args: &Args) -> crate::cli::db::DbContext {
    match get_prerequisites(args).await {
        Ok(context) => context,
        Err(error) => {
            log::error!("{error}");
            std::process::exit(1);
        }
    }
}
