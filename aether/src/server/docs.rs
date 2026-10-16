//! `aether --serve-docs`: the documentation website on its own.
//!
//! Needs no config file and no database; the pages are compiled into the binary. (`aether --serve`
//! mounts the same site at `[docs] path`, see `serve.rs`.)

use super::serve::wait_for_shutdown_signal;
use std::error::Error;

pub async fn run_docs_server(bind: &str) -> Result<(), Box<dyn Error>> {
    let listener = tokio::net::TcpListener::bind(bind).await?;
    let addr = listener.local_addr()?;
    log::info!("Serving the documentation: http://{addr}");
    if addr.ip().is_unspecified() {
        log::info!("Serving the documentation: http://{}:{}", super::serve::get_local_ip(), addr.port());
    }
    axum::serve(listener, aether_docs::router(""))
        .with_graceful_shutdown(wait_for_shutdown_signal())
        .await?;
    Ok(())
}
