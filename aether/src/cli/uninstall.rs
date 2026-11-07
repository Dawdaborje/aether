//! `--uninstall-plugin`, `--purge`, `--list-orphans` and `--purge-orphans`.

use std::io::{BufRead, Write};

use aether_core_api::plugin_activity::{self, Actor, Entry, kind};
use aether_core_api::plugin_commands::LifecyclePhase;
use aether_core::plugin_manager::uninstall::{
    self, PluginData, UninstallError, UninstallMode,
};

use super::args::Args;
use super::db::DbContext;

pub async fn run(ctx: DbContext, org: &str, args: &Args) -> Result<(), Box<dyn std::error::Error>> {
    let db = ctx.db;
    let namespace = ctx.namespace.clone();
    let core = ctx.database.clone();
    let actor = Actor::system(&format!("cli:{}", super::commands::operating_system_user()));

    if args.list_orphans {
        let orphans = uninstall::list_orphans(db, &namespace, &core, org).await?;
        print_orphans(org, &orphans);
    }

    if let Some(names) = &args.purge_orphans {
        for name in names {
            let data = uninstall::orphan_data(db, &namespace, &core, org, name).await?;
            print_data("Will drop", &data);
            if !confirm(org, args.yes)? {
                return Err("purge cancelled; nothing was dropped".into());
            }
            backup(db, org, &data, args).await?;
            uninstall::purge_orphans(db, &namespace, &core, org, name).await?;
            println!("Dropped the data of '{name}' in organization '{org}'.");
            record(db, &namespace, &core, org, kind::PURGE, name, None, &actor, "cli:purge", &data).await;
        }
    }

    if let Some(names) = &args.uninstall_plugin {
        let mode = if args.purge { UninstallMode::Purge } else { UninstallMode::KeepData };
        let going = uninstall::check_uninstall(db, &namespace, &core, org, names).await?;
        if mode == UninstallMode::Purge {
            for (name, _) in &going {
                let data = uninstall::owned_data(db, name).await?;
                print_data("Will drop", &data);
            }
            if !confirm(org, args.yes)? {
                return Err("uninstall cancelled; nothing was changed".into());
            }
            for (name, _) in &going {
                let data = uninstall::owned_data(db, name).await?;
                backup(db, org, &data, args).await?;
            }
        }
        // The plugin's own cleanup runs while it is still installed.
        let hooked: Vec<String> = going.iter().map(|(name, _)| name.clone()).collect();
        if let Err(message) = super::commands::run_lifecycle_seeds(ctx, org, &hooked, LifecyclePhase::Uninstall).await {
            eprintln!("on_uninstall hooks could not run: {message}; continuing");
        }
        let report = uninstall::uninstall_plugins(db, &namespace, &core, org, names, mode).await?;
        for (name, version) in &report.uninstalled {
            println!("Uninstalled plugin '{name}@{version}' from organization '{org}'.");
            let data = report
                .kept
                .iter()
                .chain(&report.purged)
                .find(|data| &data.plugin == name);
            let entry_kind = if mode == UninstallMode::Purge { kind::PURGE } else { kind::UNINSTALL };
            record(db, &namespace, &core, org, entry_kind, name, Some(version), &actor, "cli:uninstall", &data.cloned().unwrap_or(PluginData { plugin: name.clone(), tables: Vec::new() })).await;
        }
        for data in &report.kept {
            print_data("Kept", data);
            println!(
                "  Install '{}' again to pick the data up, or drop it with --purge-orphans {} --org {org}.",
                data.plugin, data.plugin
            );
        }
        for data in &report.purged {
            print_data("Dropped", data);
        }
    }
    Ok(())
}

fn print_data(verb: &str, data: &PluginData) {
    println!("{verb} {} table(s) of '{}' ({} record(s)):", data.tables.len(), data.plugin, data.rows());
    for table in &data.tables {
        println!("  {} ({} record(s))", table.table, table.rows);
    }
}

fn print_orphans(org: &str, orphans: &[PluginData]) {
    if orphans.is_empty() {
        println!("No plugin has left data behind in organization '{org}'.");
        return;
    }
    for data in orphans {
        print_data("Orphaned:", data);
    }
}

/// Ask for the organization's name; `--yes` answers for the person.
fn confirm(org: &str, yes: bool) -> Result<bool, std::io::Error> {
    if yes {
        return Ok(true);
    }
    print!("This cannot be undone. Type the organization name ({org}) to continue: ");
    std::io::stdout().flush()?;
    let mut line = String::new();
    std::io::stdin().lock().read_line(&mut line)?;
    Ok(line.trim() == org)
}

async fn backup(
    db: &surrealdb::Surreal<surrealdb::engine::remote::ws::Client>,
    org: &str,
    data: &PluginData,
    args: &Args,
) -> Result<(), UninstallError> {
    if args.no_backup || data.tables.is_empty() {
        return Ok(());
    }
    let path = uninstall::backup_tables(db, org, data, &args.backup_dir).await?;
    println!("Backed up the records of '{}' to {}.", data.plugin, path.display());
    Ok(())
}

#[allow(clippy::too_many_arguments)]
async fn record(
    db: &surrealdb::Surreal<surrealdb::engine::remote::ws::Client>,
    namespace: &str,
    core: &str,
    org: &str,
    entry_kind: &'static str,
    plugin: &str,
    version: Option<&String>,
    actor: &Actor,
    source: &str,
    data: &PluginData,
) {
    plugin_activity::record_in(
        db,
        namespace,
        core,
        &Entry {
            kind: entry_kind,
            plugin: plugin.to_string(),
            from_version: version.cloned(),
            to_version: None,
            organization: Some(org.to_string()),
            actor: actor.clone(),
            source: source.to_string(),
            ok: true,
            error: None,
            detail: serde_json::json!({
                "tables": data.tables.iter().map(|table| &table.table).collect::<Vec<_>>(),
                "records": data.rows(),
            }),
        },
    )
    .await;
}
