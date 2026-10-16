use std::{env, path::{Path, PathBuf}};

use aether_core::application::services::change_user_password;
use aether_core::org_admin::create_user;
use aether_core::roles;
use aether_orm::{SchemaStatus, core_schema_status, find_user_by_login};
use clap::Parser;
use surrealdb::types::ToSql;

use crate::server::serve::run_server;

use super::{
    args::Args,
    db::{get_prerequisites, load_search_roots},
    generation::{generate_default_config_template, generate_plugin_workspace},
    initialization::{
        initialize_system, migrate_system, print_bootstrap_summary, print_migrate_summary,
    },
    logging::init_logger,
    organization::{
        FirstMember, OrganizationRequest, StorageTarget, assign_user, create_organization,
        transfer_ownership,
        provision_existing_organization,
    },
    plugin_manager::{
        activate_theme, check_plugins, install_plugins, print_check_summary,
        print_install_summary, print_load_summary, print_upgrade_summary, upgrade_plugins,
    },
    scaffold::{self, Language, create_plugin},
    seed::seed_system,
};

/// Parse CLI args, configure logging, and dispatch commands.
pub async fn run() -> Result<(), Box<dyn std::error::Error>> {
    let mut args = Args::parse();
    // An MCP client shows the server's stderr to its user; the default `debug` would bury it.
    if args.mcp && args.log == "debug" {
        args.log = "warn".to_string();
    }
    init_logger(&args);

    log::warn!("Starting in '{}' environment", args.environment);

    dispatch(args).await
}

/// What was created, and what to do next.
fn report_plugin(created: &scaffold::CreatedPlugin) {
    println!("Created {} plugin '{}' at {}.", created.language.label(), created.name, created.directory.display());
    if let Some(workspace) = &created.workspace {
        println!("Registered it in workspace '{workspace}' under [workspace.plugins].");
    }
    if !created.language.is_wasm() {
        println!("Nothing to compile: edit {}, then run `aether --load-plugins .` in {} (and `aether --install-plugin {} --org <org>`).", created.language.script_file().unwrap_or("the script"), created.directory.display(), created.name);
    }
}

async fn dispatch(args: Args) -> Result<(), Box<dyn std::error::Error>> {
    let current_path = env::current_dir()?;

    if let Some(plugin_paths) = &args.sync_models {
        for path in plugin_paths {
            // Links to other plugins' models get those models' ids from the catalog.
            let wanted = aether_core::data_model::foreign_targets(path)?;
            let foreign = if wanted.is_empty() {
                std::collections::HashMap::new()
            } else {
                let ctx = get_db_context(&args).await;
                aether_core::plugin_manager::catalog::foreign_model_ids(ctx.db, &ctx.namespace, &ctx.database, &wanted).await?
            };
            for synced in aether_core::data_model::sync_package_with(path, &foreign)? {
                if synced.assigned > 0 {
                    println!("Model '{}': {} id(s) written to {}.", synced.model, synced.assigned, synced.path.display());
                } else {
                    println!("Model '{}': ids already assigned.", synced.model);
                }
            }
        }
    }

    if let Some(cli_paths) = &args.load_plugins {
        let ctx = get_db_context(&args).await;
        let entries = if cli_paths.is_empty() {
            ctx.config.plugin_paths.clone()
        } else {
            cli_paths.clone()
        };
        if entries.is_empty() {
            return Err(
                "--load-plugins needs package paths, workspace paths or workspace names on the command line, or a non-empty `plugin_paths` list in the config file"
                    .into(),
            );
        }
        let search_roots = load_search_roots(&args)?;
        if args.dry_run {
            let plan =
                aether_core::plugin_manager::load_paths::resolve_load_paths(&entries, &search_roots)
                    .await?;
            let report = check_plugins(
                ctx.db,
                &ctx.namespace,
                &ctx.database,
                &ctx.config.app_dir,
                &plan.packages,
            )
            .await?;
            print_check_summary(&report);
            if !report.errors.is_empty() {
                return Err(format!(
                    "--load-plugins --dry-run: {} of {} package(s) failed validation",
                    report.errors.len(),
                    plan.packages.len()
                )
                .into());
            }
        } else {
            let loaded = aether_core::plugin_manager::load::load_into_catalog(
                ctx.db,
                &ctx.namespace,
                &ctx.database,
                &ctx.config.app_dir,
                None,
                &entries,
                &search_roots,
            )
            .await
            .map_err(|error| error.to_string())?;
            print_load_summary(&loaded);
            let actor = aether_core::plugin_activity::Actor::system(&format!(
                "cli:{}",
                crate::cli::commands::operating_system_user()
            ));
            for plugin in loaded.iter().filter(|plugin| plugin.created) {
                aether_core::plugin_activity::record_in(
                    ctx.db,
                    &ctx.namespace,
                    &ctx.database,
                    &aether_core::plugin_activity::Entry {
                        kind: aether_core::plugin_activity::kind::CATALOG_LOAD,
                        plugin: plugin.name.clone(),
                        from_version: None,
                        to_version: Some(plugin.version.clone()),
                        organization: None,
                        actor: actor.clone(),
                        source: "cli:load".into(),
                        ok: true,
                        error: None,
                        detail: serde_json::Value::Null,
                    },
                )
                .await;
            }
        }
    }

    if let Some(specs) = &args.install_plugin {
        // clap's `requires = "org"` guarantees this is set.
        let Some(org) = &args.org else {
            return Err("--org must be provided when installing plugins".into());
        };
        let ctx = get_db_context(&args).await;
        let report = install_plugins(ctx.db, &ctx.namespace, &ctx.database, org, specs).await?;
        print_install_summary(org, &report);
        let actor = aether_core::plugin_activity::Actor::system(&format!(
            "cli:{}",
            crate::cli::commands::operating_system_user()
        ));
        for (name, version) in &report.installed {
            aether_core::plugin_activity::record_in(
                ctx.db,
                &ctx.namespace,
                &ctx.database,
                &aether_core::plugin_activity::Entry {
                    kind: aether_core::plugin_activity::kind::INSTALL,
                    plugin: name.clone(),
                    from_version: None,
                    to_version: Some(version.clone()),
                    organization: Some(org.clone()),
                    actor: actor.clone(),
                    source: "cli:install".into(),
                    ok: true,
                    error: None,
                    detail: serde_json::Value::Null,
                },
            )
            .await;
        }
        let seeded: Vec<String> = report.installed.iter().map(|(name, _)| name.clone()).collect();
        crate::cli::commands::run_lifecycle_seeds(
            ctx,
            org,
            &seeded,
            aether_core::plugin_manager::commands::LifecyclePhase::Install,
        )
        .await?;
    }

    if let Some(specs) = &args.upgrade_plugin {
        let Some(org) = &args.org else {
            return Err("--org must be provided when upgrading plugins".into());
        };
        let ctx = get_db_context(&args).await;
        let report = upgrade_plugins(ctx.db, &ctx.namespace, &ctx.database, org, specs).await?;
        print_upgrade_summary(org, &report);
        let actor = aether_core::plugin_activity::Actor::system(&format!(
            "cli:{}",
            crate::cli::commands::operating_system_user()
        ));
        for (name, from, to) in &report.upgraded {
            aether_core::plugin_activity::record_in(
                ctx.db,
                &ctx.namespace,
                &ctx.database,
                &aether_core::plugin_activity::Entry {
                    kind: aether_core::plugin_activity::kind::UPGRADE,
                    plugin: name.clone(),
                    from_version: Some(from.clone()),
                    to_version: Some(to.clone()),
                    organization: Some(org.clone()),
                    actor: actor.clone(),
                    source: "cli:upgrade".into(),
                    ok: true,
                    error: None,
                    detail: serde_json::Value::Null,
                },
            )
            .await;
        }
        let seeded: Vec<String> = report.upgraded.iter().map(|(name, _, _)| name.clone()).collect();
        crate::cli::commands::run_lifecycle_seeds(
            ctx,
            org,
            &seeded,
            aether_core::plugin_manager::commands::LifecyclePhase::Upgrade,
        )
        .await?;
    }

    if let Some(theme) = &args.activate_theme {
        let Some(org) = &args.org else {
            return Err("--org must be provided when activating a theme".into());
        };
        let ctx = get_db_context(&args).await;
        activate_theme(ctx.db, &ctx.namespace, org, theme).await?;
        println!("Theme '{theme}' is now the active theme for organization '{org}'.");
    }

    if args.list_commands || args.command.is_some() {
        let Some(org) = &args.org else {
            return Err("--org must be provided to list or run plugin commands".into());
        };
        let ctx = get_db_context(&args).await;
        let outcome = if let Some(command) = &args.command {
            crate::cli::commands::run(ctx, org, command, &args.command_args, args.command_json.as_deref()).await
        } else {
            crate::cli::commands::list(ctx, org).await
        };
        if let Err(message) = outcome {
            eprintln!("{message}");
            std::process::exit(1);
        }
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
            // `aether --gen` alone: ask what to create and how.
            "" => {
                let around = scaffold::surroundings(&current_path).await;
                let stdin = std::io::stdin();
                let plan = scaffold::run_wizard(stdin.lock(), std::io::stdout(), &current_path, &around)?;
                match plan {
                    scaffold::Plan::Plugin { path, language, details } => {
                        report_plugin(&scaffold::create_plugin_with(&path, language, &details).await?);
                    }
                    scaffold::Plan::Workspace { path, details } => {
                        let created = scaffold::create_workspace(&path, &details).await?;
                        println!("Created workspace '{}' at {}.", created.name, created.directory.display());
                        println!("Add plugins to it with `aether --gen` from inside {}.", created.directory.display());
                    }
                    scaffold::Plan::Config { path } => {
                        let target = current_path.join(&path);
                        generate_default_config_template(target.clone()).await?;
                        println!("Wrote {}.", target.display());
                    }
                }
            }
            "plugin" => {
                let plugin_path = args
                    .plugin_path
                    .as_deref()
                    .map(Path::new)
                    .unwrap_or_else(|| Path::new("my_plugin"));
                let language: Language = args.plugin_language.parse()?;
                let created = create_plugin(plugin_path, language).await?;
                report_plugin(&created);
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

    if args.create_user {
        let (Some(username), Some(email), Some(password)) = (&args.username, &args.email, &args.password) else {
            log::error!("--username, --email and --password must be provided when creating a user");
            return Ok(());
        };
        let ctx = get_db_context(&args).await;
        match create_user(ctx.db, &ctx.namespace, username, email, password).await {
            Ok(_) => {
                println!("Created user '{username}'.");
                // Put them in an organization straight away when one is named.
                if let Some(org_db_name) = &args.org_db_name {
                    match assign_user(ctx.db, &ctx.namespace, username, org_db_name).await {
                        Ok(()) => println!("Assigned user '{username}' to organization '{org_db_name}'."),
                        Err(err) => log::error!("Created the user but could not assign them to '{org_db_name}': {err}"),
                    }
                }
            }
            Err(err) => log::error!("Failed to create user '{username}': {err}"),
        }
        return Ok(());
    }

    if args.grant_developer || args.revoke_developer {
        let on = args.grant_developer;
        if on && args.revoke_developer {
            log::error!("use either --grant-developer or --revoke-developer, not both");
            return Ok(());
        }
        let Some(login) = &args.username else {
            log::error!("--username must name the user whose developer access changes");
            return Ok(());
        };
        let ctx = get_db_context(&args).await;
        let scope = args.org.as_deref();
        match aether_core::org_admin::set_developer_by_login(ctx.db, &ctx.namespace, login, scope, on).await {
            Ok(()) => match (on, scope) {
                (true, Some(org)) => println!("'{login}' is now a developer of '{org}'."),
                (false, Some(org)) => println!("'{login}' is no longer a developer of '{org}'."),
                (true, None) => println!("'{login}' is now an unrestricted developer."),
                (false, None) => println!("'{login}' is no longer an unrestricted developer."),
            },
            Err(err) => log::error!("Failed to change developer access: {err}"),
        }
        return Ok(());
    }

    if args.grant_role.is_some() || args.revoke_role.is_some() || args.list_roles {
        let Some(org) = &args.org else {
            log::error!("--org must name the organization database when working with roles");
            return Ok(());
        };
        let ctx = get_db_context(&args).await;
        if let Some(role) = &args.grant_role {
            let Some(login) = &args.username else {
                log::error!("--username must name the user to give '{role}' to");
                return Ok(());
            };
            match roles::grant(ctx.db, &ctx.namespace, org, login, role).await {
                Ok(()) => println!("'{login}' now has the role '{role}' in '{org}'."),
                Err(err) => log::error!("Failed to give the role: {err}"),
            }
        }
        if let Some(role) = &args.revoke_role {
            let Some(login) = &args.username else {
                log::error!("--username must name the user to take '{role}' from");
                return Ok(());
            };
            match roles::revoke(ctx.db, &ctx.namespace, org, login, role).await {
                Ok(()) => println!("'{login}' no longer has the role '{role}' in '{org}'."),
                Err(err) => log::error!("Failed to take the role away: {err}"),
            }
        }
        if args.list_roles {
            match roles::list_in(ctx.db, &ctx.namespace, org).await {
                Ok(found) if found.is_empty() => println!("No roles in '{org}' yet."),
                Ok(found) => {
                    for role in found {
                        let holders = if role.holders.is_empty() {
                            "nobody".to_string()
                        } else {
                            role.holders
                                .iter()
                                .map(|h| h.display_name.as_str())
                                .collect::<Vec<_>>()
                                .join(", ")
                        };
                        let kind = if role.is_system { "system" } else { "custom" };
                        println!("{:<28} {:<26} {:<8} {}", role.name, role.label, kind, holders);
                    }
                }
                Err(err) => log::error!("Failed to list roles: {err}"),
            }
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
        // The organization's own address; the first user's email is the sensible default.
        let org_email = args.org_email.as_deref().unwrap_or(email.as_str());
        let request = OrganizationRequest {
            name: organization_name,
            email: org_email,
            db_name: args.org_db_name.as_deref(),
            member: FirstMember::User { username, email, password },
            description: None,
            parent_db_name: None,
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

    if let Some(to_login) = &args.transfer_org_ownership {
        let Some(org) = &args.org else {
            log::error!("--org must name the organization when transferring ownership");
            return Ok(());
        };
        let Some(owner_login) = &args.username else {
            log::error!("--username must be the current owner transferring ownership");
            return Ok(());
        };
        let ctx = get_db_context(&args).await;
        ctx.db.use_ns(&ctx.namespace).await?;
        ctx.db.use_db("core").await?;
        let actor_id = match find_user_by_login(ctx.db, owner_login).await {
            Ok(Some(user)) => format!("{}:{}", user.id.table, user.id.key.to_sql()),
            Ok(None) => {
                log::error!("No user '{owner_login}' exists");
                return Ok(());
            }
            Err(err) => {
                log::error!("Looking up '{owner_login}': {err}");
                return Ok(());
            }
        };
        match transfer_ownership(
            ctx.db,
            &ctx.namespace,
            org,
            &actor_id,
            to_login,
            args.transfer_note.as_deref(),
        )
        .await
        {
            Ok(()) => println!(
                "Ownership of '{org}' transferred from '{owner_login}' to '{to_login}'."
            ),
            Err(err) => log::error!("Failed to transfer ownership: {err}"),
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

    if args.migrate {
        let ctx = connect_context(&args).await;
        match migrate_system(ctx.db, &ctx.namespace, &ctx.database).await {
            Ok(result) => print_migrate_summary(&result),
            Err(err) => {
                log::error!("Migrate failed: {err}");
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

    if args.mcp {
        #[cfg(feature = "mcp")]
        if let Err(err) = crate::mcp::run_stdio(mcp_docs_sources(&args)).await {
            log::error!("MCP server failed: {err}");
            std::process::exit(1);
        }
        #[cfg(not(feature = "mcp"))]
        return Err("this build of aether does not include the MCP server (build with the `mcp` feature)".into());
    }

    if let Some(bind) = &args.serve_docs {
        let sources = docs_sources(&args).await;
        if let Err(err) = crate::server::docs::run_docs_server(bind, sources, args.docs_all).await {
            log::error!("Documentation server failed: {err}");
            std::process::exit(1);
        }
    }

    if let Some(_host) = &args.serve {
        let ctx = get_db_context(&args).await;
        if let Err(err) = run_server(
            ctx.config,
            args.http_port,
            ctx.db,
            args.scheduling,
            args.watch,
            ctx.config_path,
        )
        .await
        {
            log::error!("Server failed: {err}");
            std::process::exit(1);
        }
    }

    if let Some(bind) = &args.start_scheduler {
        let ctx = get_db_context(&args).await;
        let bind = Some(bind.as_str()).filter(|bind| !bind.is_empty());
        if let Err(err) = crate::server::scheduler::run_scheduler(ctx.config, ctx.db, bind).await {
            log::error!("Scheduler failed: {err}");
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

/// Connect, and require that the core schema is current. Every command except
/// `--init` / `--migrate` works on an initialized database, so a fresh or stale
/// one gets a plain instruction instead of a "table does not exist" error.
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
                "The database `{}/{}` is missing {} migration(s) shipped with this version of Aether ({}). Run `aether --migrate` to apply them.",
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

/// Where `--serve-docs` and `--mcp` find plugin docs: `--plugin-docs` folders, the `plugin_paths` of the config
/// file, and the catalog when the config reaches a database. All optional: with none of them only Aether's own
/// documentation is served.
pub(crate) async fn docs_sources(args: &Args) -> crate::server::docs::DocsSources {
    use crate::server::docs::DocsSources;
    let mut sources = DocsSources { search_roots: super::db::load_search_roots(args).unwrap_or_default(), ..Default::default() };
    sources.entries.extend(args.plugin_docs.iter().cloned());
    if !matches!(super::db::resolve_config_file(args), Ok(Some(_))) {
        return sources;
    }
    match super::db::get_prerequisites(args).await {
        Ok(ctx) => {
            sources.entries.extend(ctx.config.plugin_paths.iter().cloned());
            // The catalog lives in the `core` database; this handle is private to the docs, so selecting it is safe.
            let db = ctx.db.clone();
            match db.use_ns(&ctx.namespace).use_db(&ctx.database).await {
                Ok(_) => sources.catalog = Some(std::sync::Arc::new(db)),
                Err(e) => log::warn!("Plugin documentation: cannot select the catalog database ({e}); using the plugin folders only"),
            }
        }
        Err(reason) => {
            log::warn!("Plugin documentation: not reading the catalog ({reason}); using the plugin folders only");
            sources.entries.extend(plugin_paths_from_file(args));
        }
    }
    sources
}

/// `plugin_paths` of the config file, read on its own so that a missing database section does not hide the
/// plugin folders. Relative entries are relative to the file.
fn plugin_paths_from_file(args: &Args) -> Vec<PathBuf> {
    let Ok(Some(file)) = super::db::resolve_config_file(args) else { return Vec::new() };
    let file = PathBuf::from(file);
    let Ok(text) = std::fs::read_to_string(&file) else { return Vec::new() };
    let Ok(doc) = text.parse::<toml_edit::DocumentMut>() else { return Vec::new() };
    let base = file.parent().map(Path::to_path_buf).unwrap_or_default();
    doc.get("plugin_paths")
        .and_then(|v| v.as_array())
        .map(|list| list.iter().filter_map(|v| v.as_str()).map(|p| if Path::new(p).is_absolute() { PathBuf::from(p) } else { base.join(p) }).collect())
        .unwrap_or_default()
}

/// Plugin docs for `aether --mcp`: the same folders as `--serve-docs`, but only from disk. The MCP server must
/// answer its handshake at once and has no business opening a database connection for documentation.
#[cfg(feature = "mcp")]
fn mcp_docs_sources(args: &Args) -> crate::server::docs::DocsSources {
    let mut sources = crate::server::docs::DocsSources { search_roots: super::db::load_search_roots(args).unwrap_or_default(), ..Default::default() };
    sources.entries.extend(args.plugin_docs.iter().cloned());
    sources.entries.extend(plugin_paths_from_file(args));
    sources
}
