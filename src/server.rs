/*
 * Moonvy MCP server: full tool set. Business logic lives in tools.rs;
 * this module only declares tool schemas and routes calls.
 */

use std::sync::Arc;

use rmcp::{
    ErrorData as McpError, ServerHandler,
    handler::server::router::tool::ToolRouter,
    handler::server::wrapper::Parameters,
    model::{
        CompleteResult, CompletionInfo, GetPromptResponse, GetPromptResult, Implementation,
        ListPromptsResult, ListResourceTemplatesResult, ListResourcesResult, Prompt,
        ReadResourceResponse, ReadResourceResult, Resource, ResourceContents, ResourceTemplate,
        ServerCapabilities, ServerInfo,
    },
    schemars, tool, tool_handler, tool_router,
};
use serde::Deserialize;
use serde_json::{Value, json};

use crate::api::MoonvyApi;
use crate::catalog::{Catalog, artifact_path, resolve_workspace_dir};
use crate::genome::TreeOptions;
use crate::tools;

#[derive(Debug, Clone)]
pub struct MoonvyServer {
    tool_router: ToolRouter<Self>,
    api: Arc<MoonvyApi>,
}

impl MoonvyServer {
    pub fn new(api: Arc<MoonvyApi>) -> Self {
        Self {
            tool_router: Self::tool_router(),
            api,
        }
    }
}

/// Workspaces advertised via resources/completion (MOONVY_WORKSPACE_DIR /
/// MOONVY_ALLOWED_WORKSPACES), deduplicated.
pub fn known_workspaces() -> Vec<String> {
    let mut dirs: Vec<String> = Vec::new();
    for raw in [
        std::env::var("MOONVY_WORKSPACE_DIR"),
        std::env::var("MOONVY_ALLOWED_WORKSPACES"),
    ]
    .into_iter()
    .flatten()
    {
        dirs.extend(
            raw.split([';', '|'])
                .map(str::trim)
                .filter(|d| !d.is_empty())
                .map(str::to_string),
        );
    }
    dirs.sort();
    dirs.dedup();
    dirs
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
    /// Include normalized style data for every node (null style keys are omitted)
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
    #[serde(default = "default_true")]
    pub detect_duplicates: bool,
    /// Collapse duplicate instances into position stubs (canonical keeps content)
    #[serde(default = "default_true")]
    pub deduplicate: bool,
    /// Text payload in the tree: truncate (40 chars, default), full, or none
    #[serde(default)]
    pub text_content: Option<crate::genome::TextContent>,
    /// Export only the subtree rooted at this node id
    pub node_id: Option<String>,
    /// Keep only nodes intersecting this rect: [x, y, w, h] in absolute
    /// coordinates (requires flatten: true)
    pub region: Option<Vec<f64>>,
    /// Include the deduplicated asset manifest (hash -> {url, refs})
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
pub struct FindNodeRequest {
    /// Moonvy design URL
    pub url: String,
    /// Case-insensitive substring to match against node names and text
    pub query: String,
    /// Optional frame/page ID filter
    pub frame: Option<String>,
    /// Maximum matches to return
    #[serde(default = "default_limit_50")]
    pub limit: u32,
}

#[derive(Debug, Deserialize, schemars::JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct AssetUrlRequest {
    /// Moonvy design or project URL
    pub url: String,
    /// Moonvy node ID or file UUID
    pub node: String,
    /// Asset type: slice, snapshot, or image (autodetected)
    pub r#type: Option<String>,
    /// Slice format/ratio: svg, base, max
    pub slice_format: Option<String>,
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
    /// Emit coordinates relative to the artboard origin. Defaults to true —
    /// absolute coordinates are what you want when reconstructing a page.
    #[serde(default = "default_true")]
    pub flatten: bool,
    /// Keep only nodes of these types
    pub only: Option<Vec<String>>,
    /// Annotate nodes whose content repeats an earlier node with duplicateOf
    #[serde(default = "default_true")]
    pub detect_duplicates: bool,
    /// Collapse duplicate instances into position stubs (canonical keeps content)
    #[serde(default = "default_true")]
    pub deduplicate: bool,
    /// Text payload in the tree: truncate (40 chars, default), full, or none
    #[serde(default)]
    pub text_content: Option<crate::genome::TextContent>,
    /// Export only the subtree rooted at this node id
    pub node_id: Option<String>,
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
    /// Crop the downloaded snapshot to the node's own area (scaled from
    /// artboard to snapshot pixels). Useful for image fills without a direct
    /// asset reference — the node is extracted from its rendered snapshot.
    #[serde(default)]
    pub crop: bool,
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
    /// Drop empty container groups and lift their children up
    #[serde(default)]
    pub skip_empty_groups: bool,
    /// Emit coordinates relative to the artboard origin
    #[serde(default)]
    pub flatten: bool,
    /// Keep only nodes of these types (children filtered recursively)
    pub only: Option<Vec<String>>,
    /// Annotate nodes whose content repeats an earlier node with duplicateOf
    #[serde(default = "default_true")]
    pub detect_duplicates: bool,
    /// Collapse duplicate instances into position stubs (canonical keeps content)
    #[serde(default = "default_true")]
    pub deduplicate: bool,
    /// Text payload in the tree: truncate (40 chars, default), full, or none
    #[serde(default)]
    pub text_content: Option<crate::genome::TextContent>,
    /// Export only the subtree rooted at this node id
    pub node_id: Option<String>,
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
    /// Drop empty container groups before comparing
    #[serde(default)]
    pub skip_empty_groups: bool,
    /// Compare absolute coordinates (relative to the artboard origin). On by
    /// default so changed rects are comparable with moonvy_get_tree flatten.
    #[serde(default = "default_true")]
    pub flatten: bool,
    /// Keep only nodes of these types before comparing
    pub only: Option<Vec<String>>,
    /// Include before/after snapshots on changed nodes (more verbose)
    #[serde(default)]
    pub with_snapshots: bool,
}

#[derive(Debug, Deserialize, schemars::JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct StyleCodeRequest {
    /// Moonvy design URL
    pub url: String,
    /// Export only the subtree rooted at this node id (find it via moonvy_find_node)
    pub node_id: Option<String>,
    /// Optional frame/page ID filter
    pub frame: Option<String>,
    /// Output format: "css" or "tailwind" (default: css)
    #[serde(default)]
    pub format: Option<String>,
}

#[derive(Debug, Deserialize, schemars::JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct SetTokenRequest {
    /// Moonvy JWT token from a logged-in browser session
    pub token: String,
}

#[derive(Debug, Deserialize, schemars::JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct LoginRequest {
    /// Maximum time to wait for the user to complete login in the browser window (ms)
    #[serde(default = "default_timeout_300000")]
    pub timeout_ms: u64,
}

fn default_timeout_300000() -> u64 {
    300_000
}

/* ---------------------------------- tools ---------------------------------- */

#[tool_router(router = tool_router)]
impl MoonvyServer {
    #[tool(
        name = "moonvy_get_design",
        description = "Design metadata: title and frame dimensions. Returns { items: [ { title, frameCount, frames: [{id,name,width,height}] } ] }. Accepts a design file URL or a project directory URL (auto-resolved)."
    )]
    async fn get_design(
        &self,
        Parameters(req): Parameters<GetDesignRequest>,
    ) -> Result<String, McpError> {
        let (_, genome) = tools::genome_for_url(&self.api, &req.url)
            .await
            .map_err(tools::tool_error)?;
        let meta = crate::genome::extract_design_meta(&genome, None);
        tools::json_string(json!({ "items": [meta] }))
    }

    #[tool(
        name = "moonvy_get_tree",
        description = "Full layer tree. Returns { items: [ {id,name,type,x,y,width,height,text?,style?,children?,duplicateOf?,snapshotHash?} ], assets? }. Supports skipEmptyGroups, flatten, only, detectDuplicates (on by default), textContent (truncate|full|none), nodeId (subtree export), region [x,y,w,h] (needs flatten), includeAssets."
    )]
    async fn get_tree(
        &self,
        Parameters(req): Parameters<GetTreeRequest>,
    ) -> Result<String, McpError> {
        let (_, genome) = tools::genome_for_url(&self.api, &req.url)
            .await
            .map_err(tools::tool_error)?;
        let node_id = req.node_id.clone();
        let options = TreeOptions {
            with_style: req.with_style,
            max_depth: req.max_depth,
            skip_empty_groups: req.skip_empty_groups,
            flatten: req.flatten,
            only: req.only,
            detect_duplicates: req.detect_duplicates,
            deduplicate: req.deduplicate,
            text_content: req.text_content.unwrap_or_default(),
            node_id: req.node_id,
            region: req.region.map(|r| {
                let mut a = [0.0; 4];
                for (i, v) in r.iter().take(4).enumerate() {
                    a[i] = *v;
                }
                a
            }),
        };
        let mut payload =
            tools::tree_payload(&genome, req.frame.as_deref(), &options, req.include_assets);
        if let Some(node_id) = &node_id {
            // A nodeId that does not exist in this design should be reported
            // instead of silently returning an empty tree.
            let empty = payload
                .get("items")
                .and_then(|v| v.as_array())
                .is_none_or(|a| a.is_empty());
            if empty {
                let obj = payload.as_object_mut().expect("payload is an object");
                obj.insert("status".into(), json!("not_found"));
                obj.insert("nodeId".into(), json!(node_id));
            }
        }
        tools::json_string(payload)
    }

    #[tool(
        name = "moonvy_extract_tokens",
        description = "Design tokens: colors, fontSizes, radii, spacing. Returns { items: [ {colors:[...],fontSizes:[...],radii:[...],spacing:[...]} ] }."
    )]
    async fn extract_tokens(
        &self,
        Parameters(req): Parameters<GetTokensRequest>,
    ) -> Result<String, McpError> {
        let (_, genome) = tools::genome_for_url(&self.api, &req.url)
            .await
            .map_err(tools::tool_error)?;
        let tokens = crate::genome::extract_tokens(&genome);
        tools::json_string(json!({ "items": [tokens] }))
    }

    #[tool(
        name = "moonvy_list_pages",
        description = "List pages/files in a Moonvy project. Returns { items: [{id,name,type,url,preview?}] }. Start here to discover design file URLs from a project directory URL."
    )]
    async fn list_pages(
        &self,
        Parameters(req): Parameters<ListPagesRequest>,
    ) -> Result<String, McpError> {
        let rows = tools::list_pages(&self.api, &req.url, req.limit, req.max_pages)
            .await
            .map_err(tools::tool_error)?;
        tools::json_string(json!({ "items": rows }))
    }

    #[tool(
        name = "moonvy_list_layers",
        description = "Flattened layer list: id, name, type, x, y, width, height. Returns { items: [...] }. Use this to discover valid node IDs."
    )]
    async fn list_layers(
        &self,
        Parameters(req): Parameters<ListLayersRequest>,
    ) -> Result<String, McpError> {
        let (_, genome) = tools::genome_for_url(&self.api, &req.url)
            .await
            .map_err(tools::tool_error)?;
        let layers =
            crate::genome::extract_layers(&genome, req.frame.as_deref(), req.limit as usize);
        tools::json_string(json!({ "items": layers }))
    }

    #[tool(
        name = "moonvy_get_node_style",
        description = "Normalized style of one node: background, color, fontSize, fontWeight, borderRadius, opacity, strokeWidth, strokeColor, gradient. Returns { items: [...] } (null when not set)."
    )]
    async fn get_node_style(
        &self,
        Parameters(req): Parameters<NodeStyleRequest>,
    ) -> Result<String, McpError> {
        let (_, genome) = tools::genome_for_url(&self.api, &req.url)
            .await
            .map_err(tools::tool_error)?;
        let rows = crate::genome::extract_node_style(&genome, &req.node);
        tools::json_string(json!({ "items": rows }))
    }

    #[tool(
        name = "moonvy_find_node",
        description = "Search a design by node name or text content (case-insensitive substring). Returns { items: [ {id,name,type,x,y,width,height,text?} ] } - the targeted alternative to dumping the whole tree."
    )]
    async fn find_node(
        &self,
        Parameters(req): Parameters<FindNodeRequest>,
    ) -> Result<String, McpError> {
        let (_, genome) = tools::genome_for_url(&self.api, &req.url)
            .await
            .map_err(tools::tool_error)?;
        let hits = crate::genome::find_nodes(
            &genome,
            req.frame.as_deref(),
            &req.query,
            req.limit as usize,
        );
        tools::json_string(json!({ "items": hits }))
    }

    #[tool(
        name = "moonvy_get_asset_url",
        description = "Resolve the direct download URL of a slice/snapshot/image without downloading. Returns { items: [ {node,name,url,type,resolvedType,nodeRect?,artboardSize?,snapshotHash?} ] } - nodeRect + artboardSize enable cropping the node out of the snapshot (pair with moonvy_download_asset crop=true). When the node has no asset of the requested type (e.g. type=slice on a non-slice node), resolvedType is \"none\" with a message instead of an error - retry with type=snapshot or type=image."
    )]
    async fn get_asset_url(
        &self,
        Parameters(req): Parameters<AssetUrlRequest>,
    ) -> Result<String, McpError> {
        let resolved = match tools::resolve_asset(
            &self.api,
            &req.url,
            &req.node,
            req.r#type.as_deref(),
            req.slice_format.as_deref(),
        )
        .await
        {
            Ok(resolved) => resolved,
            Err(error) => {
                let message = error.to_string();
                // A node without that kind of asset is a normal outcome (e.g.
                // type=slice on a non-slice node), not a server fault. Report
                // it as structured data so the model can retry with a
                // different asset type instead of hitting a -32603.
                if tools::is_asset_unavailable(&message) {
                    return tools::json_string(json!({
                        "items": [{
                            "node": req.node,
                            "type": req.r#type.clone().unwrap_or_else(|| "auto".to_string()),
                            "resolvedType": "none",
                            "message": message,
                        }]
                    }));
                }
                return Err(tools::tool_error(error));
            }
        };
        let mut item = json!({
            "node": req.node,
            "name": resolved.name,
            "url": resolved.url,
            "type": req.r#type.unwrap_or_else(|| "auto".to_string()),
            "resolvedType": resolved.resolved_type,
        });
        if let Some((x, y, w, h)) = resolved.node_rect {
            item["nodeRect"] = json!([x, y, w, h]);
        }
        if let Some((w, h)) = resolved.artboard_size {
            item["artboardSize"] = json!([w, h]);
        }
        if let Some(hash) = resolved.snapshot_hash {
            item["snapshotHash"] = json!(hash);
        }
        tools::json_string(json!({ "items": [item] }))
    }

    #[tool(
        name = "moonvy_get_design_context",
        description = "One-call bundle: { design: metadata, tree: { items, assets? }, tokens: { items } }. THE recommended entry point for design work. Tree supports skipEmptyGroups, flatten, only, detectDuplicates, includeAssets."
    )]
    async fn get_design_context(
        &self,
        Parameters(req): Parameters<ContextRequest>,
    ) -> Result<String, McpError> {
        let (_, genome) = tools::genome_for_url(&self.api, &req.url)
            .await
            .map_err(tools::tool_error)?;
        let meta = crate::genome::extract_design_meta(&genome, None);
        let options = TreeOptions {
            with_style: true,
            max_depth: req.max_depth,
            skip_empty_groups: req.skip_empty_groups,
            flatten: req.flatten,
            only: req.only,
            detect_duplicates: req.detect_duplicates,
            deduplicate: req.deduplicate,
            text_content: req.text_content.unwrap_or_default(),
            node_id: req.node_id,
            ..Default::default()
        };
        let tree = tools::tree_payload(&genome, None, &options, req.include_assets);
        let tokens = crate::genome::extract_tokens(&genome);
        tools::json_string(json!({ "design": meta, "tree": tree, "tokens": { "items": [tokens] } }))
    }

    #[tool(
        name = "moonvy_download_asset",
        description = "Download a slice, snapshot or image fill from a Moonvy node. Returns { items: [ {success,path,size,name,url} ] }. out must be an absolute directory or file path."
    )]
    async fn download_asset(
        &self,
        Parameters(req): Parameters<DownloadAssetRequest>,
    ) -> Result<String, McpError> {
        let (save_path, size, file_name, url, snapshot_size) = tools::download_asset(
            &self.api,
            &req.url,
            &req.node,
            req.r#type.as_deref(),
            req.slice_format.as_deref(),
            req.name.as_deref(),
            req.out.as_deref(),
            req.crop,
        )
        .await
        .map_err(tools::tool_error)?;
        let mut item = json!({
            "success": true,
            "path": save_path.to_string_lossy(),
            "size": size,
            "name": file_name,
            "url": url,
            "cropped": req.crop,
        });
        if let Some((w, h)) = snapshot_size {
            item["snapshotSize"] = json!([w, h]);
        }
        tools::json_string(json!({ "items": [item] }))
    }

    #[tool(
        name = "moonvy_sync_project",
        description = "Scan a Moonvy project and write .moonvy-mcp/catalog.json (design index) into the frontend workspace. Run once per project; afterwards moonvy_search_designs and moonvy_get_tree_by_name resolve designs by name."
    )]
    async fn sync_project(
        &self,
        Parameters(req): Parameters<SyncProjectRequest>,
    ) -> Result<String, McpError> {
        let workspace = resolve_workspace_dir(&req.workspace_dir).map_err(tools::tool_error)?;
        let clean_url = tools::sanitize_url(&req.project_url);
        let mut previous = Catalog::load(&workspace).map_err(tools::tool_error)?;
        let rows = tools::list_pages(&self.api, &req.project_url, req.limit, req.max_pages)
            .await
            .map_err(tools::tool_error)?;
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
            name: req
                .name
                .clone()
                .unwrap_or_else(|| "Moonvy Project".to_string()),
            url: clean_url,
            last_synced_at: now.clone(),
        });
        previous.updated_at = Some(now);
        previous.version = 1;
        previous.designs = designs;
        previous.save(&workspace).map_err(tools::tool_error)?;

        let aliases_file = artifact_path(&workspace, "aliases");
        if !aliases_file.exists() {
            std::fs::create_dir_all(aliases_file.parent().expect("aliases path has a parent"))
                .map_err(tools::tool_error)?;
            std::fs::write(&aliases_file, "{}\n").map_err(tools::tool_error)?;
        }

        tools::json_string(json!({
            "workspaceDir": workspace.to_string_lossy(),
            "catalogPath": artifact_path(&workspace, "catalog").to_string_lossy(),
            "aliasesPath": aliases_file.to_string_lossy(),
            "aliasesCreated": true,
            "includeTypes": include_types,
            "scannedCount": rows.len(),
            "designCount": previous.designs.len(),
            "sourceCount": previous.sources.len(),
            "designs": previous.designs.iter().map(|d| serde_json::to_value(d).unwrap_or(Value::Null)).collect::<Vec<_>>(),
        }))
    }

    #[tool(
        name = "moonvy_search_designs",
        description = "Search the synced catalog (.moonvy-mcp/catalog.json) by design name, ID, URL, alias, tag or file path. Returns { matches: [{name,url,score,matchReason}] }. Requires moonvy_sync_project first."
    )]
    async fn search_designs(
        &self,
        Parameters(req): Parameters<SearchDesignsRequest>,
    ) -> Result<String, McpError> {
        let workspace = resolve_workspace_dir(&req.workspace_dir).map_err(tools::tool_error)?;
        let matches =
            tools::search_designs(&workspace, &req.query, req.limit).map_err(tools::tool_error)?;
        tools::json_string(json!({
            "workspaceDir": workspace.to_string_lossy(),
            "catalogPath": artifact_path(&workspace, "catalog").to_string_lossy(),
            "aliasesPath": artifact_path(&workspace, "aliases").to_string_lossy(),
            "query": req.query,
            "matches": matches,
        }))
    }

    #[tool(
        name = "moonvy_get_tree_by_name",
        description = "Resolve one design from the synced catalog by name/alias/tag/URL/ID, then return its layer tree. Returns { status: ok|not_found|ambiguous, tree: { items } } when ok."
    )]
    async fn get_tree_by_name(
        &self,
        Parameters(req): Parameters<TreeByNameRequest>,
    ) -> Result<String, McpError> {
        let workspace = resolve_workspace_dir(&req.workspace_dir).map_err(tools::tool_error)?;
        let matches =
            tools::search_designs(&workspace, &req.name, 20).map_err(tools::tool_error)?;
        // Exact matches (score 100) take priority: a query that hits one
        // design exactly must not be reported as ambiguous just because other
        // designs contain the name as a substring.
        let exact: Vec<&Value> = matches
            .iter()
            .filter(|m| m.get("score").and_then(|s| s.as_u64()) == Some(100))
            .collect();
        let selected = if exact.len() == 1 {
            exact[0]
        } else if exact.is_empty() && matches.len() == 1 {
            &matches[0]
        } else {
            let status = if matches.is_empty() {
                "not_found"
            } else {
                "ambiguous"
            };
            return tools::json_string(json!({
                "status": status,
                "workspaceDir": workspace.to_string_lossy(),
                "catalogPath": artifact_path(&workspace, "catalog").to_string_lossy(),
                "query": req.name,
                "matches": matches,
            }));
        };
        let design_url = selected
            .get("url")
            .and_then(|u| u.as_str())
            .unwrap_or_default()
            .to_string();
        let design: crate::catalog::CatalogDesign =
            serde_json::from_value(selected.clone()).map_err(tools::tool_error)?;
        let (_, genome) = tools::genome_for_url(&self.api, &design_url)
            .await
            .map_err(tools::tool_error)?;
        let options = TreeOptions {
            with_style: req.with_style,
            max_depth: req.max_depth,
            skip_empty_groups: req.skip_empty_groups,
            flatten: req.flatten,
            only: req.only,
            detect_duplicates: req.detect_duplicates,
            deduplicate: req.deduplicate,
            text_content: req.text_content.unwrap_or_default(),
            node_id: req.node_id,
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

    #[tool(
        name = "moonvy_get_style_code",
        description = "Generate CSS or Tailwind snippets for a design (or a single node subtree). Returns { items: [ {id,name,selector,code} ] } - one entry per node, with absolute positioning (flatten), background/gradient/border-radius/stroke/text styles resolved. Use nodeId (from moonvy_find_node) to scope output to one component; format: \"css\" (default) or \"tailwind\"."
    )]
    async fn get_style_code(
        &self,
        Parameters(req): Parameters<StyleCodeRequest>,
    ) -> Result<String, McpError> {
        let format =
            crate::genome::StyleCodeFormat::parse(req.format.as_deref()).ok_or_else(|| {
                McpError::invalid_params("format must be \"css\" or \"tailwind\"".to_string(), None)
            })?;
        let (_, genome) = tools::genome_for_url(&self.api, &req.url)
            .await
            .map_err(tools::tool_error)?;
        let node_id = req.node_id.clone();
        let options = TreeOptions {
            with_style: true,
            flatten: true,
            skip_empty_groups: true,
            detect_duplicates: true,
            text_content: crate::genome::TextContent::Truncate,
            node_id: req.node_id,
            ..Default::default()
        };
        let tree = crate::genome::extract_tree(&genome, req.frame.as_deref(), &options);
        if node_id.is_some() && tree.is_empty() {
            return tools::json_string(json!({
                "status": "not_found",
                "items": [],
                "nodeId": node_id,
                "format": format!("{:?}", format).to_lowercase(),
            }));
        }
        let items = crate::genome::generate_style_code(&tree, format);
        tools::json_string(json!({
            "items": items,
            "format": format!("{:?}", format).to_lowercase(),
            "count": items.len(),
        }))
    }

    #[tool(
        name = "moonvy_set_token",
        description = "Save a Moonvy JWT for API access (valid ~180 days). Returns expiry info. Prefer moonvy_login for a guided browser login."
    )]
    async fn set_token(
        &self,
        Parameters(req): Parameters<SetTokenRequest>,
    ) -> Result<String, McpError> {
        let info = crate::token::save_token(&req.token).map_err(tools::tool_error)?;
        tools::json_string(serde_json::to_value(info).map_err(tools::tool_error)?)
    }

    #[tool(
        name = "moonvy_login",
        description = "Guided login: opens Chrome/Edge at moonvy.com, waits for the user to log in, captures and saves the auth token. THE recovery step when tools report [AUTH_REQUIRED] or [AUTH_EXPIRED] - call it and then retry the failed tool. Falls back to manual: set MOONVY_TOKEN or copy window.app.api.$options.token into moonvy_set_token."
    )]
    async fn login(&self, Parameters(req): Parameters<LoginRequest>) -> Result<String, McpError> {
        let outcome = crate::login::login(req.timeout_ms)
            .await
            .map_err(tools::tool_error)?;
        tools::json_string(json!({
            "loggedIn": true,
            "method": outcome.method,
            "savedAt": outcome.token_info.saved_at,
            "expiresAt": outcome.token_info.expires_at,
            "daysUntilExpiry": outcome.token_info.days_until_expiry,
            "userId": outcome.token_info.user_id,
            "email": outcome.token_info.email,
        }))
    }

    #[tool(
        name = "moonvy_diff_designs",
        description = "Compare two design URLs by node id (with same-name fallback). Same-name nodes are paired by minimum-total geometry distance (Hungarian assignment) and must match node type, so unrelated same-name layers are NOT cross-matched; single-page root containers with identical viewport size are excluded from added/removed (only their children are diffed). Returns { summary: {added,removed,changed,aNodes,bNodes}, added, removed, changed:[{id,name,fields}] } with before/after snapshots when withSnapshots. Coordinates are absolute by default (flatten). Supports skipEmptyGroups, only."
    )]
    async fn diff_designs(
        &self,
        Parameters(req): Parameters<DiffDesignsRequest>,
    ) -> Result<String, McpError> {
        let (a, b) = tokio::join!(
            tools::genome_for_url(&self.api, &req.url_a),
            tools::genome_for_url(&self.api, &req.url_b),
        );
        let (_, genome_a) = a.map_err(tools::tool_error)?;
        let (_, genome_b) = b.map_err(tools::tool_error)?;
        let options = TreeOptions {
            with_style: req.with_style,
            skip_empty_groups: req.skip_empty_groups,
            flatten: req.flatten,
            only: req.only,
            text_content: crate::genome::TextContent::Truncate,
            ..Default::default()
        };
        let tree_a = crate::genome::extract_tree(&genome_a, req.frame.as_deref(), &options);
        let tree_b = crate::genome::extract_tree(&genome_b, req.frame.as_deref(), &options);
        let diff = crate::genome::diff_trees(&tree_a, &tree_b);
        let a_nodes = crate::genome::count_nodes(&tree_a);
        let b_nodes = crate::genome::count_nodes(&tree_b);
        if !req.with_snapshots {
            // Strip the (verbose) before/after snapshots from changed nodes.
            let mut changed: Vec<Value> = Vec::with_capacity(diff.changed.len());
            for c in &diff.changed {
                changed.push(json!({
                    "id": c.id,
                    "name": c.name,
                    "fields": c.fields,
                }));
            }
            let mut payload = json!({
                "summary": {
                    "added": diff.added.len(),
                    "removed": diff.removed.len(),
                    "changed": diff.changed.len(),
                    "aNodes": a_nodes,
                    "bNodes": b_nodes,
                },
                "added": diff.added,
                "removed": diff.removed,
                "changed": changed,
            });
            tools::compact_numbers(&mut payload);
            return tools::json_string(payload);
        }
        let mut payload = json!({
            "summary": {
                "added": diff.added.len(),
                "removed": diff.removed.len(),
                "changed": diff.changed.len(),
                "aNodes": a_nodes,
                "bNodes": b_nodes,
            },
            "added": diff.added,
            "removed": diff.removed,
            "changed": diff.changed,
        });
        tools::compact_numbers(&mut payload);
        tools::json_string(payload)
    }
}

/* ------------------------------- server impl ------------------------------- */

fn prompt_attr(name: &str, description: &str) -> Prompt {
    Prompt::new(name.to_string(), Some(description.to_string()), None)
}

fn prompt_text(description: &str, text: String) -> GetPromptResponse {
    GetPromptResult::new(vec![rmcp::model::PromptMessage::new_text(
        rmcp::model::Role::User,
        text,
    )])
    .with_description(description)
    .into()
}

/// Read a string argument from a prompt request.
fn arg(request: &rmcp::model::GetPromptRequestParams, key: &str) -> Option<String> {
    request
        .arguments
        .as_ref()
        .and_then(|a| a.get(key))
        .and_then(|v| v.as_str())
        .map(str::to_string)
}

#[tool_handler(router = self.tool_router)]
impl ServerHandler for MoonvyServer {
    fn call_tool(
        &self,
        request: rmcp::model::CallToolRequestParams,
        context: rmcp::service::RequestContext<rmcp::RoleServer>,
    ) -> impl std::future::Future<Output = Result<rmcp::model::CallToolResponse, McpError>>
    + rmcp::service::MaybeSendFuture
    + '_ {
        // Lifecycle: record activity so the optional idle-timeout watchdog
        // does not kill the server while a session is actively using it.
        crate::touch_activity();
        self.tool_router.call(
            rmcp::handler::server::tool::ToolCallContext::new(self, request, context),
        )
    }

    fn get_info(&self) -> ServerInfo {
        let mut implementation = Implementation::from_build_env();
        implementation.name = "openmoonvy-mcp-rs".to_string();
        implementation.version = env!("CARGO_PKG_VERSION").to_string();
        ServerInfo::new(
            ServerCapabilities::builder()
                .enable_tools()
                .enable_prompts()
                .enable_resources()
                .build(),
        )
        .with_server_info(implementation)
        .with_instructions(
            "Moonvy design extraction server (Rust). Requires a Moonvy auth token: \
                 set MOONVY_TOKEN or save ~/.moonvy-ai/token.json. \
                 Tools: moonvy_get_design, moonvy_get_design_context, moonvy_get_tree, \
                 moonvy_list_pages, moonvy_list_layers, moonvy_find_node, moonvy_get_node_style, \
                 moonvy_extract_tokens, moonvy_get_asset_url, moonvy_download_asset, \
                 moonvy_sync_project, moonvy_search_designs, moonvy_get_tree_by_name, \
                 moonvy_diff_designs, moonvy_get_style_code, moonvy_set_token.",
        )
    }

    fn list_prompts(
        &self,
        _request: Option<rmcp::model::PaginatedRequestParams>,
        _context: rmcp::service::RequestContext<rmcp::RoleServer>,
    ) -> impl std::future::Future<Output = Result<ListPromptsResult, McpError>>
    + rmcp::service::MaybeSendFuture
    + '_ {
        let prompts = vec![
            prompt_attr(
                "moonvy_restore_design",
                "End-to-end workflow to turn a Moonvy design into frontend code: resolve the design, extract context, download assets, and generate implementation guidance.",
            ),
            prompt_attr(
                "moonvy_handoff",
                "Build a design handoff brief from a workspace catalog: design list, aliases and working-set guidance.",
            ),
            prompt_attr(
                "moonvy_diff_states",
                "Compare two design states (e.g. normal vs hover) and turn the difference into implementation guidance.",
            ),
        ];
        std::future::ready(Ok(ListPromptsResult::with_all_items(prompts)))
    }

    fn get_prompt(
        &self,
        request: rmcp::model::GetPromptRequestParams,
        _context: rmcp::service::RequestContext<rmcp::RoleServer>,
    ) -> impl std::future::Future<Output = Result<GetPromptResponse, McpError>>
    + rmcp::service::MaybeSendFuture
    + '_ {
        std::future::ready(get_prompt_impl(&request))
    }

    fn list_resources(
        &self,
        _request: Option<rmcp::model::PaginatedRequestParams>,
        _context: rmcp::service::RequestContext<rmcp::RoleServer>,
    ) -> impl std::future::Future<Output = Result<ListResourcesResult, McpError>>
    + rmcp::service::MaybeSendFuture
    + '_ {
        let resources: Vec<Resource> = known_workspaces()
            .into_iter()
            .flat_map(|dir| {
                ["catalog", "aliases"].into_iter().map(move |kind| {
                    Resource::new(
                        format!("moonvy://{kind}/{dir}"),
                        format!("moonvy-{kind}-{}", dir.replace(['\\', '/', ':'], "_")),
                    )
                    .with_description(format!("Moonvy {kind} for workspace {dir}"))
                    .with_mime_type("application/json")
                })
            })
            .collect();
        std::future::ready(Ok(ListResourcesResult::with_all_items(resources)))
    }

    fn list_resource_templates(
        &self,
        _request: Option<rmcp::model::PaginatedRequestParams>,
        _context: rmcp::service::RequestContext<rmcp::RoleServer>,
    ) -> impl std::future::Future<Output = Result<ListResourceTemplatesResult, McpError>>
    + rmcp::service::MaybeSendFuture
    + '_ {
        let templates = vec![
            ResourceTemplate::new("moonvy://catalog/{workspaceId}", "moonvy-catalog")
                .with_description("Sanitized Moonvy project catalog (.moonvy-mcp/catalog.json) for a frontend workspace.")
                .with_mime_type("application/json"),
            ResourceTemplate::new("moonvy://aliases/{workspaceId}", "moonvy-aliases")
                .with_description("Frontend path <-> Moonvy design alias mappings (.moonvy-mcp/aliases.json).")
                .with_mime_type("application/json"),
        ];
        std::future::ready(Ok(ListResourceTemplatesResult::with_all_items(templates)))
    }

    fn read_resource(
        &self,
        request: rmcp::model::ReadResourceRequestParams,
        _context: rmcp::service::RequestContext<rmcp::RoleServer>,
    ) -> impl std::future::Future<Output = Result<ReadResourceResponse, McpError>>
    + rmcp::service::MaybeSendFuture
    + '_ {
        std::future::ready(read_resource_impl(&request.uri))
    }

    fn complete(
        &self,
        request: rmcp::model::CompleteRequestParams,
        _context: rmcp::service::RequestContext<rmcp::RoleServer>,
    ) -> impl std::future::Future<Output = Result<CompleteResult, McpError>>
    + rmcp::service::MaybeSendFuture
    + '_ {
        std::future::ready(complete_impl(&request))
    }
}

fn get_prompt_impl(
    request: &rmcp::model::GetPromptRequestParams,
) -> Result<GetPromptResponse, McpError> {
    match request.name.as_str() {
        "moonvy_restore_design" => {
            let url = arg(request, "url")
                .filter(|u| !u.is_empty())
                .ok_or_else(|| {
                    McpError::invalid_params("url is required for moonvy_restore_design", None)
                })?;
            let stack = arg(request, "stack")
                .unwrap_or_else(|| "not specified (use sensible defaults)".to_string());
            let workspace_dir = arg(request, "workspaceDir");
            let lines = [
                format!("Design restore workflow for: {url}"),
                format!("Target stack: {stack}"),
                String::new(),
                "Steps:".to_string(),
                "1. Call moonvy_get_design_context on the URL (one call: metadata + layer tree with styles + tokens).".to_string(),
                "2. Use the frame dimensions as the layout viewport; map tree nodes to UI components.".to_string(),
                "3. Apply tokens (colors/fontSizes/radii/spacing) as design-system values.".to_string(),
                "4. Call moonvy_download_asset for slice/snapshot assets (out must be an absolute path).".to_string(),
                match workspace_dir {
                    Some(ws) => format!("5. Optionally moonvy_sync_project to index the project (workspace: {ws})."),
                    None => "5. Optionally moonvy_sync_project to index the project for name-based lookups.".to_string(),
                },
                String::new(),
                "Guidance:".to_string(),
                "- Check child nodes for real colors and radii (parent nodes often carry none).".to_string(),
                "- Reuse tokens instead of hardcoding hex values.".to_string(),
                "- Export icons and slices as SVG via moonvy_download_asset with sliceFormat: \"svg\".".to_string(),
            ];
            Ok(prompt_text(
                &format!("Design restore workflow for {url}"),
                lines.join("\n"),
            ))
        }
        "moonvy_handoff" => {
            let workspace_dir = arg(request, "workspaceDir").unwrap_or_default();
            let workspace = resolve_workspace_dir(&workspace_dir).map_err(tools::tool_error)?;
            let catalog = Catalog::load(&workspace).map_err(tools::tool_error)?;
            let aliases: std::collections::HashMap<String, Value> =
                std::fs::read_to_string(artifact_path(&workspace, "aliases"))
                    .ok()
                    .and_then(|raw| serde_json::from_str(&raw).ok())
                    .unwrap_or_default();
            let query = arg(request, "query");

            let designs: Vec<String> = match query {
                Some(query) => crate::tools::search_designs(&workspace, &query, 20)
                    .unwrap_or_default()
                    .into_iter()
                    .map(|m| {
                        m.get("name")
                            .and_then(|v| v.as_str())
                            .unwrap_or("?")
                            .to_string()
                    })
                    .collect(),
                None => catalog
                    .designs
                    .iter()
                    .take(20)
                    .map(|d| format!("{} ({}) {}", d.name, d.r#type, d.url))
                    .collect(),
            };
            let mut lines = vec![
                format!("Workspace: {}", workspace.display()),
                format!(
                    "Catalog: {}",
                    artifact_path(&workspace, "catalog").display()
                ),
                format!("Designs ({}):", designs.len()),
            ];
            lines.extend(designs.into_iter().map(|d| format!("- {d}")));
            if !aliases.is_empty() {
                lines.push("Aliases:".to_string());
                for (from, to) in aliases {
                    lines.push(format!("- {from} -> {to}"));
                }
            }
            lines.push(
                "Use moonvy_get_tree_by_name to fetch a design tree, moonvy_extract_tokens for tokens, and moonvy_download_asset for assets.".to_string(),
            );
            Ok(prompt_text(
                &format!(
                    "Handoff for {} designs in {}",
                    catalog.designs.len().min(20),
                    workspace.display()
                ),
                lines.join("\n"),
            ))
        }
        "moonvy_diff_states" => {
            let url_a = arg(request, "urlA")
                .filter(|u| !u.is_empty())
                .ok_or_else(|| {
                    McpError::invalid_params("urlA is required for moonvy_diff_states", None)
                })?;
            let url_b = arg(request, "urlB")
                .filter(|u| !u.is_empty())
                .ok_or_else(|| {
                    McpError::invalid_params("urlB is required for moonvy_diff_states", None)
                })?;
            let stack = arg(request, "stack")
                .unwrap_or_else(|| "not specified (use sensible defaults)".to_string());
            let lines = [
                format!("State diff workflow: {url_a} vs {url_b}"),
                format!("Target stack: {stack}"),
                String::new(),
                "Steps:".to_string(),
                "1. Call moonvy_get_tree on both URLs with withStyle: true, flatten: true, skipEmptyGroups: true (absolute coordinates, no container noise).".to_string(),
                "2. Call moonvy_diff_designs with the same options to get added/removed/changed layers with before/after snapshots.".to_string(),
                "3. Treat `changed` as the state transition: read `fields` (text/style/rect/childrenCount) and apply only those deltas on top of the base state.".to_string(),
                "4. `added` layers belong to the new state only; `removed` layers must be hidden or removed.".to_string(),
                "5. For any image/snapshot node, use moonvy_get_asset_url to fetch the direct URL, or moonvy_download_asset to save it.".to_string(),
                String::new(),
                "Guidance:".to_string(),
                "- If both designs are separate pages, diff pairs nodes by id first, then by identical names; review unmatched added/removed as real structural changes.".to_string(),
                "- Skip empty groups on both sides so coordinate deltas stay meaningful.".to_string(),
            ];
            Ok(prompt_text(
                &format!("State diff workflow: {url_a} vs {url_b}"),
                lines.join("\n"),
            ))
        }
        other => Err(McpError::invalid_params(
            format!("Prompt {other} not found"),
            None,
        )),
    }
}

fn read_resource_impl(uri: &str) -> Result<ReadResourceResponse, McpError> {
    let (kind, encoded) = if let Some(rest) = uri.strip_prefix("moonvy://catalog/") {
        ("catalog", rest)
    } else if let Some(rest) = uri.strip_prefix("moonvy://aliases/") {
        ("aliases", rest)
    } else {
        return Err(McpError::invalid_params(
            format!("Invalid resource URI: {uri}"),
            None,
        ));
    };
    let workspace = resolve_workspace_dir(encoded).map_err(tools::tool_error)?;
    let text = std::fs::read_to_string(artifact_path(&workspace, kind)).map_err(|_| {
        McpError::internal_error(
            format!("No {kind}.json yet for workspace {encoded}; run moonvy_sync_project first."),
            None,
        )
    })?;
    Ok(
        ReadResourceResult::new(vec![ResourceContents::TextResourceContents {
            uri: uri.to_string(),
            mime_type: Some("application/json".to_string()),
            text,
            meta: None,
        }])
        .into(),
    )
}

fn complete_impl(request: &rmcp::model::CompleteRequestParams) -> Result<CompleteResult, McpError> {
    let is_workspace_template = match &request.r#ref {
        rmcp::model::Reference::Resource(r) => r.uri.contains("{workspaceId}"),
        _ => false,
    };
    if !is_workspace_template || request.argument.name != "workspaceId" {
        return Ok(CompleteResult::new(
            CompletionInfo::new(vec![]).map_err(tools::tool_error)?,
        ));
    }
    let query = request.argument.value.to_lowercase();
    let values: Vec<String> = known_workspaces()
        .into_iter()
        .filter(|d| d.to_lowercase().contains(&query))
        .collect();
    Ok(CompleteResult::new(
        CompletionInfo::new(values).map_err(tools::tool_error)?,
    ))
}
