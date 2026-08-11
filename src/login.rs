/*
 * Browser-assisted login (moonvy_login): launches Chrome/Edge with a fresh
 * profile and remote debugging, opens moonvy.com, waits for the user to log
 * in, and captures `window.app.api.$options.token` over the Chrome DevTools
 * Protocol. Falls back to manual guidance (moonvy_set_token) on any failure.
 */

use std::path::{Path, PathBuf};
use std::process::{Child, Command, Stdio};
use std::time::{Duration, Instant};

use anyhow::{Context, anyhow, bail};
use futures_util::{SinkExt, StreamExt};
use serde_json::{Value, json};
use tokio::net::TcpStream;
use tokio_tungstenite::MaybeTlsStream;
use tokio_tungstenite::WebSocketStream;
use tokio_tungstenite::connect_async;
use tokio_tungstenite::tungstenite::Message;

pub const LOGIN_URL: &str = "https://moonvy.com/app/login";

/// Evaluated on every poll; returns the auth token or "".
const TOKEN_EXPR: &str = r#"(function(){try{var t=window.app&&window.app.api&&window.app.api.$options?window.app.api.$options.token:'';return typeof t==='string'?t:'';}catch(e){return '';}})()"#;

pub struct LoginOutcome {
    pub method: &'static str,
    pub token_info: crate::token::TokenInfo,
}

type Ws = WebSocketStream<MaybeTlsStream<TcpStream>>;

/// Launch a browser with remote debugging and return the process, profile
/// dir and the debug port (read from DevToolsActivePort).
async fn launch_browser(browser: &Path) -> anyhow::Result<(Child, PathBuf, u16)> {
    let profile = std::env::temp_dir().join(format!("moonvy-login-{}", std::process::id()));
    let _ = std::fs::create_dir_all(&profile);
    let mut child = Command::new(browser)
        .arg("--remote-debugging-port=0")
        .arg(format!("--user-data-dir={}", profile.display()))
        .arg("--no-first-run")
        .arg("--no-default-browser-check")
        .arg("--remote-allow-origins=*")
        .arg(LOGIN_URL)
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .spawn()
        .with_context(|| format!("failed to launch {}", browser.display()))?;

    let port_file = profile.join("DevToolsActivePort");
    let deadline = Instant::now() + Duration::from_secs(15);
    while Instant::now() < deadline {
        if let Ok(raw) = std::fs::read_to_string(&port_file) {
            let port = raw
                .lines()
                .next()
                .and_then(|l| l.trim().parse::<u16>().ok());
            if let Some(port) = port {
                return Ok((child, profile, port));
            }
        }
        tokio::time::sleep(Duration::from_millis(200)).await;
    }
    let _ = child.kill();
    bail!("browser started but the debugging port did not come up");
}

async fn send(
    ws: &mut Ws,
    id: u64,
    session_id: Option<&str>,
    method: &str,
    params: Value,
) -> anyhow::Result<()> {
    let mut msg = json!({ "id": id, "method": method, "params": params });
    if let Some(sid) = session_id {
        msg["sessionId"] = Value::String(sid.to_string());
    }
    ws.send(Message::Text(msg.to_string().into()))
        .await
        .context("failed to send CDP message")?;
    Ok(())
}

/// Read the next response (messages without an id are events, skipped).
async fn recv(ws: &mut Ws) -> anyhow::Result<Value> {
    loop {
        match ws.next().await {
            Some(Ok(Message::Text(t))) => {
                let value: Value = serde_json::from_str(&t)?;
                if value.get("id").is_some() {
                    return Ok(value);
                }
            }
            Some(Ok(_)) => {}
            Some(Err(e)) => return Err(anyhow!(e)),
            None => return Err(anyhow!("debugging socket closed")),
        }
    }
}

/// Poll the login page for a non-empty auth token until the deadline.
async fn capture_token(port: u16, timeout: Duration) -> anyhow::Result<String> {
    let url = format!("ws://127.0.0.1:{port}/devtools/browser");
    let (mut ws, _) = connect_async(&url)
        .await
        .context("failed to connect to the browser debugging socket")?;

    send(
        &mut ws,
        1,
        None,
        "Target.createTarget",
        json!({ "url": LOGIN_URL }),
    )
    .await?;
    let target_id = recv(&mut ws).await?["result"]["targetId"]
        .as_str()
        .map(str::to_string)
        .ok_or_else(|| anyhow!("Target.createTarget failed"))?;

    send(
        &mut ws,
        2,
        None,
        "Target.attachToTarget",
        json!({ "targetId": target_id, "flatten": true }),
    )
    .await?;
    let session_id = recv(&mut ws).await?["result"]["sessionId"]
        .as_str()
        .map(str::to_string)
        .ok_or_else(|| anyhow!("Target.attachToTarget failed"))?;

    let deadline = Instant::now() + timeout;
    let mut id = 3u64;
    loop {
        id += 1;
        send(
            &mut ws,
            id,
            Some(&session_id),
            "Runtime.evaluate",
            json!({ "expression": TOKEN_EXPR, "returnByValue": true }),
        )
        .await?;
        let reply = recv(&mut ws).await?;
        if let Some(token) = reply["result"]["result"]["value"].as_str()
            && !token.is_empty()
        {
            return Ok(token.to_string());
        }
        if Instant::now() >= deadline {
            return Err(anyhow!(
                "Timed out waiting for login. Finish the login in the browser, then retry."
            ));
        }
        tokio::time::sleep(Duration::from_millis(1200)).await;
    }
}

fn which_on_path(name: &str) -> Option<PathBuf> {
    let path = std::env::var_os("PATH")?;
    for dir in std::env::split_paths(&path) {
        let full = dir.join(name);
        if full.is_file() {
            return Some(full);
        }
    }
    None
}

fn find_browser() -> Option<PathBuf> {
    let mut candidates: Vec<PathBuf> = Vec::new();
    #[cfg(windows)]
    {
        for var in ["ProgramFiles(x86)", "ProgramFiles", "LOCALAPPDATA"] {
            for name in [
                "Microsoft/Edge/Application/msedge.exe",
                "Google/Chrome/Application/chrome.exe",
            ] {
                if let Some(dir) = std::env::var_os(var) {
                    candidates.push(PathBuf::from(dir).join(name));
                }
            }
        }
        for name in ["msedge.exe", "chrome.exe", "chromium.exe"] {
            if let Some(p) = which_on_path(name) {
                candidates.push(p);
            }
        }
    }
    #[cfg(target_os = "macos")]
    {
        for name in [
            "Google Chrome.app/Contents/MacOS/Google Chrome",
            "Microsoft Edge.app/Contents/MacOS/Microsoft Edge",
        ] {
            candidates.push(PathBuf::from("/Applications").join(name));
        }
    }
    #[cfg(target_os = "linux")]
    {
        for name in [
            "google-chrome",
            "microsoft-edge",
            "chromium",
            "chromium-browser",
        ] {
            if let Some(p) = which_on_path(name) {
                candidates.push(p);
            }
        }
    }
    candidates.into_iter().find(|p| p.is_file())
}

/// Guided login: drive the browser, capture the token, persist it.
pub async fn login(timeout_ms: u64) -> anyhow::Result<LoginOutcome> {
    let browser = find_browser().ok_or_else(|| {
        anyhow!("No Chrome/Edge browser found to drive. Open {LOGIN_URL}, log in, then run moonvy_set_token with window.app.api.$options.token.")
    })?;
    let timeout = Duration::from_millis(timeout_ms.max(30_000));
    let (mut child, profile, port) = launch_browser(&browser).await?;
    let token = match capture_token(port, timeout).await {
        Ok(token) => token,
        Err(e) => {
            let _ = child.kill();
            let _ = std::fs::remove_dir_all(&profile);
            return Err(e.context("Browser login failed. Fall back to moonvy_set_token (window.app.api.$options.token)."));
        }
    };
    let _ = child.kill();
    let _ = std::fs::remove_dir_all(&profile);
    let token_info = crate::token::save_token(&token)?;
    Ok(LoginOutcome {
        method: "auto-captured",
        token_info,
    })
}
