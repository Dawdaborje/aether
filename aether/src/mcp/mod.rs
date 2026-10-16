//! `aether --mcp`: a Model Context Protocol server over stdio, so an LLM client can read Aether's documentation
//! and build plugins with it. The documentation is compiled into the binary (`aether_docs`), so the server needs no
//! files, no config and no database.
//!
//! stdout carries only protocol messages: logs go to stderr (see `cli/logging.rs`).

mod server;
pub mod tools;

use rmcp::{ServiceExt, transport::stdio};

pub async fn run_stdio() -> Result<(), Box<dyn std::error::Error>> {
    log::info!("MCP server on stdio ({} documentation pages)", aether_docs::bundle().docs.len());
    let running = server::AetherMcp::new().serve(stdio()).await?;
    running.waiting().await?;
    Ok(())
}
