/**
 * Frontend workspace catalog: .moonvy-mcp/catalog.json + aliases.json
 * and name-based search (ported from the TypeScript version).
 */

use anyhow::Context;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::path::{Path, PathBuf};

pub const MOONVY_DIR: &str = ".moonvy-mcp";

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(default)]
pub struct CatalogDesign {
    pub id: String,
    pub name: String,
    pub r#type: String,
    pub url: String,
    pub project_id: String,
    pub parent_id: String,
    pub aliases: Vec<String>,
    pub tags: Vec<String>,
    pub last_synced_at: String,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(default)]
pub struct CatalogSource {
    pub name: String,
    pub url: String,
    pub last_synced_at: String,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(default)]
pub struct Catalog {
    pub version: u32,
    pub updated_at: Option<String>,
    pub sources: Vec<CatalogSource>,
    pub designs: Vec<CatalogDesign>,
}

#[derive(Debug, Clone, Default, Serialize)]
pub struct CatalogMatch {
    #[serde(flatten)]
    pub design: CatalogDesign,
    pub score: u32,
    pub match_reason: String,
}

impl Catalog {
    pub fn load(workspace_dir: &Path) -> anyhow::Result<Self> {
        let path = catalog_path(workspace_dir);
        match std::fs::read_to_string(&path) {
            Ok(raw) => serde_json::from_str(&raw).with_context(|| format!("catalog is not valid JSON: {}", path.display())),
            Err(_) => Ok(Self::default()),
        }
    }

    pub fn save(&self, workspace_dir: &Path) -> anyhow::Result<()> {
        let path = catalog_path(workspace_dir);
        std::fs::create_dir_all(path.parent().expect("catalog path has a parent"))
            .with_context(|| format!("failed to create {}", MOONVY_DIR))?;
        let json = serde_json::to_string_pretty(self)?;
        std::fs::write(&path, format!("{json}\n")).with_context(|| format!("failed to write {}", path.display()))
    }
}

fn catalog_path(workspace_dir: &Path) -> PathBuf {
    workspace_dir.join(MOONVY_DIR).join("catalog.json")
}



fn normalize_for_search(value: &str) -> String {
    value.trim().to_lowercase()
}

pub fn search_catalog(catalog: &Catalog, aliases: &HashMap<String, serde_json::Value>, query: &str) -> Vec<CatalogMatch> {
    let q = normalize_for_search(query);
    if q.is_empty() {
        return Vec::new();
    }
    // Collect alias targets that match the query
    let alias_targets: Vec<String> = aliases
        .iter()
        .flat_map(|(from, to)| {
            let targets: Vec<String> = match to {
                serde_json::Value::Array(items) => items.iter().filter_map(|v| v.as_str().map(str::to_string)).collect(),
                serde_json::Value::String(s) => vec![s.clone()],
                _ => Vec::new(),
            };
            let from_matches = normalize_for_search(from).contains(&q);
            let to_matches = targets.iter().any(|t| normalize_for_search(t).contains(&q));
            if from_matches || to_matches { targets } else { Vec::new() }
        })
        .collect();

    let mut matches: Vec<CatalogMatch> = catalog
        .designs
        .iter()
        .filter_map(|design| {
            let fields = [
                design.id.clone(),
                design.name.clone(),
                design.url.clone(),
            ]
            .into_iter()
            .chain(design.aliases.iter().cloned())
            .chain(design.tags.iter().cloned())
            .collect::<Vec<_>>();

            let mut exact = false;
            let mut includes = false;
            for field in &fields {
                let normalized = normalize_for_search(field);
                if normalized == q {
                    exact = true;
                }
                if normalized.contains(&q) {
                    includes = true;
                }
                if exact && includes {
                    break;
                }
            }
            let alias_match = alias_targets.iter().any(|t| normalize_for_search(t) == normalize_for_search(&design.name))
                || design.aliases.iter().any(|alias| alias_targets.iter().any(|t| normalize_for_search(t) == normalize_for_search(alias)))
                || alias_targets.iter().any(|t| normalize_for_search(t) == normalize_for_search(&design.id));

            if !exact && !includes && !alias_match {
                return None;
            }
            let (score, reason) = if exact {
                (100u32, "exact".to_string())
            } else if alias_match {
                (80u32, "alias-map".to_string())
            } else {
                (50u32, "contains".to_string())
            };
            Some(CatalogMatch { design: design.clone(), score, match_reason: reason })
        })
        .collect();
    matches.sort_by(|a, b| b.score.cmp(&a.score).then(a.design.name.cmp(&b.design.name)));
    matches
}

pub fn design_summary(design: &CatalogDesign) -> CatalogDesign {
    design.clone()
}

/// Basic workspace validation: absolute path, must exist as a directory.
pub fn resolve_workspace_dir(dir: &str) -> anyhow::Result<PathBuf> {
    let path = PathBuf::from(dir);
    if !path.is_absolute() {
        anyhow::bail!("workspaceDir must be an absolute path to the real frontend project root.");
    }
    if !path.is_dir() {
        anyhow::bail!("workspaceDir \"{dir}\" does not exist; create the frontend project root first.");
    }
    Ok(path)
}
