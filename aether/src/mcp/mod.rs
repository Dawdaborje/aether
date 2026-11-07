//! `aether --mcp`: a Model Context Protocol server over stdio, so an LLM client can read Aether's documentation
//! and build plugins with it. The documentation is compiled into the binary (`aether_docs`), so the server needs no
//! files, no config and no database. The docs that plugins ship are added from the plugin folders given with
//! `--plugin-docs` and the `plugin_paths` of the config file (read from disk; no database is contacted).
//!
//! stdout carries only protocol messages: logs go to stderr (see `cli/logging.rs`).

mod live;
mod server;
pub mod tools;

pub use live::Live;
use crate::server::docs::DocsSources;
use rmcp::{ServiceExt, transport::stdio};

pub async fn run_stdio(sources: DocsSources, live: Option<Live>) -> Result<(), Box<dyn std::error::Error>> {
    log::info!(
        "MCP server on stdio ({} documentation pages, {} plugin folder(s))",
        aether_docs::bundle().docs.len(),
        sources.entries.len()
    );
    if let Some(live) = &live {
        log::info!("MCP live mode: organization `{}`, writing {}", live.org, if live.allow_write { "ON" } else { "off" });
    }
    let running = server::AetherMcp::new(sources, live).serve(stdio()).await?;
    running.waiting().await?;
    Ok(())
}
