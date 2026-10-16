//! `aether --mcp`: a Model Context Protocol server over stdio, so an LLM client can read Aether's documentation
//! and build plugins with it. The documentation is compiled into the binary (`aether_docs`), so the server needs no
//! files, no config and no database. The docs that plugins ship are added from the plugin folders given with
//! `--plugin-docs` and the `plugin_paths` of the config file (read from disk; no database is contacted).
//!
//! stdout carries only protocol messages: logs go to stderr (see `cli/logging.rs`).

mod server;
pub mod tools;

use crate::server::docs::DocsSources;
use rmcp::{ServiceExt, transport::stdio};

pub async fn run_stdio(sources: DocsSources) -> Result<(), Box<dyn std::error::Error>> {
    log::info!(
        "MCP server on stdio ({} documentation pages, {} plugin folder(s))",
        aether_docs::bundle().docs.len(),
        sources.entries.len()
    );
    let running = server::AetherMcp::new(sources).serve(stdio()).await?;
    running.waiting().await?;
    Ok(())
}
