/*
 * openMoonvy-mcp-rs — Moonvy design extraction MCP server (Rust).
 *
 * PoC scope: moonvy_get_design / moonvy_get_tree / moonvy_extract_tokens
 * over the pure Moonvy API (Bearer token, no browser at runtime).
 *
 * Lifecycle: the server exits when (a) stdin closes (normal MCP shutdown),
 * (b) the parent process disappears (watchdog), or (c) the optional idle
 * timeout (MOONVY_IDLE_TIMEOUT_SECS) elapses without any request.
 */

mod api;
mod catalog;
mod genome;
mod login;
mod server;
mod token;
mod tools;

use std::sync::Arc;
use std::time::Duration;

use rmcp::ServiceExt;
use rmcp::transport::stdio;
use sysinfo::{Pid, ProcessesToUpdate, System};

use api::MoonvyApi;
use server::MoonvyServer;

/// Idle timeout env var (seconds, 0/absent = disabled).
const IDLE_TIMEOUT_ENV: &str = "MOONVY_IDLE_TIMEOUT_SECS";
/// Watchdog poll interval.
const WATCHDOG_POLL: Duration = Duration::from_secs(3);
/// Track the last activity via a shared timestamp updated by the tool router.
static LAST_ACTIVITY: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(0);

/// Seconds since the Unix epoch of the last tool call.
pub fn touch_activity() {
    use std::time::{SystemTime, UNIX_EPOCH};
    let now = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_secs())
        .unwrap_or(0);
    LAST_ACTIVITY.store(now, std::sync::atomic::Ordering::Relaxed);
}

fn read_idle_timeout() -> Option<Duration> {
    let raw = std::env::var(IDLE_TIMEOUT_ENV).ok()?;
    let secs: u64 = raw.trim().parse().ok()?;
    (secs > 0).then(|| Duration::from_secs(secs))
}

/// Resolve the parent PID at startup. Prefer MOONVY_PARENT_PID (some MCP
/// hosts pass it explicitly), otherwise query the process table.
fn parent_pid() -> Option<u32> {
    if let Ok(raw) = std::env::var("MOONVY_PARENT_PID") {
        if let Ok(pid) = raw.trim().parse::<u32>() {
            return Some(pid);
        }
    }
    let mut sys = System::new();
    sys.refresh_processes(ProcessesToUpdate::All, true);
    let me = sys.process(Pid::from_u32(std::process::id()))?;
    me.parent().map(|p| p.as_u32())
}

/// Watchdog task: exit the process when the parent disappears, or when the
/// optional idle timeout elapses without any tool call.
fn spawn_watchdog(parent: Option<u32>, idle: Option<Duration>) {
    tokio::spawn(async move {
        let mut sys = System::new();
        loop {
            tokio::time::sleep(WATCHDOG_POLL).await;

            if let Some(ppid) = parent {
                sys.refresh_processes(ProcessesToUpdate::All, true);
                let alive = sys
                    .process(Pid::from_u32(ppid))
                    .is_some_and(|p| p.status() != sysinfo::ProcessStatus::Dead);
                if !alive {
                    eprintln!("[openmoonvy-mcp-rs] parent {ppid} exited, shutting down");
                    std::process::exit(0);
                }
            }

            if let Some(timeout) = idle {
                let now = std::time::SystemTime::now()
                    .duration_since(std::time::UNIX_EPOCH)
                    .map(|d| d.as_secs())
                    .unwrap_or(0);
                let last = LAST_ACTIVITY.load(std::sync::atomic::Ordering::Relaxed);
                if last > 0 && now.saturating_sub(last) >= timeout.as_secs() {
                    eprintln!(
                        "[openmoonvy-mcp-rs] idle {timeout:?} without requests, shutting down"
                    );
                    std::process::exit(0);
                }
            }
        }
    });
}

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    let token = token::load_token()?;
    let api = Arc::new(MoonvyApi::new(token)?);
    let server = MoonvyServer::new(api);

    let parent = parent_pid();
    let idle = read_idle_timeout();
    if parent.is_some() || idle.is_some() {
        spawn_watchdog(parent, idle);
        if let Some(ppid) = parent {
            eprintln!("[openmoonvy-mcp-rs] lifecycle watchdog: parent={ppid}");
        }
    }

    let service = server.serve(stdio()).await?;
    service.waiting().await?;
    Ok(())
}
