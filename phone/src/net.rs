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

pub static RT: LazyLock<tokio::runtime::Runtime> =
    LazyLock::new(|| tokio::runtime::Builder::new_multi_thread().worker_threads(2).enable_all().build().expect("runtime"));
pub static HTTP: LazyLock<reqwest::Client> = LazyLock::new(reqwest::Client::new);
pub static GEN: AtomicU64 = AtomicU64::new(0);

pub fn stop() {
    GEN.fetch_add(1, Ordering::Relaxed);
}

const CALL_FRESH_S: i64 = 75;
const CATCH_UP_MAX_S: i64 = 12 * 3600;

fn start_since(cursor: i64) -> String {
    let now = crate::jx::now_ms() / 1000;
    if cursor > 0 { (cursor - 5).max(now - CATCH_UP_MAX_S).to_string() } else { format!("{CALL_FRESH_S}s") }
}

pub fn listen(env: &mut JNIEnv, ctx: &JObject, topic: String, generation: u64) {
    let alive = move || GEN.load(Ordering::Relaxed) == generation;
    let since = start_since(crate::settings::cursor(env, ctx).unwrap_or(0));
    let (tx, rx) = std::sync::mpsc::channel::<Value>();
    let task = RT.spawn(async move {
        let (mut last_id, mut backoff) = (None::<String>, 2u64);
        while alive() {
            let since = last_id.clone().unwrap_or_else(|| since.clone());
            let r = HTTP.get(format!("https://ntfy.sh/{topic}/json?since={since}")).timeout(Duration::from_secs(24 * 3600)).send().await;
            if let Ok(mut r) = r.and_then(|r| r.error_for_status()) {
                crate::android::log("listening for calls");
                backoff = 2;
                let mut line = Vec::new();
                // ntfy sends a keepalive every ~45 s.
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
        let got = rx.recv_timeout(Duration::from_secs(1));
        crate::jx::ok(env, |env| crate::inbox::auto(env, ctx));
        let Ok(msg) = got else { continue };
        let at = msg["time"].as_i64().unwrap_or(0) * 1000;
        let text = msg["message"].as_str().unwrap_or("").trim().to_string();
        let link = msg["click"].as_str().unwrap_or("").to_string();
        crate::jx::ok(env, |env| match msg["title"].as_str() {
            Some("Contact mode") => crate::settings::received(env, ctx, &text, at),
            Some("Claude is calling") if crate::jx::now_ms() - at < CALL_FRESH_S * 1000 && link.starts_with("https://") => {
                crate::ring::ring(env, ctx, &link, &text)
            }
            Some("Chat reply") => crate::inbox::reply_arrived(env, ctx, &text),
            Some("Chat note") => crate::inbox::note_arrived(env, ctx, &text),
            Some("Chat receipt") => crate::inbox::receipt_arrived(env, ctx, &text),
            _ => Ok(()),
        });
        crate::jx::ok(env, |env| crate::settings::set_cursor(env, ctx, at / 1000));
    }
    task.abort();
}

pub fn hang_up(link: &str) {
    if let Some((base, query)) = link.split_once("/?").filter(|_| link.starts_with("https://")) {
        let url = format!("{base}/decline?{query}");
        RT.spawn(async move {
            let _ = HTTP.post(url).timeout(Duration::from_secs(10)).send().await;
        });
    }
}

fn publish(topic: String, title: &'static str, body: String) {
    RT.spawn(async move {
        let req = HTTP.post(format!("https://ntfy.sh/{topic}")).header("Title", title).header("Priority", "1").body(body);
        let _ = req.timeout(Duration::from_secs(10)).send().await;
    });
}

pub async fn publish_chat(topic: &str, body: String) -> bool {
    let req = HTTP.post(format!("https://ntfy.sh/{topic}")).header("Title", "Chat").header("Priority", "1").body(body);
    req.timeout(Duration::from_secs(10)).send().await.is_ok_and(|r| r.status().is_success())
}

pub async fn upload(topic: &str, name: &str, bytes: Vec<u8>) -> Option<String> {
    let req = HTTP.put(format!("https://ntfy.sh/{topic}")).query(&[("filename", name)]).header("Title", "Chat file").header("Priority", "1");
    let r = req.body(bytes).timeout(Duration::from_secs(120)).send().await.ok()?.error_for_status().ok()?;
    let url = r.json::<Value>().await.ok()?["attachment"]["url"].as_str()?.to_string();
    url.strip_prefix("https://ntfy.sh/file/").is_some_and(|f| !f.is_empty() && f.chars().all(|c| c.is_ascii_alphanumeric() || "._-".contains(c))).then_some(url)
}

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

pub fn publish_mode(topic: String, mode: String) {
    publish(topic, "Contact mode", mode);
}

pub fn request_call(topic: String, agent: String) {
    publish(topic, "Call request", agent);
}

pub fn latest_mode_blocking(topic: &str) -> Option<(String, i64)> {
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
