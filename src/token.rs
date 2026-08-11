/*
 * Moonvy auth token loading: MOONVY_TOKEN env var, or ~/.moonvy-ai/token.json.
 */

use anyhow::Context;
use serde::{Deserialize, Serialize};
use std::path::PathBuf;

#[derive(Debug, Clone, Default, Deserialize, Serialize)]
#[serde(default)]
struct StoredToken {
    token: String,
    expires_at: Option<String>,
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
    if let Ok(token) = std::env::var("MOONVY_TOKEN")
        && !token.trim().is_empty()
    {
        return Ok(token);
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

/// Decode a JWT payload (base64url, no signature verification) to extract exp/userId/email.
pub fn decode_jwt(token: &str) -> Option<JwtPayload> {
    let part = token.split('.').nth(1)?;
    let decoded = base64_url_decode(part)?;
    serde_json::from_str::<JwtPayload>(&decoded).ok()
}

fn base64_url_decode(input: &str) -> Option<String> {
    let mut padded = input.replace('-', "+").replace('_', "/");
    while !padded.len().is_multiple_of(4) {
        padded.push('=');
    }
    use base64::Engine;
    base64::engine::general_purpose::STANDARD
        .decode(padded)
        .ok()
        .map(|bytes| String::from_utf8_lossy(&bytes).to_string())
}

#[derive(Debug, Clone, Default, serde::Deserialize, serde::Serialize)]
pub struct JwtPayload {
    pub exp: Option<i64>,
    pub iat: Option<i64>,
    pub user_id: Option<String>,
    pub email: Option<String>,
    pub r#type: Option<String>,
}

#[derive(Debug, Clone, Default, serde::Serialize)]
pub struct TokenInfo {
    pub saved: bool,
    pub saved_at: String,
    pub expires_at: Option<String>,
    pub days_until_expiry: Option<i64>,
    pub user_id: Option<String>,
    pub email: Option<String>,
}

/// Save a token to the on-disk store (with JWT-derived metadata).
pub fn save_token(token: &str) -> anyhow::Result<TokenInfo> {
    let path = default_token_file();
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent).context("failed to create token directory")?;
    }
    let payload = decode_jwt(token).unwrap_or_default();
    let expires_at = payload.exp.map(|exp| {
        chrono::DateTime::from_timestamp(exp, 0)
            .map(|t| t.to_rfc3339_opts(chrono::SecondsFormat::Millis, true))
            .unwrap_or_default()
    });
    let days_until_expiry = payload.exp.map(|exp| {
        let now = chrono::Utc::now().timestamp();
        (exp - now).max(0) / 86_400
    });
    let stored = StoredToken {
        token: token.to_string(),
        expires_at: expires_at.clone(),
    };
    let json = serde_json::to_string_pretty(&stored)?;
    std::fs::write(&path, format!("{json}\n")).context("failed to write token file")?;
    Ok(TokenInfo {
        saved: true,
        saved_at: chrono::Utc::now().to_rfc3339_opts(chrono::SecondsFormat::Millis, true),
        expires_at,
        days_until_expiry,
        user_id: payload.user_id,
        email: payload.email,
    })
}
