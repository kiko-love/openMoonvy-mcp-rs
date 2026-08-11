/*
 * Moonvy API client: authenticated fetch against global-api.moonvy.com.
 */

use crate::genome::Genome;
use anyhow::{Context, anyhow};
use reqwest::header::{AUTHORIZATION, CONTENT_TYPE};
use serde_json::Value;

pub const API_BASE: &str = "https://global-api.moonvy.com/v2";

#[derive(Debug, Clone)]
pub struct MoonvyApi {
    token: String,
    http: reqwest::Client,
    base: String,
}

#[derive(Debug, Clone, Deserialize, Default)]
#[serde(default)]
pub struct MoonvyNode {
    pub id: String,
    pub name: Option<String>,
    pub parent_id: Option<String>,
    pub is_dir: Option<bool>,
    pub files: Option<Files>,
    pub meta: Option<NodeMeta>,
    pub preview: Option<Preview>,
}

#[derive(Debug, Clone, Deserialize, Default)]
#[serde(default)]
pub struct Preview {
    pub normal: Option<String>,
    pub large: Option<String>,
}

#[derive(Debug, Clone, Deserialize, Default)]
#[serde(default)]
pub struct Files {
    pub genome: Option<GenomeFile>,
    pub file: Option<FileRef>,
}

#[derive(Debug, Clone, Deserialize, Default)]
#[serde(default)]
pub struct GenomeFile {
    pub url: Option<String>,
}

#[derive(Debug, Clone, Deserialize, Default)]
#[serde(default)]
pub struct FileRef {
    pub url: Option<String>,
}

#[derive(Debug, Clone, Deserialize, Default)]
#[serde(default)]
pub struct NodeMeta {
    pub assets: Option<serde_json::Map<String, Value>>,
}

use serde::Deserialize;

impl MoonvyApi {
    pub fn new(token: String) -> anyhow::Result<Self> {
        let http = reqwest::Client::builder()
            .user_agent("moonvy-rs/0.1.0")
            .build()
            .context("failed to build http client")?;
        Ok(Self {
            token,
            http,
            base: API_BASE.to_string(),
        })
    }

    pub async fn get_node(
        &self,
        project_id: &str,
        id: &str,
        lv: &str,
    ) -> anyhow::Result<MoonvyNode> {
        self.post(
            "/anynode/get",
            &serde_json::json!({ "projectId": project_id, "id": id, "lv": lv }),
        )
        .await
    }

    pub async fn list_nodes(
        &self,
        project_id: &str,
        page_index: u32,
        scope_id: Option<&str>,
    ) -> anyhow::Result<Value> {
        let mut body = serde_json::json!({ "projectId": project_id, "pageIndex": page_index });
        if let Some(id) = scope_id {
            body["id"] = serde_json::Value::String(id.to_string());
        }
        self.post("/anynode/list", &body).await
    }

    pub async fn fetch_genome(&self, url: &str) -> anyhow::Result<Genome> {
        let value = self.raw_json(url).await?;
        serde_json::from_value(value).context("genome is not valid JSON")
    }

    /// Fetch a genome URL and parse it as raw JSON (untyped), skipping the
    /// gzip autodetection only when the body is already JSON text.
    pub async fn raw_json(&self, url: &str) -> anyhow::Result<Value> {
        let resp = self
            .http
            .get(url)
            .send()
            .await
            .with_context(|| format!("genome fetch failed: {url}"))?;
        let status = resp.status();
        if !status.is_success() {
            return Err(anyhow!("genome fetch failed: {status}"));
        }
        let bytes = resp.bytes().await.context("failed to read genome body")?;
        if bytes.starts_with(b"{") || bytes.starts_with(b"[") {
            return serde_json::from_slice(&bytes)
                .map_err(|e| anyhow!("genome is not valid JSON: {e}"));
        }
        let decompressed = flate2::read::GzDecoder::new(bytes.as_ref());
        serde_json::from_reader(decompressed).context("failed to decompress genome")
    }

    /// Download a file (asset / preview) into memory.
    pub async fn download_file(&self, url: &str) -> anyhow::Result<Vec<u8>> {
        let resp = self
            .http
            .get(url)
            .send()
            .await
            .with_context(|| format!("download failed: {url}"))?;
        let status = resp.status();
        if !status.is_success() {
            return Err(anyhow!("download failed: HTTP {status}"));
        }
        let bytes = resp.bytes().await.context("failed to read download body")?;
        Ok(bytes.to_vec())
    }

    async fn post<T: serde::de::DeserializeOwned>(
        &self,
        path: &str,
        body: &Value,
    ) -> anyhow::Result<T> {
        let resp = self
            .http
            .post(format!("{}{}", self.base, path))
            .header(CONTENT_TYPE, "application/json")
            .header(AUTHORIZATION, format!("Bearer {}", self.token))
            .json(body)
            .send()
            .await
            .with_context(|| format!("moonvy api {path} request failed"))?;
        let status = resp.status();
        let text = resp.text().await.unwrap_or_default();
        if !status.is_success() {
            if status.as_u16() == 401 {
                return Err(anyhow!(
                    "[AUTH_REQUIRED] moonvy api {path} returned 401. Call moonvy_login, or set MOONVY_TOKEN / run moonvy_set_token."
                ));
            }
            return Err(anyhow!(
                "moonvy api {path} returned {status}: {}",
                text.chars().take(200).collect::<String>()
            ));
        }
        serde_json::from_str(&text).with_context(|| format!("moonvy api {path} returned non-JSON"))
    }
}

/// Moonvy URL: /project/:projectId/:dirId/:fileId
pub struct MoonvyUrl {
    pub project_id: String,
    pub dir_id: Option<String>,
    pub file_id: Option<String>,
}

pub fn parse_moonvy_url(url: &str) -> Option<MoonvyUrl> {
    let segments: Vec<&str> = url.split('/').filter(|s| !s.is_empty()).collect();
    let idx = segments.iter().position(|s| *s == "project")?;
    let rest = &segments[idx + 1..];
    let project_id = rest.first()?.to_string();
    let dir_id = rest.get(1).map(|s| s.to_string());
    let file_id = rest.get(2).map(|s| s.to_string());
    Some(MoonvyUrl {
        project_id,
        dir_id,
        file_id,
    })
}

pub fn file_url_for(project_id: &str, parent_id: Option<&str>, id: &str) -> String {
    match parent_id.filter(|p| *p != id) {
        Some(parent) => format!("https://moonvy.com/project/{project_id}/{parent}/{id}"),
        None => format!("https://moonvy.com/project/{project_id}/{id}"),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parse_moonvy_url_full() {
        let url = parse_moonvy_url("https://moonvy.com/project/p1/d1/f1").unwrap();
        assert_eq!(url.project_id, "p1");
        assert_eq!(url.dir_id.as_deref(), Some("d1"));
        assert_eq!(url.file_id.as_deref(), Some("f1"));
    }

    #[test]
    fn parse_moonvy_url_dir_only() {
        let url = parse_moonvy_url("https://moonvy.com/project/p1/d1").unwrap();
        assert_eq!(url.project_id, "p1");
        assert_eq!(url.dir_id.as_deref(), Some("d1"));
        assert!(url.file_id.is_none());
    }

    #[test]
    fn parse_moonvy_url_invalid() {
        assert!(parse_moonvy_url("https://evil.com/x").is_none());
        assert!(parse_moonvy_url("not-a-url").is_none());
    }

    #[test]
    fn file_url_construction() {
        assert_eq!(
            file_url_for("p", Some("d"), "f"),
            "https://moonvy.com/project/p/d/f"
        );
        assert_eq!(
            file_url_for("p", None, "f"),
            "https://moonvy.com/project/p/f"
        );
    }
}
