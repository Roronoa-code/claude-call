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

/// Listens until `stop`: rings for fresh calls (ring.rs) and stores contact-mode changes (settings.rs).
/// Reconnects with backoff; after a drop it catches up on what it missed.
pub fn listen(env: &mut JNIEnv, ctx: &JObject, topic: String, generation: u64) {
    let alive = move || GEN.load(Ordering::Relaxed) == generation;
    let (tx, rx) = std::sync::mpsc::channel::<Value>();
    // The network runs on the runtime; the Java calls run here, on the listener's own thread.
    let task = RT.spawn(async move {
        let (mut last_id, mut backoff) = (None::<String>, 2u64);
        while alive() {
            let since = last_id.clone().unwrap_or_else(|| "75s".into());
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
            _ => Ok(()),
        });
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

/// Publishes the contact mode chosen on the phone, so the PC agents follow it (in the background).
pub fn publish_mode(topic: String, mode: String) {
    RT.spawn(async move {
        let req = HTTP.post(format!("https://ntfy.sh/{topic}")).header("Title", "Contact mode").header("Priority", "1").body(mode);
        let _ = req.timeout(Duration::from_secs(10)).send().await;
    });
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
