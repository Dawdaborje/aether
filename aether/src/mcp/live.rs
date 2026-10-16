//! Live mode of `aether --mcp`: tools that work on a real Aether instance (`-c aether.toml --org <org>`).
//!
//! Everything goes through the same functions the command line uses (`--install-plugin`, `--command`, ...), as the
//! actor `system:mcp:<os user>`, and is recorded in the plugin activity log. Differences from the command line:
//! nothing is printed (stdout is the protocol), and the organization is fixed at start-up, so the model cannot
//! wander to other organizations.
//!
//! The database is opened on first use, not at start-up, so a down database is a tool error and never delays the
//! handshake. Writing (loading, installing, upgrading, running commands) needs `--mcp-allow-write`.

use crate::cli::args::Args;
use crate::cli::db::{get_prerequisites, load_search_roots};
use crate::cli::plugin_manager::check_plugins;
use aether_core::plugin_activity::{Actor, Entry, kind, record_in};
use aether_core::plugin_manager::catalog::{self, PluginSpec};
use aether_core::plugin_manager::commands::{self, CommandError, LifecyclePhase};
use aether_core::plugin_manager::load::load_into_catalog;
use aether_core::plugin_manager::load_paths::resolve_load_paths;
use aether_core::state::AppState;
use std::fmt::Write;
use std::path::PathBuf;
use std::sync::Arc;
use tokio::sync::OnceCell;

/// The live tools, and the ones among them that change things.
pub const READ_TOOLS: &[&str] = &["list_installed_plugins", "list_catalog_plugins", "get_org_models", "list_plugin_commands", "check_plugin_against_catalog"];
pub const WRITE_TOOLS: &[&str] = &["load_plugin", "install_plugin", "upgrade_plugin", "run_command"];

pub struct Live {
    args: Args,
    pub org: String,
    pub allow_write: bool,
    ctx: OnceCell<Result<Arc<Ctx>, String>>,
}

struct Ctx {
    db: &'static surrealdb::Surreal<surrealdb::engine::remote::ws::Client>,
    namespace: String,
    database: String,
    app_dir: PathBuf,
    search_roots: Vec<PathBuf>,
    state: AppState,
}

impl Live {
    pub fn new(args: Args, org: String, allow_write: bool) -> Self {
        Self { args, org, allow_write, ctx: OnceCell::new() }
    }

    async fn ctx(&self) -> Result<Arc<Ctx>, String> {
        self.ctx
            .get_or_init(|| async {
                let ctx = get_prerequisites(&self.args).await.map_err(|reason| {
                    format!("Cannot reach the Aether instance: {reason}. Live tools need a config file (-c aether.toml) with the database settings, and a running SurrealDB.")
                })?;
                let search_roots = load_search_roots(&self.args).unwrap_or_default();
                let app_dir = ctx.config.app_dir.clone();
                let (namespace, database) = (ctx.namespace.clone(), ctx.database.clone());
                let db = ctx.db;
                let state = AppState::new(ctx.db.clone(), ctx.config, ctx.namespace, ctx.database, ctx.config_path)
                    .await
                    .map_err(|error| format!("could not start the kernel: {error}"))?;
                Ok(Arc::new(Ctx { db, namespace, database, app_dir, search_roots, state }))
            })
            .await
            .clone()
    }

    fn require_write(&self) -> Result<(), String> {
        if self.allow_write {
            Ok(())
        } else {
            Err(format!("Writing is off. Start the server with --mcp-allow-write to let it load, install or upgrade plugins and run commands in `{}`.", self.org))
        }
    }

    fn actor(&self) -> Actor {
        Actor::system(&format!("mcp:{}", crate::cli::commands::operating_system_user()))
    }

    fn actor_name(&self) -> String {
        format!("system:mcp:{}", crate::cli::commands::operating_system_user())
    }

    // ---- reading ----

    pub async fn list_installed_plugins(&self) -> Result<String, String> {
        let ctx = self.ctx().await?;
        let rows = installed(&ctx, &self.org).await?;
        Ok(format_installed(&self.org, &rows))
    }

    pub async fn list_catalog_plugins(&self) -> Result<String, String> {
        let ctx = self.ctx().await?;
        let core = ctx.state.core().await.map_err(|e| e.to_string())?;
        let mut response = core
            .query("SELECT name, version, label, description FROM plugins WHERE is_active = true ORDER BY name, date_created DESC;")
            .await
            .and_then(|r| r.check())
            .map_err(|e| e.to_string())?;
        let rows: Vec<serde_json::Value> = response.take(0).map_err(|e| e.to_string())?;
        Ok(format_catalog(&rows))
    }

    pub async fn get_org_models(&self, plugin: Option<&str>) -> Result<String, String> {
        let ctx = self.ctx().await?;
        let core = ctx.state.core().await.map_err(|e| e.to_string())?;
        let mut out = String::new();
        for (name, version, enabled) in installed(&ctx, &self.org).await? {
            if plugin.is_some_and(|p| p != name) {
                continue;
            }
            let mut response = core
                .query("SELECT name, label, definition FROM plugin_models WHERE plugin IN (SELECT VALUE id FROM plugins WHERE name = $name AND version = $version);")
                .bind(("name", name.clone()))
                .bind(("version", version.clone()))
                .await
                .and_then(|r| r.check())
                .map_err(|e| e.to_string())?;
            let models: Vec<serde_json::Value> = response.take(0).map_err(|e| e.to_string())?;
            if models.is_empty() {
                continue;
            }
            let _ = writeln!(out, "## {name} {version}{}", if enabled { "" } else { " (disabled)" });
            for model in &models {
                out.push_str(&format_model(model));
            }
            out.push('\n');
        }
        if out.is_empty() {
            return Ok(match plugin {
                Some(p) => format!("`{}` has no plugin `{p}` installed with models.", self.org),
                None => format!("No plugin installed in `{}` defines models.", self.org),
            });
        }
        Ok(format!("Models of the plugins installed in `{}`. Fields are name:type, `*` marks required.\n\n{out}", self.org))
    }

    pub async fn list_plugin_commands(&self) -> Result<String, String> {
        let ctx = self.ctx().await?;
        let found = commands::list(&ctx.state, &self.org).await.map_err(|e| e.to_string())?;
        if found.is_empty() {
            return Ok(format!("No plugin installed in `{}` offers commands.", self.org));
        }
        let mut out = format!("Commands of the plugins installed in `{}`. Run one with run_command.\n", self.org);
        let mut plugin = "";
        for info in &found {
            if info.plugin != plugin {
                plugin = &info.plugin;
                let _ = writeln!(out, "\n{}{}", info.plugin, if info.enabled { "" } else { "  (disabled)" });
            }
            let _ = writeln!(out, "  {}.{} — {}", info.plugin, info.def.name, info.def.help);
            for arg in &info.def.args {
                let mut note = vec![];
                if !arg.help.is_empty() {
                    note.push(arg.help.clone());
                }
                if arg.required {
                    note.push("required".into());
                }
                if let Some(default) = &arg.default {
                    note.push(format!("default {default}"));
                }
                let _ = writeln!(out, "      {}: {}  {}", arg.name, arg.kind(), note.join("; "));
            }
        }
        Ok(out)
    }

    /// The load-time checks that need the catalog (links to other plugins' models, components from other plugins).
    pub async fn check_plugin_against_catalog(&self, path: &str) -> Result<String, String> {
        let ctx = self.ctx().await?;
        let plan = resolve_load_paths(&[PathBuf::from(path.trim())], &ctx.search_roots).await.map_err(|e| e.to_string())?;
        let report = check_plugins(ctx.db, &ctx.namespace, &ctx.database, &ctx.app_dir, &plan.packages).await.map_err(|e| e.to_string())?;
        if report.errors.is_empty() {
            let names: Vec<String> = report.ok.iter().map(|p| format!("{} {}", p.name, p.version)).collect();
            Ok(format!("VALID against the catalog: {}. Nothing was loaded; load_plugin (needs --mcp-allow-write) registers it.", names.join(", ")))
        } else {
            let problems: Vec<String> = report.errors.iter().map(|(p, e)| format!("- {}: {e}", p.display())).collect();
            Err(format!("INVALID against the catalog:\n{}\n\nFix these and check again.", problems.join("\n")))
        }
    }

    // ---- writing ----

    pub async fn load_plugin(&self, path: &str) -> Result<String, String> {
        self.require_write()?;
        let ctx = self.ctx().await?;
        let loaded = load_into_catalog(ctx.db, &ctx.namespace, &ctx.database, &ctx.app_dir, None, &[PathBuf::from(path.trim())], &ctx.search_roots)
            .await
            .map_err(|e| e.to_string())?;
        for plugin in loaded.iter().filter(|p| p.created) {
            record_in(ctx.db, &ctx.namespace, &ctx.database, &entry(kind::CATALOG_LOAD, &plugin.name, None, Some(&plugin.version), None, "mcp:load", self.actor())).await;
        }
        let lines: Vec<String> = loaded.iter().map(|p| format!("- {} {}{}", p.name, p.version, if p.created { " (new)" } else { " (unchanged)" })).collect();
        Ok(format!("Loaded into the catalog:\n{}\n\nIt is not installed anywhere yet: install_plugin installs it in `{}`.", lines.join("\n"), self.org))
    }

    pub async fn install_plugin(&self, plugin: &str, version: Option<&str>) -> Result<String, String> {
        self.require_write()?;
        let ctx = self.ctx().await?;
        let spec = spec(plugin, version)?;
        let report = catalog::install_plugins(ctx.db, &ctx.namespace, &ctx.database, &self.org, &[spec]).await.map_err(|e| e.to_string())?;
        for (name, v) in &report.installed {
            record_in(ctx.db, &ctx.namespace, &ctx.database, &entry(kind::INSTALL, name, None, Some(v), Some(&self.org), "mcp:install", self.actor())).await;
        }
        let names: Vec<String> = report.installed.iter().map(|(n, _)| n.clone()).collect();
        let seeds = self.seeds(&ctx, &names, LifecyclePhase::Install).await;
        let mut out = format!(
            "Installed in `{}`: {}.\nAlready installed: {}.",
            self.org,
            list_or_none(report.installed.iter().map(|(n, v)| format!("{n} {v}"))),
            list_or_none(report.already_installed.iter().map(|(n, v)| format!("{n} {v}")))
        );
        out.push_str(&seeds);
        Ok(out)
    }

    pub async fn upgrade_plugin(&self, plugin: &str, version: Option<&str>) -> Result<String, String> {
        self.require_write()?;
        let ctx = self.ctx().await?;
        let spec = spec(plugin, version)?;
        let report = catalog::upgrade_plugins(ctx.db, &ctx.namespace, &ctx.database, &self.org, &[spec]).await.map_err(|e| e.to_string())?;
        for (name, from, to) in &report.upgraded {
            record_in(ctx.db, &ctx.namespace, &ctx.database, &entry(kind::UPGRADE, name, Some(from), Some(to), Some(&self.org), "mcp:upgrade", self.actor())).await;
        }
        let names: Vec<String> = report.upgraded.iter().map(|(n, _, _)| n.clone()).collect();
        let seeds = self.seeds(&ctx, &names, LifecyclePhase::Upgrade).await;
        Ok(format!(
            "Upgraded in `{}`: {}.\nAlready current: {}.{seeds}",
            self.org,
            list_or_none(report.upgraded.iter().map(|(n, f, t)| format!("{n} {f} -> {t}"))),
            list_or_none(report.already_current.iter().map(|(n, v)| format!("{n} {v}")))
        ))
    }

    pub async fn run_command(&self, command: &str, arguments: Option<&serde_json::Value>) -> Result<String, String> {
        self.require_write()?;
        let Some((plugin, name)) = command.split_once('.') else {
            return Err(format!("`{command}` is not <plugin>.<command>; list_plugin_commands shows what is available"));
        };
        let ctx = self.ctx().await?;
        let json = arguments.filter(|a| !a.is_null()).map(|a| a.to_string());
        match commands::run(&ctx.state, &self.org, plugin, name, &[], json.as_deref(), &self.actor_name()).await {
            Ok(value) => serde_json::to_string_pretty(&value).map_err(|e| e.to_string()),
            Err(CommandError::Failed(message)) => Err(format!("{command} failed: {message}")),
            Err(error) => Err(error.to_string()),
        }
    }

    /// The `on_install` / `on_upgrade` seed commands, as the command line runs them; a note about each.
    async fn seeds(&self, ctx: &Ctx, plugins: &[String], phase: LifecyclePhase) -> String {
        if plugins.is_empty() {
            return String::new();
        }
        match commands::run_lifecycle(&ctx.state, &self.org, plugins, phase, &self.actor_name()).await {
            Ok(reports) => reports
                .iter()
                .map(|r| format!("\nSeed {}.{}: {}", r.plugin, r.command, if r.ok { "ok".to_string() } else { format!("FAILED — {}", r.detail.as_deref().unwrap_or("unknown error")) }))
                .collect(),
            Err(e) => format!("\nSeed commands could not run: {e}"),
        }
    }
}

async fn installed(ctx: &Ctx, org: &str) -> Result<Vec<(String, String, bool)>, String> {
    let db = ctx.state.org(org).await.map_err(|e| format!("cannot open organization `{org}`: {e}"))?;
    let rows = aether_core::plugin_manager::installed::list_cached(&ctx.state.cache, org, &db).await.map_err(|e| e.to_string())?;
    Ok(rows.into_iter().map(|r| (r.plugin_name, r.version, r.is_enabled)).collect())
}

fn spec(plugin: &str, version: Option<&str>) -> Result<PluginSpec, String> {
    let raw = match version.map(str::trim).filter(|v| !v.is_empty()) {
        Some(v) => format!("{}@{v}", plugin.trim()),
        None => plugin.trim().to_string(),
    };
    raw.parse::<PluginSpec>().map_err(|e| e.to_string())
}

fn entry(kind: &'static str, plugin: &str, from: Option<&String>, to: Option<&String>, org: Option<&str>, source: &str, actor: Actor) -> Entry {
    Entry {
        kind,
        plugin: plugin.to_string(),
        from_version: from.cloned(),
        to_version: to.cloned(),
        organization: org.map(str::to_string),
        actor,
        source: source.into(),
        ok: true,
        error: None,
        detail: serde_json::Value::Null,
    }
}

// ---- text for the model (pure, tested) ----

fn list_or_none(items: impl Iterator<Item = String>) -> String {
    let v: Vec<String> = items.collect();
    if v.is_empty() { "none".into() } else { v.join(", ") }
}

pub fn format_installed(org: &str, rows: &[(String, String, bool)]) -> String {
    if rows.is_empty() {
        return format!("No plugin is installed in `{org}`.");
    }
    let mut out = format!("Plugins installed in `{org}`:\n");
    for (name, version, enabled) in rows {
        let _ = writeln!(out, "- {name} {version}{}", if *enabled { "" } else { " (disabled)" });
    }
    out
}

/// Rows `{name, version, label, description}` newest version first per plugin.
pub fn format_catalog(rows: &[serde_json::Value]) -> String {
    if rows.is_empty() {
        return "The catalog has no active plugin. load_plugin (needs --mcp-allow-write) registers one.".into();
    }
    let text = |row: &serde_json::Value, key: &str| row.get(key).and_then(|v| v.as_str()).unwrap_or("").to_string();
    let mut out = String::from("Plugins in the catalog (newest version first). install_plugin installs one in the organization.\n");
    let mut last = String::new();
    for row in rows {
        let name = text(row, "name");
        if name != last {
            let (label, description) = (text(row, "label"), text(row, "description"));
            let _ = write!(out, "\n- {name}");
            if !label.is_empty() {
                let _ = write!(out, " — {label}");
            }
            if !description.is_empty() {
                let _ = write!(out, ": {}", description.split_whitespace().collect::<Vec<_>>().join(" "));
            }
            out.push_str("\n  versions:");
            last = name;
        }
        let _ = write!(out, " {}", text(row, "version"));
    }
    out.push('\n');
    out
}

/// A `plugin_models` row `{name, label, definition}` as one line per field.
pub fn format_model(row: &serde_json::Value) -> String {
    let name = row.get("name").and_then(|v| v.as_str()).unwrap_or("?");
    let label = row.get("label").and_then(|v| v.as_str()).filter(|l| !l.is_empty() && *l != name);
    let mut out = format!("- {name}{}\n", label.map(|l| format!(" ({l})")).unwrap_or_default());
    let fields = row.pointer("/definition/fields").and_then(|f| f.as_array()).cloned().unwrap_or_default();
    for field in fields {
        let fname = field.get("name").and_then(|v| v.as_str()).unwrap_or("?");
        let kind = match field.get("type") {
            Some(serde_json::Value::String(s)) => s.clone(),
            Some(other) => other.to_string(),
            None => "?".into(),
        };
        let required = field.get("required").and_then(|v| v.as_bool()).unwrap_or(false);
        let _ = writeln!(out, "    {fname}:{kind}{}", if required { "*" } else { "" });
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn installed_plugins_read_clearly() {
        assert!(format_installed("acme", &[]).contains("No plugin is installed in `acme`"));
        let out = format_installed("acme", &[("crm".into(), "0.2.0".into(), true), ("hr".into(), "1.0.0".into(), false)]);
        assert!(out.contains("- crm 0.2.0\n") && out.contains("- hr 1.0.0 (disabled)"), "{out}");
    }

    #[test]
    fn the_catalog_groups_versions_per_plugin() {
        let rows = vec![
            json!({"name": "crm", "version": "0.2.0", "label": "CRM", "description": "Customers\n  and deals"}),
            json!({"name": "crm", "version": "0.1.0", "label": "CRM", "description": "Customers"}),
            json!({"name": "hr", "version": "1.0.0", "label": null, "description": null}),
        ];
        let out = format_catalog(&rows);
        assert!(out.contains("- crm — CRM: Customers and deals\n  versions: 0.2.0 0.1.0"), "{out}");
        assert!(out.contains("- hr\n  versions: 1.0.0"), "{out}");
        assert!(format_catalog(&[]).contains("no active plugin"));
    }

    #[test]
    fn models_list_their_fields() {
        let row = json!({"name": "lead", "label": "Lead", "definition": {"fields": [
            {"name": "title", "type": "text", "required": true},
            {"name": "value", "type": "decimal"},
            {"name": "owner", "type": {"link": "user"}}
        ]}});
        let out = format_model(&row);
        assert!(out.starts_with("- lead (Lead)\n"), "{out}");
        assert!(out.contains("    title:text*\n") && out.contains("    value:decimal\n") && out.contains("owner:{\"link\":\"user\"}"), "{out}");
        assert_eq!(format_model(&json!({"name": "x", "label": "x"})), "- x\n");
    }

    #[test]
    fn specs_combine_name_and_version() {
        assert_eq!(spec("crm", None).unwrap().version, None);
        assert_eq!(spec(" crm ", Some("0.2.0")).unwrap().version.as_deref(), Some("0.2.0"));
        assert!(spec("", None).is_err());
    }
}
