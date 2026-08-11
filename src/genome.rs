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
    value.clamp(0.0, 255.0).round() as u8
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

/* -------------------------------- misc helpers ----------------------------- */

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

    fn walk(node: &GenomeNode, colors: &mut Vec<String>, font_sizes: &mut Vec<f64>, radii: &mut Vec<i64>, spacing: &mut Vec<i64>) {
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
            walk(child, colors, font_sizes, radii, spacing);
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
        walk(page, &mut colors, &mut font_sizes, &mut radii, &mut spacing);
    }

    colors.sort();
    font_sizes.sort_by(|a, b| a.total_cmp(b));
    radii.sort_unstable();
    spacing.retain(|s| *s > 0);
    spacing.sort_unstable();
    Tokens { colors, font_sizes, radii, spacing }
}


#[cfg(test)]
mod tests {
    use super::*;

    fn sample_genome() -> Genome {
        Genome {
            pages: vec![
                GenomeNode {
                    id: Some("1:0".into()),
                    name: Some("Home".into()),
                    rect: Some(Rect { x: 0.0, y: 0.0, w: 1440.0, h: 900.0 }),
                    children: vec![GenomeNode {
                        id: Some("1:1".into()),
                        name: Some("Header".into()),
                        r#type: Some("frame".into()),
                        rect: Some(Rect { x: 0.0, y: 0.0, w: 1440.0, h: 80.0 }),
                        fills: vec![Fill { r#type: Some("color".into()), color: Some(Color { r: 255.0, g: 0.0, b: 0.0, alpha: None }), opacity: Some(1.0), ..Default::default() }],
                        children: vec![
                            GenomeNode {
                                id: Some("1:2".into()),
                                name: Some("Logo".into()),
                                r#type: Some("text".into()),
                                rect: Some(Rect { x: 20.0, y: 24.0, w: 100.0, h: 32.0 }),
                                textbox: Some(Textbox {
                                    text: Some("LOGO".into()),
                                    segments: vec![TextSegment {
                                        font_size: Some(16.0),
                                        font_weight: Some(700.0),
                                        font_name: Some(FontName { family: Some("Inter".into()) }),
                                        fills: vec![Fill { r#type: Some("color".into()), color: Some(Color { r: 0.0, g: 0.0, b: 0.0, alpha: None }), ..Default::default() }],
                                        ..Default::default()
                                    }],
                                }),
                                ..Default::default()
                            },
                            GenomeNode {
                                id: Some("1:3".into()),
                                name: Some("Btn".into()),
                                r#type: Some("rectangle".into()),
                                rect: Some(Rect { x: 1300.0, y: 16.0, w: 100.0, h: 48.0 }),
                                border_radius: Some(8.0),
                                fills: vec![Fill { r#type: Some("color".into()), color: Some(Color { r: 0.0, g: 0.0, b: 255.0, alpha: None }), opacity: Some(1.0), ..Default::default() }],
                                ..Default::default()
                            },
                        ],
                        ..Default::default()
                    }],
                    ..Default::default()
                },
                GenomeNode {
                    id: Some("2:0".into()),
                    name: Some("About".into()),
                    rect: Some(Rect { x: 0.0, y: 0.0, w: 1024.0, h: 768.0 }),
                    ..Default::default()
                },
            ],
            styles: Some(Styles { fill_styles: vec![] }),
            images: Default::default(),
        }
    }

    fn chain_genome() -> Genome {
        Genome {
            pages: vec![GenomeNode {
                id: Some("p:0".into()),
                name: Some("Page".into()),
                r#type: Some("page".into()),
                rect: Some(Rect { x: 100.0, y: 200.0, w: 1440.0, h: 900.0 }),
                children: vec![GenomeNode {
                    id: Some("g:1".into()),
                    name: Some("group1".into()),
                    r#type: Some("group".into()),
                    rect: Some(Rect { x: 10.0, y: 20.0, w: 800.0, h: 600.0 }),
                    children: vec![GenomeNode {
                        id: Some("g:2".into()),
                        name: Some("group2".into()),
                        r#type: Some("group".into()),
                        rect: Some(Rect { x: 30.0, y: 40.0, w: 500.0, h: 400.0 }),
                        children: vec![
                            GenomeNode {
                                id: Some("t:1".into()),
                                name: Some("Title".into()),
                                r#type: Some("text".into()),
                                rect: Some(Rect { x: 5.0, y: 5.0, w: 200.0, h: 40.0 }),
                                textbox: Some(Textbox { text: Some("Hello".into()), ..Default::default() }),
                                ..Default::default()
                            },
                            GenomeNode {
                                id: Some("i:1".into()),
                                name: Some("Icon".into()),
                                r#type: Some("frame".into()),
                                rect: Some(Rect { x: 60.0, y: 60.0, w: 50.0, h: 50.0 }),
                                fills: vec![Fill { r#type: Some("color".into()), color: Some(Color { r: 1.0, g: 2.0, b: 3.0, alpha: None }), opacity: Some(1.0), ..Default::default() }],
                                ..Default::default()
                            },
                        ],
                        ..Default::default()
                    }],
                    ..Default::default()
                }],
                ..Default::default()
            }],
            styles: None,
            images: Default::default(),
        }
    }

    #[test]
    fn ids_equal_exact_and_compound() {
        assert!(ids_equal(Some("4:1221"), Some("4:1221")));
        assert!(ids_equal(Some("I4:1222;4:1005;4:69"), Some("4:69")));
        assert!(ids_equal(Some("I4:1222;4:1005;4:69"), Some("4:1005")));
        assert!(!ids_equal(Some("1:1"), Some("1:1x")));
        assert!(!ids_equal(Some("1:1"), Some("1:11")));
        assert!(ids_equal(Some("1:1"), Some("1:1;2:3")));
        assert!(!ids_equal(None, Some("1:1")));
        assert!(!ids_equal(Some(""), Some("")));
    }

    #[test]
    fn style_stroke_and_gradient() {
        let genome = sample_genome();
        let stroke_node = GenomeNode {
            r#type: Some("rectangle".into()),
            rect: Some(Rect { x: 0.0, y: 0.0, w: 100.0, h: 100.0 }),
            fills: vec![Fill { r#type: Some("color".into()), color: Some(Color { r: 255.0, g: 0.0, b: 0.0, alpha: None }), opacity: Some(1.0), ..Default::default() }],
            strokes: vec![Stroke { fills: vec![Fill { r#type: Some("color".into()), color: Some(Color { r: 0.0, g: 0.0, b: 0.0, alpha: None }), opacity: Some(1.0), ..Default::default() }], w: Some(1.6) }],
            ..Default::default()
        };
        let style = extract_raw_node_style(&genome, &stroke_node);
        assert_eq!(style.stroke_width, Some(1.6));
        assert_eq!(style.stroke_color.as_deref(), Some("#000000"));
        assert!(style.gradient.is_none());

        let grad_node = GenomeNode {
            r#type: Some("rectangle".into()),
            rect: Some(Rect { x: 0.0, y: 0.0, w: 100.0, h: 100.0 }),
            fills: vec![Fill {
                r#type: Some("gradient".into()),
                gradient: Some(Gradient {
                    r#type: Some("linear".into()),
                    stops: vec![
                        GradientStop { color: Some(Color { r: 255.0, g: 162.0, b: 55.0, alpha: Some(1.0) }), position: Some(0.0) },
                        GradientStop { color: Some(Color { r: 0.0, g: 0.0, b: 0.0, alpha: Some(1.0) }), position: Some(1.0) },
                    ],
                    angle: Some(90.0),
                }),
                ..Default::default()
            }],
            ..Default::default()
        };
        let style = extract_raw_node_style(&genome, &grad_node);
        assert!(style.background.is_none());
        assert_eq!(style.gradient.as_ref().map(|g| g.r#type.as_str()), Some("linear"));
        assert_eq!(style.gradient.as_ref().map(|g| g.stops.len()), Some(2));
        assert_eq!(style.gradient.as_ref().map(|g| g.stops[0].color.as_str()), Some("rgba(255,162,55,1)"));
        assert_eq!(style.gradient.as_ref().and_then(|g| g.angle), Some(90.0));
    }

    #[test]
    fn tree_skip_empty_groups() {
        let genome = chain_genome();
        let full = extract_tree(&genome, None, &TreeOptions::default());
        assert_eq!(full[0].children.as_ref().unwrap()[0].name, "group1");

        let options = TreeOptions { skip_empty_groups: true, ..Default::default() };
        let filtered = extract_tree(&genome, None, &options);
        let children = filtered[0].children.as_ref().unwrap();
        assert_eq!(children[0].name, "Title");
        assert_eq!(children[1].name, "Icon");
    }

    #[test]
    fn tree_flatten_coordinates() {
        let genome = chain_genome();
        let options = TreeOptions { flatten: true, ..Default::default() };
        let flat = extract_tree(&genome, None, &options);
        let page = &flat[0];
        assert_eq!(page.x, 0, "artboard origin zeroed");
        assert_eq!(page.y, 0);
        let icon = &page.children.as_ref().unwrap()[0].children.as_ref().unwrap()[0].children.as_ref().unwrap()[1];
        assert_eq!(icon.x, 10 + 30 + 60, "nested offsets accumulated");
        assert_eq!(icon.y, 20 + 40 + 60);
    }

    #[test]
    fn tree_only_filter() {
        let genome = chain_genome();
        let options = TreeOptions { only: Some(vec!["text".into()]), ..Default::default() };
        let texts = extract_tree(&genome, None, &options);
        assert_eq!(texts.len(), 1);
        assert_eq!(texts[0].id, "t:1");
    }

    #[test]
    fn tree_detect_duplicates() {
        let genome = Genome {
            pages: vec![GenomeNode {
                id: Some("p:0".into()),
                name: Some("Page".into()),
                r#type: Some("page".into()),
                rect: Some(Rect { x: 0.0, y: 0.0, w: 100.0, h: 100.0 }),
                children: vec![
                    GenomeNode {
                        id: Some("a:1".into()),
                        name: Some("Btn".into()),
                        r#type: Some("frame".into()),
                        rect: Some(Rect { x: 0.0, y: 0.0, w: 50.0, h: 50.0 }),
                        fills: vec![Fill { r#type: Some("color".into()), color: Some(Color { r: 1.0, g: 2.0, b: 3.0, alpha: None }), opacity: Some(1.0), ..Default::default() }],
                        ..Default::default()
                    },
                    GenomeNode {
                        id: Some("a:2".into()),
                        name: Some("Btn".into()),
                        r#type: Some("frame".into()),
                        rect: Some(Rect { x: 60.0, y: 0.0, w: 50.0, h: 50.0 }),
                        fills: vec![Fill { r#type: Some("color".into()), color: Some(Color { r: 1.0, g: 2.0, b: 3.0, alpha: None }), opacity: Some(1.0), ..Default::default() }],
                        ..Default::default()
                    },
                ],
                ..Default::default()
            }],
            styles: None,
            images: Default::default(),
        };
        let options = TreeOptions { with_style: true, detect_duplicates: true, ..Default::default() };
        let tree = extract_tree(&genome, None, &options);
        let second = &tree[0].children.as_ref().unwrap()[1];
        assert_eq!(second.duplicate_of.as_deref(), Some("a:1"));
    }

    #[test]
    fn tokens_colors_fonts_radii() {
        let tokens = extract_tokens(&sample_genome());
        assert!(tokens.colors.contains(&"#ff0000".to_string()));
        assert!(tokens.colors.contains(&"#000000".to_string()));
        assert_eq!(tokens.font_sizes, vec![16.0]);
        assert_eq!(tokens.radii, vec![8]);
        assert!(tokens.spacing.iter().all(|s| *s > 0));
    }

    #[test]
    fn layers_limit_and_frame_filter() {
        let genome = sample_genome();
        assert_eq!(extract_layers(&genome, None, 50).len(), 5);
        assert_eq!(extract_layers(&genome, Some("1:0"), 50).len(), 4);
        assert_eq!(extract_layers(&genome, Some("1:0"), 2).len(), 2);
        assert_eq!(extract_layers(&genome, Some("missing"), 50).len(), 0);
    }

    #[test]
    fn design_meta_title_and_frames() {
        let meta = extract_design_meta(&sample_genome(), None);
        assert_eq!(meta.title, "Home");
        assert_eq!(meta.frame_count, 2);
        assert_eq!(meta.frames[0].name, "Home");
        assert_eq!(meta.frames[0].width, 1440);
    }

    #[test]
    fn diff_added_removed_changed() {
        let a = vec![
            TreeNode { id: "1".into(), name: "A".into(), r#type: "frame".into(), x: 0, y: 0, width: 10, height: 10, text: Some("same".into()), ..Default::default() },
            TreeNode { id: "3".into(), name: "C".into(), r#type: "frame".into(), x: 0, y: 0, width: 10, height: 10, ..Default::default() },
        ];
        let b = vec![
            TreeNode { id: "1".into(), name: "A".into(), r#type: "frame".into(), x: 0, y: 0, width: 10, height: 10, text: Some("changed".into()), ..Default::default() },
            TreeNode { id: "4".into(), name: "D".into(), r#type: "frame".into(), x: 0, y: 0, width: 10, height: 10, ..Default::default() },
        ];
        let diff = diff_trees(&a, &b);
        assert_eq!(diff.removed.len(), 1);
        assert_eq!(diff.removed[0].id, "3");
        assert_eq!(diff.added.len(), 1);
        assert_eq!(diff.added[0].id, "4");
        assert_eq!(diff.changed.len(), 1);
        assert_eq!(diff.changed[0].id, "1");
        assert_eq!(diff.changed[0].fields, vec!["text"]);
    }
}
