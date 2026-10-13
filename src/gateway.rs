//! Shared tunnel gateway: ONE Cloudflare quick tunnel for every CallMe server on this PC (one per Claude/Codex session).
//! trycloudflare rate-limits new tunnels (HTTP 429), so servers don't open their own: each registers {token, port}
//! here, and public requests are passed to the server whose call token (?t=) they carry, streamed straight through.
//! Started hidden and detached by the first server that needs it; exits after 2 h with no servers registered.
//!   Control (127.0.0.1:47810, never reachable through the tunnel):
//!     GET  /health             -> { url, bornAt, backends }   url is null while the tunnel is (re)starting;
//!                                 bornAt = when it became reachable (servers wait ~45 s after that before ringing)
//!     POST /register {token, port}   servers repeat it every minute; entries lapse after 3 minutes
//!   Public (127.0.0.1:47809, what the tunnel points at): everything else.
//! Log: gateway.log
use crate::{call::PORT, cfg, dir, now_iso, voice::HTTP};
use axum::{body::Body, extract::Request, http::StatusCode, response::IntoResponse};
use futures_util::TryStreamExt;
use serde_json::{Value, json};
use std::{
    collections::HashMap,
    io::Write,
    sync::{LazyLock, Mutex},
    time::{Duration, Instant, SystemTime, UNIX_EPOCH},
};
use tokio::{io::AsyncBufReadExt, process::Command, sync::mpsc};

const CONTROL: &str = "http://127.0.0.1:47810";
const TUNNEL_SETTLE_MS: u64 = 45_000;

// ---- Server side: find (or start) the gateway and register with it. ----
async fn gateway_info() -> Option<Value> {
    let r = HTTP.get(format!("{CONTROL}/health")).timeout(Duration::from_secs(2)).send().await.ok()?;
    r.status().is_success().then_some(())?;
    serde_json::from_str(&r.text().await.ok()?).ok()
}

static STARTING: tokio::sync::Mutex<()> = tokio::sync::Mutex::const_new(());

pub async fn ensure_gateway() -> bool {
    if gateway_info().await.is_some() {
        return true;
    }
    let _one_at_a_time = STARTING.lock().await;
    if gateway_info().await.is_some() {
        return true;
    }
    // Detached and hidden: no console window, and it outlives this session for the others.
    if let Ok(exe) = std::env::current_exe() {
        let mut cmd = std::process::Command::new(exe);
        cmd.arg("gateway").current_dir(dir()).stdout(std::process::Stdio::null()).stderr(std::process::Stdio::null());
        let _ = crate::spawn_hidden(&mut cmd);
    }
    for _ in 0..20 {
        if gateway_info().await.is_some() {
            return true;
        }
        tokio::time::sleep(Duration::from_millis(250)).await;
    }
    gateway_info().await.is_some()
}

pub async fn register() {
    let body = json!({ "token": cfg().token, "port": PORT.get() }).to_string();
    let _ = HTTP.post(format!("{CONTROL}/register")).body(body).timeout(Duration::from_secs(2)).send().await;
}

fn now_ms() -> u64 {
    SystemTime::now().duration_since(UNIX_EPOCH).unwrap().as_millis() as u64
}

/// The public address of this call server. A new trycloudflare address takes ~30 s to resolve on the phone, and a
/// too-early lookup gets cached as "not found", so calls only ring once the tunnel is TUNNEL_SETTLE_MS old.
pub async fn ensure_tunnel() -> Result<String, String> {
    if cfg().local {
        return Ok(format!("http://127.0.0.1:{}", PORT.get().unwrap()));
    }
    let until = Instant::now() + Duration::from_secs(120);
    while Instant::now() < until {
        if ensure_gateway().await {
            register().await;
            if let Some(g) = gateway_info().await.filter(|g| g["url"].is_string()) {
                let wait = (g["bornAt"].as_u64().unwrap_or(0) + TUNNEL_SETTLE_MS).saturating_sub(now_ms());
                tokio::time::sleep(Duration::from_millis(wait)).await;
                if gateway_info().await.is_some_and(|n| n["url"] == g["url"]) {
                    return Ok(g["url"].as_str().unwrap().to_string()); // still the same tunnel after settling
                }
                continue;
            }
        }
        tokio::time::sleep(Duration::from_secs(2)).await; // the gateway is (re)opening its tunnel; it retries by itself
    }
    Err("could not open a tunnel (trycloudflare busy or offline; see gateway.log); try again in a few minutes".into())
}

// ---- The gateway process. ----
struct Backend {
    port: u16,
    seen: Instant,
}
static BACKENDS: LazyLock<Mutex<HashMap<String, Backend>>> = LazyLock::new(Default::default);
static TUNNEL: Mutex<(Option<String>, u64)> = Mutex::new((None, 0)); // url, bornAt
static CHILD_PID: Mutex<Option<u32>> = Mutex::new(None);

fn glog(msg: &str) {
    if let Ok(mut f) = std::fs::OpenOptions::new().create(true).append(true).open(dir().join("gateway.log")) {
        let _ = writeln!(f, "{} {msg}", now_iso());
    }
}

fn kill_tunnel() {
    if let Some(pid) = *CHILD_PID.lock().unwrap() {
        let _ = std::process::Command::new("taskkill").args(["/F", "/PID", &pid.to_string()]).output();
    }
}

pub async fn run() {
    // Both ports first: if another gateway already has them, this one just exits (no second tunnel).
    let (Ok(control), Ok(public)) =
        (tokio::net::TcpListener::bind("127.0.0.1:47810").await, tokio::net::TcpListener::bind("127.0.0.1:47809").await)
    else {
        return;
    };
    let _ = std::fs::write(dir().join("gateway.log"), "");
    glog(&format!("gateway up, pid {}", std::process::id()));
    tokio::spawn(keep_tunnel());
    // Forget servers that stopped checking in; leave (and free the tunnel) after a long time with none.
    tokio::spawn(async {
        let mut last_backend = Instant::now();
        loop {
            tokio::time::sleep(Duration::from_secs(60)).await;
            let mut b = BACKENDS.lock().unwrap();
            b.retain(|_, v| {
                let keep = v.seen.elapsed() < Duration::from_secs(180);
                if !keep {
                    glog(&format!("server gone, port {}", v.port));
                }
                keep
            });
            if !b.is_empty() {
                last_backend = Instant::now();
            } else if last_backend.elapsed() > Duration::from_secs(2 * 60 * 60) {
                glog("idle, exiting");
                kill_tunnel();
                std::process::exit(0);
            }
        }
    });
    let control_app = axum::Router::new().fallback(control_route);
    let public_app = axum::Router::new().fallback(proxy);
    tokio::spawn(async move { axum::serve(control, control_app).await });
    let _ = axum::serve(public, public_app).await;
}

async fn control_route(req: Request) -> axum::response::Response {
    match (req.method().as_str(), req.uri().path()) {
        ("GET", "/health") => {
            let (url, born_at) = TUNNEL.lock().unwrap().clone();
            let n = BACKENDS.lock().unwrap().len();
            ([("content-type", "application/json")], json!({ "url": url, "bornAt": born_at, "backends": n }).to_string()).into_response()
        }
        ("POST", "/register") => {
            let Ok(body) = axum::body::to_bytes(req.into_body(), 1000).await else { return StatusCode::BAD_REQUEST.into_response() };
            let v: Value = serde_json::from_slice(&body).unwrap_or_default();
            let token = v["token"].as_str().unwrap_or("");
            let Some(port) = v["port"].as_u64().and_then(|p| u16::try_from(p).ok()) else { return StatusCode::BAD_REQUEST.into_response() };
            if token.len() != 32 || !token.chars().all(|c| c.is_ascii_hexdigit() && !c.is_ascii_uppercase()) {
                return StatusCode::BAD_REQUEST.into_response();
            }
            let mut b = BACKENDS.lock().unwrap();
            if !b.contains_key(token) {
                glog(&format!("server registered on port {port}"));
            }
            b.insert(token.into(), Backend { port, seen: Instant::now() });
            StatusCode::NO_CONTENT.into_response()
        }
        _ => StatusCode::NOT_FOUND.into_response(),
    }
}

/// Passes a public request to the server its call token belongs to, streamed both ways (the voice is never held back).
async fn proxy(req: Request) -> axum::response::Response {
    if req.uri().path() == "/__gateway" {
        return "ok".into_response(); // reachability probe
    }
    let token = req.uri().query().unwrap_or("").split('&').find_map(|kv| kv.strip_prefix("t=")).map(String::from);
    let port = {
        let b = BACKENDS.lock().unwrap();
        // Calls carry their token; untokened requests are the call page's public script, which any server can give.
        match &token {
            Some(t) => b.get(t).map(|v| v.port),
            None => b.values().max_by_key(|v| v.seen).map(|v| v.port),
        }
    };
    let Some(port) = port else { return StatusCode::NOT_FOUND.into_response() };
    let (parts, body) = req.into_parts();
    let path = parts.uri.path_and_query().map_or("/", |p| p.as_str());
    let mut out = HTTP.request(parts.method.clone(), format!("http://127.0.0.1:{port}{path}"));
    for (k, v) in &parts.headers {
        if !matches!(k.as_str(), "host" | "connection" | "content-length" | "transfer-encoding") {
            out = out.header(k, v);
        }
    }
    let out = out.body(reqwest::Body::wrap_stream(body.into_data_stream()));
    match out.send().await {
        Ok(back) => {
            let mut res = axum::response::Response::builder().status(back.status().as_u16());
            for (k, v) in back.headers() {
                if !matches!(k.as_str(), "connection" | "transfer-encoding") {
                    res = res.header(k, v);
                }
            }
            res.body(Body::from_stream(back.bytes_stream().map_err(std::io::Error::other))).unwrap()
        }
        Err(_) => StatusCode::BAD_GATEWAY.into_response(),
    }
}

/// The tunnel: started once and kept; restarted if it dies. Backs off when trycloudflare says 429.
async fn keep_tunnel() {
    let mut wait = 0u64;
    loop {
        if wait > 0 {
            glog(&format!("tunnel retry in {}s", wait / 1000));
            tokio::time::sleep(Duration::from_millis(wait)).await;
        }
        match open_tunnel().await {
            Ok(mut child) => {
                wait = 0;
                let _ = child.wait().await; // runs until it dies
                glog("tunnel died; restarting");
                *TUNNEL.lock().unwrap() = (None, 0);
            }
            Err(e) => {
                glog(&format!("tunnel failed: {e}"));
                wait = (wait * 2).max(if e.contains("429") { 30_000 } else { 5_000 }).min(5 * 60_000);
            }
        }
    }
}

fn find_url(s: &str) -> Option<String> {
    s.match_indices("https://").find_map(|(i, _)| {
        let rest = &s[i + 8..];
        let host = &rest[..rest.find(".trycloudflare.com")?];
        (!host.is_empty() && host.chars().all(|c| c.is_ascii_lowercase() || c.is_ascii_digit() || c == '-'))
            .then(|| format!("https://{host}.trycloudflare.com"))
    })
}

fn find_failure(s: &str) -> Option<String> {
    if let Some(i) = s.find("failed with status ") {
        let code: String = s[i + 19..].chars().take_while(char::is_ascii_digit).collect();
        if !code.is_empty() {
            return Some(format!("failed with status {code}"));
        }
    }
    s.find("provisioning failed").map(|i| s[i..].lines().next().unwrap_or("").to_string())
}

async fn open_tunnel() -> Result<tokio::process::Child, String> {
    let mut cmd = Command::new(dir().join("bin/cloudflared.exe"));
    cmd.args(["tunnel", "--no-autoupdate", "--url", "http://127.0.0.1:47809"])
        .stdin(std::process::Stdio::null())
        .stdout(std::process::Stdio::piped())
        .stderr(std::process::Stdio::piped())
        .kill_on_drop(true);
    #[cfg(windows)]
    cmd.creation_flags(0x0800_0000); // CREATE_NO_WINDOW
    let mut child = cmd.spawn().map_err(|e| e.to_string())?;
    *CHILD_PID.lock().unwrap() = child.id();
    // Keep reading both outputs for the tunnel's whole life, or cloudflared blocks on a full pipe.
    let (tx, mut rx) = mpsc::unbounded_channel::<String>();
    let out = child.stdout.take().unwrap();
    let err = child.stderr.take().unwrap();
    for pipe in [Box::new(out) as Box<dyn tokio::io::AsyncRead + Unpin + Send>, Box::new(err)] {
        let tx = tx.clone();
        tokio::spawn(async move {
            let mut lines = tokio::io::BufReader::new(pipe).lines();
            while let Ok(Some(l)) = lines.next_line().await {
                let _ = tx.send(l);
            }
        });
    }
    drop(tx);
    let found = tokio::time::timeout(Duration::from_secs(30), async {
        while let Some(l) = rx.recv().await {
            if let Some(u) = find_url(&l) {
                return Ok(u);
            }
            if let Some(f) = find_failure(&l) {
                return Err(f);
            }
        }
        Err("cloudflared exited".to_string())
    })
    .await
    .map_err(|_| "tunnel did not start in 30s".to_string())??;
    // A new quick tunnel takes a moment before its name resolves publicly.
    for _ in 0..40 {
        let probe = HTTP.get(format!("{found}/__gateway")).timeout(Duration::from_secs(5)).send().await;
        if probe.is_ok_and(|r| r.status().is_success()) {
            break;
        }
        tokio::time::sleep(Duration::from_millis(1500)).await;
    }
    *TUNNEL.lock().unwrap() = (Some(found.clone()), now_ms());
    glog(&format!("tunnel {found}"));
    Ok(child)
}
