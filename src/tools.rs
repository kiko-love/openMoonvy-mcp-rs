/**
 * Shared tool logic: page listing, asset download, catalog sync, helpers.
 */

use std::collections::{HashMap, HashSet};
use std::path::PathBuf;

use rmcp::ErrorData as McpError;
use serde_json::{Value, json};

use crate::api::{MoonvyApi, MoonvyNode, parse_moonvy_url, MoonvyUrl};
use crate::catalog::{Catalog, CatalogDesign, design_summary, search_catalog};
use crate::genome::{Genome, TreeOptions, extract_tree, find_node};

pub fn tool_error<E: std::fmt::Display>(error: E) -> McpError {
    McpError::internal_error(error.to_string(), None)
}

pub fn parse_ids(url: &str) -> anyhow::Result<MoonvyUrl> {
    parse_moonvy_url(url).ok_or_else(|| anyhow::anyhow!("Could not parse Moonvy URL"))
}

pub fn json_string(value: Value) -> Result<String, McpError> {
    serde_json::to_string(&value).map_err(tool_error)
}


pub async fn genome_for_url(api: &MoonvyApi, url: &str) -> anyhow::Result<(MoonvyUrl, Genome)> {
    let ids = parse_ids(url)?;
    let node_id = ids.file_id.clone().or_else(|| ids.dir_id.clone()).ok_or_else(|| anyhow::anyhow!("No file or directory ID in URL"))?;
    let node = api.get_node(&ids.project_id, &node_id, "full").await?;
    let genome_url = node
        .files
        .as_ref()
        .and_then(|f| f.genome.as_ref())
        .and_then(|g| g.url.clone())
        .ok_or_else(|| anyhow::anyhow!("No genome file found for node \"{node_id}\""))?;
    let genome = api.fetch_genome(&genome_url).await?;
    Ok((ids, genome))
}

pub fn now_iso() -> String {
    chrono::Utc::now().to_rfc3339_opts(chrono::SecondsFormat::Millis, true)
}

pub fn sanitize_url(url: &str) -> String {
    match url::Url::parse(url) {
        Ok(mut parsed) => {
            parsed.set_username("").ok();
            parsed.set_password(None).ok();
            parsed.set_query(None);
            parsed.set_fragment(None);
            parsed.to_string()
        }
        Err(_) => url.to_string(),
    }
}

pub fn sanitize_name(name: &str) -> String {
    name.chars()
        .map(|c| {
            if c.is_alphanumeric() || c == '_' || c == '-' || ('\u{4e00}'..='\u{9fff}').contains(&c) {
                c
            } else {
                '_'
            }
        })
        .collect()
}

fn infer_extension(url: &str) -> String {
    for ext in ["svg", "png", "jpg", "jpeg", "webp", "gif"] {
        if url.to_lowercase().contains(&format!(".{ext}")) {
            return format!(".{ext}");
        }
    }
    String::new()
}

/* ------------------------------- page listing ------------------------------ */

#[derive(Debug, Clone, Default, serde::Serialize)]
pub struct PageRow {
    pub id: String,
    pub name: String,
    pub r#type: String,
    pub parent_id: Option<String>,
    pub project_id: String,
    pub url: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub preview: Option<PagePreview>,
}

#[derive(Debug, Clone, Default, serde::Serialize)]
pub struct PagePreview {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub normal: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub large: Option<String>,
}

fn pick_string(obj: &serde_json::Map<String, Value>, keys: &[&str]) -> Option<String> {
    for key in keys {
        if let Some(Value::String(s)) = obj.get(*key) {
            if !s.is_empty() {
                return Some(s.clone());
            }
        }
    }
    None
}

fn extract_items(response: &Value) -> Vec<&Value> {
    let mut stack: Vec<&Value> = vec![response];
    while let Some(value) = stack.pop() {
        match value {
            Value::Array(items) => return items.iter().collect(),
            Value::Object(map) => {
                if let Some(candidate) = map.get("data").or_else(|| map.get("result")).or_else(|| map.get("list")) {
                    stack.push(candidate);
                }
            }
            _ => {}
        }
    }
    Vec::new()
}

fn get_number(response: &Value, keys: &[&str]) -> Option<i64> {
    let mut stack: Vec<&Value> = vec![response];
    while let Some(v) = stack.pop() {
        if let Some(n) = v.as_i64() {
            return Some(n);
        }
        if let Some(map) = v.as_object() {
            for key in keys {
                if let Some(next) = map.get(*key) {
                    stack.push(next);
                }
            }
        }
    }
    None
}

fn is_last_page(response: &Value, collected: usize) -> bool {
    let mut stack: Vec<&Value> = vec![response];
    let mut has_next: Option<bool> = None;
    while let Some(v) = stack.pop() {
        if let Some(b) = v.as_bool() {
            has_next = Some(b);
            break;
        }
        if let Some(map) = v.as_object() {
            for key in ["hasNext", "hasMore"] {
                if let Some(next) = map.get(key) {
                    stack.push(next);
                }
            }
        }
    }
    if has_next == Some(false) {
        return true;
    }
    if let Some(total) = get_number(response, &["total"]) {
        if collected as i64 >= total {
            return true;
        }
    }
    match (get_number(response, &["pageSize", "limit"]), get_number(response, &["length"])) {
        (Some(size), Some(len)) => len < size,
        _ => false,
    }
}

fn normalize_item(item: &Value, project_id: &str, input_dir_id: Option<&str>) -> Option<PageRow> {
    let obj = item.as_object()?;
    let id = pick_string(obj, &["id", "nodeId", "fileId", "_id", "uuid"])?;
    let parent_id = pick_string(obj, &["parentId", "pid", "dirId", "folderId", "parent_id"]);
    let row = PageRow {
        id: id.clone(),
        name: pick_string(obj, &["name", "title", "displayName"]).unwrap_or_default(),
        r#type: pick_string(obj, &["type", "nodeType", "kind", "fileType"]).unwrap_or_default(),
        parent_id: parent_id.clone(),
        project_id: project_id.to_string(),
        url: crate::api::file_url_for(project_id, parent_id.as_deref().or(input_dir_id), &id),
        preview: None,
    };
    if let Some(preview) = obj.get("preview").and_then(|p| p.as_object()) {
        let normal = pick_string(preview, &["normal"]);
        let large = pick_string(preview, &["large"]);
        if normal.is_some() || large.is_some() {
            return Some(PageRow { preview: Some(PagePreview { normal, large }), ..row });
        }
    }
    Some(row)
}

fn can_have_children(row_type: &str) -> bool {
    matches!(row_type.to_lowercase().as_str(), "any" | "dir" | "directory" | "folder")
}

/// Paginated BFS listing of a project directory.
pub async fn list_pages(api: &MoonvyApi, url: &str, limit: u32, max_pages: u32) -> anyhow::Result<Vec<PageRow>> {
    let ids = parse_ids(url)?;
    let mut rows: Vec<PageRow> = Vec::new();
    let mut seen: HashSet<String> = HashSet::new();
    let mut queue: Vec<Option<String>> = vec![ids.dir_id.clone()];
    let mut seen_scopes: HashSet<String> = queue.iter().flatten().cloned().collect();
    seen_scopes.insert("project".to_string());
    let mut scanned_pages: u32 = 0;
    let mut head = 0usize;

    while head < queue.len() && rows.len() < limit as usize && scanned_pages < max_pages {
        let scope_id = queue[head].clone();
        head += 1;
        for page_index in 0u32.. {
            if rows.len() >= limit as usize || scanned_pages >= max_pages {
                break;
            }
            scanned_pages += 1;
            let response = api.list_nodes(&ids.project_id, page_index, scope_id.as_deref()).await?;
            let items = extract_items(&response);
            if items.is_empty() {
                break;
            }
            for item in items {
                if let Some(row) = normalize_item(item, &ids.project_id, ids.dir_id.as_deref()) {
                    if can_have_children(&row.r#type) && !seen_scopes.contains(&row.id) {
                        seen_scopes.insert(row.id.clone());
                        queue.push(Some(row.id.clone()));
                    }
                    if seen.insert(row.id.clone()) {
                        rows.push(row);
                        if rows.len() >= limit as usize {
                            break;
                        }
                    }
                }
            }
            if is_last_page(&response, rows.len()) {
                break;
            }
        }
    }
    Ok(rows)
}

/* -------------------------------- asset ------------------------------------ */

fn resolve_asset_url(hash: &str, assets: &serde_json::Map<String, Value>, genome: &Genome) -> String {
    if let Some(Value::String(url)) = assets.get(hash) {
        if !url.is_empty() {
            return url.clone();
        }
    }
    genome.images.get(hash).and_then(|i| i.url.clone()).unwrap_or_else(|| format!("https://fs.moonvy.com/{hash}"))
}

fn find_parent_of<'a>(node: &'a crate::genome::GenomeNode, target_id: &str) -> Option<&'a crate::genome::GenomeNode> {
    for child in &node.children {
        if crate::genome::ids_equal(child.id.as_deref(), Some(target_id)) {
            return Some(node);
        }
        if let Some(found) = find_parent_of(child, target_id) {
            return Some(found);
        }
    }
    None
}

fn find_parent_snapshot(genome: &Genome, layer: &crate::genome::GenomeNode) -> Option<(String, Option<String>)> {
    let layer_id = layer.id.clone()?;
    let mut current = genome.pages.iter().find_map(|page| find_parent_of(page, &layer_id));
    let mut seen = HashSet::new();
    while let Some(node) = current {
        if !seen.insert(node.id.clone().unwrap_or_default()) {
            break;
        }
        if let Some(snapshot) = node.snapshot.clone().or_else(|| node.snapshot_preview.clone()) {
            return Some((snapshot, node.name.clone()));
        }
        let id = node.id.clone()?;
        current = genome.pages.iter().find_map(|page| find_parent_of(page, &id));
    }
    None
}

/// Download a slice/snapshot/image from a Moonvy node; returns (path, size, name, url).
pub async fn download_asset(api: &MoonvyApi, url: &str, node: &str, r#type: Option<&str>, slice_format: Option<&str>, name: Option<&str>, out: Option<&str>) -> anyhow::Result<(PathBuf, u64, String, String)> {
    let ids = parse_ids(url)?;
    let mut download_url: Option<String> = None;
    let mut fallback_name = "asset".to_string();
    let mut asset_extension = String::new();

    if let Some(file_id) = &ids.file_id {
        let file_node = api.get_node(&ids.project_id, file_id, "full").await?;
        let assets: serde_json::Map<String, Value> = file_node.meta.as_ref().and_then(|m| m.assets.clone()).unwrap_or_default();
        let genome = api.fetch_genome(&file_node.files.as_ref().and_then(|f| f.genome.as_ref()).and_then(|g| g.url.clone()).ok_or_else(|| anyhow::anyhow!("No genome file found"))?).await?;

        let layer = genome.pages.iter().find_map(|page| find_node(page, node)).cloned();
        if let Some(layer) = layer {
            fallback_name = layer.name.clone().unwrap_or_else(|| "unnamed".to_string());
            let resolved_type = r#type.map(str::to_string).unwrap_or_else(|| {
                if layer.slices.as_ref().and_then(|s| s.as_object()).is_some() {
                    "slice".to_string()
                } else if layer.snapshot.is_some() {
                    "snapshot".to_string()
                } else if layer.fills.iter().any(|f| f.r#type.as_deref() == Some("image")) {
                    "image".to_string()
                } else {
                    "snapshot".to_string()
                }
            });

            match resolved_type.as_str() {
                "slice" => {
                    let slices = layer.slices.as_ref().and_then(|s| s.as_object()).ok_or_else(|| anyhow::anyhow!("Node does not have slices."))?;
                    let format = slice_format.unwrap_or("svg");
                    let slice_info = slices
                        .get(format)
                        .or_else(|| slices.get("max"))
                        .or_else(|| slices.get("base"))
                        .and_then(|s| s.as_object())
                        .and_then(|o| o.get("id"))
                        .and_then(|v| v.as_str());
                    let hash = slice_info.map(str::to_string).ok_or_else(|| anyhow::anyhow!("Format \"{format}\" not found on slice."))?;
                    download_url = Some(resolve_asset_url(&hash, &assets, &genome));
                    asset_extension = if format == "svg" { ".svg".to_string() } else { ".png".to_string() };
                }
                "snapshot" => {
                    let mut hash = layer.snapshot.clone().or_else(|| layer.snapshot_preview.clone());
                    if hash.is_none() {
                        if let Some((parent_hash, parent_name)) = find_parent_snapshot(&genome, &layer) {
                            hash = Some(parent_hash);
                            if let Some(parent_name) = parent_name {
                                fallback_name = parent_name;
                            }
                        }
                    }
                    let hash = hash.ok_or_else(|| anyhow::anyhow!("No snapshot found for this node or its parents."))?;
                    download_url = Some(resolve_asset_url(&hash, &assets, &genome));
                    asset_extension = ".png".to_string();
                }
                "image" => {
                    let image_fill = layer.fills.iter().find(|f| f.r#type.as_deref() == Some("image"));
                    let hash = image_fill
                        .and_then(|f| f.image_hash.clone().or_else(|| f.id.clone()).or_else(|| f.hash.clone()))
                        .ok_or_else(|| anyhow::anyhow!("Image fill does not have a valid asset reference."))?;
                    download_url = Some(resolve_asset_url(&hash, &assets, &genome));
                    asset_extension = genome.images.get(&hash).and_then(|i| i.r#type.clone()).map(|t| format!(".{t}")).unwrap_or_else(|| ".png".to_string());
                }
                other => anyhow::bail!("Invalid type: {other}"),
            }
        }
    }

    if download_url.is_none() {
        let node_info: MoonvyNode = api.get_node(&ids.project_id, node, "full").await?;
        fallback_name = node_info.name.clone().unwrap_or_else(|| "unnamed".to_string());
        download_url = if r#type == Some("snapshot") {
            node_info.preview.as_ref().and_then(|p| p.large.clone().or_else(|| p.normal.clone()))
        } else {
            node_info.files.as_ref().and_then(|f| f.file.as_ref()).and_then(|f| f.url.clone())
                .or_else(|| node_info.preview.as_ref().and_then(|p| p.large.clone().or_else(|| p.normal.clone())))
        };
        let url = download_url.ok_or_else(|| anyhow::anyhow!("Node does not have any downloadable asset or preview."))?;
        download_url = Some(url);
    }

    let url = download_url.ok_or_else(|| anyhow::anyhow!("No download URL resolved"))?;
    let bytes = api.download_file(&url).await?;

    if asset_extension.is_empty() {
        asset_extension = infer_extension(&url);
    }

    let out_path = PathBuf::from(out.unwrap_or("."));
    if !out_path.is_absolute() {
        anyhow::bail!("out must be an absolute path");
    }
    let (out_dir, file_name) = if out_path.is_dir() {
        (out_path, None)
    } else if out_path.extension().is_some() {
        let parent = out_path.parent().map(PathBuf::from).unwrap_or_else(|| PathBuf::from("."));
        (parent, Some(out_path.file_name().unwrap_or_default().to_string_lossy().to_string()))
    } else {
        (out_path, None)
    };
    std::fs::create_dir_all(&out_dir)?;
    let base_name = name.unwrap_or(&fallback_name).to_string();
    let final_name = file_name.unwrap_or_else(|| format!("{}{}", sanitize_name(&base_name), asset_extension));
    let save_path = out_dir.join(&final_name);
    std::fs::write(&save_path, &bytes)?;
    Ok((save_path, bytes.len() as u64, final_name, url))
}

/* -------------------------------- catalog ---------------------------------- */

/// Build catalog designs from a page listing (type filter + previous alias/tag preservation).
pub fn normalize_catalog_designs(rows: &[PageRow], include_types: &[String], previous: &Catalog, now: &str) -> Vec<CatalogDesign> {
    let previous_index: HashMap<String, CatalogDesign> = previous
        .designs
        .iter()
        .flat_map(|d| vec![(d.id.clone(), d.clone()), (d.url.clone(), d.clone()), (d.name.clone(), d.clone())])
        .collect();
    rows.iter()
        .filter(|row| include_types.is_empty() || include_types.contains(&row.r#type.to_lowercase()))
        .map(|row| {
            let prev = previous_index.get(&row.id).or_else(|| previous_index.get(&row.url)).or_else(|| previous_index.get(&row.name));
            CatalogDesign {
                id: row.id.clone(),
                name: row.name.clone(),
                r#type: row.r#type.clone(),
                url: sanitize_url(&row.url),
                project_id: row.project_id.clone(),
                parent_id: row.parent_id.clone().unwrap_or_default(),
                aliases: prev.map(|p| p.aliases.clone()).unwrap_or_default(),
                tags: prev.map(|p| p.tags.clone()).unwrap_or_default(),
                last_synced_at: now.to_string(),
            }
        })
        .filter(|d| !d.url.is_empty() && !d.name.is_empty())
        .collect()
}

pub fn search_designs(workspace: &std::path::Path, query: &str, limit: u32) -> anyhow::Result<Vec<Value>> {
    let catalog = Catalog::load(workspace)?;
    let aliases: HashMap<String, Value> = std::fs::read_to_string(workspace.join(crate::catalog::MOONVY_DIR).join("aliases.json"))
        .ok()
        .and_then(|raw| serde_json::from_str(&raw).ok())
        .unwrap_or_default();
    let matches = search_catalog(&catalog, &aliases, query);
    Ok(matches.into_iter().take(limit as usize).map(|m| serde_json::to_value(m).unwrap_or(Value::Null)).collect())
}

pub fn design_summary_json(design: &CatalogDesign) -> Value {
    serde_json::to_value(design_summary(design)).unwrap_or(Value::Null)
}

pub fn tree_payload(genome: &Genome, frame: Option<&str>, options: &TreeOptions, include_assets: bool) -> Value {
    let tree = extract_tree(genome, frame, options);
    let mut payload = json!({ "items": tree });
    if include_assets {
        let assets: Vec<Value> = genome
            .images
            .iter()
            .take(50)
            .map(|(hash, info)| json!({ "hash": hash, "url": info.url.clone().unwrap_or_default(), "type": info.r#type.clone() }))
            .collect();
        payload["assets"] = Value::Array(assets);
    }
    payload
}
