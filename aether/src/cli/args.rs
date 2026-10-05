use std::path::PathBuf;

use aether_core::plugin_manager::catalog::PluginSpec;
use clap::Parser;

#[derive(Parser, Debug)]
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

    #[arg(long = "gen", num_args = 0..=1, default_missing_value = "workspace")]
    /// Generate a scaffold: `workspace`, `plugin`, or `aether_config`
    pub generate: Option<String>,

    #[arg(long, value_name = "PATH")]
    /// Directory of the plugin to generate; its last segment is the plugin name
    pub plugin_path: Option<String>,

    #[arg(long, default_value = "go", value_name = "LANGUAGE")]
    /// Language of a generated plugin: go, rust, typescript, javascript or python
    pub plugin_language: String,

    #[arg(long, default_missing_value = "7890")]
    pub http_port: Option<u16>,

    #[arg(short, long, default_missing_value = "0.0.0.0:7890", num_args = 0..=1)]
    /// Bind address for the HTTP server (e.g. 0.0.0.0:7890)
    pub serve: Option<String>,

    #[arg(short='o', long, default_missing_value = "", num_args = 0..=1)]
    /// Run the job scheduler on its own, with its control API on this address (default: `[scheduler] bind`, 127.0.0.1:7895). The HTTP server finds it by itself. Needs the same database and `app_dir` as the HTTP server.
    pub start_scheduler: Option<String>,

    #[arg(short = 'k', long, action = clap::ArgAction::SetTrue)]
    /// Serve the event listener outside the event bus
    pub serve_listener: bool,

    #[arg(short = 'w', long, action = clap::ArgAction::SetTrue)]
    /// Watch plugins for changes and reload them
    pub watch: bool,

    #[arg(short = 'l', long = "log", default_value = "debug")]
    /// Logging level (error, warn, info, debug, trace)
    pub log: String,

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
        action = clap::ArgAction::SetTrue
    )]
    /// Bootstrap the platform: apply core migrations and create the initial developer account
    pub init: bool,

    #[arg(long, action = clap::ArgAction::SetTrue)]
    /// Seed reference data into the core database
    pub seed: bool,

    #[arg(long, action = clap::ArgAction::SetTrue)]
    /// Delete audit rows older than `[audit] retention_days` from every organization, then exit
    pub purge_audit: bool,

    #[arg(long = "no-scheduling", action = clap::ArgAction::SetFalse)]
    /// Do not run the job scheduler inside `--serve` (another process, `--start-scheduler`, runs the jobs)
    pub scheduling: bool,

    #[arg(short = 'u', long, value_name = "NAME[@VERSION]", num_args = 1.., requires = "org")]
    /// Move an organization's installed plugins to the newest loaded version, or the one named (e.g. `--upgrade-plugin notes --org acme`)
    pub upgrade_plugin: Option<Vec<PluginSpec>>,

    #[arg(long, value_name = "PATH", num_args = 1..)]
    /// Register plugin packages in the core catalog (e.g. `--load-plugin app_dir/plugins/partner`)
    pub load_plugin: Option<Vec<PathBuf>>,

    #[arg(short = 'r', long, value_name = "NAME[@VERSION]", num_args = 1.., requires = "org")]
    /// Install catalog plugins for an organization (e.g. `--install-plugin partner crm@0.2.0 --org acme`)
    pub install_plugin: Option<Vec<PluginSpec>>,

    #[arg(long, value_name = "PATH", num_args = 1..)]
    /// Give the models in a plugin's `models/*.json` their ids and write them back (e.g. `--sync-models plugins/test/notes`)
    pub sync_models: Option<Vec<PathBuf>>,

    #[arg(long, value_name = "DB_NAME")]
    /// Organization database that `--install-plugin`, `--upgrade-plugin` and `--activate-theme` act on
    pub org: Option<String>,

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
    pub create_user: bool,

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

    #[arg(long, value_name = "NAME")]
    /// Create an organization, its first user, and their membership
    pub create_org: Option<String>,

    #[arg(long)]
    /// Database name for the new organization; defaults to a slug from its name
    pub org_db_name: Option<String>,

    #[arg(long, value_name = "DB_NAME")]
    /// Create the folder in app_dir and the media location for an organization that already exists
    pub provision_org: Option<String>,

    #[arg(long, value_name = "LOGIN")]
    /// Assign an existing user by username or email to an organization (requires `--org-db-name`)
    pub assign_user: Option<String>,
}
