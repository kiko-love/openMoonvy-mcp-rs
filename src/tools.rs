/*
 * Shared tool logic: page listing, asset download, catalog sync, helpers.
 */

use std::collections::{HashMap, HashSet};
use std::path::PathBuf;

use rmcp::ErrorData as McpError;
use serde_json::{Value, json};

use crate::api::{MoonvyApi, MoonvyNode, MoonvyUrl, parse_moonvy_url};
use crate::catalog::{Catalog, CatalogDesign, search_catalog};
use crate::genome::{Genome, GenomeNode, TreeOptions, absolute_rect, extract_tree, find_node};

pub fn tool_error<E: std::fmt::Display>(error: E) -> McpError {
    McpError::internal_error(format!("[moonvy_error] {error}"), None)
}

pub fn parse_ids(url: &str) -> anyhow::Result<MoonvyUrl> {
    parse_moonvy_url(url).ok_or_else(|| anyhow::anyhow!("Could not parse Moonvy URL"))
}

pub fn json_string(value: Value) -> Result<String, McpError> {
    serde_json::to_string(&value).map_err(tool_error)
}

/// Disk cache for genomes (~/.moonvy-ai/genome-cache, 5 min TTL).
fn genome_cache_dir() -> PathBuf {
    std::env::var("MOONVY_CACHE_DIR")
        .map(PathBuf::from)
        .unwrap_or_else(|_| {
            let home = dirs::home_dir().unwrap_or_else(|| PathBuf::from("."));
            home.join(".moonvy-ai").join("genome-cache")
        })
}

const GENOME_CACHE_TTL_MS: u128 = 5 * 60 * 1000;

fn read_genome_cache(key: &str) -> Option<Genome> {
    let path = genome_cache_dir().join(format!("{key}.json"));
    let meta = std::fs::metadata(&path).ok()?;
    let modified = meta.modified().ok()?;
    if std::time::SystemTime::now()
        .duration_since(modified)
        .ok()?
        .as_millis()
        > GENOME_CACHE_TTL_MS
    {
        return None;
    }
    let cached: Genome = std::fs::read_to_string(path)
        .ok()
        .and_then(|raw| serde_json::from_str(&raw).ok())?;
    // Reject caches written by older schema versions (e.g. snake_case
    // serialization) that deserialize into an empty genome without error.
    if cached.pages.is_empty() && cached.images.is_empty() {
        return None;
    }
    Some(cached)
}

fn write_genome_cache(key: &str, genome: &Genome) {
    let dir = genome_cache_dir();
    if std::fs::create_dir_all(&dir).is_err() {
        return;
    }
    if let Ok(json) = serde_json::to_string(genome) {
        let _ = std::fs::write(dir.join(format!("{key}.json")), json);
    }
}

/// Auto-correct a project-directory URL to a concrete design file URL.
/// Supports an optional `?design=<name>` query param: when present, the best
/// matching design (case-insensitive substring) is selected instead of the
/// first one. Without a match, the error lists the available design names so
/// the caller can pick (e.g. "验证码登录", "密码登录").
async fn resolve_design_url(api: &MoonvyApi, url: &str) -> anyhow::Result<String> {
    let ids = parse_ids(url)?;
    if ids.file_id.is_some() {
        return Ok(url.to_string());
    }
    let rows = list_pages(api, url, 500, 10).await?;
    let designs: Vec<&PageRow> = rows
        .iter()
        .filter(|r| r.r#type == "design" && !r.url.is_empty())
        .collect();
    if designs.is_empty() {
        anyhow::bail!("Project directory has no design files");
    }
    // Optional `?design=` selector: substring match over design names.
    let wanted = url::Url::parse(url)
        .ok()
        .and_then(|u| {
            u.query_pairs()
                .find(|(k, _)| k == "design")
                .map(|(_, v)| v.to_string())
        })
        .filter(|v| !v.is_empty());
    match wanted {
        Some(w) => {
            let lower = w.to_lowercase();
            if let Some(hit) = designs
                .iter()
                .find(|d| d.name.to_lowercase().contains(&lower))
            {
                return Ok(hit.url.clone());
            }
            let names: Vec<String> = designs.iter().map(|d| d.name.clone()).collect();
            anyhow::bail!(
                "No design matches \"{w}\". Available designs: {}",
                names.join(", ")
            )
        }
        None => Ok(designs[0].url.clone()),
    }
}

pub async fn genome_for_url(api: &MoonvyApi, url: &str) -> anyhow::Result<(MoonvyUrl, Genome)> {
    let url = resolve_design_url(api, url).await?;
    let ids = parse_ids(&url)?;
    let node_id = ids
        .file_id
        .clone()
        .or_else(|| ids.dir_id.clone())
        .ok_or_else(|| anyhow::anyhow!("No file or directory ID in URL"))?;

    if let Some(cached) = read_genome_cache(&node_id) {
        return Ok((ids, cached));
    }
    let node = api.get_node(&ids.project_id, &node_id, "full").await?;
    let genome_url = node
        .files
        .as_ref()
        .and_then(|f| f.genome.as_ref())
        .and_then(|g| g.url.clone())
        .ok_or_else(|| anyhow::anyhow!("No genome file found for node \"{node_id}\""))?;
    let genome = api.fetch_genome(&genome_url).await?;
    write_genome_cache(&node_id, &genome);
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
            if c.is_alphanumeric() || c == '_' || c == '-' || ('\u{4e00}'..='\u{9fff}').contains(&c)
            {
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
#[serde(rename_all = "camelCase")]
pub struct PageRow {
    pub id: String,
    pub name: String,
    pub r#type: String,
    pub parent_id: Option<String>,
    pub project_id: String,
    pub url: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub preview: Option<PagePreview>,
    /// Best-effort timestamps from the list API (may be absent).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub created_at: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub updated_at: Option<String>,
    /// Best-effort frame dimensions from the list API (may be absent).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub width: Option<i64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub height: Option<i64>,
}

#[derive(Debug, Clone, Default, serde::Serialize)]
#[serde(rename_all = "camelCase")]
pub struct PagePreview {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub normal: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub large: Option<String>,
}

fn pick_string(obj: &serde_json::Map<String, Value>, keys: &[&str]) -> Option<String> {
    for key in keys {
        if let Some(Value::String(s)) = obj.get(*key)
            && !s.is_empty()
        {
            return Some(s.clone());
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
                if let Some(candidate) = map
                    .get("data")
                    .or_else(|| map.get("result"))
                    .or_else(|| map.get("list"))
                {
                    stack.push(candidate);
                }
            }
            _ => {}
        }
    }
    Vec::new()
}

/// Pagination metadata pulled from a list response in one pass:
/// total, hasNext/hasMore, pageSize/limit, and the collected list length.
struct Pagination {
    total: Option<i64>,
    has_next: Option<bool>,
    page_size: Option<i64>,
    length: Option<i64>,
}

fn pagination_meta(response: &Value, collected: usize) -> Pagination {
    let mut meta = Pagination {
        total: None,
        has_next: None,
        page_size: None,
        length: None,
    };
    let mut stack: Vec<&Value> = vec![response];
    while let Some(v) = stack.pop() {
        match v {
            Value::Number(n) => {
                if meta.total.is_none() && n.is_i64() {
                    meta.total = n.as_i64();
                }
            }
            Value::Bool(b) => {
                if meta.has_next.is_none() {
                    meta.has_next = Some(*b);
                }
            }
            Value::Object(map) => {
                for (key, value) in map {
                    match key.as_str() {
                        "total" => meta.total = value.as_i64(),
                        "hasNext" | "hasMore" => meta.has_next = value.as_bool(),
                        "pageSize" | "limit" => meta.page_size = value.as_i64(),
                        "length" => meta.length = value.as_i64(),
                        "data" | "result" | "list" => stack.push(value),
                        _ => {}
                    }
                }
            }
            _ => {}
        }
    }
    if meta.length.is_none() {
        meta.length = Some(collected as i64);
    }
    meta
}

fn is_last_page(meta: &Pagination) -> bool {
    meta.has_next == Some(false)
        || meta
            .total
            .is_some_and(|total| meta.length.is_some_and(|len| len >= total))
        || meta
            .page_size
            .is_some_and(|size| meta.length.is_some_and(|len| len < size))
}

fn normalize_item(item: &Value, project_id: &str, input_dir_id: Option<&str>) -> Option<PageRow> {
    let obj = item.as_object()?;
    let id = pick_string(obj, &["id", "nodeId", "fileId", "_id", "uuid"])?;
    let parent_id = pick_string(obj, &["parentId", "pid", "dirId", "folderId", "parent_id"]);
    let created_at = pick_string(
        obj,
        &[
            "createdAt",
            "createTime",
            "ctime",
            "createdAtTime",
            "created",
        ],
    );
    let updated_at = pick_string(
        obj,
        &[
            "updatedAt",
            "updateTime",
            "mtime",
            "updatedAtTime",
            "updated",
        ],
    );
    let mut width = obj
        .get("width")
        .and_then(|v| v.as_i64())
        .or_else(|| obj.get("w").and_then(|v| v.as_i64()));
    let mut height = obj
        .get("height")
        .and_then(|v| v.as_i64())
        .or_else(|| obj.get("h").and_then(|v| v.as_i64()));
    // Some list APIs pack dimensions into a nested size/rect object.
    for key in ["size", "rect", "dimensions", "frame"] {
        if let Some(map) = obj.get(key).and_then(|v| v.as_object()) {
            if width.is_none() {
                width = map
                    .get("width")
                    .and_then(|v| v.as_i64())
                    .or_else(|| map.get("w").and_then(|v| v.as_i64()));
            }
            if height.is_none() {
                height = map
                    .get("height")
                    .and_then(|v| v.as_i64())
                    .or_else(|| map.get("h").and_then(|v| v.as_i64()));
            }
        }
    }
    let row = PageRow {
        id: id.clone(),
        name: pick_string(obj, &["name", "title", "displayName"]).unwrap_or_default(),
        r#type: pick_string(obj, &["type", "nodeType", "kind", "fileType"]).unwrap_or_default(),
        parent_id: parent_id.clone(),
        project_id: project_id.to_string(),
        url: crate::api::file_url_for(project_id, parent_id.as_deref().or(input_dir_id), &id),
        preview: None,
        created_at,
        updated_at,
        width,
        height,
    };
    if let Some(preview) = obj.get("preview").and_then(|p| p.as_object()) {
        let normal = pick_string(preview, &["normal"]);
        let large = pick_string(preview, &["large"]);
        if normal.is_some() || large.is_some() {
            return Some(PageRow {
                preview: Some(PagePreview { normal, large }),
                ..row
            });
        }
    }
    Some(row)
}

fn can_have_children(row_type: &str) -> bool {
    matches!(
        row_type.to_lowercase().as_str(),
        "any" | "dir" | "directory" | "folder"
    )
}

/// Paginated BFS listing of a project directory.
pub async fn list_pages(
    api: &MoonvyApi,
    url: &str,
    limit: u32,
    max_pages: u32,
) -> anyhow::Result<Vec<PageRow>> {
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
            let response = api
                .list_nodes(&ids.project_id, page_index, scope_id.as_deref())
                .await?;
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
            let meta = pagination_meta(&response, rows.len());
            if is_last_page(&meta) {
                break;
            }
        }
    }
    Ok(rows)
}

/* -------------------------------- asset ------------------------------------ */

fn resolve_asset_url(
    hash: &str,
    assets: &serde_json::Map<String, Value>,
    genome: &Genome,
) -> String {
    if let Some(Value::String(url)) = assets.get(hash)
        && !url.is_empty()
    {
        return url.clone();
    }
    genome
        .images
        .get(hash)
        .and_then(|i| i.url.clone())
        .unwrap_or_else(|| format!("https://fs.moonvy.com/{hash}"))
}

/// id (or any of its `;`-separated segments) -> parent node, built once.
fn parent_index(genome: &Genome) -> HashMap<String, &GenomeNode> {
    fn walk<'a>(node: &'a GenomeNode, out: &mut HashMap<String, &'a GenomeNode>) {
        for child in &node.children {
            if let Some(id) = &child.id {
                for segment in id.split(';').map(str::trim).filter(|s| !s.is_empty()) {
                    out.insert(segment.to_string(), node);
                }
            }
            walk(child, out);
        }
    }
    let mut index = HashMap::new();
    for page in &genome.pages {
        walk(page, &mut index);
    }
    index
}

fn lookup_parent<'a>(
    index: &'a HashMap<String, &'a GenomeNode>,
    id: &str,
) -> Option<&'a GenomeNode> {
    index.get(id).copied().or_else(|| {
        id.split(';')
            .map(str::trim)
            .filter(|s| !s.is_empty())
            .find_map(|s| index.get(s).copied())
    })
}

/// Walk up the ancestor chain and return the first snapshot (hash, name).
fn find_parent_snapshot(genome: &Genome, layer_id: &str) -> Option<(String, Option<String>)> {
    let index = parent_index(genome);
    let mut current = lookup_parent(&index, layer_id);
    let mut seen: HashSet<String> = HashSet::new();
    while let Some(node) = current {
        let Some(id) = node.id.clone() else { break };
        if !seen.insert(id.clone()) {
            break;
        }
        if let Some(snapshot) = node
            .snapshot
            .clone()
            .or_else(|| node.snapshot_preview.clone())
        {
            return Some((snapshot, node.name.clone()));
        }
        current = lookup_parent(&index, &id);
    }
    None
}

/// Resolved asset: download URL plus a display name and file extension.
pub struct ResolvedAsset {
    pub url: String,
    pub name: String,
    pub extension: String,
    /// Actual asset type resolved (slice|snapshot|image|image-fallback-snapshot).
    pub resolved_type: String,
    /// When the asset was resolved via a snapshot (own or ancestor), the
    /// node's absolute rect in artboard coordinates, for cropping.
    pub node_rect: Option<(f64, f64, f64, f64)>,
    /// Snapshot hash when resolved via snapshot.
    pub snapshot_hash: Option<String>,
    /// Size of the containing artboard (page root) in design coordinates.
    pub artboard_size: Option<(f64, f64)>,
}

/// Parse PNG (IHDR) or JPEG (SOF0/SOF2) dimensions from image bytes without
/// decoding the full image. Returns (width, height).
pub fn probe_image_size(bytes: &[u8]) -> Option<(u32, u32)> {
    if bytes.len() >= 24 && bytes[0..8] == [0x89, b'P', b'N', b'G', 0x0d, 0x0a, 0x1a, 0x0a] {
        let w = u32::from_be_bytes([bytes[16], bytes[17], bytes[18], bytes[19]]);
        let h = u32::from_be_bytes([bytes[20], bytes[21], bytes[22], bytes[23]]);
        return Some((w, h));
    }
    if bytes.len() >= 4 && bytes[0] == 0xff && bytes[1] == 0xd8 {
        let mut i = 2usize;
        while i + 9 < bytes.len() {
            if bytes[i] != 0xff {
                i += 1;
                continue;
            }
            let marker = bytes[i + 1];
            // SOF0..SOF15 (except DHT C4, JPG C8, DAC CC)
            if matches!(marker, 0xc0..=0xcf) && !matches!(marker, 0xc4 | 0xc8 | 0xcc) {
                let h = u16::from_be_bytes([bytes[i + 5], bytes[i + 6]]);
                let w = u16::from_be_bytes([bytes[i + 7], bytes[i + 8]]);
                return Some((w as u32, h as u32));
            }
            let seg_len = u16::from_be_bytes([bytes[i + 2], bytes[i + 3]]) as usize;
            i += 2 + seg_len;
        }
    }
    None
}

/// Crop a downloaded snapshot to the node's absolute rect, scaled from
/// artboard coordinates to snapshot pixels. Returns the cropped PNG bytes.
pub fn crop_snapshot_bytes(
    bytes: &[u8],
    artboard_size: (f64, f64),
    node_rect: (f64, f64, f64, f64),
) -> anyhow::Result<Vec<u8>> {
    let (aw, ah) = artboard_size;
    if aw <= 0.0 || ah <= 0.0 {
        anyhow::bail!("Invalid artboard size {aw}x{ah}");
    }
    let img = image::load_from_memory(bytes)
        .map_err(|e| anyhow::anyhow!("Failed to decode snapshot image: {e}"))?;
    let (sw, sh) = (img.width(), img.height());
    let scale_x = sw as f64 / aw;
    let scale_y = sh as f64 / ah;
    let (nx, ny, nw, nh) = node_rect;
    let x = (nx * scale_x).round() as u32;
    let y = (ny * scale_y).round() as u32;
    let w = (nw * scale_x).round() as u32;
    let h = (nh * scale_y).round() as u32;
    let w = w.max(1);
    let h = h.max(1);
    let x = x.min(sw.saturating_sub(1));
    let y = y.min(sh.saturating_sub(1));
    let w = w.min(sw - x);
    let h = h.min(sh - y);
    let cropped = img.crop_imm(x, y, w.max(1), h.max(1));
    let mut out = Vec::new();
    cropped
        .write_to(&mut std::io::Cursor::new(&mut out), image::ImageFormat::Png)
        .map_err(|e| anyhow::anyhow!("Failed to encode cropped PNG: {e}"))?;
    Ok(out)
}

/// Resolve the download URL of a slice/snapshot/image from a Moonvy node
/// without fetching the bytes. Shared by moonvy_download_asset (which then
/// downloads and persists) and moonvy_get_asset_url (URL-only).
pub async fn resolve_asset(
    api: &MoonvyApi,
    url: &str,
    node: &str,
    r#type: Option<&str>,
    slice_format: Option<&str>,
) -> anyhow::Result<ResolvedAsset> {
    let ids = parse_ids(url)?;
    let mut download_url: Option<String> = None;
    let mut fallback_name = "asset".to_string();
    let mut asset_extension = String::new();
    let mut resolved_type = String::new();
    let mut node_rect: Option<(f64, f64, f64, f64)> = None;
    let mut snapshot_hash: Option<String> = None;
    let mut artboard_size: Option<(f64, f64)> = None;

    if let Some(file_id) = &ids.file_id {
        let file_node = api.get_node(&ids.project_id, file_id, "full").await?;
        let assets: serde_json::Map<String, Value> = file_node
            .meta
            .as_ref()
            .and_then(|m| m.assets.clone())
            .unwrap_or_default();
        let genome = api
            .fetch_genome(
                &file_node
                    .files
                    .as_ref()
                    .and_then(|f| f.genome.as_ref())
                    .and_then(|g| g.url.clone())
                    .ok_or_else(|| anyhow::anyhow!("No genome file found"))?,
            )
            .await?;
        // Artboard size = containing page root rect (for snapshot crop scaling).
        artboard_size = genome
            .pages
            .iter()
            .find(|p| find_node(p, node).is_some())
            .and_then(|p| p.rect.clone())
            .map(|r| (r.w, r.h));

        let layer = genome
            .pages
            .iter()
            .find_map(|page| find_node(page, node))
            .cloned();
        if let Some(layer) = layer {
            fallback_name = layer.name.clone().unwrap_or_else(|| "unnamed".to_string());
            let inferred_type = r#type.map(str::to_string).unwrap_or_else(|| {
                if layer.slices.as_ref().and_then(|s| s.as_object()).is_some() {
                    "slice".to_string()
                } else if layer.snapshot.is_some() {
                    "snapshot".to_string()
                } else if layer
                    .fills
                    .iter()
                    .any(|f| f.r#type.as_deref() == Some("image"))
                {
                    "image".to_string()
                } else {
                    "snapshot".to_string()
                }
            });
            resolved_type = inferred_type.clone();

            match inferred_type.as_str() {
                "slice" => {
                    let slices = layer
                        .slices
                        .as_ref()
                        .and_then(|s| s.as_object())
                        .ok_or_else(|| anyhow::anyhow!("Node does not have slices."))?;
                    let format = slice_format.unwrap_or("svg");
                    let slice_info = slices
                        .get(format)
                        .or_else(|| slices.get("max"))
                        .or_else(|| slices.get("base"))
                        .and_then(|s| s.as_object())
                        .and_then(|o| o.get("id"))
                        .and_then(|v| v.as_str());
                    let hash = slice_info.map(str::to_string).ok_or_else(|| {
                        anyhow::anyhow!("Format \"{format}\" not found on slice.")
                    })?;
                    download_url = Some(resolve_asset_url(&hash, &assets, &genome));
                    asset_extension = if format == "svg" {
                        ".svg".to_string()
                    } else {
                        ".png".to_string()
                    };
                }
                "snapshot" => {
                    let mut hash = layer
                        .snapshot
                        .clone()
                        .or_else(|| layer.snapshot_preview.clone());
                    let mut snapshot_source = "own".to_string();
                    if hash.is_none()
                        && let Some(layer_id) = layer.id.as_deref()
                        && let Some((parent_hash, parent_name)) =
                            find_parent_snapshot(&genome, layer_id)
                    {
                        hash = Some(parent_hash);
                        snapshot_source = "ancestor".to_string();
                        if let Some(parent_name) = parent_name {
                            fallback_name = parent_name;
                        }
                    }
                    let hash = hash.ok_or_else(|| {
                        anyhow::anyhow!("No snapshot found for this node or its parents.")
                    })?;
                    download_url = Some(resolve_asset_url(&hash, &assets, &genome));
                    asset_extension = ".png".to_string();
                    resolved_type = format!("snapshot:{snapshot_source}");
                    snapshot_hash = Some(hash);
                    node_rect = layer
                        .id
                        .as_deref()
                        .and_then(|id| absolute_rect(&genome, id))
                        .map(|r| (r.x, r.y, r.w, r.h));
                }
                "image" => {
                    let image_fill = layer
                        .fills
                        .iter()
                        .find(|f| f.r#type.as_deref() == Some("image"));
                    let hash = image_fill
                        .and_then(|f| {
                            f.image_hash
                                .clone()
                                .or_else(|| f.id.clone())
                                .or_else(|| f.hash.clone())
                        })
                        .filter(|h| {
                            // A hash only counts when it actually resolves to a
                            // URL (assets manifest or genome.images); otherwise
                            // treat the fill as having no asset reference and
                            // fall back to the rendered snapshot.
                            !h.is_empty()
                                && (assets.contains_key(h) || genome.images.contains_key(h))
                        });
                    if let Some(hash) = hash {
                        download_url = Some(resolve_asset_url(&hash, &assets, &genome));
                        asset_extension = genome
                            .images
                            .get(&hash)
                            .and_then(|i| i.r#type.clone())
                            .map(|t| format!(".{t}"))
                            .unwrap_or_else(|| ".png".to_string());
                    } else if let Some(layer_id) = layer.id.as_deref() {
                        // Fallback: image fill without a resolvable asset
                        // reference — use the node's own rendered snapshot,
                        // or the nearest ancestor's, and record the node rect
                        // so callers can crop exactly the node's area.
                        let mut hash = layer
                            .snapshot
                            .clone()
                            .or_else(|| layer.snapshot_preview.clone());
                        let mut snapshot_source = "own".to_string();
                        if hash.is_none() {
                            if let Some((parent_hash, _parent_name)) =
                                find_parent_snapshot(&genome, layer_id)
                            {
                                hash = Some(parent_hash);
                                snapshot_source = "ancestor".to_string();
                            }
                        }
                        if let Some(hash) = hash {
                            download_url = Some(resolve_asset_url(&hash, &assets, &genome));
                            asset_extension = ".png".to_string();
                            resolved_type =
                                format!("image-fallback-snapshot:{snapshot_source}");
                            snapshot_hash = Some(hash);
                            node_rect =
                                absolute_rect(&genome, layer_id).map(|r| (r.x, r.y, r.w, r.h));
                        } else {
                            anyhow::bail!(
                                "Image fill does not have a valid asset reference and no snapshot fallback exists."
                            );
                        }
                    } else {
                        anyhow::bail!("Image fill does not have a valid asset reference.");
                    }
                }
                other => anyhow::bail!("Invalid type: {other}"),
            }
        }
    }

    if download_url.is_none() {
        let node_info: MoonvyNode = api.get_node(&ids.project_id, node, "full").await?;
        fallback_name = node_info
            .name
            .clone()
            .unwrap_or_else(|| "unnamed".to_string());
        download_url = if r#type == Some("snapshot") {
            node_info
                .preview
                .as_ref()
                .and_then(|p| p.large.clone().or_else(|| p.normal.clone()))
        } else {
            node_info
                .files
                .as_ref()
                .and_then(|f| f.file.as_ref())
                .and_then(|f| f.url.clone())
                .or_else(|| {
                    node_info
                        .preview
                        .as_ref()
                        .and_then(|p| p.large.clone().or_else(|| p.normal.clone()))
                })
        };
        let url = download_url.ok_or_else(|| {
            anyhow::anyhow!("Node does not have any downloadable asset or preview.")
        })?;
        download_url = Some(url);
    }

    let url = download_url.ok_or_else(|| anyhow::anyhow!("No download URL resolved"))?;
    if asset_extension.is_empty() {
        asset_extension = infer_extension(&url);
    }
    Ok(ResolvedAsset {
        url,
        name: fallback_name,
        extension: asset_extension,
        resolved_type,
        node_rect,
        snapshot_hash,
        artboard_size,
    })
}

/// Download a slice/snapshot/image from a Moonvy node; returns
/// (path, size, name, url, snapshot_size).
/// When `crop` is true and the asset resolved via a snapshot with a known
/// node rect + artboard size, the downloaded image is cropped to the node's
/// area (scaled from artboard to snapshot pixels) and saved as PNG.
pub async fn download_asset(
    api: &MoonvyApi,
    url: &str,
    node: &str,
    r#type: Option<&str>,
    slice_format: Option<&str>,
    name: Option<&str>,
    out: Option<&str>,
    crop: bool,
) -> anyhow::Result<(PathBuf, u64, String, String, Option<(u32, u32)>)> {
    let resolved = resolve_asset(api, url, node, r#type, slice_format).await?;
    let mut bytes = api.download_file(&resolved.url).await?;

    // Report the original (pre-crop) pixel size of the downloaded image so
    // callers can compute the snapshot scale vs. artboard size.
    let snapshot_size = probe_image_size(&bytes);

    // Crop to the node rect when requested and geometry is available. The
    // snapshot pixels are scaled from artboard coordinates by the ratio of
    // snapshot size to artboard size (Moonvy renders 2x by default, so a
    // 1440x900 artboard becomes a 2880x1800 snapshot).
    let (extension, extra_suffix) = if crop {
        match (resolved.node_rect, resolved.artboard_size) {
            (Some(node_rect), Some(artboard)) => {
                let cropped = crop_snapshot_bytes(&bytes, artboard, node_rect)?;
                bytes = cropped;
                (".png".to_string(), "_crop".to_string())
            }
            _ => (resolved.extension.clone(), String::new()),
        }
    } else {
        (resolved.extension.clone(), String::new())
    };

    let out_path = PathBuf::from(out.unwrap_or("."));
    if !out_path.is_absolute() {
        anyhow::bail!("out must be an absolute path");
    }
    let (out_dir, file_name) = if out_path.is_dir() {
        (out_path, None)
    } else if out_path.extension().is_some() {
        let parent = out_path
            .parent()
            .map(PathBuf::from)
            .unwrap_or_else(|| PathBuf::from("."));
        (
            parent,
            Some(
                out_path
                    .file_name()
                    .unwrap_or_default()
                    .to_string_lossy()
                    .to_string(),
            ),
        )
    } else {
        (out_path, None)
    };
    std::fs::create_dir_all(&out_dir)?;
    let base_name = name.unwrap_or(&resolved.name).to_string();
    let final_name = file_name.unwrap_or_else(|| {
        let stem = sanitize_name(&base_name);
        if extra_suffix.is_empty() {
            format!("{stem}{extension}")
        } else {
            format!("{stem}{extra_suffix}{extension}")
        }
    });
    let save_path = out_dir.join(&final_name);
    std::fs::write(&save_path, &bytes)?;
    Ok((
        save_path,
        bytes.len() as u64,
        final_name,
        resolved.url,
        snapshot_size,
    ))
}

/* -------------------------------- catalog ---------------------------------- */

/// Build catalog designs from a page listing (type filter + previous alias/tag preservation).
pub fn normalize_catalog_designs(
    rows: &[PageRow],
    include_types: &[String],
    previous: &Catalog,
    now: &str,
) -> Vec<CatalogDesign> {
    let previous_index: HashMap<String, CatalogDesign> = previous
        .designs
        .iter()
        .flat_map(|d| {
            vec![
                (d.id.clone(), d.clone()),
                (d.url.clone(), d.clone()),
                (d.name.clone(), d.clone()),
            ]
        })
        .collect();
    rows.iter()
        .filter(|row| {
            include_types.is_empty() || include_types.contains(&row.r#type.to_lowercase())
        })
        .map(|row| {
            let prev = previous_index
                .get(&row.id)
                .or_else(|| previous_index.get(&row.url))
                .or_else(|| previous_index.get(&row.name));
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
                width: row.width,
                height: row.height,
            }
        })
        .filter(|d| !d.url.is_empty() && !d.name.is_empty())
        .collect()
}

pub fn search_designs(
    workspace: &std::path::Path,
    query: &str,
    limit: u32,
) -> anyhow::Result<Vec<Value>> {
    let catalog = Catalog::load(workspace)?;
    let aliases: HashMap<String, Value> =
        std::fs::read_to_string(crate::catalog::artifact_path(workspace, "aliases"))
            .ok()
            .and_then(|raw| serde_json::from_str(&raw).ok())
            .unwrap_or_default();
    let matches = search_catalog(&catalog, &aliases, query);
    Ok(matches
        .into_iter()
        .take(limit as usize)
        .map(|m| serde_json::to_value(m).unwrap_or(Value::Null))
        .collect())
}

/// Rewrite whole-number floats as integers (16.0 -> 16) to shrink payloads.
pub fn compact_numbers(value: &mut Value) {
    match value {
        Value::Number(n) => {
            if let Some(f) = n.as_f64()
                && f.fract() == 0.0
                && f >= i64::MIN as f64
                && f <= i64::MAX as f64
            {
                *value = Value::Number(serde_json::Number::from(f as i64));
            }
        }
        Value::Array(items) => {
            for item in items {
                compact_numbers(item);
            }
        }
        Value::Object(map) => {
            for item in map.values_mut() {
                compact_numbers(item);
            }
        }
        _ => {}
    }
}

pub fn tree_payload(
    genome: &Genome,
    frame: Option<&str>,
    options: &TreeOptions,
    include_assets: bool,
) -> Value {
    let tree = extract_tree(genome, frame, options);
    let mut payload = json!({ "items": tree });
    if include_assets {
        // hash -> {url, type, refs:[nodeId...]}: deduplicated by content hash
        // and annotated with every tree node that references the asset via
        // its snapshotHash.
        let mut refs: HashMap<String, Vec<String>> = HashMap::new();
        let mut stack: Vec<&crate::genome::TreeNode> = tree.iter().collect();
        while let Some(node) = stack.pop() {
            if let Some(hash) = node.snapshot_hash.as_deref() {
                refs.entry(hash.to_string())
                    .or_default()
                    .push(node.id.clone());
            }
            if let Some(children) = &node.children {
                stack.extend(children.iter());
            }
        }
        let assets: Vec<Value> = genome
            .images
            .iter()
            .take(50)
            .map(|(hash, info)| {
                let mut entry = json!({
                    "hash": hash,
                    "url": info.url.clone().unwrap_or_default(),
                    "type": info.r#type.clone(),
                });
                if let Some(node_ids) = refs.get(hash) {
                    entry["refs"] = Value::Array(
                        node_ids
                            .iter()
                            .map(|id| Value::String(id.clone()))
                            .collect(),
                    );
                }
                entry
            })
            .collect();
        payload["assets"] = Value::Array(assets);
    }
    compact_numbers(&mut payload);
    payload
}

#[cfg(test)]
mod tests {
    use super::*;

    fn sample_item_json(extra: &str) -> Value {
        serde_json::from_str(&format!(
            r#"{{
                "id": "file-1",
                "name": "首页",
                "type": "design",
                "parentId": "dir-1",
                "createdAt": "2026-01-02T03:04:05Z",
                "updatedAt": "2026-08-10T00:00:00Z",
                "size": {{ "width": 1440, "height": 900 }},
                "preview": {{ "normal": "https://cdn.example.com/p.png" }}
                {extra}
            }}"#
        ))
        .unwrap()
    }

    #[test]
    fn normalize_item_parses_metadata_and_serializes_camel_case() {
        let row = normalize_item(&sample_item_json(""), "proj-1", None).unwrap();
        assert_eq!(row.name, "首页");
        assert_eq!(row.parent_id.as_deref(), Some("dir-1"));
        assert_eq!(row.created_at.as_deref(), Some("2026-01-02T03:04:05Z"));
        assert_eq!(row.updated_at.as_deref(), Some("2026-08-10T00:00:00Z"));
        assert_eq!(row.width, Some(1440));
        assert_eq!(row.height, Some(900));
        assert_eq!(
            row.preview.as_ref().and_then(|p| p.normal.as_deref()),
            Some("https://cdn.example.com/p.png")
        );

        let value = serde_json::to_value(&row).unwrap();
        let map = value.as_object().unwrap();
        assert!(
            map.contains_key("parentId"),
            "must serialize camelCase parentId"
        );
        assert!(
            map.contains_key("projectId"),
            "must serialize camelCase projectId"
        );
        assert!(
            map.contains_key("createdAt"),
            "must serialize camelCase createdAt"
        );
        assert!(map.contains_key("width"), "width must be present");
    }

    #[test]
    fn normalize_item_tolerates_missing_metadata() {
        let row = normalize_item(
            &serde_json::json!({ "id": "file-2", "name": "首页" }),
            "proj-1",
            None,
        )
        .unwrap();
        assert!(row.created_at.is_none());
        assert!(row.updated_at.is_none());
        assert!(row.width.is_none());
        assert!(row.height.is_none());
        let value = serde_json::to_value(&row).unwrap();
        let map = value.as_object().unwrap();
        assert!(
            !map.contains_key("createdAt") && !map.contains_key("width"),
            "absent metadata must be omitted, not null"
        );
    }

    #[test]
    fn normalize_item_reads_flat_dimensions() {
        let row = normalize_item(
            &serde_json::json!({ "id": "file-3", "name": "登录", "width": 1280, "height": 720 }),
            "proj-1",
            None,
        )
        .unwrap();
        assert_eq!(row.width, Some(1280));
        assert_eq!(row.height, Some(720));
    }

    #[test]
    fn probe_png_dimensions_from_ihdr() {
        // Minimal valid PNG header: 8-byte signature + IHDR chunk with
        // width=2880 (0x0B40), height=1800 (0x0708).
        let mut bytes = vec![0x89, b'P', b'N', b'G', 0x0d, 0x0a, 0x1a, 0x0a];
        bytes.extend_from_slice(&[
            0x00, 0x00, 0x00, 0x0d, // IHDR length
            b'I', b'H', b'D', b'R',
            0x00, 0x00, 0x0b, 0x40, // width 2880
            0x00, 0x00, 0x07, 0x08, // height 1800
            0x08, 0x06, 0x00, 0x00, 0x00, // bit depth, color type, compression, filter, interlace
            0x00, 0x00, 0x00, 0x00, // CRC placeholder
        ]);
        let (w, h) = probe_image_size(&bytes).expect("png dims");
        assert_eq!((w, h), (2880, 1800));
    }

    #[test]
    fn probe_jpeg_dimensions_from_sof() {
        // SOI + APP0 + SOF0: width 1440, height 900.
        let mut bytes = vec![0xff, 0xd8, 0xff, 0xe0];
        bytes.extend_from_slice(&[0x00, 0x10, b'J', b'F', b'I', b'F', 0x00, 0x01, 0x01, 0x00, 0x00, 0x01, 0x00, 0x01, 0x00, 0x00]);
        bytes.extend_from_slice(&[0xff, 0xc0, 0x00, 0x11, 0x08]);
        bytes.extend_from_slice(&[0x03, 0x84, 0x05, 0xa0]); // h=900, w=1440
        bytes.extend_from_slice(&[0x03, 0x01, 0x11, 0x00, 0x02, 0x11, 0x00, 0x03, 0x11, 0x00]);
        let (w, h) = probe_image_size(&bytes).expect("jpeg dims");
        assert_eq!((w, h), (1440, 900));
    }

    #[test]
    fn probe_returns_none_for_garbage() {
        assert!(probe_image_size(b"not an image at all").is_none());
        assert!(probe_image_size(&[]).is_none());
    }

    #[test]
    fn crop_scales_artboard_to_snapshot_pixels() {
        // 1x1 red PNG decoded via image crate; artboard 1440x900, snapshot
        // rendered at 2x (2880x1800) — build the 2x snapshot by constructing
        // a small image and checking the crop math on a node rect.
        let mut img = image::RgbaImage::new(4, 2);
        for px in img.pixels_mut() {
            *px = image::Rgba([255, 0, 0, 255]);
        }
        let mut png = Vec::new();
        img.write_to(
            &mut std::io::Cursor::new(&mut png),
            image::ImageFormat::Png,
        )
        .expect("encode png");

        // Artboard 4x2 (simulating 1440x900 at 1/360 scale), node rect
        // covering the left half: [0,0,2,2] -> snapshot pixels [0,0,2,2].
        let cropped = crop_snapshot_bytes(&png, (4.0, 2.0), (0.0, 0.0, 2.0, 2.0)).expect("crop");
        let decoded = image::load_from_memory(&cropped).expect("decode cropped");
        assert_eq!((decoded.width(), decoded.height()), (2, 2));

        // Right-half crop [2,0,2,2] must land within bounds.
        let cropped2 = crop_snapshot_bytes(&png, (4.0, 2.0), (2.0, 0.0, 2.0, 2.0)).expect("crop");
        let decoded2 = image::load_from_memory(&cropped2).expect("decode cropped");
        assert_eq!((decoded2.width(), decoded2.height()), (2, 2));
    }

    #[test]
    fn crop_clamps_out_of_bounds_rect() {
        let mut img = image::RgbaImage::new(2, 2);
        for px in img.pixels_mut() {
            *px = image::Rgba([0, 0, 255, 255]);
        }
        let mut png = Vec::new();
        img.write_to(
            &mut std::io::Cursor::new(&mut png),
            image::ImageFormat::Png,
        )
        .expect("encode png");
        // Node rect partially outside the artboard: must clamp, not panic.
        let cropped = crop_snapshot_bytes(&png, (2.0, 2.0), (1.5, 1.5, 4.0, 4.0)).expect("crop");
        assert!(!cropped.is_empty());
    }

    #[test]
    fn crop_rejects_zero_artboard() {
        let mut img = image::RgbaImage::new(1, 1);
        img.put_pixel(0, 0, image::Rgba([0, 0, 0, 255]));
        let mut png = Vec::new();
        img.write_to(
            &mut std::io::Cursor::new(&mut png),
            image::ImageFormat::Png,
        )
        .expect("encode png");
        assert!(crop_snapshot_bytes(&png, (0.0, 0.0), (0.0, 0.0, 1.0, 1.0)).is_err());
    }
}
