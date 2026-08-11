/**
 * Moonvy auth token loading: MOONVY_TOKEN env var, or ~/.moonvy-ai/token.json.
 */

use anyhow::Context;
use serde::Deserialize;
use std::path::PathBuf;

#[derive(Debug, Clone, Deserialize)]
#[serde(default)]
struct StoredToken {
    token: String,
    expires_at: Option<String>,
}

impl Default for StoredToken {
    fn default() -> Self {
        Self { token: String::new(), expires_at: None }
    }
}

fn default_token_file() -> PathBuf {
    std::env::var("MOONVY_TOKEN_FILE")
        .map(PathBuf::from)
        .unwrap_or_else(|_| {
            let home = dirs::home_dir().unwrap_or_else(|| PathBuf::from("."));
            home.join(".moonvy-ai").join("token.json")
        })
}

/// Load the auth token. Prefers MOONVY_TOKEN, falls back to the on-disk store.
pub fn load_token() -> anyhow::Result<String> {
    if let Ok(token) = std::env::var("MOONVY_TOKEN") {
        if !token.trim().is_empty() {
            return Ok(token);
        }
    }
    let path = default_token_file();
    let raw = std::fs::read_to_string(&path)
        .with_context(|| format!("no token: set MOONVY_TOKEN or save {}", path.display()))?;
    let stored: StoredToken = serde_json::from_str(&raw).context("token file is not valid JSON")?;
    if stored.token.trim().is_empty() {
        anyhow::bail!("token file has an empty token: {}", path.display());
    }
    Ok(stored.token)
}
