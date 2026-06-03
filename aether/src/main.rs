use aether_core::config_manager::model::gen_config_file;
use axum::{
    Json, Router,
    http::StatusCode,
    routing::{get, post},
};
use clap::Parser;
use std::env;

#[derive(Parser, Debug)]
struct Args {
    #[arg(short, long)]
    verbose: bool,

    #[arg(short, long)]
    config_file: Option<String>,
}

#[tokio::main]
async fn main() {
    let args = Args::parse();

    match args.config_file {
        Some(file) => gen_config_file(&file),
        None => {
            // Use default config
        }
    }

    let app = Router::new();
    let listener = tokio::net::TcpListener::bind("0.0.0.0:3000").await.unwrap();
    axum::serve(listener, app).await.unwrap();
}
