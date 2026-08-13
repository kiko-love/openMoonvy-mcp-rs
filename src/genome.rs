/*
 * Moonvy genome data structures and parsing.
 *
 * Node shapes, style normalization and tree options
 * (skipEmptyGroups / flatten / only / detectDuplicates).
 */

use rmcp::schemars;
use serde::{Deserialize, Serialize};
use std::collections::{HashMap, HashSet};

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(default)]
pub struct Rect {
    pub x: f64,
    pub y: f64,
    pub w: f64,
    pub h: f64,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(default)]
pub struct Color {
    pub r: f64,
    pub g: f64,
    pub b: f64,
    pub alpha: Option<f64>,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(default)]
pub struct GradientStop {
    pub color: Option<Color>,
    pub position: Option<f64>,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(default)]
pub struct Gradient {
    pub r#type: Option<String>,
    pub stops: Vec<GradientStop>,
    pub angle: Option<f64>,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
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

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(default)]
pub struct Stroke {
    pub fills: Vec<Fill>,
    pub w: Option<f64>,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(default, rename_all = "camelCase")]
pub struct FontName {
    pub family: Option<String>,
    /// e.g. "Semibold", "Bold" — mapped to a numeric weight when the segment
    /// does not carry an explicit font_weight.
    pub style: Option<String>,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(default, rename_all = "camelCase")]
pub struct LineHeight {
    pub unit: Option<String>,
    pub value: Option<f64>,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(default, rename_all = "camelCase")]
pub struct LetterSpacing {
    pub unit: Option<String>,
    pub value: Option<f64>,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(default, rename_all = "camelCase")]
pub struct TextSegment {
    pub font_size: Option<f64>,
    pub font_weight: Option<f64>,
    pub font_name: Option<FontName>,
    pub line_height: Option<LineHeight>,
    pub letter_spacing: Option<LetterSpacing>,
    pub fills: Vec<Fill>,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(default, rename_all = "camelCase")]
pub struct Textbox {
    pub text: Option<String>,
    pub segments: Vec<TextSegment>,
    /// Horizontal alignment of the text block ("left"|"center"|"right"|"justify"),
    /// when exported by the source tool.
    pub align: Option<String>,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(default, rename_all = "camelCase")]
pub struct Blend {
    pub opacity: Option<f64>,
    /// Mask markers (present on the clipping shape child of a mask group):
    /// `isMask: true` / `isPureMask: true` in real genome data.
    pub is_mask: Option<bool>,
    pub is_pure_mask: Option<bool>,
}

/// Accept `borderRadius` as a plain number, an array of per-corner radii
/// (Moonvy exports all four corners), or null — keeps a single f64.
fn deserialize_radius<'de, D>(deserializer: D) -> Result<Option<f64>, D::Error>
where
    D: serde::Deserializer<'de>,
{
    let value = Option::<serde_json::Value>::deserialize(deserializer)?;
    Ok(match value {
        None => None,
        Some(serde_json::Value::Number(n)) => n.as_f64(),
        Some(serde_json::Value::Array(radii)) => radii.first().and_then(serde_json::Value::as_f64),
        _ => None,
    })
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(default, rename_all = "camelCase")]
pub struct GenomeNode {
    pub id: Option<String>,
    pub name: Option<String>,
    pub r#type: Option<String>,
    pub rect: Option<Rect>,
    pub fills: Vec<Fill>,
    pub strokes: Vec<Stroke>,
    pub textbox: Option<Textbox>,
    pub blend: Option<Blend>,
    #[serde(default, deserialize_with = "deserialize_radius")]
    pub border_radius: Option<f64>,
    pub fill_link: Option<String>,
    /// Some nodes carry `slices: true` (boolean) instead of a map.
    pub slices: Option<serde_json::Value>,
    pub snapshot: Option<String>,
    pub snapshot_preview: Option<String>,
    pub children: Vec<GenomeNode>,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(default, rename_all = "camelCase")]
pub struct Styles {
    pub fill_styles: Vec<StyleDef>,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(default, rename_all = "camelCase")]
pub struct StyleDef {
    pub id: Option<String>,
    pub data: Vec<Fill>,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(default)]
pub struct ImageInfo {
    pub url: Option<String>,
    pub r#type: Option<String>,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(default)]
pub struct Genome {
    pub pages: Vec<GenomeNode>,
    pub styles: Option<Styles>,
    pub images: HashMap<String, ImageInfo>,
}

/* ----------------------------- style resolution ---------------------------- */

fn is_zero(v: &Option<f64>) -> bool {
    v.is_none_or(|f| f == 0.0)
}
fn is_one(v: &Option<f64>) -> bool {
    v.is_none_or(|f| f == 1.0)
}

#[derive(Debug, Clone, Default, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct NodeStyle {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub background: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub color: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub font_size: Option<f64>,
    #[serde(skip_serializing_if = "is_default_weight")]
    pub font_weight: Option<f64>,
    /// 0 is the default radius (no rounding); omitted when unset or zero.
    #[serde(skip_serializing_if = "is_zero")]
    pub border_radius: Option<f64>,
    /// 1.0 is the default opacity; omitted when unset or one.
    #[serde(skip_serializing_if = "is_one")]
    pub opacity: Option<f64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub font_family: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub line_height: Option<f64>,
    /// 0 is the default letter spacing; omitted when unset or zero.
    #[serde(skip_serializing_if = "is_zero")]
    pub letter_spacing: Option<f64>,
    /// 1px is the default stroke; omitted when unset or one.
    #[serde(skip_serializing_if = "is_one")]
    pub stroke_width: Option<f64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub stroke_color: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub gradient: Option<GradientStyle>,
    /// Horizontal text alignment ("left"|"center"|"right"|"justify"); only
    /// set on text nodes when the source exports it.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub text_align: Option<String>,
}

/// 400 is the default weight; omitted when unset or regular.
fn is_default_weight(v: &Option<f64>) -> bool {
    v.is_none_or(|f| f == 400.0)
}

#[derive(Debug, Clone, Default, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct GradientStyle {
    pub r#type: String,
    pub stops: Vec<GradientStopStyle>,
    /// 0 is the default angle; omitted when zero.
    #[serde(skip_serializing_if = "is_zero_angle")]
    pub angle: Option<f64>,
}

fn is_zero_angle(v: &Option<f64>) -> bool {
    v.is_none_or(|f| f == 0.0)
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
                |c| {
                    format!(
                        "rgba({},{},{},{})",
                        clamp_u8(c.r),
                        clamp_u8(c.g),
                        clamp_u8(c.b),
                        c.alpha.unwrap_or(1.0)
                    )
                },
            );
            GradientStopStyle {
                color,
                position: stop.position.unwrap_or(0.0),
            }
        })
        .collect();
    Some(GradientStyle {
        r#type: gradient
            .r#type
            .clone()
            .unwrap_or_else(|| "linear".to_string()),
        stops,
        angle: gradient.angle,
    })
}

fn linked_fill<'a>(genome: &'a Genome, fill_link: Option<&'a str>) -> Option<&'a Fill> {
    let link = fill_link?;
    genome
        .styles
        .as_ref()?
        .fill_styles
        .iter()
        .find(|s| s.id.as_deref() == Some(link))?
        .data
        .first()
}

/// Map a font style label ("Semibold", "Bold", ...) to a numeric weight.
fn style_to_weight(style: Option<&str>) -> Option<f64> {
    let style = style?.to_lowercase();
    if style.contains("thin") {
        Some(100.0)
    } else if style.contains("light") {
        Some(300.0)
    } else if style.contains("medium") {
        Some(500.0)
    } else if style.contains("semibold") {
        Some(600.0)
    } else if style.contains("bold") {
        Some(700.0)
    } else if style.contains("black") || style.contains("heavy") {
        Some(900.0)
    } else if style.contains("regular") || style.contains("normal") {
        Some(400.0)
    } else {
        None
    }
}

/// Normalized style for one node. `None` fields mean "not set"; transparency
/// is encoded in rgba(...,0) rather than null.
pub fn extract_raw_node_style(genome: &Genome, raw: &GenomeNode) -> NodeStyle {
    let first_fill = raw
        .fills
        .first()
        .or_else(|| linked_fill(genome, raw.fill_link.as_deref()));
    let mut background = resolve_fill_color(first_fill);
    let mut gradient = if background.is_none() {
        resolve_gradient(first_fill)
    } else {
        None
    };

    let mut color = None;
    let mut font_size = None;
    let mut font_weight = None;
    let mut font_family = None;
    let mut line_height = None;
    let mut letter_spacing = None;
    if let Some(seg) = raw.textbox.as_ref().and_then(|t| t.segments.first()) {
        font_size = seg.font_size;
        font_weight = seg
            .font_weight
            .or_else(|| style_to_weight(seg.font_name.as_ref().and_then(|f| f.style.as_deref())));
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
        text_align: raw.textbox.as_ref().and_then(|t| t.align.clone()),
    }
}

/* -------------------------------- tree output ------------------------------ */

#[derive(Debug, Clone, Default, Serialize)]
#[serde(rename_all = "camelCase")]
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
    /// True when this container holds a clipping shape child (blend.isMask /
    /// isPureMask): it is a mask group whose children are clipped.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub is_mask_group: Option<bool>,
    /// Rendered preview hash of this node (its own snapshot / snapshotPreview).
    /// Resolves against the `assets` manifest (includeAssets) or
    /// https://fs.moonvy.com/{hash}.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub snapshot_hash: Option<String>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, schemars::JsonSchema)]
pub enum TextContent {
    /// Truncate text to 40 chars in the tree; full text via find_node.
    #[default]
    Truncate,
    Full,
    None,
}

impl<'de> serde::Deserialize<'de> for TextContent {
    fn deserialize<D: serde::Deserializer<'de>>(d: D) -> Result<Self, D::Error> {
        let s = String::deserialize(d)?;
        match s.to_lowercase().as_str() {
            "truncate" => Ok(Self::Truncate),
            "full" => Ok(Self::Full),
            "none" => Ok(Self::None),
            other => Err(serde::de::Error::custom(format!(
                "textContent must be truncate|full|none, got {other}"
            ))),
        }
    }
}

const TEXT_TRUNCATE_CHARS: usize = 40;

fn truncate_text(text: &str) -> String {
    let mut chars = text.chars();
    let truncated: String = chars.by_ref().take(TEXT_TRUNCATE_CHARS).collect();
    if chars.next().is_some() {
        format!("{truncated}…")
    } else {
        truncated
    }
}

#[derive(Debug, Clone)]
pub struct TreeOptions {
    pub with_style: bool,
    pub max_depth: usize,
    pub skip_empty_groups: bool,
    pub flatten: bool,
    pub only: Option<Vec<String>>,
    pub detect_duplicates: bool,
    /// When true (default), duplicate instances are emitted as lightweight
    /// stubs ({id,name,type,x,y,width,height,duplicateOf}) — position kept,
    /// content/style deduplicated against the canonical instance.
    pub deduplicate: bool,
    pub text_content: TextContent,
    /// Export only the subtree rooted at this node id.
    pub node_id: Option<String>,
    /// Keep only nodes intersecting this rect (absolute coordinates,
    /// requires `flatten`). Rect: [x, y, w, h].
    pub region: Option<[f64; 4]>,
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
            deduplicate: true,
            text_content: TextContent::Truncate,
            node_id: None,
            region: None,
        }
    }
}

/// A container is "empty" (pure nesting noise) only if it carries no visual
/// content AND no mask semantics: mask groups hold a clipping shape child
/// (`blend.isMask` / `isPureMask`) that must survive skipEmptyGroups, and the
/// clipping shape itself must not be dropped as an empty leaf.
fn is_empty_container(raw: &GenomeNode) -> bool {
    let no_content = raw
        .textbox
        .as_ref()
        .and_then(|t| t.text.as_deref())
        .is_none()
        && raw.fills.is_empty()
        && raw.strokes.is_empty()
        && raw.slices.is_none()
        && raw.snapshot.is_none()
        && raw.snapshot_preview.is_none();
    let is_mask_shape = raw.blend.as_ref().is_some_and(has_mask_marker);
    no_content
        && !is_mask_shape
        && !raw
            .children
            .iter()
            .any(|c| c.blend.as_ref().is_some_and(has_mask_marker))
}

/// Mask markers found on the clipping shape child of a mask group: real
/// genome data may carry `isMask` and/or `isPureMask`.
fn has_mask_marker(blend: &Blend) -> bool {
    blend.is_mask == Some(true) || blend.is_pure_mask == Some(true)
}

fn node_signature(raw: &GenomeNode, style: Option<&NodeStyle>) -> String {
    let rect = raw.rect.clone().unwrap_or_default();
    let mut parts = vec![
        raw.name.clone().unwrap_or_default(),
        raw.r#type.clone().unwrap_or_default(),
        raw.textbox
            .as_ref()
            .and_then(|t| t.text.clone())
            .unwrap_or_default(),
        format!("{}/{}", rect.w.round() as i64, rect.h.round() as i64),
    ];
    if let Some(style) = style {
        parts.push(format!(
            "{}|{}|{:?}|{:?}|{:?}|{:?}",
            style.background.as_deref().unwrap_or(""),
            style.color.as_deref().unwrap_or(""),
            style.font_size,
            style.border_radius,
            style.stroke_width,
            style
                .gradient
                .as_ref()
                .map(|g| g.r#type.clone())
                .unwrap_or_default(),
        ));
    }
    parts.join("|")
}

pub fn extract_tree(
    genome: &Genome,
    frame_filter: Option<&str>,
    options: &TreeOptions,
) -> Vec<TreeNode> {
    let only: Option<Vec<String>> = options
        .only
        .as_ref()
        .filter(|list| !list.is_empty())
        .map(|list| list.iter().map(|t| t.to_lowercase()).collect());
    // `only: ["image"]` is a semantic filter: any node that renders an image
    // (image fill, snapshot or snapshotPreview), regardless of its type tag.
    let only_image = only
        .as_ref()
        .is_some_and(|list| list.contains(&"image".to_string()));

    /// Shared state threaded through the tree build.
    struct Ctx<'a> {
        genome: &'a Genome,
        options: &'a TreeOptions,
        only: &'a Option<Vec<String>>,
        only_image: bool,
        signatures: &'a mut HashMap<String, String>,
    }

    fn to_node(
        ctx: &mut Ctx<'_>,
        raw: &GenomeNode,
        depth: usize,
        offset_x: f64,
        offset_y: f64,
    ) -> Vec<TreeNode> {
        let rect = raw.rect.clone().unwrap_or_default();
        let is_root = depth == 0;
        let base_x = if ctx.options.flatten { offset_x } else { 0.0 };
        let base_y = if ctx.options.flatten { offset_y } else { 0.0 };
        let x = if is_root && ctx.options.flatten {
            0.0
        } else {
            base_x + rect.x
        };
        let y = if is_root && ctx.options.flatten {
            0.0
        } else {
            base_y + rect.y
        };
        let mut node = TreeNode {
            id: raw.id.clone().unwrap_or_default(),
            name: raw.name.clone().unwrap_or_default(),
            r#type: raw.r#type.clone().unwrap_or_default(),
            x: x.round() as i64,
            y: y.round() as i64,
            width: rect.w.round() as i64,
            height: rect.h.round() as i64,
            text: raw.textbox.as_ref().and_then(|t| t.text.clone()).map(|t| {
                match ctx.options.text_content {
                    TextContent::Full => t,
                    TextContent::Truncate => truncate_text(&t),
                    TextContent::None => String::new(),
                }
            }),
            style: if ctx.options.with_style {
                Some(extract_raw_node_style(ctx.genome, raw))
            } else {
                None
            },
            children: None,
            duplicate_of: None,
            is_mask_group: (!raw.children.is_empty()
                && raw
                    .children
                    .iter()
                    .any(|c| c.blend.as_ref().is_some_and(has_mask_marker)))
            .then_some(true),
            snapshot_hash: raw
                .snapshot
                .clone()
                .or_else(|| raw.snapshot_preview.clone()),
        };
        if ctx.options.text_content == TextContent::None {
            node.text = None;
        }

        // region filter (absolute coords): drop nodes outside the rect.
        if let Some([rx, ry, rw, rh]) = ctx.options.region
            && (x >= rx + rw || y >= ry + rh || x + rect.w <= rx || y + rect.h <= ry)
        {
            return Vec::new();
        }

        let has_children = !raw.children.is_empty();
        if has_children && depth < ctx.options.max_depth {
            let children: Vec<TreeNode> = raw
                .children
                .iter()
                .flat_map(|child| {
                    to_node(
                        ctx,
                        child,
                        depth + 1,
                        if ctx.options.flatten { x } else { 0.0 },
                        if ctx.options.flatten { y } else { 0.0 },
                    )
                })
                .collect();
            if !children.is_empty() {
                node.children = Some(children);
            }
        }

        if let Some(only) = ctx.only {
            let type_matches = only.contains(&node.r#type.to_lowercase());
            let image_matches = ctx.only_image
                && (raw.snapshot.is_some()
                    || raw.snapshot_preview.is_some()
                    || raw
                        .fills
                        .iter()
                        .any(|f| f.r#type.as_deref() == Some("image")));
            if !type_matches && !image_matches {
                return node.children.unwrap_or_default();
            }
        }
        if ctx.options.skip_empty_groups && is_empty_container(raw) && depth > 0 {
            // Empty container with children: lift them one level; empty
            // leaf container: drop it entirely (pure nesting noise).
            return node.children.unwrap_or_default();
        }
        if ctx.options.detect_duplicates && ctx.options.with_style {
            let signature = node_signature(raw, node.style.as_ref());
            if let Some(first) = ctx.signatures.get(&signature) {
                if *first != node.id {
                    node.duplicate_of = Some(first.clone());
                    if ctx.options.deduplicate {
                        // Keep position, drop the duplicated payload: the
                        // canonical instance carries content/style/text.
                        return vec![TreeNode {
                            id: node.id.clone(),
                            name: node.name.clone(),
                            r#type: node.r#type.clone(),
                            x: node.x,
                            y: node.y,
                            width: node.width,
                            height: node.height,
                            text: None,
                            style: None,
                            children: None,
                            duplicate_of: node.duplicate_of.clone(),
                            is_mask_group: None,
                            snapshot_hash: None,
                        }];
                    }
                }
            } else {
                ctx.signatures.insert(signature, node.id.clone());
            }
        }
        vec![node]
    }

    let mut ctx = Ctx {
        genome,
        options,
        only: &only,
        only_image,
        signatures: &mut HashMap::new(),
    };
    // nodeId: export only the subtree rooted at that node (absolute coords
    // rebase to 0 under flatten, mirroring the page-root behavior).
    if let Some(node_id) = options.node_id.as_deref() {
        return genome
            .pages
            .iter()
            .find_map(|page| find_node(page, node_id))
            .map(|raw| to_node(&mut ctx, raw, 0, 0.0, 0.0))
            .unwrap_or_default();
    }
    genome
        .pages
        .iter()
        .filter(|page| frame_filter.is_none() || ids_equal(page.id.as_deref(), frame_filter))
        .flat_map(|page| to_node(&mut ctx, page, 0, 0.0, 0.0))
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
    let mut all: Vec<&GenomeNode> = Vec::new();
    for page in &genome.pages {
        if frame_filter.is_some() && !ids_equal(page.id.as_deref(), frame_filter) {
            continue;
        }
        collect_nodes_flat(page, &mut all);
    }
    all.iter()
        .take(limit)
        .map(|node| {
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

fn collect_nodes_flat<'a>(node: &'a GenomeNode, out: &mut Vec<&'a GenomeNode>) {
    out.push(node);
    for child in &node.children {
        collect_nodes_flat(child, out);
    }
}

#[derive(Debug, Clone, Default, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct SearchHit {
    pub id: String,
    pub name: String,
    pub r#type: String,
    pub x: i64,
    pub y: i64,
    pub width: i64,
    pub height: i64,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub text: Option<String>,
    /// Nearest ancestor that is a component-like container (group/artboard/
    /// frame/component). Lets the caller drill into the whole component
    /// subtree with get_tree(nodeId) instead of the whole page.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub container_id: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub container_name: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub container_type: Option<String>,
}

fn is_component_container(node: &GenomeNode) -> bool {
    matches!(
        node.r#type.as_deref(),
        Some("group" | "artboard" | "frame" | "component" | "section")
    )
}

/// Case-insensitive substring search over node names and text contents.
/// Returns depth-first matches (parents before children), truncated by limit.
/// Each hit carries its nearest component-like ancestor so callers can jump
/// straight to the containing component subtree.
pub fn find_nodes(
    genome: &Genome,
    frame_filter: Option<&str>,
    query: &str,
    limit: usize,
) -> Vec<SearchHit> {
    let query = query.trim().to_lowercase();
    if query.is_empty() || limit == 0 {
        return Vec::new();
    }
    let mut hits = Vec::new();
    for page in &genome.pages {
        if frame_filter.is_some() && !ids_equal(page.id.as_deref(), frame_filter) {
            continue;
        }
        // Stack entries: (node, nearest component-like ancestor seen so far).
        let mut stack: Vec<(&GenomeNode, Option<&GenomeNode>)> = vec![(page, None)];
        while let Some((node, container)) = stack.pop() {
            let text = node.textbox.as_ref().and_then(|t| t.text.clone());
            let name = node.name.clone().unwrap_or_default();
            let matches = name.to_lowercase().contains(&query)
                || text
                    .as_deref()
                    .is_some_and(|t| t.to_lowercase().contains(&query));
            if matches {
                let rect = node.rect.clone().unwrap_or_default();
                hits.push(SearchHit {
                    id: node.id.clone().unwrap_or_default(),
                    name,
                    r#type: node.r#type.clone().unwrap_or_default(),
                    x: rect.x.round() as i64,
                    y: rect.y.round() as i64,
                    width: rect.w.round() as i64,
                    height: rect.h.round() as i64,
                    text,
                    container_id: container.and_then(|c| c.id.clone()),
                    container_name: container.and_then(|c| c.name.clone()),
                    container_type: container
                        .and_then(|c| c.r#type.clone())
                        .or_else(|| container.map(|_| "container".to_string())),
                });
                if hits.len() >= limit {
                    return hits;
                }
            }
            let next_container = if is_component_container(node) {
                Some(node)
            } else {
                container
            };
            // Depth-first with LIFO stack: push children in reverse so the
            // first child is visited first (parents already matched above).
            for child in node.children.iter().rev() {
                stack.push((child, next_container));
            }
        }
    }
    hits
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

/// Absolute rect (x, y, w, h) of a node relative to its page artboard origin.
/// Walks the ancestor chain and accumulates rect offsets, mirroring the
/// `flatten` behavior of extract_tree (the page root's own offset is zeroed).
/// Used by snapshot cropping so the crop region matches the flattened tree.
pub fn absolute_rect(genome: &Genome, node_id: &str) -> Option<Rect> {
    fn walk<'a>(node: &'a GenomeNode, target_id: &str, acc_x: f64, acc_y: f64) -> Option<Rect> {
        let rect = node.rect.clone().unwrap_or_default();
        let x = acc_x + rect.x;
        let y = acc_y + rect.y;
        if ids_equal(node.id.as_deref(), Some(target_id)) {
            return Some(Rect {
                x,
                y,
                w: rect.w,
                h: rect.h,
            });
        }
        for child in &node.children {
            if let Some(found) = walk(child, target_id, x, y) {
                return Some(found);
            }
        }
        None
    }
    for page in &genome.pages {
        // Flattened coordinates zero the page root offset: start the walk
        // from the root's own rect so children accumulate relative to it.
        if let Some(found) = walk(page, node_id, 0.0, 0.0) {
            let root_rect = page.rect.clone().unwrap_or_default();
            return Some(Rect {
                x: found.x - root_rect.x,
                y: found.y - root_rect.y,
                w: found.w,
                h: found.h,
            });
        }
    }
    None
}

/// Total node count across a tree (recursive).
pub fn count_nodes(nodes: &[TreeNode]) -> usize {
    nodes
        .iter()
        .map(|n| 1 + n.children.as_ref().map(|c| count_nodes(c)).unwrap_or(0))
        .sum()
}

#[derive(Debug, Clone, Default, Serialize)]
#[serde(rename_all = "camelCase")]
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
    let raw = genome
        .pages
        .iter()
        .find_map(|page| find_node(page, node_id));
    let raw = raw.cloned().unwrap_or_else(|| GenomeNode {
        id: Some(node_id.to_string()),
        name: Some("Unknown Node".to_string()),
        r#type: Some("unknown".to_string()),
        rect: Some(Rect {
            x: 0.0,
            y: 0.0,
            w: 0.0,
            h: 0.0,
        }),
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
#[serde(rename_all = "camelCase")]
pub struct ChangedNode {
    pub id: String,
    pub name: String,
    pub fields: Vec<String>,
    /// Snapshot of the node in the first design (only when paired).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub before: Option<TreeNode>,
    /// Snapshot of the node in the second design (only when paired).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub after: Option<TreeNode>,
}

fn flatten_tree(nodes: &[TreeNode]) -> Vec<&TreeNode> {
    let mut out = Vec::new();
    let mut stack: Vec<&TreeNode> = nodes.iter().collect();
    while let Some(node) = stack.pop() {
        out.push(node);
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
            s.background,
            s.color,
            s.font_size,
            s.border_radius,
            s.stroke_width,
            s.gradient.as_ref().map(|g| g.r#type.clone()),
            s.opacity
        ),
        None => String::new(),
    }
}

fn changed_fields(a: &TreeNode, b: &TreeNode) -> Option<Vec<String>> {
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
    (!fields.is_empty()).then_some(fields)
}

/// Diff nodes carry only their own payload: subtree structure is implied by
/// changed/childrenCount and would multiply the output many times over.
fn strip_children(mut node: TreeNode) -> TreeNode {
    node.children = None;
    node
}

/// Diff two trees by node id, falling back to same-name pairing for nodes
/// whose ids differ across designs (e.g. two separate pages for normal vs
/// hover states). Returns (paired_changes, added, removed).
pub fn diff_trees(a: &[TreeNode], b: &[TreeNode]) -> TreeDiff {
    let by_id_a: HashMap<String, &TreeNode> = flatten_tree(a)
        .into_iter()
        .map(|n| (n.id.clone(), n))
        .collect();
    let by_id_b: HashMap<String, &TreeNode> = flatten_tree(b)
        .into_iter()
        .map(|n| (n.id.clone(), n))
        .collect();
    let mut matched_a: HashSet<String> = HashSet::new();
    let mut changed = Vec::new();
    let mut added = Vec::new();

    // Roots are the tree's top-level nodes (pages/artboards). When both sides
    // are single-page designs with the same viewport size, the page container
    // is just a different uuid/name wrapper — reporting it as added/removed is
    // noise; its real changes surface through the children.
    let root_ids_a: HashSet<&str> = a.iter().map(|n| n.id.as_str()).collect();
    let root_ids_b: HashSet<&str> = b.iter().map(|n| n.id.as_str()).collect();
    let single_page_match =
        a.len() == 1 && b.len() == 1 && a[0].width == b[0].width && a[0].height == b[0].height;

    // Name -> ids of unmatched A nodes (built once, used only for fallback).
    let mut by_name: HashMap<&str, Vec<&String>> = HashMap::new();
    for (id, node) in &by_id_a {
        if !node.name.is_empty() {
            by_name.entry(node.name.as_str()).or_default().push(id);
        }
    }

    let record_change = |matched: &mut HashSet<String>,
                         changed: &mut Vec<ChangedNode>,
                         node_a: &TreeNode,
                         node_b: &TreeNode,
                         a_id: String| {
        matched.insert(a_id);
        if let Some(fields) = changed_fields(node_a, node_b) {
            changed.push(ChangedNode {
                id: node_b.id.clone(),
                name: node_b.name.clone(),
                fields,
                before: Some(node_a.clone()),
                after: Some(node_b.clone()),
            });
        }
    };

    // Pass 1: exact id match.
    for (id, node_b) in &by_id_b {
        if let Some(node_a) = by_id_a.get(id) {
            record_change(&mut matched_a, &mut changed, node_a, node_b, id.clone());
        }
    }
    // Pass 2: same-name fallback for ids that could not be paired by id.
    // When several A nodes share a name, solve the assignment with the
    // Hungarian algorithm (minimum total geometry distance) and require the
    // same node type — arbitrary first-unmatched pairing cross-matches
    // unrelated same-name nodes (e.g. icon 1 <-> icon 2) and floods the diff
    // with fake `rect` changes.
    const COST_INF: i64 = i64::MAX / 4;

    fn geo_distance(a: &TreeNode, b: &TreeNode) -> i64 {
        (a.x - b.x).saturating_pow(2)
            + (a.y - b.y).saturating_pow(2)
            + (a.width - b.width).saturating_pow(2)
            + (a.height - b.height).saturating_pow(2)
    }

    /// Hungarian algorithm (minimum-cost perfect matching, O(n^3)).
    /// `cost` is n x n; returns for each row the assigned column.
    fn hungarian_min(cost: &[Vec<i64>]) -> Vec<usize> {
        let n = cost.len();
        let mut u = vec![0i64; n + 1];
        let mut v = vec![0i64; n + 1];
        let mut p = vec![0usize; n + 1];
        let mut way = vec![0usize; n + 1];
        for i in 1..=n {
            p[0] = i;
            let mut j0 = 0usize;
            let mut minv = vec![i64::MAX; n + 1];
            let mut used = vec![false; n + 1];
            loop {
                used[j0] = true;
                let i0 = p[j0];
                let mut delta = i64::MAX;
                let mut j1 = 0usize;
                for j in 1..=n {
                    if !used[j] {
                        let cur = cost[i0 - 1][j - 1] - u[i0] - v[j];
                        if cur < minv[j] {
                            minv[j] = cur;
                            way[j] = j0;
                        }
                        if minv[j] < delta {
                            delta = minv[j];
                            j1 = j;
                        }
                    }
                }
                for j in 0..=n {
                    if used[j] {
                        u[p[j]] += delta;
                        v[j] -= delta;
                    } else {
                        minv[j] -= delta;
                    }
                }
                j0 = j1;
                if p[j0] == 0 {
                    break;
                }
            }
            loop {
                let j1 = way[j0];
                p[j0] = p[j1];
                j0 = j1;
                if j0 == 0 {
                    break;
                }
            }
        }
        let mut assign = vec![0usize; n];
        for j in 1..=n {
            if p[j] != 0 {
                assign[p[j] - 1] = j - 1;
            }
        }
        assign
    }

    let push_added = |added: &mut Vec<TreeNode>, node_b: &TreeNode| {
        let is_page_wrapper = single_page_match
            && is_page_container(node_b)
            && root_ids_b.contains(node_b.id.as_str());
        if !is_page_wrapper {
            added.push(strip_children(node_b.clone()));
        }
    };

    // Unmatched B nodes grouped by name (deterministic order).
    let mut b_sorted: Vec<(&String, &&TreeNode)> = by_id_b
        .iter()
        .filter(|(id, _)| !matched_a.contains(*id))
        .collect();
    b_sorted.sort_by_key(|(ida, _)| *ida);
    let mut b_by_name: HashMap<&str, Vec<(&String, &&TreeNode)>> = HashMap::new();
    for (id, node) in b_sorted {
        if node.name.is_empty() {
            push_added(&mut added, node);
        } else {
            b_by_name
                .entry(node.name.as_str())
                .or_default()
                .push((id, node));
        }
    }

    for (name, b_group) in b_by_name {
        let candidates: Vec<(&String, &TreeNode)> = by_name
            .get(name)
            .map(|ids| {
                let mut v: Vec<(&String, &TreeNode)> = ids
                    .iter()
                    .filter(|a_id| !matched_a.contains(**a_id))
                    .map(|a_id| (*a_id, by_id_a[*a_id]))
                    .collect();
                v.sort_by_key(|(ida, _)| *ida);
                v
            })
            .unwrap_or_default();
        if candidates.is_empty() {
            for (_, node) in &b_group {
                push_added(&mut added, node);
            }
            continue;
        }
        let nb = b_group.len();
        let na = candidates.len();
        let n = nb.max(na);
        let mut cost = vec![vec![COST_INF; n]; n];
        for (i, (_, node_b)) in b_group.iter().enumerate() {
            for (j, (_, node_a)) in candidates.iter().enumerate() {
                if node_a.r#type == node_b.r#type {
                    cost[i][j] = geo_distance(node_a, node_b);
                }
            }
        }
        let assign = hungarian_min(&cost);
        for (i, (_, node_b)) in b_group.iter().enumerate() {
            let j = assign[i];
            let paired = j < na && cost[i][j] < COST_INF;
            if paired {
                let (a_id, node_a) = &candidates[j];
                record_change(
                    &mut matched_a,
                    &mut changed,
                    node_a,
                    node_b,
                    (*a_id).clone(),
                );
            } else {
                push_added(&mut added, node_b);
            }
        }
    }
    let removed: Vec<TreeNode> = by_id_a
        .iter()
        .filter(|(id, _)| !matched_a.contains(*id))
        .filter(|(id, _)| {
            !(single_page_match
                && is_page_container(by_id_a[id.as_str()])
                && root_ids_a.contains(id.as_str()))
        })
        .map(|(_, node)| strip_children((*node).clone()))
        .collect();
    TreeDiff {
        added,
        removed,
        changed,
    }
}

/// True for page/artboard-level containers whose uuid changes across designs
/// are pure wrapper noise (not real layout changes).
fn is_page_container(node: &TreeNode) -> bool {
    matches!(
        node.r#type.as_str(),
        "artboard" | "page" | "frame" | "canvas" | "board"
    )
}

/* -------------------------------- style code ------------------------------ */

/// Output format for moonvy_get_style_code.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum StyleCodeFormat {
    #[default]
    Css,
    Tailwind,
}

impl StyleCodeFormat {
    pub fn parse(s: Option<&str>) -> Option<Self> {
        match s.map(str::to_lowercase).as_deref() {
            None | Some("css") => Some(Self::Css),
            Some("tailwind" | "tailwindcss" | "tw") => Some(Self::Tailwind),
            _ => None,
        }
    }
}

#[derive(Debug, Clone, Default, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct StyleCodeItem {
    pub id: String,
    pub name: String,
    /// CSS class / Tailwind-ready component label derived from the node name.
    pub selector: String,
    /// Generated style declaration (css block or tailwind class string).
    pub code: String,
}

/// Tailwind spacing scale for sizes/radii: 4px steps map to w-*/h-*/rounded-*.
fn tw_scale(px: f64) -> Option<String> {
    let v = px / 4.0;
    (v.fract() == 0.0 && (1.0..=24.0).contains(&v)).then(|| format!("{}", v as i64))
}

fn tw_size_class(prop: &str, px: f64) -> String {
    match tw_scale(px) {
        Some(n) => format!("{prop}-{n}"),
        None => format!("{prop}-[{px}px]"),
    }
}

/// Generate CSS / Tailwind snippets for a flattened tree. Nodes with no
/// visual style (empty containers) collapse to a positioning-only line.
pub fn generate_style_code(nodes: &[TreeNode], format: StyleCodeFormat) -> Vec<StyleCodeItem> {
    fn class_name(node: &TreeNode) -> String {
        let base: String = node
            .name
            .chars()
            .filter(|c| c.is_alphanumeric())
            .take(24)
            .collect();
        let base = if base.is_empty() {
            "node".to_string()
        } else {
            base
        };
        let suffix: String = node.id.chars().take(6).collect();
        format!("{base}-{suffix}")
    }

    fn css_rule(node: &TreeNode, cls: &str) -> String {
        let mut lines = vec![format!(".{cls} {{")];
        if node.r#type == "artboard" {
            lines.push("  position: relative;".to_string());
        } else {
            lines.push("  position: absolute;".to_string());
            lines.push(format!("  left: {}px;", node.x));
            lines.push(format!("  top: {}px;", node.y));
        }
        lines.push(format!("  width: {}px;", node.width));
        lines.push(format!("  height: {}px;", node.height));
        if let Some(style) = &node.style {
            if let Some(bg) = &style.background {
                lines.push(format!("  background: {bg};"));
            }
            if let Some(gradient) = &style.gradient {
                let stops = gradient
                    .stops
                    .iter()
                    .map(|s| format!("{} {}%", s.color, s.position * 100.0))
                    .collect::<Vec<_>>()
                    .join(", ");
                lines.push(format!("  background: linear-gradient({stops});"));
            }
            if let Some(radius) = style.border_radius {
                // A radius >= half the smaller side renders as a full circle
                // (browsers clamp), so emit 50% instead of a raw px value.
                let full = node.width.min(node.height) as f64 / 2.0;
                let value = if radius >= full {
                    "50%".to_string()
                } else {
                    format!("{radius}px")
                };
                lines.push(format!("  border-radius: {value};"));
            }
            if let (Some(color), Some(width)) = (&style.stroke_color, style.stroke_width) {
                lines.push(format!("  border: {width}px solid {color};"));
            }
            if let Some(opacity) = style.opacity {
                lines.push(format!("  opacity: {opacity};"));
            }
            if let Some(color) = &style.color {
                lines.push(format!("  color: {color};"));
            }
            if let Some(size) = style.font_size {
                lines.push(format!("  font-size: {size}px;"));
            }
            if let Some(weight) = style.font_weight {
                let weight = weight.round() as i64;
                lines.push(format!("  font-weight: {weight};"));
            }
            if let Some(height) = style.line_height {
                lines.push(format!("  line-height: {height}px;"));
            }
            if let Some(family) = &style.font_family {
                lines.push(format!("  font-family: '{family}';"));
            }
        }
        lines.push("}".to_string());
        lines.join("\n")
    }

    fn tw_rule(node: &TreeNode) -> String {
        let mut classes = Vec::new();
        if node.r#type != "artboard" {
            classes.push("absolute".to_string());
            classes.push(format!("left-[{}px]", node.x));
            classes.push(format!("top-[{}px]", node.y));
        } else {
            classes.push("relative".to_string());
        }
        classes.push(tw_size_class("w", node.width as f64));
        classes.push(tw_size_class("h", node.height as f64));
        if let Some(style) = &node.style {
            if let Some(bg) = &style.background {
                classes.push(format!("bg-[{bg}]"));
            }
            if let Some(gradient) = &style.gradient {
                let stops = gradient
                    .stops
                    .iter()
                    .map(|s| format!("{} {}%", s.color, s.position * 100.0))
                    .collect::<Vec<_>>()
                    .join(", ");
                classes.push(format!("bg-[linear-gradient({stops})]"));
            }
            if let Some(radius) = style.border_radius {
                let full = node.width.min(node.height) as f64 / 2.0;
                if radius >= full {
                    classes.push("rounded-full".to_string());
                } else {
                    classes.push(format!("rounded-[{radius}px]"));
                }
            }
            if let Some(opacity) = style.opacity {
                classes.push(format!("opacity-[{opacity}]"));
            }
            if style.stroke_color.is_some() || style.stroke_width.is_some() {
                let width = style.stroke_width.unwrap_or(1.0);
                let color = style.stroke_color.clone().unwrap_or_else(|| "#fff".into());
                classes.push(format!("border-[{width}px]"));
                classes.push(format!("border-[{color}]"));
            }
            if let Some(color) = &style.color {
                if color.eq_ignore_ascii_case("#ffffff") {
                    classes.push("text-white".to_string());
                } else {
                    classes.push(format!("text-[{color}]"));
                }
            }
            if let Some(size) = style.font_size {
                classes.push(format!("text-[{size}px]"));
            }
            if let Some(weight) = style.font_weight {
                let weight = weight.round() as i64;
                let name = match weight {
                    100 => "thin",
                    200 => "extralight",
                    300 => "light",
                    400 => "normal",
                    500 => "medium",
                    600 => "semibold",
                    700 => "bold",
                    800 => "extrabold",
                    900 => "black",
                    _ => "",
                };
                classes.push(if name.is_empty() {
                    format!("font-[{weight}]")
                } else {
                    format!("font-{name}")
                });
            }
        }
        classes.join(" ")
    }

    nodes
        .iter()
        .map(|node| {
            let selector = class_name(node);
            let code = match format {
                StyleCodeFormat::Css => css_rule(node, &selector),
                StyleCodeFormat::Tailwind => tw_rule(node),
            };
            StyleCodeItem {
                id: node.id.clone(),
                name: node.name.clone(),
                selector,
                code,
            }
        })
        .collect()
}

/* -------------------------------- misc helpers ----------------------------- */

pub fn ids_equal(a: Option<&str>, b: Option<&str>) -> bool {
    let (Some(a), Some(b)) = (a, b) else {
        return false;
    };
    let a = a.trim();
    let b = b.trim();
    if a.is_empty() || b.is_empty() {
        return false;
    }
    if a == b {
        return true;
    }
    let seg_a: Vec<&str> = a
        .split(';')
        .map(str::trim)
        .filter(|s| !s.is_empty())
        .collect();
    let seg_b: Vec<&str> = b
        .split(';')
        .map(str::trim)
        .filter(|s| !s.is_empty())
        .collect();
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
    DesignMeta {
        title,
        frame_count,
        frames,
    }
}

#[derive(Debug, Clone, Default, Serialize)]
pub struct Tokens {
    pub colors: Vec<String>,
    pub font_sizes: Vec<f64>,
    pub radii: Vec<i64>,
    pub spacing: Vec<i64>,
}

pub fn extract_tokens(genome: &Genome) -> Tokens {
    let mut colors: HashSet<String> = HashSet::new();
    let mut font_sizes: HashSet<u64> = HashSet::new();
    let mut radii: HashSet<i64> = HashSet::new();
    let mut spacing: HashSet<i64> = HashSet::new();

    fn walk(
        node: &GenomeNode,
        colors: &mut HashSet<String>,
        font_sizes: &mut HashSet<u64>,
        radii: &mut HashSet<i64>,
        spacing: &mut HashSet<i64>,
    ) {
        for fill in &node.fills {
            if let Some(c) = resolve_fill_color(Some(fill)) {
                colors.insert(c);
            }
        }
        if let Some(r) = node.border_radius
            && r.is_finite()
        {
            radii.insert(r.round() as i64);
        }
        if let Some(rect) = &node.rect {
            if rect.x.is_finite() {
                spacing.insert(rect.x.round() as i64);
            }
            if rect.y.is_finite() {
                spacing.insert(rect.y.round() as i64);
            }
        }
        if let Some(segments) = node.textbox.as_ref().map(|t| &t.segments) {
            for seg in segments {
                if let Some(size) = seg.font_size {
                    font_sizes.insert(size.to_bits());
                }
                for fill in &seg.fills {
                    if let Some(c) = resolve_fill_color(Some(fill)) {
                        colors.insert(c);
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
                    colors.insert(c);
                }
            }
        }
    }
    for page in &genome.pages {
        walk(page, &mut colors, &mut font_sizes, &mut radii, &mut spacing);
    }

    let mut colors: Vec<String> = colors.into_iter().collect();
    colors.sort();
    let mut font_sizes: Vec<f64> = font_sizes.into_iter().map(f64::from_bits).collect();
    font_sizes.sort_by(|a, b| a.total_cmp(b));
    let mut radii: Vec<i64> = radii.into_iter().collect();
    radii.sort_unstable();
    let mut spacing: Vec<i64> = spacing.into_iter().filter(|s| *s > 0).collect();
    spacing.sort_unstable();
    Tokens {
        colors,
        font_sizes,
        radii,
        spacing,
    }
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
                    rect: Some(Rect {
                        x: 0.0,
                        y: 0.0,
                        w: 1440.0,
                        h: 900.0,
                    }),
                    children: vec![GenomeNode {
                        id: Some("1:1".into()),
                        name: Some("Header".into()),
                        r#type: Some("frame".into()),
                        rect: Some(Rect {
                            x: 0.0,
                            y: 0.0,
                            w: 1440.0,
                            h: 80.0,
                        }),
                        fills: vec![Fill {
                            r#type: Some("color".into()),
                            color: Some(Color {
                                r: 255.0,
                                g: 0.0,
                                b: 0.0,
                                alpha: None,
                            }),
                            opacity: Some(1.0),
                            ..Default::default()
                        }],
                        children: vec![
                            GenomeNode {
                                id: Some("1:2".into()),
                                name: Some("Logo".into()),
                                r#type: Some("text".into()),
                                rect: Some(Rect {
                                    x: 20.0,
                                    y: 24.0,
                                    w: 100.0,
                                    h: 32.0,
                                }),
                                textbox: Some(Textbox {
                                    text: Some("LOGO".into()),
                                    align: None,
                                    segments: vec![TextSegment {
                                        font_size: Some(16.0),
                                        font_weight: Some(700.0),
                                        font_name: Some(FontName {
                                            family: Some("Inter".into()),
                                            style: None,
                                        }),
                                        fills: vec![Fill {
                                            r#type: Some("color".into()),
                                            color: Some(Color {
                                                r: 0.0,
                                                g: 0.0,
                                                b: 0.0,
                                                alpha: None,
                                            }),
                                            ..Default::default()
                                        }],
                                        ..Default::default()
                                    }],
                                }),
                                ..Default::default()
                            },
                            GenomeNode {
                                id: Some("1:3".into()),
                                name: Some("Btn".into()),
                                r#type: Some("rectangle".into()),
                                rect: Some(Rect {
                                    x: 1300.0,
                                    y: 16.0,
                                    w: 100.0,
                                    h: 48.0,
                                }),
                                border_radius: Some(8.0),
                                fills: vec![Fill {
                                    r#type: Some("color".into()),
                                    color: Some(Color {
                                        r: 0.0,
                                        g: 0.0,
                                        b: 255.0,
                                        alpha: None,
                                    }),
                                    opacity: Some(1.0),
                                    ..Default::default()
                                }],
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
                    rect: Some(Rect {
                        x: 0.0,
                        y: 0.0,
                        w: 1024.0,
                        h: 768.0,
                    }),
                    ..Default::default()
                },
            ],
            styles: Some(Styles {
                fill_styles: vec![],
            }),
            images: Default::default(),
        }
    }

    fn chain_genome() -> Genome {
        Genome {
            pages: vec![GenomeNode {
                id: Some("p:0".into()),
                name: Some("Page".into()),
                r#type: Some("page".into()),
                rect: Some(Rect {
                    x: 100.0,
                    y: 200.0,
                    w: 1440.0,
                    h: 900.0,
                }),
                children: vec![GenomeNode {
                    id: Some("g:1".into()),
                    name: Some("group1".into()),
                    r#type: Some("group".into()),
                    rect: Some(Rect {
                        x: 10.0,
                        y: 20.0,
                        w: 800.0,
                        h: 600.0,
                    }),
                    children: vec![GenomeNode {
                        id: Some("g:2".into()),
                        name: Some("group2".into()),
                        r#type: Some("group".into()),
                        rect: Some(Rect {
                            x: 30.0,
                            y: 40.0,
                            w: 500.0,
                            h: 400.0,
                        }),
                        children: vec![
                            GenomeNode {
                                id: Some("t:1".into()),
                                name: Some("Title".into()),
                                r#type: Some("text".into()),
                                rect: Some(Rect {
                                    x: 5.0,
                                    y: 5.0,
                                    w: 200.0,
                                    h: 40.0,
                                }),
                                textbox: Some(Textbox {
                                    text: Some("Hello".into()),
                                    ..Default::default()
                                }),
                                ..Default::default()
                            },
                            GenomeNode {
                                id: Some("i:1".into()),
                                name: Some("Icon".into()),
                                r#type: Some("frame".into()),
                                rect: Some(Rect {
                                    x: 60.0,
                                    y: 60.0,
                                    w: 50.0,
                                    h: 50.0,
                                }),
                                fills: vec![Fill {
                                    r#type: Some("color".into()),
                                    color: Some(Color {
                                        r: 1.0,
                                        g: 2.0,
                                        b: 3.0,
                                        alpha: None,
                                    }),
                                    opacity: Some(1.0),
                                    ..Default::default()
                                }],
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
            rect: Some(Rect {
                x: 0.0,
                y: 0.0,
                w: 100.0,
                h: 100.0,
            }),
            fills: vec![Fill {
                r#type: Some("color".into()),
                color: Some(Color {
                    r: 255.0,
                    g: 0.0,
                    b: 0.0,
                    alpha: None,
                }),
                opacity: Some(1.0),
                ..Default::default()
            }],
            strokes: vec![Stroke {
                fills: vec![Fill {
                    r#type: Some("color".into()),
                    color: Some(Color {
                        r: 0.0,
                        g: 0.0,
                        b: 0.0,
                        alpha: None,
                    }),
                    opacity: Some(1.0),
                    ..Default::default()
                }],
                w: Some(1.6),
            }],
            ..Default::default()
        };
        let style = extract_raw_node_style(&genome, &stroke_node);
        assert_eq!(style.stroke_width, Some(1.6));
        assert_eq!(style.stroke_color.as_deref(), Some("#000000"));
        assert!(style.gradient.is_none());

        let grad_node = GenomeNode {
            r#type: Some("rectangle".into()),
            rect: Some(Rect {
                x: 0.0,
                y: 0.0,
                w: 100.0,
                h: 100.0,
            }),
            fills: vec![Fill {
                r#type: Some("gradient".into()),
                gradient: Some(Gradient {
                    r#type: Some("linear".into()),
                    stops: vec![
                        GradientStop {
                            color: Some(Color {
                                r: 255.0,
                                g: 162.0,
                                b: 55.0,
                                alpha: Some(1.0),
                            }),
                            position: Some(0.0),
                        },
                        GradientStop {
                            color: Some(Color {
                                r: 0.0,
                                g: 0.0,
                                b: 0.0,
                                alpha: Some(1.0),
                            }),
                            position: Some(1.0),
                        },
                    ],
                    angle: Some(90.0),
                }),
                ..Default::default()
            }],
            ..Default::default()
        };
        let style = extract_raw_node_style(&genome, &grad_node);
        assert!(style.background.is_none());
        assert_eq!(
            style.gradient.as_ref().map(|g| g.r#type.as_str()),
            Some("linear")
        );
        assert_eq!(style.gradient.as_ref().map(|g| g.stops.len()), Some(2));
        assert_eq!(
            style.gradient.as_ref().map(|g| g.stops[0].color.as_str()),
            Some("rgba(255,162,55,1)")
        );
        assert_eq!(style.gradient.as_ref().and_then(|g| g.angle), Some(90.0));
    }

    #[test]
    fn tree_skip_empty_groups() {
        let genome = chain_genome();
        let full = extract_tree(&genome, None, &TreeOptions::default());
        assert_eq!(full[0].children.as_ref().unwrap()[0].name, "group1");

        let options = TreeOptions {
            skip_empty_groups: true,
            ..Default::default()
        };
        let filtered = extract_tree(&genome, None, &options);
        let children = filtered[0].children.as_ref().unwrap();
        assert_eq!(children[0].name, "Title");
        assert_eq!(children[1].name, "Icon");
    }

    #[test]
    fn tree_flatten_coordinates() {
        let genome = chain_genome();
        let options = TreeOptions {
            flatten: true,
            ..Default::default()
        };
        let flat = extract_tree(&genome, None, &options);
        let page = &flat[0];
        assert_eq!(page.x, 0, "artboard origin zeroed");
        assert_eq!(page.y, 0);
        let icon = &page.children.as_ref().unwrap()[0]
            .children
            .as_ref()
            .unwrap()[0]
            .children
            .as_ref()
            .unwrap()[1];
        assert_eq!(icon.x, 10 + 30 + 60, "nested offsets accumulated");
        assert_eq!(icon.y, 20 + 40 + 60);
    }

    #[test]
    fn tree_only_filter() {
        let genome = chain_genome();
        let options = TreeOptions {
            only: Some(vec!["text".into()]),
            ..Default::default()
        };
        let texts = extract_tree(&genome, None, &options);
        assert_eq!(texts.len(), 1);
        assert_eq!(texts[0].id, "t:1");
    }

    #[test]
    fn tree_only_image_semantic() {
        let genome = Genome {
            pages: vec![GenomeNode {
                id: Some("p:0".into()),
                name: Some("Page".into()),
                r#type: Some("page".into()),
                rect: Some(Rect {
                    x: 0.0,
                    y: 0.0,
                    w: 100.0,
                    h: 100.0,
                }),
                children: vec![
                    GenomeNode {
                        id: Some("a:1".into()),
                        name: Some("Cover".into()),
                        r#type: Some("rectangle".into()),
                        rect: Some(Rect {
                            x: 0.0,
                            y: 0.0,
                            w: 50.0,
                            h: 50.0,
                        }),
                        snapshot: Some("snap:1".into()),
                        ..Default::default()
                    },
                    GenomeNode {
                        id: Some("a:2".into()),
                        name: Some("Avatar".into()),
                        r#type: Some("ellipse".into()),
                        rect: Some(Rect {
                            x: 60.0,
                            y: 0.0,
                            w: 30.0,
                            h: 30.0,
                        }),
                        fills: vec![Fill {
                            r#type: Some("image".into()),
                            ..Default::default()
                        }],
                        ..Default::default()
                    },
                    GenomeNode {
                        id: Some("a:3".into()),
                        name: Some("Box".into()),
                        r#type: Some("rectangle".into()),
                        rect: Some(Rect {
                            x: 0.0,
                            y: 60.0,
                            w: 20.0,
                            h: 20.0,
                        }),
                        fills: vec![Fill {
                            r#type: Some("color".into()),
                            color: Some(Color {
                                r: 1.0,
                                g: 2.0,
                                b: 3.0,
                                alpha: None,
                            }),
                            opacity: Some(1.0),
                            ..Default::default()
                        }],
                        ..Default::default()
                    },
                ],
                ..Default::default()
            }],
            styles: None,
            images: Default::default(),
        };
        let options = TreeOptions {
            only: Some(vec!["image".into()]),
            ..Default::default()
        };
        let tree = extract_tree(&genome, None, &options);
        assert_eq!(tree.len(), 2, "snapshot + image-fill nodes matched");
        assert_eq!(tree[0].id, "a:1");
        assert_eq!(tree[1].id, "a:2");
    }

    #[test]
    fn tree_text_truncate() {
        let genome = Genome {
            pages: vec![GenomeNode {
                id: Some("p:0".into()),
                name: Some("Page".into()),
                r#type: Some("page".into()),
                rect: Some(Rect {
                    x: 0.0,
                    y: 0.0,
                    w: 100.0,
                    h: 100.0,
                }),
                children: vec![GenomeNode {
                    id: Some("t:1".into()),
                    name: Some("Title".into()),
                    r#type: Some("text".into()),
                    rect: Some(Rect {
                        x: 0.0,
                        y: 0.0,
                        w: 50.0,
                        h: 20.0,
                    }),
                    textbox: Some(Textbox {
                        text: Some("x".repeat(120)),
                        ..Default::default()
                    }),
                    ..Default::default()
                }],
                ..Default::default()
            }],
            styles: None,
            images: Default::default(),
        };
        // Default: truncated with an ellipsis.
        let tree = extract_tree(&genome, None, &TreeOptions::default());
        let text = tree[0].children.as_ref().unwrap()[0].text.clone().unwrap();
        assert_eq!(text.chars().count(), 41, "40 chars + ellipsis");
        assert!(text.ends_with('…'));
        // Full.
        let options = TreeOptions {
            text_content: TextContent::Full,
            ..Default::default()
        };
        let tree = extract_tree(&genome, None, &options);
        assert_eq!(
            tree[0].children.as_ref().unwrap()[0].text.as_deref(),
            Some("x".repeat(120).as_str())
        );
        // None.
        let options = TreeOptions {
            text_content: TextContent::None,
            ..Default::default()
        };
        let tree = extract_tree(&genome, None, &options);
        assert_eq!(tree[0].children.as_ref().unwrap()[0].text, None);
    }

    #[test]
    fn tree_node_id_subtree() {
        let genome = chain_genome();
        let options = TreeOptions {
            node_id: Some("g:1".into()),
            ..Default::default()
        };
        let tree = extract_tree(&genome, None, &options);
        assert_eq!(tree.len(), 1);
        assert_eq!(tree[0].name, "group1");
        assert_eq!(tree[0].children.as_ref().unwrap().len(), 1);
        // Unknown id -> empty.
        let options = TreeOptions {
            node_id: Some("nope".into()),
            ..Default::default()
        };
        assert!(extract_tree(&genome, None, &options).is_empty());
    }

    #[test]
    fn tree_region_filter() {
        let genome = chain_genome();
        // Icon sits at absolute x=10+30+60=100, y=20+40+60=120 (50x50).
        let options = TreeOptions {
            flatten: true,
            region: Some([90.0, 110.0, 70.0, 70.0]),
            ..Default::default()
        };
        let tree = extract_tree(&genome, None, &options);
        let kids = &tree[0].children.as_ref().unwrap()[0]
            .children
            .as_ref()
            .unwrap()[0]
            .children
            .as_ref()
            .unwrap();
        assert_eq!(kids.len(), 1, "Title (above the region) must be filtered");
        assert_eq!(kids[0].name, "Icon");
        // Region outside the icon -> filtered out.
        let options = TreeOptions {
            flatten: true,
            region: Some([500.0, 500.0, 50.0, 50.0]),
            ..Default::default()
        };
        let tree = extract_tree(&genome, None, &options);
        fn contains_name(nodes: &[TreeNode], name: &str) -> bool {
            nodes.iter().any(|n| {
                n.name == name || n.children.as_ref().is_some_and(|c| contains_name(c, name))
            })
        }
        assert!(
            !contains_name(&tree, "Icon"),
            "region must exclude the icon"
        );
    }

    #[test]
    fn mask_group_survives_skip_empty_groups() {
        let genome = Genome {
            pages: vec![GenomeNode {
                id: Some("p:0".into()),
                name: Some("Page".into()),
                r#type: Some("page".into()),
                rect: Some(Rect {
                    x: 0.0,
                    y: 0.0,
                    w: 100.0,
                    h: 100.0,
                }),
                children: vec![GenomeNode {
                    id: Some("mask:1".into()),
                    name: Some("蒙版组 94".into()),
                    r#type: Some("group".into()),
                    rect: Some(Rect {
                        x: 0.0,
                        y: 0.0,
                        w: 60.0,
                        h: 60.0,
                    }),
                    children: vec![
                        GenomeNode {
                            id: Some("shape:1".into()),
                            name: Some("裁剪形状".into()),
                            r#type: Some("layer".into()),
                            rect: Some(Rect {
                                x: 0.0,
                                y: 0.0,
                                w: 60.0,
                                h: 60.0,
                            }),
                            blend: Some(Blend {
                                opacity: Some(1.0),
                                is_mask: Some(true),
                                is_pure_mask: Some(true),
                            }),
                            ..Default::default()
                        },
                        GenomeNode {
                            id: Some("content:1".into()),
                            name: Some("被裁剪内容".into()),
                            r#type: Some("layer".into()),
                            rect: Some(Rect {
                                x: 0.0,
                                y: 0.0,
                                w: 60.0,
                                h: 60.0,
                            }),
                            fills: vec![Fill {
                                r#type: Some("color".into()),
                                color: Some(Color {
                                    r: 1.0,
                                    g: 2.0,
                                    b: 3.0,
                                    alpha: None,
                                }),
                                opacity: Some(1.0),
                                ..Default::default()
                            }],
                            ..Default::default()
                        },
                    ],
                    ..Default::default()
                }],
                ..Default::default()
            }],
            styles: None,
            images: Default::default(),
        };
        let options = TreeOptions {
            skip_empty_groups: true,
            ..Default::default()
        };
        let tree = extract_tree(&genome, None, &options);
        let mask = &tree[0].children.as_ref().unwrap()[0];
        assert_eq!(
            mask.name, "蒙版组 94",
            "mask group must survive skipEmptyGroups"
        );
        assert_eq!(mask.children.as_ref().unwrap().len(), 2);
        assert_eq!(
            mask.is_mask_group,
            Some(true),
            "mask group must carry isMaskGroup marker"
        );

        // Without mask markers the same empty group is lifted away.
        let mut stripped = genome;
        stripped.pages[0].children[0].children[0].blend = None;
        let tree = extract_tree(&stripped, None, &options);
        assert_eq!(tree[0].children.as_ref().unwrap()[0].name, "被裁剪内容");
    }

    #[test]
    fn style_null_keys_omitted_in_json() {
        let style = NodeStyle::default();
        let value = serde_json::to_value(&style).unwrap();
        let map = value.as_object().unwrap();
        assert!(
            map.is_empty(),
            "all-null style must serialize to an empty object"
        );
        let style = NodeStyle {
            background: Some("#fff".into()),
            ..Default::default()
        };
        let value = serde_json::to_value(&style).unwrap();
        let map = value.as_object().unwrap();
        assert_eq!(map.len(), 1);
        assert_eq!(map["background"], "#fff");
    }

    #[test]
    fn duplicates_require_same_geometry() {
        let make = |w: f64, h: f64| GenomeNode {
            id: Some("n".into()),
            name: Some("Btn".into()),
            r#type: Some("frame".into()),
            rect: Some(Rect {
                x: 0.0,
                y: 0.0,
                w,
                h,
            }),
            fills: vec![Fill {
                r#type: Some("color".into()),
                color: Some(Color {
                    r: 1.0,
                    g: 2.0,
                    b: 3.0,
                    alpha: None,
                }),
                opacity: Some(1.0),
                ..Default::default()
            }],
            ..Default::default()
        };
        let genome = Genome {
            pages: vec![GenomeNode {
                id: Some("p:0".into()),
                name: Some("Page".into()),
                r#type: Some("page".into()),
                rect: Some(Rect {
                    x: 0.0,
                    y: 0.0,
                    w: 100.0,
                    h: 100.0,
                }),
                children: vec![
                    GenomeNode {
                        id: Some("a:1".into()),
                        ..make(50.0, 50.0)
                    },
                    GenomeNode {
                        id: Some("a:2".into()),
                        ..make(50.0, 50.0)
                    },
                    GenomeNode {
                        id: Some("a:3".into()),
                        ..make(80.0, 50.0)
                    },
                ],
                ..Default::default()
            }],
            styles: None,
            images: Default::default(),
        };
        let options = TreeOptions {
            with_style: true,
            detect_duplicates: true,
            ..Default::default()
        };
        let tree = extract_tree(&genome, None, &options);
        let kids = tree[0].children.as_ref().unwrap();
        assert_eq!(kids[0].duplicate_of, None);
        assert_eq!(kids[1].duplicate_of.as_deref(), Some("a:1"));
        assert_eq!(
            kids[2].duplicate_of, None,
            "different size is not a duplicate"
        );
    }

    #[test]
    fn find_nodes_by_name_and_text() {
        let genome = chain_genome();
        let hits = find_nodes(&genome, None, "title", 10);
        assert_eq!(hits.len(), 1);
        assert_eq!(hits[0].id, "t:1");
        assert_eq!(hits[0].text.as_deref(), Some("Hello"));

        let hits = find_nodes(&genome, None, "ICON", 10);
        assert_eq!(hits.len(), 1);
        assert_eq!(hits[0].id, "i:1");

        let hits = find_nodes(&genome, None, "missing", 10);
        assert!(hits.is_empty());

        let hits = find_nodes(&genome, None, "icon", 1);
        assert_eq!(hits.len(), 1, "limit respected");

        let hits = find_nodes(&genome, None, "", 10);
        assert!(hits.is_empty(), "empty query returns nothing");
    }

    #[test]
    fn find_nodes_reports_component_container() {
        let genome = Genome {
            pages: vec![GenomeNode {
                id: Some("p:0".into()),
                name: Some("Page".into()),
                r#type: Some("page".into()),
                rect: Some(Rect {
                    x: 0.0,
                    y: 0.0,
                    w: 100.0,
                    h: 100.0,
                }),
                children: vec![GenomeNode {
                    id: Some("modal:1".into()),
                    name: Some("弹窗容器".into()),
                    r#type: Some("artboard".into()),
                    rect: Some(Rect {
                        x: 10.0,
                        y: 10.0,
                        w: 80.0,
                        h: 80.0,
                    }),
                    children: vec![GenomeNode {
                        id: Some("t:1".into()),
                        name: Some("标题".into()),
                        r#type: Some("text".into()),
                        textbox: Some(Textbox {
                            text: Some("编辑个人资料".into()),
                            ..Default::default()
                        }),
                        ..Default::default()
                    }],
                    ..Default::default()
                }],
                ..Default::default()
            }],
            styles: None,
            images: Default::default(),
        };
        let hits = find_nodes(&genome, None, "编辑个人资料", 10);
        assert_eq!(hits.len(), 1);
        assert_eq!(hits[0].id, "t:1");
        assert_eq!(hits[0].container_id.as_deref(), Some("modal:1"));
        assert_eq!(hits[0].container_name.as_deref(), Some("弹窗容器"));
        assert_eq!(hits[0].container_type.as_deref(), Some("artboard"));
    }

    #[test]
    fn style_code_css_generation() {
        let node = |id: &str, name: &str, x: i64, y: i64| TreeNode {
            id: id.into(),
            name: name.into(),
            r#type: "layer".into(),
            x,
            y,
            width: 80,
            height: 80,
            style: Some(NodeStyle {
                background: Some("#76b7a1".into()),
                border_radius: Some(45.0),
                stroke_color: Some("rgba(255,255,255,0.10)".into()),
                stroke_width: Some(1.0),
                ..Default::default()
            }),
            ..Default::default()
        };
        let items = generate_style_code(&[node("n1", "头像", 400, 163)], StyleCodeFormat::Css);
        assert_eq!(items.len(), 1);
        let code = &items[0].code;
        assert!(code.contains("position: absolute;"));
        assert!(code.contains("left: 400px;"));
        assert!(code.contains("top: 163px;"));
        assert!(code.contains("width: 80px;"));
        assert!(code.contains("background: #76b7a1;"));
        // 80x80 with radius 45 == full circle -> 50%.
        assert!(code.contains("border-radius: 50%;"));
        assert!(code.contains("border: 1px solid rgba(255,255,255,0.10);"));
        assert!(items[0].selector.starts_with("头像-"));
    }

    #[test]
    fn style_code_tailwind_generation() {
        let text_node = TreeNode {
            id: "n2".into(),
            name: "昵称".into(),
            r#type: "text".into(),
            x: 420,
            y: 367,
            width: 141,
            height: 30,
            style: Some(NodeStyle {
                color: Some("#ffffff".into()),
                font_size: Some(20.0),
                font_weight: Some(500.0),
                ..Default::default()
            }),
            ..Default::default()
        };
        let items = generate_style_code(&[text_node], StyleCodeFormat::Tailwind);
        let code = &items[0].code;
        assert!(code.contains("absolute"));
        assert!(code.contains("left-[420px]"));
        assert!(code.contains("top-[367px]"));
        assert!(code.contains("w-[141px]"));
        assert!(code.contains("h-[30px]"));
        assert!(code.contains("text-white"));
        assert!(code.contains("text-[20px]"));
        assert!(code.contains("font-medium"));
    }

    #[test]
    fn style_code_full_radius_rounds_to_full() {
        let node = TreeNode {
            id: "n3".into(),
            name: "按钮".into(),
            r#type: "layer".into(),
            width: 60,
            height: 60,
            style: Some(NodeStyle {
                background: Some("#0a9657".into()),
                border_radius: Some(8.0),
                ..Default::default()
            }),
            ..Default::default()
        };
        let css = generate_style_code(std::slice::from_ref(&node), StyleCodeFormat::Css);
        assert!(
            css[0].code.contains("border-radius: 8px;"),
            "small radius stays px"
        );
        let mut full = node;
        full.style.as_mut().unwrap().border_radius = Some(40.0);
        let css = generate_style_code(&[full], StyleCodeFormat::Css);
        assert!(
            css[0].code.contains("border-radius: 50%;"),
            "half-size radius is a full circle"
        );
    }

    #[test]
    fn style_code_format_parse() {
        assert_eq!(
            StyleCodeFormat::parse(Some("css")),
            Some(StyleCodeFormat::Css)
        );
        assert_eq!(
            StyleCodeFormat::parse(Some("tailwind")),
            Some(StyleCodeFormat::Tailwind)
        );
        assert_eq!(
            StyleCodeFormat::parse(Some("tw")),
            Some(StyleCodeFormat::Tailwind)
        );
        assert_eq!(StyleCodeFormat::parse(None), Some(StyleCodeFormat::Css));
        assert_eq!(StyleCodeFormat::parse(Some("nope")), None);
    }

    #[test]
    fn tree_detect_duplicates() {
        let genome = Genome {
            pages: vec![GenomeNode {
                id: Some("p:0".into()),
                name: Some("Page".into()),
                r#type: Some("page".into()),
                rect: Some(Rect {
                    x: 0.0,
                    y: 0.0,
                    w: 100.0,
                    h: 100.0,
                }),
                children: vec![
                    GenomeNode {
                        id: Some("a:1".into()),
                        name: Some("Btn".into()),
                        r#type: Some("frame".into()),
                        rect: Some(Rect {
                            x: 0.0,
                            y: 0.0,
                            w: 50.0,
                            h: 50.0,
                        }),
                        fills: vec![Fill {
                            r#type: Some("color".into()),
                            color: Some(Color {
                                r: 1.0,
                                g: 2.0,
                                b: 3.0,
                                alpha: None,
                            }),
                            opacity: Some(1.0),
                            ..Default::default()
                        }],
                        ..Default::default()
                    },
                    GenomeNode {
                        id: Some("a:2".into()),
                        name: Some("Btn".into()),
                        r#type: Some("frame".into()),
                        rect: Some(Rect {
                            x: 60.0,
                            y: 0.0,
                            w: 50.0,
                            h: 50.0,
                        }),
                        fills: vec![Fill {
                            r#type: Some("color".into()),
                            color: Some(Color {
                                r: 1.0,
                                g: 2.0,
                                b: 3.0,
                                alpha: None,
                            }),
                            opacity: Some(1.0),
                            ..Default::default()
                        }],
                        ..Default::default()
                    },
                ],
                ..Default::default()
            }],
            styles: None,
            images: Default::default(),
        };
        let options = TreeOptions {
            with_style: true,
            detect_duplicates: true,
            ..Default::default()
        };
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
            TreeNode {
                id: "1".into(),
                name: "A".into(),
                r#type: "frame".into(),
                x: 0,
                y: 0,
                width: 10,
                height: 10,
                text: Some("same".into()),
                ..Default::default()
            },
            TreeNode {
                id: "3".into(),
                name: "C".into(),
                r#type: "frame".into(),
                x: 0,
                y: 0,
                width: 10,
                height: 10,
                ..Default::default()
            },
        ];
        let b = vec![
            TreeNode {
                id: "1".into(),
                name: "A".into(),
                r#type: "frame".into(),
                x: 0,
                y: 0,
                width: 10,
                height: 10,
                text: Some("changed".into()),
                ..Default::default()
            },
            TreeNode {
                id: "4".into(),
                name: "D".into(),
                r#type: "frame".into(),
                x: 0,
                y: 0,
                width: 10,
                height: 10,
                ..Default::default()
            },
        ];
        let diff = diff_trees(&a, &b);
        assert_eq!(diff.removed.len(), 1);
        assert_eq!(diff.removed[0].id, "3");
        assert_eq!(diff.added.len(), 1);
        assert_eq!(diff.added[0].id, "4");
        assert_eq!(diff.changed.len(), 1);
        assert_eq!(diff.changed[0].id, "1");
        assert_eq!(diff.changed[0].fields, vec!["text"]);
        assert_eq!(
            diff.changed[0].before.as_ref().map(|n| n.text.as_deref()),
            Some(Some("same"))
        );
        assert_eq!(
            diff.changed[0].after.as_ref().map(|n| n.text.as_deref()),
            Some(Some("changed"))
        );
    }

    #[test]
    fn diff_name_fallback_matches_same_name_nodes() {
        let a = vec![
            TreeNode {
                id: "1".into(),
                name: "Avatar".into(),
                r#type: "frame".into(),
                text: Some("old".into()),
                ..Default::default()
            },
            TreeNode {
                id: "2".into(),
                name: "Title".into(),
                r#type: "text".into(),
                text: Some("Hello".into()),
                ..Default::default()
            },
        ];
        let b = vec![
            TreeNode {
                id: "999".into(),
                name: "Avatar".into(),
                r#type: "frame".into(),
                text: Some("new".into()),
                ..Default::default()
            },
            TreeNode {
                id: "2".into(),
                name: "Title".into(),
                r#type: "text".into(),
                text: Some("Hello".into()),
                ..Default::default()
            },
        ];
        let diff = diff_trees(&a, &b);
        assert_eq!(diff.added.len(), 0, "id mismatch paired by name");
        assert_eq!(diff.removed.len(), 0);
        assert_eq!(diff.changed.len(), 1);
        assert_eq!(diff.changed[0].id, "999");
        assert_eq!(diff.changed[0].fields, vec!["text"]);
        assert_eq!(
            diff.changed[0].before.as_ref().map(|n| n.id.as_str()),
            Some("1")
        );
        assert_eq!(
            diff.changed[0].after.as_ref().map(|n| n.id.as_str()),
            Some("999")
        );
    }

    #[test]
    fn diff_name_fallback_keeps_true_adds() {
        let a = vec![TreeNode {
            id: "1".into(),
            name: "Avatar".into(),
            r#type: "frame".into(),
            ..Default::default()
        }];
        let b = vec![
            TreeNode {
                id: "1".into(),
                name: "Avatar".into(),
                r#type: "frame".into(),
                ..Default::default()
            },
            TreeNode {
                id: "777".into(),
                name: "Badge".into(),
                r#type: "frame".into(),
                ..Default::default()
            },
        ];
        let diff = diff_trees(&a, &b);
        assert_eq!(diff.added.len(), 1);
        assert_eq!(diff.added[0].id, "777");
        assert!(diff.changed.is_empty());
        assert!(diff.removed.is_empty());
    }

    #[test]
    fn diff_name_fallback_pairs_nearest_geometry_not_arbitrary() {
        // Three same-name nodes ("Icon") at different positions; B has the
        // same set with new uuids. Nearest-geometry pairing must pair each B
        // node with the A node at the same spot — no fake rect changes.
        let icon = |id: &str, x: i64, y: i64| TreeNode {
            id: id.into(),
            name: "Icon".into(),
            r#type: "layer".into(),
            x,
            y,
            width: 24,
            height: 24,
            ..Default::default()
        };
        let a = vec![icon("a1", 0, 0), icon("a2", 100, 0), icon("a3", 0, 100)];
        let b = vec![icon("b1", 0, 0), icon("b2", 100, 0), icon("b3", 0, 100)];
        let diff = diff_trees(&a, &b);
        assert_eq!(
            diff.added.len(),
            0,
            "all same-name nodes must pair by position"
        );
        assert_eq!(diff.removed.len(), 0);
        assert!(
            diff.changed.is_empty(),
            "identical positions must not report rect changes"
        );

        // Now B's third icon actually moved to (5, 5): only that pair changes.
        let b_moved = vec![icon("b1", 0, 0), icon("b2", 100, 0), icon("b3", 5, 5)];
        let diff = diff_trees(&a, &b_moved);
        assert_eq!(diff.changed.len(), 1);
        assert_eq!(diff.changed[0].id, "b3");
        assert_eq!(diff.changed[0].fields, vec!["rect"]);
    }

    #[test]
    fn diff_same_name_different_type_not_paired() {
        let a = vec![TreeNode {
            id: "a1".into(),
            name: "Badge".into(),
            r#type: "layer".into(),
            ..Default::default()
        }];
        let b = vec![TreeNode {
            id: "b1".into(),
            name: "Badge".into(),
            r#type: "text".into(),
            ..Default::default()
        }];
        let diff = diff_trees(&a, &b);
        assert_eq!(diff.added.len(), 1, "type mismatch must not pair by name");
        assert_eq!(diff.added[0].id, "b1");
        assert_eq!(diff.removed.len(), 1);
        assert_eq!(diff.removed[0].id, "a1");
        assert!(diff.changed.is_empty());
    }

    #[test]
    fn diff_single_page_root_wrapper_not_reported() {
        // Two single-page designs, same viewport, different page uuid/name:
        // the page container itself must not land in added/removed.
        let page = |id: &str, name: &str, child_id: &str| TreeNode {
            id: id.into(),
            name: name.into(),
            r#type: "artboard".into(),
            width: 1440,
            height: 900,
            children: Some(vec![TreeNode {
                id: child_id.into(),
                name: "内容".into(),
                r#type: "layer".into(),
                ..Default::default()
            }]),
            ..Default::default()
        };
        let a = vec![page("page-a", "首页 - 个人资料", "child-a")];
        let b = vec![page("page-b", "首页 - 个人资料（hover）", "child-b")];
        let diff = diff_trees(&a, &b);
        assert!(
            diff.added
                .iter()
                .all(|n| n.name != "首页 - 个人资料（hover）"),
            "page wrapper must not be reported as added"
        );
        assert!(
            diff.removed.iter().all(|n| n.name != "首页 - 个人资料"),
            "page wrapper must not be reported as removed"
        );
        // The inner "内容" child has the same name on both sides, so it pairs
        // via the same-name fallback: no structural adds/removes, no changes.
        assert!(
            diff.added.is_empty(),
            "same-name child must pair, got {:?}",
            diff.added
        );
        assert!(diff.removed.is_empty());
        assert!(diff.changed.is_empty());
    }

    #[test]
    fn mask_group_survives_with_pure_mask_marker_only() {
        // Real genome data sometimes only carries `isPureMask` on the clip
        // shape; the mask group must still survive skipEmptyGroups.
        let genome = Genome {
            pages: vec![GenomeNode {
                id: Some("p:0".into()),
                name: Some("Page".into()),
                r#type: Some("page".into()),
                rect: Some(Rect {
                    x: 0.0,
                    y: 0.0,
                    w: 100.0,
                    h: 100.0,
                }),
                children: vec![GenomeNode {
                    id: Some("mask:1".into()),
                    name: Some("蒙版组 94".into()),
                    r#type: Some("group".into()),
                    rect: Some(Rect {
                        x: 0.0,
                        y: 0.0,
                        w: 60.0,
                        h: 60.0,
                    }),
                    children: vec![GenomeNode {
                        id: Some("shape:1".into()),
                        name: Some("裁剪形状".into()),
                        r#type: Some("layer".into()),
                        blend: Some(Blend {
                            opacity: Some(1.0),
                            is_mask: None,
                            is_pure_mask: Some(true),
                        }),
                        ..Default::default()
                    }],
                    ..Default::default()
                }],
                ..Default::default()
            }],
            styles: None,
            images: Default::default(),
        };
        let options = TreeOptions {
            skip_empty_groups: true,
            ..Default::default()
        };
        let tree = extract_tree(&genome, None, &options);
        let mask = &tree[0].children.as_ref().unwrap()[0];
        assert_eq!(
            mask.name, "蒙版组 94",
            "isPureMask-only group must survive skipEmptyGroups"
        );
        assert_eq!(
            mask.is_mask_group,
            Some(true),
            "mask group must carry isMaskGroup marker"
        );
    }

    #[test]
    fn tree_snapshot_hash_exposed() {
        let genome = Genome {
            pages: vec![GenomeNode {
                id: Some("p:0".into()),
                name: Some("Page".into()),
                r#type: Some("page".into()),
                rect: Some(Rect {
                    x: 0.0,
                    y: 0.0,
                    w: 100.0,
                    h: 100.0,
                }),
                children: vec![GenomeNode {
                    id: Some("img:1".into()),
                    name: Some("Avatar".into()),
                    r#type: Some("ellipse".into()),
                    rect: Some(Rect {
                        x: 0.0,
                        y: 0.0,
                        w: 40.0,
                        h: 40.0,
                    }),
                    snapshot: Some("snap-abc".into()),
                    ..Default::default()
                }],
                ..Default::default()
            }],
            styles: None,
            images: Default::default(),
        };
        let tree = extract_tree(&genome, None, &TreeOptions::default());
        let avatar = &tree[0].children.as_ref().unwrap()[0];
        assert_eq!(avatar.snapshot_hash.as_deref(), Some("snap-abc"));

        // Snapshot preview is used as a fallback.
        let genome_preview = Genome {
            pages: vec![GenomeNode {
                id: Some("p:0".into()),
                name: Some("Page".into()),
                r#type: Some("page".into()),
                rect: Some(Rect {
                    x: 0.0,
                    y: 0.0,
                    w: 100.0,
                    h: 100.0,
                }),
                children: vec![GenomeNode {
                    id: Some("img:2".into()),
                    name: Some("Card".into()),
                    r#type: Some("frame".into()),
                    rect: Some(Rect {
                        x: 0.0,
                        y: 0.0,
                        w: 40.0,
                        h: 40.0,
                    }),
                    snapshot_preview: Some("snap-xyz".into()),
                    ..Default::default()
                }],
                ..Default::default()
            }],
            styles: None,
            images: Default::default(),
        };
        let tree = extract_tree(&genome_preview, None, &TreeOptions::default());
        assert_eq!(
            tree[0].children.as_ref().unwrap()[0]
                .snapshot_hash
                .as_deref(),
            Some("snap-xyz")
        );
    }

    #[test]
    fn tree_skip_empty_groups_drops_empty_leaves() {
        let genome = Genome {
            pages: vec![GenomeNode {
                id: Some("p:0".into()),
                name: Some("Page".into()),
                r#type: Some("page".into()),
                rect: Some(Rect {
                    x: 0.0,
                    y: 0.0,
                    w: 100.0,
                    h: 100.0,
                }),
                children: vec![
                    GenomeNode {
                        id: Some("g:1".into()),
                        name: Some("wrap".into()),
                        r#type: Some("group".into()),
                        rect: Some(Rect {
                            x: 0.0,
                            y: 0.0,
                            w: 50.0,
                            h: 50.0,
                        }),
                        children: vec![GenomeNode {
                            id: Some("t:1".into()),
                            name: Some("Title".into()),
                            r#type: Some("text".into()),
                            rect: Some(Rect {
                                x: 0.0,
                                y: 0.0,
                                w: 50.0,
                                h: 20.0,
                            }),
                            textbox: Some(Textbox {
                                text: Some("Hi".into()),
                                ..Default::default()
                            }),
                            ..Default::default()
                        }],
                        ..Default::default()
                    },
                    GenomeNode {
                        id: Some("g:2".into()),
                        name: Some("empty-leaf".into()),
                        r#type: Some("group".into()),
                        rect: Some(Rect {
                            x: 0.0,
                            y: 60.0,
                            w: 10.0,
                            h: 10.0,
                        }),
                        ..Default::default()
                    },
                    GenomeNode {
                        id: Some("r:1".into()),
                        name: Some("Box".into()),
                        r#type: Some("rectangle".into()),
                        rect: Some(Rect {
                            x: 0.0,
                            y: 80.0,
                            w: 30.0,
                            h: 30.0,
                        }),
                        fills: vec![Fill {
                            r#type: Some("color".into()),
                            color: Some(Color {
                                r: 1.0,
                                g: 2.0,
                                b: 3.0,
                                alpha: None,
                            }),
                            opacity: Some(1.0),
                            ..Default::default()
                        }],
                        ..Default::default()
                    },
                ],
                ..Default::default()
            }],
            styles: None,
            images: Default::default(),
        };
        let options = TreeOptions {
            skip_empty_groups: true,
            ..Default::default()
        };
        let tree = extract_tree(&genome, None, &options);
        let kids = tree[0].children.as_ref().unwrap();
        assert_eq!(kids.len(), 2, "wrapper lifted, empty leaf dropped");
        assert_eq!(kids[0].id, "t:1");
        assert_eq!(kids[1].id, "r:1");

        let kept = extract_tree(&genome, None, &TreeOptions::default());
        assert_eq!(kept[0].children.as_ref().unwrap().len(), 3);
    }

    #[test]
    fn deserialize_real_world_json_shape() {
        // Real genome JSON is camelCase; every field must survive serde.
        let raw = r#"{
            "genomeVer": 1,
            "images": {
                "abc123": { "url": "https://fs.moonvy.com/x.png", "type": "png" }
            },
            "styles": { "fillStyles": [{ "id": "s1", "data": [{ "type": "color", "color": { "r": 10, "g": 20, "b": 30 }, "opacity": 1 }] }] },
            "pages": [{
                "id": "p:0",
                "name": "Home",
                "type": "page",
                "rect": { "x": 0, "y": 0, "w": 1440, "h": 900 },
                "children": [{
                    "id": "n:1",
                    "name": "Card",
                    "type": "frame",
                    "rect": { "x": 10, "y": 20, "w": 200, "h": 100 },
                    "borderRadius": 12,
                    "snapshotPreview": "snap-preview",
                    "fills": [{ "type": "color", "color": { "r": 1, "g": 2, "b": 3 }, "opacity": 1 }],
                    "strokes": [{ "w": 1.5, "fills": [{ "type": "color", "color": { "r": 0, "g": 0, "b": 0 }, "opacity": 1 }] }],
                    "children": [{
                        "id": "t:1",
                        "name": "最近游戏",
                        "type": "text",
                        "rect": { "x": 0, "y": 0, "w": 80, "h": 30 },
                        "textbox": {
                            "text": "最近游戏",
                            "segments": [{
                                "start": 0, "end": 4,
                                "fontSize": 20,
                                "fontName": { "family": "PingFangSC", "style": "Semibold" },
                                "lineHeight": { "unit": "px", "value": 29 },
                                "letterSpacing": { "unit": "px", "value": 0 },
                                "fills": [{ "type": "color", "color": { "r": 255, "g": 255, "b": 255 }, "opacity": 1 }]
                            }]
                        }
                    }]
                }]
            }]
        }"#;
        let genome: Genome = serde_json::from_str(raw).expect("parse real-world genome");

        // camelCase fields that previously got dropped.
        assert_eq!(genome.pages[0].children[0].border_radius, Some(12.0));
        assert_eq!(
            genome.pages[0].children[0].snapshot_preview.as_deref(),
            Some("snap-preview")
        );
        assert_eq!(genome.styles.as_ref().unwrap().fill_styles.len(), 1);
        let node = &genome.pages[0].children[0].children[0];
        assert_eq!(
            node.textbox
                .as_ref()
                .and_then(|t| t.text.clone())
                .as_deref(),
            Some("最近游戏")
        );
        let seg = node.textbox.as_ref().unwrap().segments.first().unwrap();
        assert_eq!(seg.font_size, Some(20.0));
        assert_eq!(
            seg.font_name.as_ref().and_then(|f| f.family.as_deref()),
            Some("PingFangSC")
        );
        assert_eq!(
            seg.font_name.as_ref().and_then(|f| f.style.as_deref()),
            Some("Semibold")
        );
        assert_eq!(seg.line_height.as_ref().and_then(|l| l.value), Some(29.0));
        assert_eq!(seg.letter_spacing.as_ref().and_then(|l| l.value), Some(0.0));
        assert_eq!(
            genome.images.get("abc123").and_then(|i| i.url.as_deref()),
            Some("https://fs.moonvy.com/x.png")
        );

        // Style extraction on the parsed node: typography + stroke + radius.
        let style = extract_raw_node_style(&genome, node);
        assert_eq!(style.font_size, Some(20.0));
        assert_eq!(style.font_weight, Some(600.0), "Semibold -> 600");
        assert_eq!(style.font_family.as_deref(), Some("PingFangSC"));
        assert_eq!(style.line_height, Some(29.0));
        assert_eq!(style.letter_spacing, Some(0.0));
        let card = extract_raw_node_style(&genome, &genome.pages[0].children[0]);
        assert_eq!(card.border_radius, Some(12.0));
        assert_eq!(card.stroke_width, Some(1.5));
        assert_eq!(card.stroke_color.as_deref(), Some("#000000"));
    }

    #[test]
    fn style_to_weight_labels() {
        assert_eq!(style_to_weight(Some("Semibold")), Some(600.0));
        assert_eq!(style_to_weight(Some("Bold")), Some(700.0));
        assert_eq!(style_to_weight(Some("PingFangSC-Regular")), Some(400.0));
        assert_eq!(style_to_weight(Some("Medium")), Some(500.0));
        assert_eq!(style_to_weight(Some("Unknown")), None);
        assert_eq!(style_to_weight(None), None);
    }

    /// Real-API smoke test against a live project directory URL. Exercises
    /// the full user-facing pipeline: page listing, directory auto-resolution,
    /// tree with all noise-reduction options, asset manifest + snapshotHash
    /// linkage, tokens, cross-design diff (normal vs hover) and a real asset
    /// download. Run with:
    ///   cargo test -- --ignored --nocapture real_api_smoke
    #[tokio::test]
    #[ignore = "requires network access and a valid Moonvy token"]
    async fn real_api_smoke() {
        use crate::api::MoonvyApi;
        use crate::token;
        use crate::tools::{
            download_asset, genome_for_url, list_pages, resolve_asset, tree_payload,
        };
        use serde_json::Value;

        let token = match token::load_token() {
            Ok(t) => t,
            Err(e) => panic!("no Moonvy token available ({e}); run moonvy_login first"),
        };
        let api = MoonvyApi::new(token).expect("api client");

        let project = "https://moonvy.com/project/4cf87739-556a-48e7-84f4-bd76f660a48a/75168c2b-5761-4edd-8c94-cd4adaa7d8da";

        // 1. Project directory URL -> design list (previews, no junk fields).
        let rows = list_pages(&api, project, 500, 20)
            .await
            .expect("list_pages");
        assert!(!rows.is_empty(), "project must expose designs");
        assert!(
            rows.iter().all(|r| !r.url.is_empty() && !r.id.is_empty()),
            "every row needs an id and url"
        );
        let profile = rows
            .iter()
            .find(|r| r.name.contains("个人资料") && !r.name.contains("头像移入"))
            .or_else(|| rows.iter().find(|r| r.name.contains("个人资料")))
            .expect("个人资料 design");
        let hover = rows
            .iter()
            .find(|r| r.name.contains("头像移入"))
            .expect("hover design");
        println!(
            "1) designs listed: {} (e.g. \"{}\" | \"{}\")",
            rows.len(),
            profile.name,
            hover.name
        );

        // 2. Directory URL auto-resolves to a design; metadata extraction.
        let (ids, genome) = genome_for_url(&api, project).await.expect("auto-resolve");
        assert!(
            ids.file_id.is_some(),
            "directory URL must resolve to a design file"
        );
        let meta = extract_design_meta(&genome, None);
        assert!(meta.frame_count > 0, "design must expose frames");
        println!(
            "2) auto-resolved: title={} frames={} ({}x{})",
            meta.title, meta.frame_count, meta.frames[0].width, meta.frames[0].height
        );

        // 3. Tree options on a concrete design (the 个人资料 page).
        let (_, genome_a) = genome_for_url(&api, &profile.url).await.expect("genome A");
        let full = extract_tree(&genome_a, None, &TreeOptions::default());
        let full_count: usize = flatten_tree(&full).len();
        let opts = TreeOptions {
            with_style: true,
            skip_empty_groups: true,
            flatten: true,
            detect_duplicates: true,
            ..Default::default()
        };
        let reduced = extract_tree(&genome_a, None, &opts);
        let reduced_count: usize = flatten_tree(&reduced).len();
        let reduced_bytes = serde_json::to_vec(&reduced).expect("serialize reduced");
        println!(
            "3) tree: full={full_count} reduced={reduced_count} ({:.1}% noise removed) | compact+flatten+style+truncate = {} bytes",
            100.0 * (full_count - reduced_count) as f64 / full_count.max(1) as f64,
            reduced_bytes.len()
        );
        assert!(
            reduced_count <= full_count,
            "skipEmptyGroups must not grow the tree"
        );
        assert!(
            reduced_bytes.len() <= 62 * 1024,
            "compact output must stay near the 60KB acceptance target (got {} bytes; the 314-node reference design projects to ~58KB)",
            reduced_bytes.len()
        );

        // Mask semantics: 蒙版 nodes must survive skipEmptyGroups.
        let mask_hits = find_nodes(&genome_a, None, "蒙版", 10);
        println!("   mask groups found: {}", mask_hits.len());
        if !mask_hits.is_empty() {
            let tree_no_skip = extract_tree(&genome_a, None, &opts);
            let kept_masks = flatten_tree(&tree_no_skip)
                .iter()
                .filter(|n| n.name.contains("蒙版"))
                .count();
            println!("   mask groups surviving skipEmptyGroups: {kept_masks}");
            assert!(
                kept_masks >= mask_hits.len(),
                "mask groups must not be dropped (kept {kept_masks} < {})",
                mask_hits.len()
            );
        }

        // 4. includeAssets manifest; every snapshotHash resolves in it.
        let payload = tree_payload(&genome_a, None, &opts, true);
        let bytes = serde_json::to_vec(&payload).expect("serialize payload");
        let manifest: Vec<String> = payload["assets"]
            .as_array()
            .expect("assets")
            .iter()
            .filter_map(|a| a["hash"].as_str().map(str::to_string))
            .collect();
        assert!(!manifest.is_empty(), "asset manifest must not be empty");
        let mut snap_hashes: Vec<String> = Vec::new();
        let mut stack: Vec<&Value> = payload["items"].as_array().expect("items").iter().collect();
        while let Some(v) = stack.pop() {
            if let Some(h) = v["snapshotHash"].as_str() {
                snap_hashes.push(h.to_string());
            }
            if let Some(children) = v["children"].as_array() {
                stack.extend(children.iter());
            }
        }
        println!(
            "4) payload: {} bytes | assets={} snapshotHash nodes={}",
            bytes.len(),
            manifest.len(),
            snap_hashes.len()
        );
        for h in &snap_hashes {
            assert!(
                manifest.contains(h) || genome_a.images.contains_key(h),
                "snapshotHash {h} unresolved"
            );
        }

        // 5. Tokens on real data.
        let tokens = extract_tokens(&genome_a);
        println!(
            "5) tokens: colors={} fontSizes={} radii={} spacing={}",
            tokens.colors.len(),
            tokens.font_sizes.len(),
            tokens.radii.len(),
            tokens.spacing.len()
        );
        assert!(!tokens.colors.is_empty(), "design must expose colors");
        // Text nodes and font-size coverage (diagnostic for token extraction).
        let styled_nodes = flatten_tree(&reduced);
        let text_nodes = styled_nodes.iter().filter(|n| n.r#type == "text").count();
        let with_font_size = styled_nodes
            .iter()
            .filter(|n| n.style.as_ref().is_some_and(|s| s.font_size.is_some()))
            .count();
        println!("   text nodes={text_nodes} nodes with fontSize={with_font_size}");

        // 5b. moonvy_find_node: targeted search instead of a full tree dump.
        let hits = find_nodes(&genome_a, None, "头像", 10);
        println!("5b) find_node(\"头像\") -> {} hits", hits.len());
        assert!(!hits.is_empty(), "头像 must exist in the 个人资料 design");
        let text_hits = find_nodes(&genome_a, None, "设置", 10);
        println!("   find_node(\"设置\") -> {} hits", text_hits.len());

        // 5c. only: ["image"] semantic filter on the real tree.
        let image_opts = TreeOptions {
            only: Some(vec!["image".into()]),
            ..Default::default()
        };
        let image_tree = extract_tree(&genome_a, None, &image_opts);
        fn count_all(nodes: &[TreeNode]) -> usize {
            nodes
                .iter()
                .map(|n| 1 + n.children.as_ref().map(|c| count_all(c)).unwrap_or(0))
                .sum()
        }
        let image_total = count_all(&image_tree);
        let mut raw_image_fills = 0usize;
        for page in &genome_a.pages {
            let mut stack: Vec<&crate::genome::GenomeNode> = vec![page];
            while let Some(n) = stack.pop() {
                if n.fills.iter().any(|f| f.r#type.as_deref() == Some("image")) {
                    raw_image_fills += 1;
                }
                stack.extend(n.children.iter());
            }
        }
        println!(
            "5c) only:[\"image\"] -> {} top nodes / {image_total} total ({} with snapshot); raw image-fill nodes={raw_image_fills}",
            image_tree.len(),
            image_tree
                .iter()
                .filter(|n| n.snapshot_hash.is_some())
                .count()
        );
        assert!(
            image_total >= raw_image_fills && image_total <= raw_image_fills + 2,
            "all image nodes matched, nothing else kept (got {image_total} vs {raw_image_fills})"
        );
        // Duplicated layers must be annotated (the 3x 图像 167 case).
        let duplicates = flatten_tree(&reduced)
            .iter()
            .filter(|n| n.duplicate_of.is_some())
            .count();
        println!("   duplicateOf annotations: {duplicates}");
        assert!(duplicates > 0, "repeated layers must carry duplicateOf");

        // 5d. moonvy_get_asset_url: direct URL without downloading.
        let resolved = resolve_asset(&api, &profile.url, &profile.id, None, None)
            .await
            .expect("resolve asset url");
        assert!(
            resolved.url.starts_with("http"),
            "resolved URL must be absolute"
        );
        println!(
            "5d) asset url: \"{}\" ({} bytes unknown; ext={})",
            resolved.url, resolved.name, resolved.extension
        );

        // 6. Diff 个人资料 (normal) vs 头像移入效果 (hover state).
        let (_, genome_b) = genome_for_url(&api, &hover.url).await.expect("genome B");
        let tree_a = extract_tree(&genome_a, None, &opts);
        let tree_b = extract_tree(&genome_b, None, &opts);
        let diff = diff_trees(&tree_a, &tree_b);
        let a_flat = flatten_tree(&tree_a).len();
        let b_flat = flatten_tree(&tree_b).len();
        println!(
            "6) diff: a={a_flat} b={b_flat} added={} removed={} changed={} (summary: +{} -{} ~{})",
            diff.added.len(),
            diff.removed.len(),
            diff.changed.len(),
            diff.added.len(),
            diff.removed.len(),
            diff.changed.len()
        );
        let paired =
            diff.added.len() < b_flat || diff.removed.len() < a_flat || !diff.changed.is_empty();
        assert!(
            paired,
            "diff must pair some nodes (id or same-name fallback)"
        );
        // Changed rects must be absolute (flatten) — comparable with the tree.
        let changed_rect_ok = diff.changed.iter().all(|c| {
            c.before
                .as_ref()
                .zip(c.after.as_ref())
                .map(|(b, a)| b.x == a.x && b.y == a.y || c.fields.contains(&"rect".to_string()))
                .unwrap_or(true)
        });
        assert!(
            changed_rect_ok,
            "changed nodes carry comparable coordinates"
        );
        if !diff.changed.is_empty() {
            let first = &diff.changed[0];
            println!(
                "   sample change: id={} name=\"{}\" fields={:?}",
                first.id, first.name, first.fields
            );
        }

        // 7. Real asset download to a temp dir (design-level snapshot).
        let out_dir = std::env::temp_dir().join("moonvy-smoke-out");
        std::fs::create_dir_all(&out_dir).expect("tmp dir");
        let (path, size, name, _url, _snapshot_size) = download_asset(
            &api,
            &profile.url,
            &profile.id,
            None,
            None,
            Some("smoke-design"),
            Some(out_dir.to_str().expect("absolute tmp path")),
            false,
        )
        .await
        .expect("download design snapshot");
        println!(
            "7) download: \"{name}\" -> {} ({} bytes)",
            path.display(),
            size
        );
        assert!(size > 0, "downloaded file must not be empty");
        assert!(path.exists(), "downloaded file must exist");
        let _ = std::fs::remove_dir_all(&out_dir);
    }

    #[tokio::test]
    #[ignore = "requires network"]
    async fn login_design_crop_e2e() {
        // End-to-end check of the two biggest real-world pain points:
        // 1. ?design= selector on a directory URL picks the login board.
        // 2. An image fill without an asset reference falls back to the
        //    rendered snapshot and crop=true extracts the node's exact area.
        use crate::api::MoonvyApi;
        use crate::token;
        use crate::tools::{download_asset, genome_for_url, resolve_asset};

        let token = token::load_token().expect("token");
        let api = MoonvyApi::new(token).expect("api");
        // Directory URL + ?design= selector: must resolve to 验证码登录 board.
        let dir_url = "https://moonvy.com/project/4cf87739-556a-48e7-84f4-bd76f660a48a/75168c2b-5761-4edd-8c94-cd4adaa7d8da?design=%E9%AA%8C%E8%AF%81%E7%A0%81%E7%99%BB%E5%BD%95";
        let (ids, genome) = genome_for_url(&api, dir_url).await.expect("genome via ?design=");
        println!(
            "1) ?design=验证码登录 -> board={} frame={}x{}",
            genome.pages[0].name.as_deref().unwrap_or("?"),
            genome.pages[0].rect.as_ref().map(|r| r.w).unwrap_or(0.0),
            genome.pages[0].rect.as_ref().map(|r| r.h).unwrap_or(0.0)
        );
        assert!(
            ids.file_id.as_deref() == Some("a4dd31b7-1381-44dc-ad3c-75532619de23"),
            "must resolve to the 验证码登录 board"
        );
        assert_eq!(
            genome.pages[0].name.as_deref(),
            Some("验证码登录-默认"),
            "board name must be 验证码登录-默认"
        );

        // Locate the left cover image node (dl_bg) inside the board.
        let cover = genome
            .pages
            .iter()
            .find_map(|p| crate::genome::find_node(p, "d1069114-8c39-4903-9e1d-5f8c3fbe8ec7"))
            .expect("cover node");
        println!(
            "3) cover node: \"{}\" rect={:?} snapshot={:?}",
            cover.name.as_deref().unwrap_or("?"),
            cover.rect,
            cover.snapshot.is_some()
        );

        // Asset resolution without explicit type: dl_bg carries a slice, so
        // autodetection must hit the slice branch (this is the "切图" the
        // design exports for the cover).
        let file_url = "https://moonvy.com/project/4cf87739-556a-48e7-84f4-bd76f660a48a/75168c2b-5761-4edd-8c94-cd4adaa7d8da/a4dd31b7-1381-44dc-ad3c-75532619de23";
        let asset = resolve_asset(&api, file_url, &cover.id.clone().unwrap(), None, None)
            .await
            .expect("resolve cover");
        println!(
            "4) resolved(cover): type={} url={}",
            asset.resolved_type, asset.url
        );
        assert!(
            asset.resolved_type == "slice",
            "dl_bg exports a slice, got {}",
            asset.resolved_type
        );
        assert!(asset.url.starts_with("https://"), "slice must have a URL");

        // The QR placeholder image in the WeChat board has neither a slice
        // nor a resolvable image hash: must fall back to the snapshot and
        // expose the node rect for cropping.
        let wechat_url = "https://moonvy.com/project/4cf87739-556a-48e7-84f4-bd76f660a48a/75168c2b-5761-4edd-8c94-cd4adaa7d8da/f168943c-a9ac-419d-bbfe-139b476d52f4";
        let qr = resolve_asset(&api, wechat_url, "3832470b-2b79-4fa3-8306-17daceead742", None, None)
            .await
            .expect("resolve qr");
        println!(
            "4b) resolved(qr): type={} nodeRect={:?} artboard={:?}",
            qr.resolved_type, qr.node_rect, qr.artboard_size
        );
        assert!(
            qr.resolved_type.starts_with("image-fallback-snapshot"),
            "qr must fall back to snapshot, got {}",
            qr.resolved_type
        );
        assert!(qr.node_rect.is_some(), "crop geometry must be exposed");

        // Crop the QR node out of the page snapshot to a temp file.
        let out_dir = std::env::temp_dir().join("moonvy-login-crop");
        let _ = std::fs::remove_dir_all(&out_dir);
        std::fs::create_dir_all(&out_dir).expect("tmp dir");
        let (path, size, name, _url, snap_size) = download_asset(
            &api,
            wechat_url,
            "3832470b-2b79-4fa3-8306-17daceead742",
            None,
            None,
            Some("wechat_qr"),
            Some(out_dir.to_str().expect("absolute tmp path")),
            true,
        )
        .await
        .expect("crop qr");
        println!(
            "5) crop: \"{name}\" -> {} ({} bytes) snapshot={:?}",
            path.display(),
            size,
            snap_size
        );
        assert!(size > 0, "cropped file must not be empty");
        assert!(path.exists(), "cropped file must exist");
        if let Some((sw, sh)) = snap_size {
            println!("   snapshot pixel size: {sw}x{sh}");
            // QR node rect 200x200 at artboard 1440x900; snapshot is 2880x1800
            // (2x), so the cropped PNG must be 400x400.
            let decoded = image::load_from_memory(
                &std::fs::read(&path).expect("read cropped png"),
            )
            .expect("decode cropped png");
            assert_eq!((decoded.width(), decoded.height()), (400, 400));
        }
        // Clean up.
        let _ = std::fs::remove_dir_all(&out_dir);
        let _ = ids;
    }

    #[tokio::test]
    #[ignore = "requires network"]
    async fn repro_issues() {
        use crate::api::MoonvyApi;
        use crate::token;
        use crate::tools::{genome_for_url, list_pages};

        let token = token::load_token().expect("token");
        let api = MoonvyApi::new(token).expect("api");
        let project = "https://moonvy.com/project/4cf87739-556a-48e7-84f4-bd76f660a48a/75168c2b-5761-4edd-8c94-cd4adaa7d8da";
        let rows = list_pages(&api, project, 500, 20).await.expect("rows");
        for r in &rows {
            println!(
                "ROW type={} id={} name={} preview={:?}",
                r.r#type, r.id, r.name, r.preview
            );
        }
        let hover = rows
            .iter()
            .find(|r| r.name.contains("头像移入"))
            .expect("hover");
        let profile = rows
            .iter()
            .find(|r| r.name.contains("个人资料") && !r.name.contains("头像移入"))
            .expect("profile");
        println!("HOVER = {}", hover.url);
        println!("PROFILE = {}", profile.url);

        let (_, genome_b) = genome_for_url(&api, &hover.url).await.expect("genome B");
        let opts = TreeOptions {
            with_style: true,
            skip_empty_groups: true,
            flatten: true,
            detect_duplicates: true,
            ..Default::default()
        };
        let tree = extract_tree(&genome_b, None, &opts);
        let mut log = String::new();
        use std::collections::HashMap;
        let mut counts: HashMap<String, usize> = HashMap::new();
        for n in flatten_tree(&tree) {
            *counts.entry(n.name.clone()).or_default() += 1;
        }
        for (name, c) in counts.iter().filter(|(_, c)| **c > 1) {
            log.push_str(&format!("DUP-NAME [{name}] x{c}\n"));
            for n in flatten_tree(&tree).iter().filter(|n| n.name == *name) {
                log.push_str(&format!(
                    "   id={} dupOf={:?} x={} y={} w={} h={} type={} style={:?}\n",
                    n.id,
                    n.duplicate_of,
                    n.x,
                    n.y,
                    n.width,
                    n.height,
                    n.r#type,
                    n.style.as_ref().map(|s| format!(
                        "bg={:?} r={:?} op={:?} grad={:?}",
                        s.background,
                        s.border_radius,
                        s.opacity,
                        s.gradient
                            .as_ref()
                            .map(|g| (g.r#type.clone(), g.stops.len()))
                    ))
                ));
            }
        }
        let dup_total = flatten_tree(&tree)
            .iter()
            .filter(|n| n.duplicate_of.is_some())
            .count();
        log.push_str(&format!("TOTAL duplicateOf = {dup_total}\n"));

        let mut stack: Vec<&crate::genome::GenomeNode> = genome_b.pages.iter().collect();
        while let Some(n) = stack.pop() {
            if let Some(b) = &n.blend
                && (b.is_mask == Some(true) || b.is_pure_mask == Some(true))
            {
                log.push_str(&format!(
                    "MASK-RAW id={:?} name={:?} type={:?} rect={:?}\n",
                    n.id, n.name, n.r#type, n.rect
                ));
            }
            stack.extend(n.children.iter());
        }

        let (_, genome_a) = genome_for_url(&api, &profile.url).await.expect("genome A");
        let tree_a = extract_tree(&genome_a, None, &opts);
        let tree_b = extract_tree(&genome_b, None, &opts);
        let diff = diff_trees(&tree_a, &tree_b);
        log.push_str(&format!(
            "DIFF added={} removed={} changed={}\n",
            diff.added.len(),
            diff.removed.len(),
            diff.changed.len()
        ));
        let rect_only = diff
            .changed
            .iter()
            .filter(|c| c.fields == vec!["rect"])
            .count();
        log.push_str(&format!("DIFF changed rect-only = {rect_only}\n"));
        for c in &diff.changed {
            log.push_str(&format!(
                "CHG id={} name=[{}] fields={:?} a=[x{} y{} w{} h{}] b=[x{} y{} w{} h{}]\n",
                c.id,
                c.name,
                c.fields,
                c.before.as_ref().map(|n| n.x).unwrap_or_default(),
                c.before.as_ref().map(|n| n.y).unwrap_or_default(),
                c.before.as_ref().map(|n| n.width).unwrap_or_default(),
                c.before.as_ref().map(|n| n.height).unwrap_or_default(),
                c.after.as_ref().map(|n| n.x).unwrap_or_default(),
                c.after.as_ref().map(|n| n.y).unwrap_or_default(),
                c.after.as_ref().map(|n| n.width).unwrap_or_default(),
                c.after.as_ref().map(|n| n.height).unwrap_or_default(),
            ));
        }
        for a in &diff.added {
            log.push_str(&format!(
                "ADD type={} name=[{}] x={} y={} w={} h={}\n",
                a.r#type, a.name, a.x, a.y, a.width, a.height
            ));
        }
        for r in &diff.removed {
            log.push_str(&format!(
                "REM type={} name=[{}] x={} y={} w={} h={}\n",
                r.r#type, r.name, r.x, r.y, r.width, r.height
            ));
        }
        let out = std::path::Path::new("C:\\Users\\ADMINI~1\\AppData\\Local\\Temp\\opencode")
            .join("repro.txt");
        std::fs::write(&out, &log).expect("write log");
        print!("{log}");
    }

    #[test]
    fn absolute_rect_accumulates_ancestor_offsets() {
        let genome = Genome {
            pages: vec![GenomeNode {
                id: Some("page-1".into()),
                name: Some("画板".into()),
                r#type: Some("artboard".into()),
                rect: Some(Rect {
                    x: 100.0,
                    y: 50.0,
                    w: 1440.0,
                    h: 900.0,
                }),
                children: vec![GenomeNode {
                    id: Some("group-1".into()),
                    name: Some("表单".into()),
                    r#type: Some("group".into()),
                    rect: Some(Rect {
                        x: 300.0,
                        y: 200.0,
                        w: 500.0,
                        h: 400.0,
                    }),
                    children: vec![GenomeNode {
                        id: Some("input-1".into()),
                        name: Some("输入框".into()),
                        r#type: Some("rectangle".into()),
                        rect: Some(Rect {
                            x: 20.0,
                            y: 30.0,
                            w: 300.0,
                            h: 44.0,
                        }),
                        ..Default::default()
                    }],
                    ..Default::default()
                }],
                ..Default::default()
            }],
            styles: None,
            images: std::collections::HashMap::new(),
        };

        // Page root offset (100,50) is zeroed, group offset (300,200) +
        // input offset (20,30) accumulate: absolute = (320, 230).
        let rect = absolute_rect(&genome, "input-1").expect("input rect");
        assert_eq!((rect.x, rect.y, rect.w, rect.h), (320.0, 230.0, 300.0, 44.0));

        // The page itself is at (0,0) in flattened space.
        let page = absolute_rect(&genome, "page-1").expect("page rect");
        assert_eq!((page.x, page.y, page.w, page.h), (0.0, 0.0, 1440.0, 900.0));

        // Unknown node id -> None.
        assert!(absolute_rect(&genome, "nope").is_none());
    }

    #[test]
    fn absolute_rect_without_page_offset_keeps_root_at_zero() {
        let genome = Genome {
            pages: vec![GenomeNode {
                id: Some("page-1".into()),
                name: Some("画板".into()),
                r#type: Some("artboard".into()),
                rect: Some(Rect {
                    x: 0.0,
                    y: 0.0,
                    w: 800.0,
                    h: 600.0,
                }),
                children: vec![GenomeNode {
                    id: Some("btn".into()),
                    name: Some("按钮".into()),
                    r#type: Some("rectangle".into()),
                    rect: Some(Rect {
                        x: 100.0,
                        y: 120.0,
                        w: 160.0,
                        h: 48.0,
                    }),
                    ..Default::default()
                }],
                ..Default::default()
            }],
            styles: None,
            images: std::collections::HashMap::new(),
        };
        let rect = absolute_rect(&genome, "btn").expect("btn rect");
        assert_eq!((rect.x, rect.y, rect.w, rect.h), (100.0, 120.0, 160.0, 48.0));
    }
}
