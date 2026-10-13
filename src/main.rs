//! CallMe: lets Claude Code phone the user for a real two-way voice conversation.
//!   callme           MCP server (stdio) plus this session's call web server, reached through the shared tunnel
//!   callme gateway   the one Cloudflare quick tunnel shared by every CallMe server on this PC (started by the first server)
mod call;
mod gateway;
mod mcp;
mod speech;
mod voice;

use std::{io::Write, path::PathBuf, sync::OnceLock};

/// The project folder: page.html, config.json, status.json, voice/, bin/cloudflared.exe.
pub fn dir() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
}

pub struct Config {
    pub topic: String,
    pub token: String,
    pub status_path: PathBuf,
    /// Tests: their own status file, never the phone's setting or the real topic's mode messages.
    pub test_status: bool,
    /// Tests: no tunnel, no phone.
    pub local: bool,
    pub no_gpu: bool,
    /// Which agent is calling (Codex sets CALLME_AGENT=Codex).
    pub agent: String,
}

static CONFIG: OnceLock<Config> = OnceLock::new();
pub fn cfg() -> &'static Config {
    CONFIG.get().expect("config")
}

/// config.json, read on each use so edits (e.g. "debug": true) apply to a running server.
pub fn config_json() -> serde_json::Value {
    std::fs::read_to_string(dir().join("config.json"))
        .ok()
        .and_then(|s| serde_json::from_str(s.trim_start_matches('\u{feff}')).ok())
        .unwrap_or_default()
}

/// Debug log of the phone's mic levels and interrupts: CALLME_DEBUG=1, or "debug": true in config.json.
pub fn debug_on() -> bool {
    std::env::var("CALLME_DEBUG").is_ok_and(|v| v == "1") || config_json()["debug"] == true
}

pub fn now_iso() -> String {
    humantime::format_rfc3339_millis(std::time::SystemTime::now()).to_string()
}

pub fn log(msg: impl AsRef<str>) {
    eprintln!("[callme] {}", msg.as_ref());
}

pub fn log_line(line: &str) {
    if let Ok(mut f) = std::fs::OpenOptions::new().create(true).append(true).open(dir().join("timing.log")) {
        let _ = writeln!(f, "{} {line}", now_iso());
    }
}

/// Starts a helper with no console window that outlives this process (like Node's detached + windowsHide).
/// Its stdin is never ours (the MCP pipe).
pub fn spawn_hidden(cmd: &mut std::process::Command) -> std::io::Result<()> {
    cmd.stdin(std::process::Stdio::null());
    #[cfg(windows)]
    {
        use std::os::windows::process::CommandExt;
        const DETACHED_PROCESS: u32 = 0x8;
        const CREATE_NEW_PROCESS_GROUP: u32 = 0x200;
        const CREATE_NO_WINDOW: u32 = 0x0800_0000;
        cmd.creation_flags(DETACHED_PROCESS | CREATE_NEW_PROCESS_GROUP | CREATE_NO_WINDOW);
    }
    cmd.spawn().map(drop)
}

#[tokio::main]
async fn main() {
    if std::env::args().nth(1).as_deref() == Some("gateway") {
        return gateway::run().await;
    }
    let env = |k: &str| std::env::var(k).ok().filter(|v| !v.is_empty());
    let config_path = dir().join("config.json");
    if !config_path.exists() {
        let topic = format!("claude-{:024x}", rand::random::<u128>() >> 32);
        let _ = std::fs::write(&config_path, serde_json::to_string_pretty(&serde_json::json!({ "topic": topic })).unwrap());
    }
    let topic = env("CALLME_TOPIC").or_else(|| config_json()["topic"].as_str().map(String::from)).expect("config.json has no topic");
    let _ = CONFIG.set(Config {
        topic,
        token: format!("{:032x}", rand::random::<u128>()),
        status_path: env("CALLME_STATUS").map(PathBuf::from).unwrap_or_else(|| dir().join("status.json")),
        test_status: env("CALLME_STATUS").is_some(),
        local: env("CALLME_LOCAL").is_some(),
        no_gpu: env("CALLME_NO_GPU").is_some(),
        agent: env("CALLME_AGENT").map(|a| a.trim().to_string()).unwrap_or_else(|| "Claude".into()),
    });
    let port = call::serve().await;
    if !cfg().local {
        // Make sure the shared gateway (and its tunnel) is up early, so it has settled before any call.
        tokio::spawn(async {
            loop {
                if gateway::ensure_gateway().await {
                    gateway::register().await;
                }
                tokio::time::sleep(std::time::Duration::from_secs(60)).await;
            }
        });
    }
    log(format!("ready on port {port} topic {}", cfg().topic));
    mcp::run().await;
}
