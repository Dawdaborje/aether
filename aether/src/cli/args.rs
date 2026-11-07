use std::path::PathBuf;

use aether_core::plugin_manager::catalog::PluginSpec;
use clap::Parser;

/// What `--migrate` brings up to date.
#[derive(clap::ValueEnum, Debug, Clone, Copy, PartialEq, Eq)]
pub enum MigrateTarget {
    /// The core database and every organization database.
    All,
    /// Only the core database.
    Core,
}

#[derive(Parser, Debug, Clone)]
#[command(name = "aether", about = "Aether kernel CLI")]
pub struct Args {
    #[arg(short, long)]
    pub verbose: bool,

    #[arg(short, long, default_missing_value = "aether.toml")]
    /// Path to aether.toml
    pub config_file: Option<String>,

    #[arg(short = 'q', long)]
    /// Write a default config template to this filename
    pub generate_config_file: Option<String>,

    #[arg(short = 'y', long, default_missing_value = "config")]
    pub get_plugins: Option<String>,

    #[arg(long = "gen", num_args = 0..=1, default_missing_value = "")]
    /// Create a plugin, a workspace or a configuration file. On its own (`aether --gen`) it asks what and how;
    /// or name it: `plugin` (with --plugin-path and --plugin-language), `workspace` or `aether_config`
    pub generate: Option<String>,

    #[arg(long, value_name = "PATH")]
    /// Directory of the plugin to generate; its last segment is the plugin name
    pub plugin_path: Option<String>,

    #[arg(long, default_value = "go", value_name = "LANGUAGE")]
    /// Language of a generated plugin: go, rust, typescript, javascript, python, rhai or lua (a script, with nothing to compile)
    pub plugin_language: String,

    #[arg(long, default_missing_value = "7890")]
    pub http_port: Option<u16>,

    #[arg(short, long, default_missing_value = "0.0.0.0:7890", num_args = 0..=1)]
    /// Bind address for the HTTP server (e.g. 0.0.0.0:7890)
    pub serve: Option<String>,

    #[arg(long, value_name = "DIR", conflicts_with_all = ["serve", "serve_docs", "mcp"])]
    /// Write Aether's documentation website as static HTML files into DIR, for any web host (GitHub Pages, S3, nginx). Needs no config file and no database. There is no search box in the copy
    pub export_docs: Option<PathBuf>,

    #[arg(long, value_name = "PATH", default_value = "", requires = "export_docs")]
    /// With --export-docs: the path the files will be served under, such as /aether-docs for a GitHub Pages project site. Default: the root of the host
    pub docs_base: String,

    #[arg(long, value_name = "ADDR", default_missing_value = "127.0.0.1:7899", num_args = 0..=1, conflicts_with = "serve")]
    /// Serve the documentation website on its own (default 127.0.0.1:7899). Needs no config file and no database: the pages are compiled into this binary
    pub serve_docs: Option<String>,

    #[arg(long, action = clap::ArgAction::SetTrue, requires = "mcp")]
    /// With --mcp and --org: let the LLM change the instance (load, install and upgrade plugins, run plugin commands). Without it live mode only reads
    pub mcp_allow_write: bool,

    #[arg(long = "plugin-docs", value_name = "PATH")]
    /// With --serve-docs or --mcp: a plugin package or workspace folder whose `[docs]` to include (repeat for several). The `plugin_paths` of the config file are included too
    pub plugin_docs: Vec<PathBuf>,

    #[arg(long, action = clap::ArgAction::SetTrue, requires = "serve_docs")]
    /// Preview mode for --serve-docs: show the docs of every plugin whatever its `visibility`. Only allowed on a loopback address (127.0.0.1), since anyone who can reach the server would see them
    pub docs_all: bool,

    #[arg(long, action = clap::ArgAction::SetTrue, conflicts_with_all = ["serve", "serve_docs"])]
    /// Run the Model Context Protocol server on stdin/stdout: documentation, plugin scaffolding and plugin validation for LLM clients. Needs no config file and no database. Logs go to stderr
    pub mcp: bool,

    #[arg(long, action = clap::ArgAction::SetTrue, conflicts_with_all = ["serve", "serve_docs", "mcp"])]
    /// Register `aether --mcp` with Claude Code and Cursor for every project (`aether setup-mcp` is the same). Add --mcp-remove to undo it
    pub setup_mcp: bool,

    #[arg(long, value_name = "CLIENT", default_value = "all", requires = "setup_mcp")]
    /// With --setup-mcp: `claude`, `cursor` or `all` (the ones found on this machine)
    pub mcp_client: String,

    #[arg(long, action = clap::ArgAction::SetTrue, requires = "setup_mcp")]
    /// With --setup-mcp: remove the registration instead
    pub mcp_remove: bool,

    #[arg(short='o', long, default_missing_value = "", num_args = 0..=1)]
    /// Run the job scheduler on its own, with its control API on this address (default: `[scheduler] bind`, 127.0.0.1:7895). The HTTP server finds it by itself. Needs the same database and `app_dir` as the HTTP server.
    pub start_scheduler: Option<String>,

    #[arg(short = 'k', long, action = clap::ArgAction::SetTrue)]
    /// Serve the event listener outside the event bus
    pub serve_listener: bool,

    #[arg(short = 'w', long, action = clap::ArgAction::SetTrue)]
    /// With --serve, reload plugins when their source folders change (development only: every organization that has a changed plugin installed is moved to the new revision)
    pub watch: bool,

    #[arg(short = 'l', long = "log", default_value = "debug")]
    /// Logging level (error, warn, info, debug, trace)
    pub log: String,

    #[arg(long = "log-file", value_name = "PATH")]
    /// Append logs to this file (creates parent directories). Overrides `[logging]` in the config.
    pub log_file: Option<String>,

    #[arg(long = "log-dir", value_name = "PATH")]
    /// Directory for `aether.log` (or `[logging].filename`). Overrides `[logging].dir`.
    pub log_dir: Option<String>,

    #[arg(long = "no-log-file", action = clap::ArgAction::SetTrue)]
    /// Do not write a log file, even when `[logging] file = true`
    pub no_log_file: bool,

    #[arg(long = "no-log-stdout", action = clap::ArgAction::SetTrue)]
    /// When writing a log file, do not also write to stderr
    pub no_log_stdout: bool,

    #[arg(long, value_name = "PATH")]
    /// Directory Aether stores plugins (WASM, manifests), compiled views and plugin configs in
    pub app_dir: Option<String>,

    #[arg(long, value_name = "PATH")]
    /// Directory for uploaded media when `[media] backend = "local"` (overrides the config file)
    pub media_dir: Option<String>,

    #[arg(short = 'e', long = "env", default_value = "dev")]
    /// Environment mode: `dev` or `prod`
    pub environment: String,

    #[arg(
        short = 'i',
        long = "init",
        alias = "initialize",
        action = clap::ArgAction::SetTrue,
        conflicts_with = "migrate"
    )]
    /// Bootstrap the platform: apply core migrations, seed settings, and create the initial developer account
    pub init: bool,

    #[arg(
        long = "migrate",
        value_name = "WHAT",
        num_args = 0..=1,
        default_missing_value = "all",
        conflicts_with = "init"
    )]
    /// Apply pending migrations (no seed, no developer account): `all` (the default) brings the
    /// core database and every organization database up to date, `core` only the core database
    pub migrate: Option<MigrateTarget>,

    #[arg(long, action = clap::ArgAction::SetTrue)]
    /// Seed reference data into the core database
    pub seed: bool,

    #[arg(long, action = clap::ArgAction::SetTrue)]
    /// Delete audit rows older than `[audit] retention_days` from every organization, then exit
    pub purge_audit: bool,

    #[arg(long, action = clap::ArgAction::SetTrue, requires = "org")]
    /// Check the audit chains of the organization database named by --org (every hash, link and checkpoint) and exit with 1 if anything is wrong
    pub audit_verify: bool,

    #[arg(long, value_name = "FILE", requires = "org")]
    /// With --audit-verify: also prove that this anchor file (from --audit-export-anchor) is part of the chain
    pub audit_anchor: Option<PathBuf>,

    #[arg(long, value_name = "DIR", requires = "org")]
    /// Write the newest checkpoint of each audit chain of the organization database named by --org into DIR, to keep somewhere the server's administrator cannot rewrite, and exit
    pub audit_export_anchor: Option<PathBuf>,

    #[arg(long = "no-scheduling", action = clap::ArgAction::SetFalse)]
    /// Do not run the job scheduler inside `--serve` (another process, `--start-scheduler`, runs the jobs)
    pub scheduling: bool,

    #[arg(
        short = 'u',
        long,
        visible_alias = "upgrade",
        value_name = "NAME[@VERSION]|all",
        num_args = 1..,
        requires = "org"
    )]
    /// Move an organization's installed plugins to the newest loaded version, or the one named
    /// (e.g. `--upgrade-plugin notes --org acme`). Use `all` to upgrade every installed plugin
    /// (`aether --upgrade all --org acme`).
    pub upgrade_plugin: Option<Vec<PluginSpec>>,

    #[arg(long, value_name = "COLOUR")]
    /// Print light and dark colour tokens for a theme, made from one seed colour (e.g.
    /// `aether --theme-palette "#6750a4"`). Paste the result into a theme's `theme.json`.
    pub theme_palette: Option<String>,

    #[arg(long, value_name = "SCHEME", default_value = "tonal", requires = "theme_palette")]
    /// With `--theme-palette`: `tonal` (tinted surfaces), `neutral`, `vivid` or `mono`
    pub palette_scheme: String,

    #[arg(long, value_name = "PATH|WORKSPACE", num_args = 0..)]
    /// Register plugin packages in the core catalog. Each argument may be a
    /// package directory, a workspace directory, or a workspace name such as
    /// `base` (looked up as `./base` or `./plugins/base`). Workspace members
    /// load in dependency order. With no arguments (`aether -c aether.toml
    /// --load-plugins`): every entry of top-level `plugin_paths` in the config.
    /// With `--dry-run`, every package is checked and all errors are reported
    /// without writing to the catalog.
    pub load_plugins: Option<Vec<PathBuf>>,

    #[arg(long, action = clap::ArgAction::SetTrue, requires = "load_plugins")]
    /// With `--load-plugins`: validate every package and print every error, without
    /// registering anything in the catalog
    pub dry_run: bool,

    #[arg(short = 'r', long, value_name = "NAME[@VERSION]", num_args = 1.., requires = "org")]
    /// Install catalog plugins for an organization (e.g. `--install-plugin partner crm@0.2.0 --org acme`)
    pub install_plugin: Option<Vec<PluginSpec>>,

    #[arg(long, value_name = "NAME", num_args = 1.., requires = "org")]
    /// Uninstall plugins from an organization and keep their data (tables and records stay, orphaned; installing the plugin again picks them up). Add `--purge` to drop the data too (e.g. `--uninstall-plugin crm --org acme`)
    pub uninstall_plugin: Option<Vec<String>>,

    #[arg(long, action = clap::ArgAction::SetTrue, requires = "uninstall_plugin")]
    /// With `--uninstall-plugin`: also drop the plugin's tables. Final: asks you to type the organization name, and backs the records up first
    pub purge: bool,

    #[arg(long, action = clap::ArgAction::SetTrue)]
    /// With `--purge` or `--purge-orphans`: do not ask for the typed confirmation
    pub yes: bool,

    #[arg(long, value_name = "DIR", default_value = "purge-backups")]
    /// With `--purge` or `--purge-orphans`: where the records are written before the tables are dropped
    pub backup_dir: PathBuf,

    #[arg(long, action = clap::ArgAction::SetTrue)]
    /// With `--purge` or `--purge-orphans`: skip the backup
    pub no_backup: bool,

    #[arg(long, action = clap::ArgAction::SetTrue, requires = "org")]
    /// List the data left behind by plugins that are no longer installed in the organization
    pub list_orphans: bool,

    #[arg(long, value_name = "NAME", num_args = 1.., requires = "org")]
    /// Drop the data an uninstalled plugin left behind (final; asks for confirmation, backs up first)
    pub purge_orphans: Option<Vec<String>>,

    #[arg(long, value_name = "PATH", num_args = 1..)]
    /// Give the models in a plugin's `models/*.json` their ids and write them back (e.g. `--sync-models plugins/test/notes`)
    pub sync_models: Option<Vec<PathBuf>>,

    #[arg(long, value_name = "DB_NAME")]
    /// Organization database that `--install-plugin`, `--upgrade-plugin` and `--activate-theme` act on
    pub org: Option<String>,

    #[arg(long = "command", value_name = "PLUGIN.COMMAND", requires = "org")]
    /// Run a command a plugin offers, in an organization (e.g. `--command currency.import_rates --org acme --arg date=2026-10-05`). `--list-commands` shows what is available
    pub command: Option<String>,

    #[arg(long = "arg", value_name = "KEY=VALUE", action = clap::ArgAction::Append, requires = "command")]
    /// An argument of the command (repeat for several); the plugin declares their names and types
    pub command_args: Vec<String>,

    #[arg(long = "json", value_name = "JSON", requires = "command")]
    /// The command's arguments as a JSON object, instead of or together with `--arg`
    pub command_json: Option<String>,

    #[arg(long = "list-commands", action = clap::ArgAction::SetTrue, requires = "org")]
    /// List the commands of every plugin installed in the organization (`--org`)
    pub list_commands: bool,

    #[arg(long, value_name = "THEME", requires = "org")]
    /// Make an installed theme the organization's active theme (e.g. `--activate-theme ocean --org acme`)
    pub activate_theme: Option<String>,

    #[arg(long)]
    /// SurrealDB namespace; overrides `namespace` under [database] (required if neither is set)
    pub db_namespace: Option<String>,

    #[arg(long)]
    /// SurrealDB user; overrides `user` under [database]
    pub db_user: Option<String>,

    #[arg(long)]
    /// SurrealDB password; overrides `password` under [database]
    pub db_password: Option<String>,

    #[arg(long)]
    /// SurrealDB host; overrides `host` under [database]
    pub db_host: Option<String>,

    #[arg(long)]
    /// SurrealDB port; overrides `port` under [database]
    pub db_port: Option<u16>,

    // User space
    #[arg(long, action = clap::ArgAction::SetTrue)]
    /// Create a login account (`--username`, `--email`, `--password`); `--org-db-name` also puts it in that organization
    pub create_user: bool,

    #[arg(long, value_name = "ROLE")]
    /// Give a role to the user named by `--username` (a username or email) in the organization `--org`, such as `hr.hr_manager` or `org_admin`
    pub grant_role: Option<String>,

    #[arg(long, value_name = "ROLE")]
    /// Take a role away from the user named by `--username` in the organization `--org`
    pub revoke_role: Option<String>,

    #[arg(long, action = clap::ArgAction::SetTrue)]
    /// List the roles of the organization `--org` and who holds them
    pub list_roles: bool,

    #[arg(long, action = clap::ArgAction::SetTrue)]
    /// Make the user named by `--username` a developer: of the organization `--org` only (they see its
    /// schedules), or, without `--org`, an unrestricted developer of the whole system
    pub grant_developer: bool,

    #[arg(long, action = clap::ArgAction::SetTrue)]
    /// Take developer access away from the user named by `--username`: in `--org`, or the unrestricted kind without it
    pub revoke_developer: bool,

    #[arg(long)]
    pub username: Option<String>,

    #[arg(long)]
    pub email: Option<String>,

    #[arg(long)]
    pub password: Option<String>,

    #[arg(long)]
    /// Initial developer username used by `--init` (default: `admin`)
    pub admin_username: Option<String>,

    #[arg(long)]
    /// Initial developer email used by `--init` (default: `admin@localhost`)
    pub admin_email: Option<String>,

    #[arg(long)]
    /// Initial developer password used by `--init`; generated when omitted
    pub admin_password: Option<String>,

    #[arg(long, value_name = "USERNAME")]
    /// Change a user's password (e.g. `--change-password admin --password new-secret`)
    pub change_password: Option<String>,

    #[arg(long)]
    /// Database name of an existing organization (`--assign-user`)
    pub org_db_name: Option<String>,

    #[arg(long, value_name = "DB_NAME")]
    /// Create the folder in app_dir and the media location for an organization that already exists
    pub provision_org: Option<String>,

    #[arg(long, value_name = "LOGIN")]
    /// Assign an existing user by username or email to an organization (requires `--org-db-name`)
    pub assign_user: Option<String>,

    #[arg(long, value_name = "LOGIN")]
    /// Transfer organization ownership to this member (`--org` and `--username` of the current owner)
    pub transfer_org_ownership: Option<String>,

    #[arg(long, value_name = "TEXT")]
    /// Optional note stored on an ownership transfer
    pub transfer_note: Option<String>,
}
