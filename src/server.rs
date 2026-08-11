/**
 * Moonvy MCP server: full tool set. Business logic lives in tools.rs;
 * this module only declares tool schemas and routes calls.
 */

use std::sync::Arc;

use rmcp::{
    ErrorData as McpError, ServerHandler,
    handler::server::router::tool::ToolRouter,
    handler::server::wrapper::Parameters,
    model::{Implementation, ServerCapabilities, ServerInfo},
    schemars, tool, tool_handler, tool_router,
};
use serde::Deserialize;
use serde_json::json;

use crate::api::MoonvyApi;
use crate::catalog::{Catalog, resolve_workspace_dir};
use crate::genome::TreeOptions;
use crate::tools;

#[derive(Debug, Clone)]
pub struct MoonvyServer {
    tool_router: ToolRouter<Self>,
    api: Arc<MoonvyApi>,
}

impl MoonvyServer {
    pub fn new(api: Arc<MoonvyApi>) -> Self {
        Self { tool_router: Self::tool_router(), api }
    }
}

/* ------------------------------- tool inputs ------------------------------- */

#[derive(Debug, Deserialize, schemars::JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct GetDesignRequest {
    /// Moonvy design URL
    pub url: String,
}

#[derive(Debug, Deserialize, schemars::JsonSchema)]
#[serde(rename_all = "camelCase")]
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
    /// Include the deduplicated asset manifest (hash -> url)
    #[serde(default)]
    pub include_assets: bool,
}

fn default_max_depth() -> usize {
    99
}

#[derive(Debug, Deserialize, schemars::JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct GetTokensRequest {
    /// Moonvy design URL
    pub url: String,
}

#[derive(Debug, Deserialize, schemars::JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct ListPagesRequest {
    /// Moonvy project URL
    pub url: String,
    /// Maximum pages/files to return
    #[serde(default = "default_limit_500")]
    pub limit: u32,
    /// Maximum API pages to scan
    #[serde(default = "default_max_pages_50")]
    pub max_pages: u32,
}

fn default_limit_500() -> u32 {
    500
}
fn default_max_pages_50() -> u32 {
    50
}

#[derive(Debug, Deserialize, schemars::JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct ListLayersRequest {
    /// Moonvy design URL
    pub url: String,
    /// Optional frame/page ID filter
    pub frame: Option<String>,
    /// Maximum layers to return
    #[serde(default = "default_limit_50")]
    pub limit: u32,
}

fn default_limit_50() -> u32 {
    50
}

#[derive(Debug, Deserialize, schemars::JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct NodeStyleRequest {
    /// Moonvy design URL
    pub url: String,
    /// Moonvy node ID
    pub node: String,
}

#[derive(Debug, Deserialize, schemars::JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct ContextRequest {
    /// Moonvy design URL (design file or project directory URL)
    pub url: String,
    /// Maximum child depth for the layer tree
    #[serde(default = "default_max_depth")]
    pub max_depth: usize,
    /// Drop empty container groups and lift their children up
    #[serde(default)]
    pub skip_empty_groups: bool,
    /// Emit coordinates relative to the artboard origin
    #[serde(default)]
    pub flatten: bool,
    /// Keep only nodes of these types
    pub only: Option<Vec<String>>,
    /// Include the deduplicated asset manifest
    #[serde(default)]
    pub include_assets: bool,
}

#[derive(Debug, Deserialize, schemars::JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct DownloadAssetRequest {
    /// Moonvy design or project URL
    pub url: String,
    /// Moonvy node ID or file UUID
    pub node: String,
    /// Asset type: slice, snapshot, or image (autodetected)
    pub r#type: Option<String>,
    /// Slice format/ratio: svg, base, max
    pub slice_format: Option<String>,
    /// Custom name for the downloaded file
    pub name: Option<String>,
    /// Absolute output directory or file path
    pub out: Option<String>,
}

#[derive(Debug, Deserialize, schemars::JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct SyncProjectRequest {
    /// Moonvy project URL
    pub project_url: String,
    /// Absolute path to the real frontend project root
    pub workspace_dir: String,
    /// Optional display name for this Moonvy source
    pub name: Option<String>,
    /// Node types to include in catalog (defaults to ["design"])
    pub types: Option<Vec<String>>,
    /// Maximum pages/files to return
    #[serde(default = "default_limit_500")]
    pub limit: u32,
    /// Maximum API pages to scan
    #[serde(default = "default_max_pages_50")]
    pub max_pages: u32,
}

#[derive(Debug, Deserialize, schemars::JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct SearchDesignsRequest {
    /// Design name, alias, tag, URL, ID, or frontend file path
    pub query: String,
    /// Absolute path to the frontend project root
    pub workspace_dir: String,
    /// Maximum matches to return
    #[serde(default = "default_limit_20")]
    pub limit: u32,
}

fn default_limit_20() -> u32 {
    20
}

#[derive(Debug, Deserialize, schemars::JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct TreeByNameRequest {
    /// Design name, alias, tag, URL, ID, or frontend file path
    pub name: String,
    /// Absolute path to the frontend project root
    pub workspace_dir: String,
    /// Optional frame/page ID filter
    pub frame: Option<String>,
    /// Include normalized style data
    #[serde(default = "default_true")]
    pub with_style: bool,
    /// Maximum child depth
    #[serde(default = "default_max_depth")]
    pub max_depth: usize,
}

fn default_true() -> bool {
    true
}

#[derive(Debug, Deserialize, schemars::JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct DiffDesignsRequest {
    /// First Moonvy design URL (e.g. the normal state)
    pub url_a: String,
    /// Second Moonvy design URL (e.g. the hover state)
    pub url_b: String,
    /// Optional frame/page ID filter applied to both designs
    pub frame: Option<String>,
    /// Include style in the comparison
    #[serde(default = "default_true")]
    pub with_style: bool,
}

/* ---------------------------------- tools ---------------------------------- */

#[tool_router(router = tool_router)]
impl MoonvyServer {
    #[tool(name = "moonvy_get_design", description = "Design metadata: title and frame dimensions. Returns { items: [ { title, frameCount, frames: [{id,name,width,height}] } ] }. Accepts a design file URL or a project directory URL (auto-resolved).")]
    async fn get_design(&self, Parameters(req): Parameters<GetDesignRequest>) -> Result<String, McpError> {
        let (_, genome) = tools::genome_for_url(&self.api, &req.url).await.map_err(tools::tool_error)?;
        let meta = crate::genome::extract_design_meta(&genome, None);
        tools::json_string(json!({ "items": [meta] }))
    }

    #[tool(name = "moonvy_get_tree", description = "Full layer tree. Returns { items: [ {id,name,type,x,y,width,height,text?,style?,children?,duplicateOf?} ], assets? }. Supports skipEmptyGroups, flatten, only, detectDuplicates, includeAssets.")]
    async fn get_tree(&self, Parameters(req): Parameters<GetTreeRequest>) -> Result<String, McpError> {
        let (_, genome) = tools::genome_for_url(&self.api, &req.url).await.map_err(tools::tool_error)?;
        let options = TreeOptions {
            with_style: req.with_style,
            max_depth: req.max_depth,
            skip_empty_groups: req.skip_empty_groups,
            flatten: req.flatten,
            only: req.only,
            detect_duplicates: req.detect_duplicates,
        };
        tools::json_string(tools::tree_payload(&genome, req.frame.as_deref(), &options, req.include_assets))
    }

    #[tool(name = "moonvy_extract_tokens", description = "Design tokens: colors, fontSizes, radii, spacing. Returns { items: [ {colors:[...],fontSizes:[...],radii:[...],spacing:[...]} ] }.")]
    async fn extract_tokens(&self, Parameters(req): Parameters<GetTokensRequest>) -> Result<String, McpError> {
        let (_, genome) = tools::genome_for_url(&self.api, &req.url).await.map_err(tools::tool_error)?;
        let tokens = crate::genome::extract_tokens(&genome);
        tools::json_string(json!({ "items": [tokens] }))
    }

    #[tool(name = "moonvy_list_pages", description = "List pages/files in a Moonvy project. Returns { items: [{id,name,type,url,preview?}] }. Start here to discover design file URLs from a project directory URL.")]
    async fn list_pages(&self, Parameters(req): Parameters<ListPagesRequest>) -> Result<String, McpError> {
        let rows = tools::list_pages(&self.api, &req.url, req.limit, req.max_pages).await.map_err(tools::tool_error)?;
        tools::json_string(json!({ "items": rows }))
    }

    #[tool(name = "moonvy_list_layers", description = "Flattened layer list: id, name, type, x, y, width, height. Returns { items: [...] }. Use this to discover valid node IDs.")]
    async fn list_layers(&self, Parameters(req): Parameters<ListLayersRequest>) -> Result<String, McpError> {
        let (_, genome) = tools::genome_for_url(&self.api, &req.url).await.map_err(tools::tool_error)?;
        let layers = crate::genome::extract_layers(&genome, req.frame.as_deref(), req.limit as usize);
        tools::json_string(json!({ "items": layers }))
    }

    #[tool(name = "moonvy_get_node_style", description = "Normalized style of one node: background, color, fontSize, fontWeight, borderRadius, opacity, strokeWidth, strokeColor, gradient. Returns { items: [...] } (null when not set).")]
    async fn get_node_style(&self, Parameters(req): Parameters<NodeStyleRequest>) -> Result<String, McpError> {
        let (_, genome) = tools::genome_for_url(&self.api, &req.url).await.map_err(tools::tool_error)?;
        let rows = crate::genome::extract_node_style(&genome, &req.node);
        tools::json_string(json!({ "items": rows }))
    }

    #[tool(name = "moonvy_get_design_context", description = "One-call bundle: { design: metadata, tree: { items, assets? }, tokens: { items } }. THE recommended entry point for design work.")]
    async fn get_design_context(&self, Parameters(req): Parameters<ContextRequest>) -> Result<String, McpError> {
        let (_, genome) = tools::genome_for_url(&self.api, &req.url).await.map_err(tools::tool_error)?;
        let meta = crate::genome::extract_design_meta(&genome, None);
        let options = TreeOptions {
            with_style: true,
            max_depth: req.max_depth,
            skip_empty_groups: req.skip_empty_groups,
            flatten: req.flatten,
            only: req.only,
            detect_duplicates: false,
        };
        let tree = tools::tree_payload(&genome, None, &options, req.include_assets);
        let tokens = crate::genome::extract_tokens(&genome);
        tools::json_string(json!({ "design": meta, "tree": tree, "tokens": { "items": [tokens] } }))
    }

    #[tool(name = "moonvy_download_asset", description = "Download a slice, snapshot or image fill from a Moonvy node. Returns { items: [ {success,path,size,name,url} ] }. out must be an absolute directory or file path.")]
    async fn download_asset(&self, Parameters(req): Parameters<DownloadAssetRequest>) -> Result<String, McpError> {
        let (save_path, size, file_name, url) = tools::download_asset(
            &self.api,
            &req.url,
            &req.node,
            req.r#type.as_deref(),
            req.slice_format.as_deref(),
            req.name.as_deref(),
            req.out.as_deref(),
        )
        .await
        .map_err(tools::tool_error)?;
        tools::json_string(json!({ "items": [{ "success": true, "path": save_path.to_string_lossy(), "size": size, "name": file_name, "url": url }] }))
    }

    #[tool(name = "moonvy_sync_project", description = "Scan a Moonvy project and write .moonvy-mcp/catalog.json (design index) into the frontend workspace. Run once per project; afterwards moonvy_search_designs and moonvy_get_tree_by_name resolve designs by name.")]
    async fn sync_project(&self, Parameters(req): Parameters<SyncProjectRequest>) -> Result<String, McpError> {
        let workspace = resolve_workspace_dir(&req.workspace_dir).map_err(tools::tool_error)?;
        let clean_url = tools::sanitize_url(&req.project_url);
        let mut previous = Catalog::load(&workspace).map_err(tools::tool_error)?;
        let rows = tools::list_pages(&self.api, &req.project_url, req.limit, req.max_pages).await.map_err(tools::tool_error)?;
        let include_types: Vec<String> = req
            .types
            .clone()
            .unwrap_or_else(|| vec!["design".to_string()])
            .iter()
            .map(|t| t.to_lowercase())
            .collect();
        let now = tools::now_iso();
        let designs = tools::normalize_catalog_designs(&rows, &include_types, &previous, &now);

        previous.sources.retain(|s| s.url != clean_url);
        previous.sources.push(crate::catalog::CatalogSource {
            name: req.name.clone().unwrap_or_else(|| "Moonvy Project".to_string()),
            url: clean_url,
            last_synced_at: now.clone(),
        });
        previous.updated_at = Some(now);
        previous.version = 1;
        previous.designs = designs;
        previous.save(&workspace).map_err(tools::tool_error)?;

        let aliases_file = workspace.join(crate::catalog::MOONVY_DIR).join("aliases.json");
        if !aliases_file.exists() {
            std::fs::create_dir_all(aliases_file.parent().expect("aliases path has a parent")).map_err(tools::tool_error)?;
            std::fs::write(&aliases_file, "{}\n").map_err(tools::tool_error)?;
        }

        tools::json_string(json!({
            "workspaceDir": workspace.to_string_lossy(),
            "catalogPath": workspace.join(crate::catalog::MOONVY_DIR).join("catalog.json").to_string_lossy(),
            "aliasesPath": aliases_file.to_string_lossy(),
            "aliasesCreated": true,
            "includeTypes": include_types,
            "scannedCount": rows.len(),
            "designCount": previous.designs.len(),
            "sourceCount": previous.sources.len(),
            "designs": previous.designs.iter().map(tools::design_summary_json).collect::<Vec<_>>(),
        }))
    }

    #[tool(name = "moonvy_search_designs", description = "Search the synced catalog (.moonvy-mcp/catalog.json) by design name, ID, URL, alias, tag or file path. Returns { matches: [{name,url,score,matchReason}] }. Requires moonvy_sync_project first.")]
    async fn search_designs(&self, Parameters(req): Parameters<SearchDesignsRequest>) -> Result<String, McpError> {
        let workspace = resolve_workspace_dir(&req.workspace_dir).map_err(tools::tool_error)?;
        let matches = tools::search_designs(&workspace, &req.query, req.limit).map_err(tools::tool_error)?;
        tools::json_string(json!({
            "workspaceDir": workspace.to_string_lossy(),
            "catalogPath": workspace.join(crate::catalog::MOONVY_DIR).join("catalog.json").to_string_lossy(),
            "aliasesPath": workspace.join(crate::catalog::MOONVY_DIR).join("aliases.json").to_string_lossy(),
            "query": req.query,
            "matches": matches,
        }))
    }

    #[tool(name = "moonvy_get_tree_by_name", description = "Resolve one design from the synced catalog by name/alias/tag/URL/ID, then return its layer tree. Returns { status: ok|not_found|ambiguous, tree: { items } } when ok.")]
    async fn get_tree_by_name(&self, Parameters(req): Parameters<TreeByNameRequest>) -> Result<String, McpError> {
        let workspace = resolve_workspace_dir(&req.workspace_dir).map_err(tools::tool_error)?;
        let matches = tools::search_designs(&workspace, &req.name, 20).map_err(tools::tool_error)?;
        if matches.len() != 1 {
            let status = if matches.is_empty() { "not_found" } else { "ambiguous" };
            return tools::json_string(json!({
                "status": status,
                "workspaceDir": workspace.to_string_lossy(),
                "catalogPath": workspace.join(crate::catalog::MOONVY_DIR).join("catalog.json").to_string_lossy(),
                "query": req.name,
                "matches": matches,
            }));
        }
        let design_url = matches[0].get("url").and_then(|u| u.as_str()).unwrap_or_default().to_string();
        let design: crate::catalog::CatalogDesign = serde_json::from_value(matches[0].clone()).map_err(tools::tool_error)?;
        let (_, genome) = tools::genome_for_url(&self.api, &design_url).await.map_err(tools::tool_error)?;
        let options = TreeOptions {
            with_style: req.with_style,
            max_depth: req.max_depth,
            ..Default::default()
        };
        let tree = crate::genome::extract_tree(&genome, req.frame.as_deref(), &options);
        tools::json_string(json!({
            "status": "ok",
            "workspaceDir": workspace.to_string_lossy(),
            "design": design,
            "tree": { "items": tree },
        }))
    }

    #[tool(name = "moonvy_diff_designs", description = "Compare two design URLs by node id and return added/removed/changed layers — use this to see what differs between states (e.g. normal vs hover).")]
    async fn diff_designs(&self, Parameters(req): Parameters<DiffDesignsRequest>) -> Result<String, McpError> {
        let (_, genome_a) = tools::genome_for_url(&self.api, &req.url_a).await.map_err(tools::tool_error)?;
        let (_, genome_b) = tools::genome_for_url(&self.api, &req.url_b).await.map_err(tools::tool_error)?;
        let options = TreeOptions { with_style: req.with_style, ..Default::default() };
        let tree_a = crate::genome::extract_tree(&genome_a, req.frame.as_deref(), &options);
        let tree_b = crate::genome::extract_tree(&genome_b, req.frame.as_deref(), &options);
        let diff = crate::genome::diff_trees(&tree_a, &tree_b);
        tools::json_string(json!({ "added": diff.added, "removed": diff.removed, "changed": diff.changed }))
    }
}

/* ------------------------------- server impl ------------------------------- */

#[tool_handler(router = self.tool_router)]
impl ServerHandler for MoonvyServer {
    fn get_info(&self) -> ServerInfo {
        let mut implementation = Implementation::from_build_env();
        implementation.name = "openmoonvy-mcp-rs".to_string();
        implementation.version = env!("CARGO_PKG_VERSION").to_string();
        ServerInfo::new(ServerCapabilities::builder().enable_tools().build())
            .with_server_info(implementation)
            .with_instructions(
                "Moonvy design extraction server (Rust). Requires a Moonvy auth token: \
                 set MOONVY_TOKEN or save ~/.moonvy-ai/token.json. \
                 Tools: moonvy_get_design, moonvy_get_design_context, moonvy_get_tree, \
                 moonvy_list_pages, moonvy_list_layers, moonvy_get_node_style, \
                 moonvy_extract_tokens, moonvy_download_asset, moonvy_sync_project, \
                 moonvy_search_designs, moonvy_get_tree_by_name, moonvy_diff_designs.",
            )
    }
}
