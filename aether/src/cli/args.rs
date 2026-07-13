use clap::Parser;


#[derive(Parser, Debug)]
pub struct Args {
    #[arg(short, long)]
    pub verbose: bool,

    #[arg(short, long, default_missing_value = "aether.toml")]
    // aether configuration
    pub config_file: Option<String>,

    #[arg(short = 'q', long)]
    // aether configuration
    pub generate_config_file: Option<String>,

    #[arg(short = 'y', long, default_missing_value = "config")]
    // aether configuration
    pub get_plugins: Option<String>,

    #[arg(long = "gen", num_args = 0..=1, default_missing_value = "workspace")]
    /// Generate a plugin scaffold. Value determines the target (e.g. `workspace`)
    pub generate: Option<String>,

    #[arg(long, default_missing_value = "7890")]
    // port
    pub http_port: Option<u16>,

    #[arg(short, long, default_missing_value = "0.0.0.0:7890", num_args = 0..=1)]
    // Server address to bind to (e.g., 0.0.0.0:7890)
    pub serve: Option<String>,

    #[arg(short = 'w', long, action = clap::ArgAction::SetTrue)]
    // Serves the event listener outside the even bus like
    pub serve_listener: bool,

    #[arg(short = 'l', long = "log", default_value = "debug")]
    /// Logging level (error, warn, info, debug, trace)
    pub log: String,

    #[arg(short = 'e', long = "environment", default_value = "dev")]
    /// Environment mode: `dev` or `prod`
    pub environment: String,

    #[arg(short, long, action = clap::ArgAction::SetTrue)]
    // Generate a default configuration for the system and exit
    pub initialize: bool,

    #[arg(long, action = clap::ArgAction::SetTrue)]
    // Generate a default configuration for the system and exit
    pub seed: bool,

    #[arg(short, long)]
    // Plugins to upgrade (e.g., `--upgrade plugin1 plugin2`)
    pub upgrade: Option<Vec<String>>,

    // database
    // these are the args for the database
    #[arg(long)]
    // Surreal db namespace
    pub db_namespace: Option<String>,

    #[arg(long)]
    // Surreal db namespace
    pub db_name: Option<String>,

    #[arg(long)]
    // Surreal db user
    pub db_user: Option<String>,

    #[arg(long)]
    // Surreal db user password
    pub db_password: Option<String>,

    #[arg(long)]
    // Surreal db host
    pub db_host: Option<String>,

    #[arg(long, default_value = "8000")]
    // Surreal db port
    pub db_port: Option<u16>,
}
