use aether::server::serve::run_server;
use aether_core::config_manager::models::gen_config_file;
use clap::Parser;

#[derive(Parser, Debug)]
struct Args {
    #[arg(short, long)]
    verbose: bool,

    #[arg(short, long, default_missing_value = "aether.conf.toml")]
    config_file: Option<String>,

    #[arg(short, long, default_missing_value = "0.0.0.0:7890", num_args = 0..=1)]
    serve: Option<String>,
}

#[tokio::main]
async fn main() {
    let args = Args::parse();

    if let Some(host) = args.serve {
        let config_file = args
            .config_file
            .unwrap_or_else(|| "aether.conf.toml".to_string());
        let config = gen_config_file(&config_file).unwrap_or_else(|err| {
            eprintln!("Failed to generate config file: {err}");
            std::process::exit(1);
        });
        let _ = run_server(&host, &config_file, config).await;
    }
}
