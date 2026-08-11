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

#[cfg(test)]
mod tests {
    use super::*;

    fn sample_catalog() -> Catalog {
        serde_json::from_str(
            r#"{
            "version": 1,
            "designs": [
              {"id":"1","name":"Home","type":"design","url":"https://moonvy.com/project/1","projectId":"","parentId":"","aliases":[],"tags":[]},
              {"id":"2","name":"Login","type":"design","url":"https://moonvy.com/project/2","projectId":"","parentId":"","aliases":[],"tags":[]},
              {"id":"3","name":"Dashboard","type":"design","url":"https://moonvy.com/project/3","projectId":"","parentId":"","aliases":[],"tags":["overview"]}
            ]
          }"#,
        )
        .unwrap()
    }

    #[test]
    fn search_exact_and_missing() {
        let catalog = sample_catalog();
        let aliases: HashMap<String, serde_json::Value> = HashMap::new();

        let exact = search_catalog(&catalog, &aliases, "Home");
        assert_eq!(exact.len(), 1);
        assert_eq!(exact[0].match_reason, "exact");
        assert_eq!(exact[0].score, 100);

        let missing = search_catalog(&catalog, &aliases, "nope");
        assert!(missing.is_empty());
    }

    #[test]
    fn search_via_aliases() {
        let catalog = sample_catalog();
        let mut aliases: HashMap<String, serde_json::Value> = HashMap::new();
        aliases.insert("home.vue".into(), serde_json::json!("Home"));
        let matches = search_catalog(&catalog, &aliases, "home.vue");
        assert_eq!(matches.len(), 1);
        assert_eq!(matches[0].match_reason, "alias-map");
        assert_eq!(matches[0].score, 80);
    }

    #[test]
    fn search_ranking_exact_over_contains() {
        let catalog = sample_catalog();
        let aliases: HashMap<String, serde_json::Value> = HashMap::new();
        let mut c = catalog.clone();
        c.designs[2].name = "Home Dashboard".to_string();
        let matches = search_catalog(&c, &aliases, "Home");
        assert_eq!(matches[0].score, 100, "exact match ranks first");
        assert_eq!(matches[0].match_reason, "exact");
    }

    #[test]
    fn catalog_roundtrip() {
        let dir = std::env::temp_dir().join(format!("moonvy-cat-{}", std::process::id()));
        std::fs::create_dir_all(dir.join(MOONVY_DIR)).unwrap();
        let catalog = sample_catalog();
        catalog.save(&dir).unwrap();
        let loaded = Catalog::load(&dir).unwrap();
        assert_eq!(loaded.designs.len(), 3);
        assert_eq!(loaded.designs[0].name, "Home");
        std::fs::remove_dir_all(dir).unwrap();
    }
}
