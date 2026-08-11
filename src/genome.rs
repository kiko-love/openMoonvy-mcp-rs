/**
 * Moonvy genome data structures and parsing.
 *
 * Mirrors the design/behavior contract of the TypeScript version
 * (moonvy-ai): same node shapes, same style normalization, same
 * tree options (skipEmptyGroups / flatten / only / detectDuplicates).
 */

use serde::{Deserialize, Serialize};
use std::collections::HashMap;

#[derive(Debug, Clone, Default, Deserialize)]
#[serde(default)]
pub struct Rect {
    pub x: f64,
    pub y: f64,
    pub w: f64,
    pub h: f64,
}

#[derive(Debug, Clone, Default, Deserialize)]
#[serde(default)]
pub struct Color {
    pub r: f64,
    pub g: f64,
    pub b: f64,
    pub alpha: Option<f64>,
}

#[derive(Debug, Clone, Default, Deserialize)]
#[serde(default)]
pub struct GradientStop {
    pub color: Option<Color>,
    pub position: Option<f64>,
}

#[derive(Debug, Clone, Default, Deserialize)]
#[serde(default)]
pub struct Gradient {
    pub r#type: Option<String>,
    pub stops: Vec<GradientStop>,
    pub angle: Option<f64>,
}

#[derive(Debug, Clone, Default, Deserialize)]
#[serde(default)]
pub struct Fill {
    pub r#type: Option<String>,
    pub color: Option<Color>,
    pub opacity: Option<f64>,
    pub image_hash: Option<String>,
    pub id: Option<String>,
    pub hash: Option<String>,
    pub gradient: Option<Gradient>,
    pub visible: Option<bool>,
}

#[derive(Debug, Clone, Default, Deserialize)]
#[serde(default)]
pub struct Stroke {
    pub fills: Vec<Fill>,
    pub w: Option<f64>,
}

#[derive(Debug, Clone, Default, Deserialize)]
#[serde(default)]
pub struct FontName {
    pub family: Option<String>,
}

#[derive(Debug, Clone, Default, Deserialize)]
#[serde(default)]
pub struct LineHeight {
    pub value: Option<f64>,
}

#[derive(Debug, Clone, Default, Deserialize)]
#[serde(default)]
pub struct LetterSpacing {
    pub value: Option<f64>,
}

#[derive(Debug, Clone, Default, Deserialize)]
#[serde(default)]
pub struct TextSegment {
    pub font_size: Option<f64>,
    pub font_weight: Option<f64>,
    pub font_name: Option<FontName>,
    pub line_height: Option<LineHeight>,
    pub letter_spacing: Option<LetterSpacing>,
    pub fills: Vec<Fill>,
}

#[derive(Debug, Clone, Default, Deserialize)]
#[serde(default)]
pub struct Textbox {
    pub text: Option<String>,
    pub segments: Vec<TextSegment>,
}

#[derive(Debug, Clone, Default, Deserialize)]
#[serde(default)]
pub struct Blend {
    pub opacity: Option<f64>,
}

#[derive(Debug, Clone, Default, Deserialize)]
#[serde(default)]
pub struct GenomeNode {
    pub id: Option<String>,
    pub name: Option<String>,
    pub r#type: Option<String>,
    pub rect: Option<Rect>,
    pub fills: Vec<Fill>,
    pub strokes: Vec<Stroke>,
    pub textbox: Option<Textbox>,
    pub blend: Option<Blend>,
    pub border_radius: Option<f64>,
    pub fill_link: Option<String>,
    /// Some nodes carry `slices: true` (boolean) instead of a map.
    pub slices: Option<serde_json::Value>,
    pub snapshot: Option<String>,
    pub snapshot_preview: Option<String>,
    pub children: Vec<GenomeNode>,
}

#[derive(Debug, Clone, Default, Deserialize)]
#[serde(default)]
pub struct Styles {
    pub fill_styles: Vec<StyleDef>,
}

#[derive(Debug, Clone, Default, Deserialize)]
#[serde(default)]
pub struct StyleDef {
    pub id: Option<String>,
    pub data: Vec<Fill>,
}

#[derive(Debug, Clone, Default, Deserialize)]
#[serde(default)]
pub struct ImageInfo {
    pub url: Option<String>,
    pub r#type: Option<String>,
}

#[derive(Debug, Clone, Default, Deserialize)]
#[serde(default)]
pub struct Genome {
    pub pages: Vec<GenomeNode>,
    pub styles: Option<Styles>,
    pub images: HashMap<String, ImageInfo>,
}

/* ----------------------------- style resolution ---------------------------- */

#[derive(Debug, Clone, Default, Serialize)]
pub struct NodeStyle {
    pub background: Option<String>,
    pub color: Option<String>,
    pub font_size: Option<f64>,
    pub font_weight: Option<f64>,
    pub border_radius: Option<f64>,
    pub opacity: Option<f64>,
    pub font_family: Option<String>,
    pub line_height: Option<f64>,
    pub letter_spacing: Option<f64>,
    pub stroke_width: Option<f64>,
    pub stroke_color: Option<String>,
    pub gradient: Option<GradientStyle>,
}

#[derive(Debug, Clone, Default, Serialize)]
pub struct GradientStyle {
    pub r#type: String,
    pub stops: Vec<GradientStopStyle>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub angle: Option<f64>,
}

#[derive(Debug, Clone, Default, Serialize)]
pub struct GradientStopStyle {
    pub color: String,
    pub position: f64,
}

fn clamp_u8(value: f64) -> u8 {
    (value.clamp(0.0, 255.0)).round() as u8
}

pub fn resolve_fill_color(fill: Option<&Fill>) -> Option<String> {
    let fill = fill?;
    if fill.r#type.as_deref() != Some("color") {
        return None;
    }
    let color = fill.color.as_ref()?;
    let r = clamp_u8(color.r);
    let g = clamp_u8(color.g);
    let b = clamp_u8(color.b);
    let a = fill.opacity.unwrap_or(1.0);
    if a < 1.0 {
        Some(format!("rgba({r},{g},{b},{a:.2})"))
    } else {
        Some(format!("#{r:02x}{g:02x}{b:02x}"))
    }
}

fn resolve_gradient(fill: Option<&Fill>) -> Option<GradientStyle> {
    let gradient = fill?.gradient.as_ref()?;
    let stops = gradient
        .stops
        .iter()
        .map(|stop| {
            let color = stop.color.as_ref().map_or_else(
                || "#000000".to_string(),
                |c| format!("rgba({},{},{},{})", clamp_u8(c.r), clamp_u8(c.g), clamp_u8(c.b), c.alpha.unwrap_or(1.0)),
            );
            GradientStopStyle { color, position: stop.position.unwrap_or(0.0) }
        })
        .collect();
    Some(GradientStyle {
        r#type: gradient.r#type.clone().unwrap_or_else(|| "linear".to_string()),
        stops,
        angle: gradient.angle,
    })
}

fn linked_fill<'a>(genome: &'a Genome, fill_link: Option<&'a str>) -> Option<&'a Fill> {
    let link = fill_link?;
    genome.styles.as_ref()?.fill_styles.iter().find(|s| s.id.as_deref() == Some(link))?.data.first()
}

/// Normalized style for one node. `None` fields mean "not set"; transparency
/// is encoded in rgba(...,0) rather than null.
pub fn extract_raw_node_style(genome: &Genome, raw: &GenomeNode) -> NodeStyle {
    let first_fill = raw.fills.first().or_else(|| linked_fill(genome, raw.fill_link.as_deref()));
    let mut background = resolve_fill_color(first_fill);
    let mut gradient = if background.is_none() { resolve_gradient(first_fill) } else { None };

    let mut color = None;
    let mut font_size = None;
    let mut font_weight = None;
    let mut font_family = None;
    let mut line_height = None;
    let mut letter_spacing = None;
    if let Some(seg) = raw.textbox.as_ref().and_then(|t| t.segments.first()) {
        font_size = seg.font_size;
        font_weight = seg.font_weight;
        font_family = seg.font_name.as_ref().and_then(|f| f.family.clone());
        line_height = seg.line_height.as_ref().and_then(|l| l.value);
        letter_spacing = seg.letter_spacing.as_ref().and_then(|l| l.value);
        color = resolve_fill_color(seg.fills.first()).or_else(|| background.clone());
    }

    if raw.r#type.as_deref() == Some("text") {
        background = None;
        gradient = None;
    }

    let stroke = raw.strokes.first();
    NodeStyle {
        background,
        color,
        font_size,
        font_weight,
        border_radius: raw.border_radius,
        opacity: raw.blend.as_ref().and_then(|b| b.opacity),
        font_family,
        line_height,
        letter_spacing,
        stroke_width: stroke.and_then(|s| s.w),
        stroke_color: stroke.and_then(|s| resolve_fill_color(s.fills.first())),
        gradient,
    }
}

/* -------------------------------- tree output ------------------------------ */

#[derive(Debug, Clone, Default, Serialize)]
pub struct TreeNode {
    pub id: String,
    pub name: String,
    pub r#type: String,
    pub x: i64,
    pub y: i64,
    pub width: i64,
    pub height: i64,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub text: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub style: Option<NodeStyle>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub children: Option<Vec<TreeNode>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub duplicate_of: Option<String>,
}

#[derive(Debug, Clone)]
pub struct TreeOptions {
    pub with_style: bool,
    pub max_depth: usize,
    pub skip_empty_groups: bool,
    pub flatten: bool,
    pub only: Option<Vec<String>>,
    pub detect_duplicates: bool,
}

impl Default for TreeOptions {
    fn default() -> Self {
        Self {
            with_style: false,
            max_depth: 99,
            skip_empty_groups: false,
            flatten: false,
            only: None,
            detect_duplicates: false,
        }
    }
}

fn is_empty_container(raw: &GenomeNode) -> bool {
    raw.textbox.as_ref().and_then(|t| t.text.as_deref()).is_none()
        && raw.fills.is_empty()
        && raw.strokes.is_empty()
        && raw.slices.is_none()
        && raw.snapshot.is_none()
        && raw.snapshot_preview.is_none()
}

fn node_signature(raw: &GenomeNode, style: Option<&NodeStyle>) -> String {
    let mut parts = vec![
        raw.name.clone().unwrap_or_default(),
        raw.r#type.clone().unwrap_or_default(),
        raw.textbox.as_ref().and_then(|t| t.text.clone()).unwrap_or_default(),
    ];
    if let Some(style) = style {
        parts.push(format!(
            "{}|{}|{:?}|{:?}|{:?}|{:?}",
            style.background.as_deref().unwrap_or(""),
            style.color.as_deref().unwrap_or(""),
            style.font_size,
            style.border_radius,
            style.stroke_width,
            style.gradient.as_ref().map(|g| g.r#type.clone()).unwrap_or_default(),
        ));
    }
    parts.join("|")
}

pub fn extract_tree(genome: &Genome, frame_filter: Option<&str>, options: &TreeOptions) -> Vec<TreeNode> {
    let only: Option<Vec<String>> = options
        .only
        .as_ref()
        .filter(|list| !list.is_empty())
        .map(|list| list.iter().map(|t| t.to_lowercase()).collect());
    let mut signatures: HashMap<String, String> = HashMap::new();

    fn to_node(
        genome: &Genome,
        raw: &GenomeNode,
        depth: usize,
        offset_x: f64,
        offset_y: f64,
        options: &TreeOptions,
        only: &Option<Vec<String>>,
        signatures: &mut HashMap<String, String>,
    ) -> Vec<TreeNode> {
        let rect = raw.rect.clone().unwrap_or_default();
        let is_root = depth == 0;
        let base_x = if options.flatten { offset_x } else { 0.0 };
        let base_y = if options.flatten { offset_y } else { 0.0 };
        let x = if is_root && options.flatten { 0.0 } else { base_x + rect.x };
        let y = if is_root && options.flatten { 0.0 } else { base_y + rect.y };
        let mut node = TreeNode {
            id: raw.id.clone().unwrap_or_default(),
            name: raw.name.clone().unwrap_or_default(),
            r#type: raw.r#type.clone().unwrap_or_default(),
            x: x.round() as i64,
            y: y.round() as i64,
            width: rect.w.round() as i64,
            height: rect.h.round() as i64,
            text: raw.textbox.as_ref().and_then(|t| t.text.clone()),
            style: if options.with_style { Some(extract_raw_node_style(genome, raw)) } else { None },
            children: None,
            duplicate_of: None,
        };

        let has_children = !raw.children.is_empty();
        if has_children && depth < options.max_depth {
            let children: Vec<TreeNode> = raw
                .children
                .iter()
                .flat_map(|child| to_node(genome, child, depth + 1, if options.flatten { x } else { 0.0 }, if options.flatten { y } else { 0.0 }, options, only, signatures))
                .collect();
            if !children.is_empty() {
                node.children = Some(children);
            }
        }

        if let Some(only) = only {
            if !only.contains(&node.r#type.to_lowercase()) {
                return node.children.unwrap_or_default();
            }
        }
        if options.skip_empty_groups && is_empty_container(raw) && has_children && depth > 0 {
            if let Some(children) = node.children {
                return children;
            }
        }
        if options.detect_duplicates && options.with_style {
            let signature = node_signature(raw, node.style.as_ref());
            if let Some(first) = signatures.get(&signature) {
                if *first != node.id {
                    node.duplicate_of = Some(first.clone());
                }
            } else {
                signatures.insert(signature, node.id.clone());
            }
        }
        vec![node]
    }

    genome
        .pages
        .iter()
        .filter(|page| frame_filter.is_none() || ids_equal(page.id.as_deref(), frame_filter))
        .flat_map(|page| to_node(genome, page, 0, 0.0, 0.0, options, &only, &mut signatures))
        .collect()
}

/* -------------------------------- layers/style ----------------------------- */

#[derive(Debug, Clone, Default, Serialize)]
pub struct Layer {
    pub id: String,
    pub name: String,
    pub r#type: String,
    pub x: i64,
    pub y: i64,
    pub width: i64,
    pub height: i64,
}

pub fn extract_layers(genome: &Genome, frame_filter: Option<&str>, limit: usize) -> Vec<Layer> {
    let mut all: Vec<(&GenomeNode, usize)> = Vec::new();
    for page in &genome.pages {
        if frame_filter.is_some() && !ids_equal(page.id.as_deref(), frame_filter) {
            continue;
        }
        collect_nodes_flat(page, 0, &mut all);
    }
    all.iter()
        .take(limit)
        .map(|(node, _)| {
            let rect = node.rect.clone().unwrap_or_default();
            Layer {
                id: node.id.clone().unwrap_or_default(),
                name: node.name.clone().unwrap_or_default(),
                r#type: node.r#type.clone().unwrap_or_default(),
                x: rect.x.round() as i64,
                y: rect.y.round() as i64,
                width: rect.w.round() as i64,
                height: rect.h.round() as i64,
            }
        })
        .collect()
}

fn collect_nodes_flat<'a>(node: &'a GenomeNode, depth: usize, out: &mut Vec<(&'a GenomeNode, usize)>) {
    out.push((node, depth));
    for child in &node.children {
        collect_nodes_flat(child, depth + 1, out);
    }
}

pub fn find_node<'a>(node: &'a GenomeNode, target_id: &str) -> Option<&'a GenomeNode> {
    if ids_equal(node.id.as_deref(), Some(target_id)) {
        return Some(node);
    }
    for child in &node.children {
        if let Some(found) = find_node(child, target_id) {
            return Some(found);
        }
    }
    None
}

#[derive(Debug, Clone, Default, Serialize)]
pub struct StyleRow {
    pub id: String,
    pub name: String,
    pub bbox_x: i64,
    pub bbox_y: i64,
    pub bbox_w: i64,
    pub bbox_h: i64,
    #[serde(flatten)]
    pub style: NodeStyle,
}

pub fn extract_node_style(genome: &Genome, node_id: &str) -> Vec<StyleRow> {
    let raw = genome.pages.iter().find_map(|page| find_node(page, node_id));
    let raw = raw.cloned().unwrap_or_else(|| GenomeNode {
        id: Some(node_id.to_string()),
        name: Some("Unknown Node".to_string()),
        r#type: Some("unknown".to_string()),
        rect: Some(Rect { x: 0.0, y: 0.0, w: 0.0, h: 0.0 }),
        ..Default::default()
    });
    let rect = raw.rect.clone().unwrap_or_default();
    let style = extract_raw_node_style(genome, &raw);
    vec![StyleRow {
        id: raw.id.clone().unwrap_or_default(),
        name: raw.name.clone().unwrap_or_default(),
        bbox_x: rect.x.round() as i64,
        bbox_y: rect.y.round() as i64,
        bbox_w: rect.w.round() as i64,
        bbox_h: rect.h.round() as i64,
        style,
    }]
}

/* ---------------------------------- diff ----------------------------------- */

#[derive(Debug, Clone, Default, Serialize)]
pub struct TreeDiff {
    pub added: Vec<TreeNode>,
    pub removed: Vec<TreeNode>,
    pub changed: Vec<ChangedNode>,
}

#[derive(Debug, Clone, Default, Serialize)]
pub struct ChangedNode {
    pub id: String,
    pub name: String,
    pub fields: Vec<String>,
}

fn flatten_tree(nodes: &[TreeNode]) -> Vec<(&TreeNode, String)> {
    let mut out = Vec::new();
    let mut stack: Vec<&TreeNode> = nodes.iter().collect();
    while let Some(node) = stack.pop() {
        out.push((node, node.id.clone()));
        if let Some(children) = &node.children {
            stack.extend(children.iter());
        }
    }
    out
}

fn style_signature(style: &Option<NodeStyle>) -> String {
    match style {
        Some(s) => format!(
            "{:?}|{:?}|{:?}|{:?}|{:?}|{:?}|{:?}",
            s.background, s.color, s.font_size, s.border_radius, s.stroke_width, s.gradient.as_ref().map(|g| g.r#type.clone()), s.opacity
        ),
        None => String::new(),
    }
}

fn changed_fields(a: &TreeNode, b: &TreeNode) -> Vec<String> {
    let mut fields = Vec::new();
    if a.text != b.text {
        fields.push("text".to_string());
    }
    if style_signature(&a.style) != style_signature(&b.style) {
        fields.push("style".to_string());
    }
    if a.children.as_ref().map(|c| c.len()) != b.children.as_ref().map(|c| c.len()) {
        fields.push("childrenCount".to_string());
    }
    if a.x != b.x || a.y != b.y || a.width != b.width || a.height != b.height {
        fields.push("rect".to_string());
    }
    fields
}

pub fn diff_trees(a: &[TreeNode], b: &[TreeNode]) -> TreeDiff {
    let by_id_a: HashMap<String, &TreeNode> = flatten_tree(a).into_iter().map(|(n, id)| (id, n)).collect();
    let by_id_b: HashMap<String, &TreeNode> = flatten_tree(b).into_iter().map(|(n, id)| (id, n)).collect();
    let mut added = Vec::new();
    let mut removed = Vec::new();
    let mut changed = Vec::new();
    for (id, node_b) in &by_id_b {
        match by_id_a.get(id) {
            None => added.push((*node_b).clone()),
            Some(node_a) => {
                let fields = changed_fields(node_a, node_b);
                if !fields.is_empty() {
                    changed.push(ChangedNode { id: id.clone(), name: node_b.name.clone(), fields });
                }
            }
        }
    }
    for (id, node_a) in &by_id_a {
        if !by_id_b.contains_key(id) {
            removed.push((*node_a).clone());
        }
    }
    TreeDiff { added, removed, changed }
}

pub fn ids_equal(a: Option<&str>, b: Option<&str>) -> bool {
    let (Some(a), Some(b)) = (a, b) else { return false };
    let a = a.trim();
    let b = b.trim();
    if a.is_empty() || b.is_empty() {
        return false;
    }
    if a == b {
        return true;
    }
    let seg_a: Vec<&str> = a.split(';').map(str::trim).filter(|s| !s.is_empty()).collect();
    let seg_b: Vec<&str> = b.split(';').map(str::trim).filter(|s| !s.is_empty()).collect();
    if seg_a.len() > 1 || seg_b.len() > 1 {
        return seg_a.iter().any(|s| seg_b.contains(s));
    }
    false
}

#[derive(Debug, Clone, Default, Serialize)]
pub struct DesignMeta {
    pub title: String,
    pub frame_count: usize,
    pub frames: Vec<Frame>,
}

#[derive(Debug, Clone, Default, Serialize)]
pub struct Frame {
    pub id: String,
    pub name: String,
    pub width: i64,
    pub height: i64,
}

pub fn extract_design_meta(genome: &Genome, node_name: Option<&str>) -> DesignMeta {
    let frames: Vec<Frame> = genome
        .pages
        .iter()
        .map(|page| {
            let rect = page.rect.clone().unwrap_or_default();
            Frame {
                id: page.id.clone().unwrap_or_default(),
                name: page.name.clone().unwrap_or_else(|| "Untitled".to_string()),
                width: rect.w.round() as i64,
                height: rect.h.round() as i64,
            }
        })
        .collect();
    let frame_count = frames.len();
    let title = node_name
        .filter(|n| !n.is_empty())
        .map(str::to_string)
        .or_else(|| frames.first().map(|f| f.name.clone()))
        .unwrap_or_else(|| "Untitled".to_string());
    DesignMeta { title, frame_count, frames }
}

#[derive(Debug, Clone, Default, Serialize)]
pub struct Tokens {
    pub colors: Vec<String>,
    pub font_sizes: Vec<f64>,
    pub radii: Vec<i64>,
    pub spacing: Vec<i64>,
}

pub fn extract_tokens(genome: &Genome) -> Tokens {
    let mut colors: Vec<String> = Vec::new();
    let mut font_sizes: Vec<f64> = Vec::new();
    let mut radii: Vec<i64> = Vec::new();
    let mut spacing: Vec<i64> = Vec::new();

    fn push_unique<T: PartialEq + Clone>(vec: &mut Vec<T>, value: T) {
        if !vec.contains(&value) {
            vec.push(value);
        }
    }

    fn walk(node: &GenomeNode, genome: &Genome, colors: &mut Vec<String>, font_sizes: &mut Vec<f64>, radii: &mut Vec<i64>, spacing: &mut Vec<i64>) {
        for fill in &node.fills {
            if let Some(c) = resolve_fill_color(Some(fill)) {
                push_unique(colors, c);
            }
        }
        if let Some(r) = node.border_radius {
            if r.is_finite() {
                push_unique(radii, r.round() as i64);
            }
        }
        if let Some(rect) = &node.rect {
            if rect.x.is_finite() {
                push_unique(spacing, rect.x.round() as i64);
            }
            if rect.y.is_finite() {
                push_unique(spacing, rect.y.round() as i64);
            }
        }
        if let Some(segments) = node.textbox.as_ref().map(|t| &t.segments) {
            for seg in segments {
                if let Some(size) = seg.font_size {
                    push_unique(font_sizes, size);
                }
                for fill in &seg.fills {
                    if let Some(c) = resolve_fill_color(Some(fill)) {
                        push_unique(colors, c);
                    }
                }
            }
        }
        for child in &node.children {
            walk(child, genome, colors, font_sizes, radii, spacing);
        }
    }

    if let Some(styles) = &genome.styles {
        for style in &styles.fill_styles {
            for fill in &style.data {
                if let Some(c) = resolve_fill_color(Some(fill)) {
                    push_unique(&mut colors, c);
                }
            }
        }
    }
    for page in &genome.pages {
        walk(page, genome, &mut colors, &mut font_sizes, &mut radii, &mut spacing);
    }

    colors.sort();
    font_sizes.sort_by(|a, b| a.total_cmp(b));
    radii.sort_unstable();
    spacing.retain(|s| *s > 0);
    spacing.sort_unstable();
    Tokens { colors, font_sizes, radii, spacing }
}
