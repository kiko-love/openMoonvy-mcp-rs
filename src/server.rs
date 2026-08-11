/**
 * Moonvy MCP server: tools over the pure API client.
 * PoC scope: moonvy_get_design / moonvy_get_tree / moonvy_extract_tokens.
 */

use std::sync::Arc;

use rmcp::{
    ErrorData as McpError, ServerHandler,
    handler::server::router::tool::ToolRouter,
    handler::server::wrapper::Parameters,
    model::{Implementation, ServerCapabilities, ServerInfo},
    schemars, tool, tool_handler, tool_router,
};

use crate::api::{MoonvyApi, parse_moonvy_url};
use crate::genome::{Tokens, TreeOptions, extract_design_meta, extract_tokens, extract_tree};

#[derive(Debug, Clone)]
pub struct MoonvyServer {
    tool_router: ToolRouter<Self>,
    api: Arc<MoonvyApi>,
}

impl MoonvyServer {
    pub fn new(api: Arc<MoonvyApi>) -> Self {
        Self { tool_router: Self::tool_router(), api }
    }

    fn node_id(url: &str) -> anyhow::Result<String> {
        let ids = parse_moonvy_url(url).ok_or_else(|| anyhow::anyhow!("Could not parse Moonvy URL"))?;
        ids.file_id.or(ids.dir_id).ok_or_else(|| anyhow::anyhow!("No file or directory ID in URL"))
    }

    async fn genome_for(&self, url: &str) -> anyhow::Result<crate::genome::Genome> {
        let ids = parse_moonvy_url(url).ok_or_else(|| anyhow::anyhow!("Could not parse Moonvy URL"))?;
        let node_id = ids.file_id.or(ids.dir_id).ok_or_else(|| anyhow::anyhow!("No file or directory ID in URL"))?;
        let node = self.api.get_node(&ids.project_id, &node_id, "full").await?;
        let genome_url = node
            .files
            .and_then(|f| f.genome)
            .and_then(|g| g.url)
            .ok_or_else(|| anyhow::anyhow!("No genome file found for node \"{node_id}\""))?;
        self.api.fetch_genome(&genome_url).await
    }

    fn tool_error<E: std::fmt::Display>(error: E) -> McpError {
        McpError::internal_error(error.to_string(), None)
    }
}

/* ------------------------------- tool inputs ------------------------------- */

#[derive(Debug, serde::Deserialize, schemars::JsonSchema)]
pub struct GetDesignRequest {
    /// Moonvy design URL
    pub url: String,
}

#[derive(Debug, serde::Deserialize, schemars::JsonSchema)]
pub struct GetTreeRequest {
    /// Moonvy design URL
    pub url: String,
    /// Optional frame/page ID filter
    pub frame: Option<String>,
    /// Include normalized style data for every node
    #[serde(default)]
    pub with_style: bool,
    /// Maximum child depth to include
    #[serde(default = "default_max_depth")]
    pub max_depth: usize,
    /// Drop empty container groups and lift their children up
    #[serde(default)]
    pub skip_empty_groups: bool,
    /// Emit coordinates relative to the artboard origin
    #[serde(default)]
    pub flatten: bool,
    /// Keep only nodes of these types (children filtered recursively)
    pub only: Option<Vec<String>>,
    /// Annotate nodes whose content repeats an earlier node with duplicateOf
    #[serde(default)]
    pub detect_duplicates: bool,
}

fn default_max_depth() -> usize {
    99
}

#[derive(Debug, serde::Deserialize, schemars::JsonSchema)]
pub struct GetTokensRequest {
    /// Moonvy design URL
    pub url: String,
}

/* ---------------------------------- tools --------------------------------- */

#[tool_router(router = tool_router)]
impl MoonvyServer {
    #[tool(name = "moonvy_get_design", description = "Design metadata: title and frame dimensions. Returns { items: [ { title, frameCount, frames: [{id,name,width,height}] } ] }.")]
    async fn get_design(&self, Parameters(req): Parameters<GetDesignRequest>) -> Result<String, McpError> {
        let genome = self.genome_for(&req.url).await.map_err(Self::tool_error)?;
        let ids = parse_moonvy_url(&req.url).and_then(|ids| ids.file_id.or(ids.dir_id));
        let node_name = match ids {
            Some(id) => self.api.get_node(&parse_moonvy_url(&req.url).unwrap().project_id, &id, "base").await.ok().and_then(|n| n.name),
            None => None,
        };
        let meta = extract_design_meta(&genome, node_name.as_deref());
        Ok(serde_json::to_string(&serde_json::json!({ "items": [meta] })).map_err(Self::tool_error)?)
    }

    #[tool(name = "moonvy_get_tree", description = "Full layer tree. Returns { items: [ {id,name,type,x,y,width,height,text?,style?,children?,duplicateOf?} ] }. Supports skipEmptyGroups (drop empty containers), flatten (artboard-relative coordinates), only (type filter), detectDuplicates.")]
    async fn get_tree(&self, Parameters(req): Parameters<GetTreeRequest>) -> Result<String, McpError> {
        let genome = self.genome_for(&req.url).await.map_err(Self::tool_error)?;
        let options = TreeOptions {
            with_style: req.with_style,
            max_depth: req.max_depth,
            skip_empty_groups: req.skip_empty_groups,
            flatten: req.flatten,
            only: req.only,
            detect_duplicates: req.detect_duplicates,
        };
        let tree = extract_tree(&genome, req.frame.as_deref(), &options);
        Ok(serde_json::to_string(&serde_json::json!({ "items": tree })).map_err(Self::tool_error)?)
    }

    #[tool(name = "moonvy_extract_tokens", description = "Design tokens: colors, fontSizes, radii, spacing. Returns { items: [ {colors:[...],fontSizes:[...],radii:[...],spacing:[...]} ] }.")]
    async fn extract_tokens(&self, Parameters(req): Parameters<GetTokensRequest>) -> Result<String, McpError> {
        let genome = self.genome_for(&req.url).await.map_err(Self::tool_error)?;
        let tokens: Tokens = extract_tokens(&genome);
        Ok(serde_json::to_string(&serde_json::json!({ "items": [tokens] })).map_err(Self::tool_error)?)
    }
}

/* ------------------------------- server impl ------------------------------- */

#[tool_handler(router = self.tool_router)]
impl ServerHandler for MoonvyServer {
    fn get_info(&self) -> ServerInfo {
        ServerInfo::new(ServerCapabilities::builder().enable_tools().build())
            .with_server_info(Implementation::from_build_env())
            .with_instructions(
                "Moonvy design extraction server (Rust). Requires a Moonvy auth token: \
                 set MOONVY_TOKEN or save ~/.moonvy-ai/token.json. \
                 Tools: moonvy_get_design, moonvy_get_tree (skipEmptyGroups/flatten/only/detectDuplicates), \
                 moonvy_extract_tokens.",
            )
    }
}
