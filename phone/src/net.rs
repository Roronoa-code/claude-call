//! The app's own networking: the always-on listener for Claude's calls and contact-mode changes on the private ntfy
//! topic, hanging up, and publishing / reading the contact mode.
use crate::settings::MODES;
use jni::{JNIEnv, objects::JObject};
use serde_json::Value;
use std::{
    sync::{
        LazyLock,
        atomic::{AtomicU64, Ordering},
    },
    time::Duration,
};

/// One runtime and one HTTP client for the app's life (the listener and the call engine): a call can end inside it
/// without tearing it down.
pub static RT: LazyLock<tokio::runtime::Runtime> =
    LazyLock::new(|| tokio::runtime::Builder::new_multi_thread().worker_threads(2).enable_all().build().expect("runtime"));
pub static HTTP: LazyLock<reqwest::Client> = LazyLock::new(reqwest::Client::new);
/// The listener generation that should be running; `stop` moves it on, which ends the current one.
pub static GEN: AtomicU64 = AtomicU64::new(0);

pub fn stop() {
    GEN.fetch_add(1, Ordering::Relaxed);
}

/// Where to start reading: from the last message seen (a few seconds back, at most 12 h), else the last 75 s.
/// Replays are safe: calls older than 75 s are ignored, the contact mode checks its time, chat replies dedupe.
fn start_since(cursor: i64) -> String {
    let now = crate::jx::now_ms() / 1000;
    if cursor > 0 { (cursor - 5).max(now - 12 * 3600).to_string() } else { "75s".into() }
}

/// Listens until `stop`: rings for fresh calls (ring.rs), stores contact-mode changes (settings.rs) and files chat
/// replies (inbox.rs). Reconnects with backoff; after a drop it catches up on what it missed.
pub fn listen(env: &mut JNIEnv, ctx: &JObject, topic: String, generation: u64) {
    let alive = move || GEN.load(Ordering::Relaxed) == generation;
    let since = start_since(crate::settings::cursor(env, ctx).unwrap_or(0));
    let (tx, rx) = std::sync::mpsc::channel::<Value>();
    // The network runs on the runtime; the Java calls run here, on the listener's own thread.
    let task = RT.spawn(async move {
        let (mut last_id, mut backoff) = (None::<String>, 2u64);
        while alive() {
            let since = last_id.clone().unwrap_or_else(|| since.clone());
            let r = HTTP.get(format!("https://ntfy.sh/{topic}/json?since={since}")).timeout(Duration::from_secs(24 * 3600)).send().await;
            if let Ok(mut r) = r.and_then(|r| r.error_for_status()) {
                crate::android::log("listening for calls");
                backoff = 2;
                let mut line = Vec::new();
                // ntfy sends a keepalive every ~45 s: silence longer than a minute = a dead link.
                while let Ok(Ok(Some(chunk))) = tokio::time::timeout(Duration::from_secs(60), r.chunk()).await {
                    for &b in chunk.iter() {
                        if b != b'\n' {
                            line.push(b);
                            continue;
                        }
                        if let Ok(msg) = serde_json::from_slice::<Value>(&line) {
                            if msg["event"] == "message" {
                                crate::android::log(&format!("ntfy: {}", msg["title"].as_str().unwrap_or("")));
                                last_id = msg["id"].as_str().map(String::from);
                                let _ = tx.send(msg);
                            }
                        }
                        line.clear();
                    }
                    if !alive() {
                        return;
                    }
                }
            }
            if !alive() {
                return;
            }
            tokio::time::sleep(Duration::from_secs(backoff)).await;
            backoff = (backoff * 2).min(60);
        }
    });
    while alive() {
        let Ok(msg) = rx.recv_timeout(Duration::from_secs(1)) else { continue };
        let at = msg["time"].as_i64().unwrap_or(0) * 1000;
        let text = msg["message"].as_str().unwrap_or("").trim().to_string();
        let link = msg["click"].as_str().unwrap_or("").to_string();
        crate::jx::ok(env, |env| match msg["title"].as_str() {
            Some("Contact mode") => crate::settings::received(env, ctx, &text, at),
            // Claude keeps a call open ~75 s; anything older has already given up.
            Some("Claude is calling") if crate::jx::now_ms() - at < 75_000 && link.starts_with("https://") => {
                crate::ring::ring(env, ctx, &link, &text)
            }
            Some("Chat reply") => crate::inbox::reply_arrived(env, ctx, &text),
            _ => Ok(()),
        });
        crate::jx::ok(env, |env| crate::settings::set_cursor(env, ctx, at / 1000));
    }
    task.abort();
}

/// Tells Claude the call was declined or hung up, so it stops waiting (in the background).
pub fn hang_up(link: &str) {
    if let Some((base, query)) = link.split_once("/?").filter(|_| link.starts_with("https://")) {
        let url = format!("{base}/decline?{query}");
        RT.spawn(async move {
            let _ = HTTP.post(url).timeout(Duration::from_secs(10)).send().await;
        });
    }
}

/// Posts on the private channel for the PC (in the background).
fn publish(topic: String, title: &'static str, body: String) {
    RT.spawn(async move {
        let req = HTTP.post(format!("https://ntfy.sh/{topic}")).header("Title", title).header("Priority", "1").body(body);
        let _ = req.timeout(Duration::from_secs(10)).send().await;
    });
}

/// A signed chat message for the PC; `done` says whether ntfy took it.
pub fn publish_chat(topic: String, body: String, done: impl FnOnce(bool) + Send + 'static) {
    RT.spawn(async move {
        let req = HTTP.post(format!("https://ntfy.sh/{topic}")).header("Title", "Chat").header("Priority", "1").body(body);
        done(req.timeout(Duration::from_secs(10)).send().await.is_ok_and(|r| r.status().is_success()));
    });
}

/// A photo or video the PC sent: an https ntfy.sh link only, at most `max` bytes (stops reading past that).
pub async fn fetch(url: &str, max: u64) -> Option<Vec<u8>> {
    if !url.starts_with("https://ntfy.sh/") {
        return None;
    }
    let mut r = HTTP.get(url).timeout(Duration::from_secs(120)).send().await.ok()?.error_for_status().ok()?;
    if r.content_length().is_some_and(|n| n > max) {
        return None;
    }
    let mut out = Vec::new();
    while let Some(chunk) = r.chunk().await.ok()? {
        out.extend_from_slice(&chunk);
        if out.len() as u64 > max {
            return None;
        }
    }
    Some(out)
}

/// The contact mode chosen on the phone, so the PC agents follow it.
pub fn publish_mode(topic: String, mode: String) {
    publish(topic, "Contact mode", mode);
}

/// The Call button: asks the PC to start gent ("claude" or "codex"), which then calls the phone.
pub fn request_call(topic: String, agent: String) {
    publish(topic, "Call request", agent);
}

/// The latest contact mode set from anywhere in the last 12 h, and when (ms). Blocks: call off the UI thread.
pub fn latest_mode(topic: &str) -> Option<(String, i64)> {
    RT.block_on(async {
        let r = HTTP.get(format!("https://ntfy.sh/{topic}/json?poll=1&since=12h")).timeout(Duration::from_secs(8)).send().await.ok()?;
        let text = r.text().await.ok()?;
        text.lines()
            .filter_map(|l| serde_json::from_str::<Value>(l).ok())
            .filter(|m| m["title"] == "Contact mode" && MODES.contains(&m["message"].as_str().unwrap_or("").trim()))
            .last()
            .map(|m| (m["message"].as_str().unwrap_or("").trim().to_string(), m["time"].as_i64().unwrap_or(0) * 1000))
    })
}
