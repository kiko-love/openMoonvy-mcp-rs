/**
 * openMoonvy-mcp-rs — Moonvy design extraction MCP server (Rust).
 *
 * PoC scope: moonvy_get_design / moonvy_get_tree / moonvy_extract_tokens
 * over the pure Moonvy API (Bearer token, no browser at runtime).
 */

mod api;
mod catalog;
mod genome;
mod server;
mod token;
mod tools;

use std::sync::Arc;

use rmcp::transport::stdio;
use rmcp::ServiceExt;

use api::MoonvyApi;
use server::MoonvyServer;

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    let token = token::load_token()?;
    let api = Arc::new(MoonvyApi::new(token)?);
    let server = MoonvyServer::new(api);

    let service = server.serve(stdio()).await?;
    service.waiting().await?;
    Ok(())
}
