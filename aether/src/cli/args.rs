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
    /// SurrealDB database name (default: `core`)
    pub db_name: Option<String>,

    #[arg(long)]
    /// SurrealDB user
    pub db_user: Option<String>,

    #[arg(long)]
    /// SurrealDB password
    pub db_password: Option<String>,

    #[arg(long)]
    /// SurrealDB host
    pub db_host: Option<String>,

    #[arg(long, default_value = "8000")]
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

    #[arg(long, action = clap::ArgAction::SetTrue)]
    /// changes the password of a user (e.g. `--change-password username`)
    pub change_password: bool,
}
