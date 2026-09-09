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
    /// Destination path for generated plugin projects
    pub plugin_path: Option<String>,

    #[arg(long, default_value = "go", value_name = "LANGUAGE")]
    /// Plugin language passed to Extism (go, rust, python, javascript, typescript)
    pub plugin_language: String,

    #[arg(long, default_missing_value = "7890")]
    pub http_port: Option<u16>,

    #[arg(short, long, default_missing_value = "0.0.0.0:7890", num_args = 0..=1)]
    /// Bind address for the HTTP server (e.g. 0.0.0.0:7890)
    pub serve: Option<String>,

    #[arg(short = 'k', long, action = clap::ArgAction::SetTrue)]
    /// Serve the event listener outside the event bus
    pub serve_listener: bool,

    #[arg(short = 'w', long, action = clap::ArgAction::SetTrue)]
    /// Watch plugins for changes and reload them
    pub watch: bool,

    #[arg(short = 'l', long = "log", default_value = "debug")]
    /// Logging level (error, warn, info, debug, trace)
    pub log: String,

    #[arg(short = 'e', long = "environment", default_value = "dev")]
    /// Environment mode: `dev` or `prod`
    pub environment: String,

    #[arg(
        short = 'i',
        long = "init",
        alias = "initialize",
        action = clap::ArgAction::SetTrue
    )]
    /// Bootstrap the platform: apply core migrations and create the initial superuser
    pub init: bool,

    #[arg(long, action = clap::ArgAction::SetTrue)]
    /// Seed reference data into the core database
    pub seed: bool,

    #[arg(short, long)]
    /// Plugins to upgrade (e.g. `--upgrade-plugin plugin1 plugin2`)
    pub upgrade_plugin: Option<Vec<String>>,

    #[arg(short = 'r', long)]
    /// Plugins to install (e.g. `--install-plugin plugin1 plugin2`)
    pub install_plugin: Option<Vec<String>>,

    #[arg(long)]
    /// SurrealDB namespace (default: from config, or `main`)
    pub db_namespace: Option<String>,

    #[arg(long)]
    /// SurrealDB user
    pub db_user: Option<String>,

    #[arg(long)]
    /// SurrealDB password
    pub db_password: Option<String>,

    #[arg(long)]
    /// SurrealDB host
    pub db_host: Option<String>,

    #[arg(long)]
    /// SurrealDB port
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
    /// Initial superuser username used by `--init` (default: `admin`)
    pub admin_username: Option<String>,

    #[arg(long)]
    /// Initial superuser email used by `--init` (default: `admin@localhost`)
    pub admin_email: Option<String>,

    #[arg(long)]
    /// Initial superuser password used by `--init`; generated when omitted
    pub admin_password: Option<String>,

    #[arg(long, value_name = "USERNAME")]
    /// Change a user's password (e.g. `--change-password admin --password new-secret`)
    pub change_password: Option<String>,

    #[arg(long, value_name = "NAME")]
    /// Create an organization, company, user, and their memberships
    pub create_org: Option<String>,

    #[arg(long)]
    /// Database name for the new organization; defaults to a slug from its name
    pub org_db_name: Option<String>,

    #[arg(long)]
    /// Company name for `--create-org`
    pub company_name: Option<String>,

    #[arg(long)]
    /// Company email for `--create-org`
    pub company_email: Option<String>,

    #[arg(long, value_name = "LOGIN")]
    /// Assign an existing user by username or email to an organization and company
    pub assign_user: Option<String>,
}
