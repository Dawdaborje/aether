//! `aether --command` and `aether --list-commands`: the command line's way into plugins.
//!
//! The command runs in the same process as the CLI: it opens the database, finds the plugin's
//! installed version in the organization and calls the function, as the kernel's `system:cli`
//! actor. The server does not need to be running.

use aether_core::{
    plugin_manager::commands::{self, CommandError},
    state::AppState,
};

use super::db::DbContext;

/// The operating system user, safe to put in an audit record.
fn operating_system_user() -> String {
    let name = std::env::var("USER").or_else(|_| std::env::var("USERNAME")).unwrap_or_default();
    let clean: String = name
        .chars()
        .filter(|c| c.is_ascii_alphanumeric() || matches!(c, '_' | '-' | '.'))
        .take(32)
        .collect();
    if clean.is_empty() { "unknown".to_string() } else { clean }
}

async fn state_for(ctx: DbContext) -> Result<AppState, String> {
    AppState::new(ctx.db.clone(), ctx.config, ctx.namespace, ctx.database)
        .await
        .map_err(|error| format!("could not start the kernel: {error}"))
}

/// Print the commands of the plugins installed in `org`.
pub async fn list(ctx: DbContext, org: &str) -> Result<(), String> {
    let state = state_for(ctx).await?;
    let found = commands::list(&state, org).await.map_err(|error| error.to_string())?;
    if found.is_empty() {
        println!("No plugin installed in '{org}' offers commands.");
        return Ok(());
    }
    let mut plugin = "";
    for info in &found {
        if info.plugin != plugin {
            plugin = &info.plugin;
            println!("{}{}", info.plugin, if info.enabled { "" } else { "  (disabled)" });
        }
        let help = if info.def.help.is_empty() { "" } else { info.def.help.as_str() };
        println!("  {:<24}{help}", info.def.name);
        for arg in &info.def.args {
            let note = [
                (!arg.help.is_empty()).then(|| arg.help.clone()),
                arg.required.then(|| "required".to_string()),
                arg.default.as_ref().map(|default| format!("default {default}")),
            ]
            .into_iter()
            .flatten()
            .collect::<Vec<_>>()
            .join("; ");
            println!("      --arg {}=<{}>  {note}", arg.name, arg.kind());
        }
    }
    println!("\nRun one with: aether --command <plugin>.<command> --org {org} [--arg key=value ...]");
    Ok(())
}

/// Run `plugin.command` in `org` and print what the function returned.
pub async fn run(ctx: DbContext, org: &str, command: &str, args: &[String], json: Option<&str>) -> Result<(), String> {
    let Some((plugin, name)) = command.split_once('.') else {
        return Err(format!("`{command}` is not <plugin>.<command>; --list-commands shows what is available"));
    };
    let state = state_for(ctx).await?;
    let actor = format!("system:cli:{}", operating_system_user());
    match commands::run(&state, org, plugin, name, args, json, &actor).await {
        Ok(value) => {
            let text = serde_json::to_string_pretty(&value).map_err(|error| error.to_string())?;
            println!("{text}");
            Ok(())
        }
        Err(CommandError::Failed(message)) => Err(format!("{command} failed: {message}")),
        Err(error) => Err(error.to_string()),
    }
}
