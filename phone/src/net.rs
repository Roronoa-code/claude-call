//! The app's own networking, called from its Kotlin shell (fyi.mani.callme.Native): the always-on listener for
//! Claude's calls and contact-mode changes on the private ntfy topic, hanging up, and the contact mode itself.
use jni::{
    JNIEnv,
    objects::{JClass, JObject, JString, JValue},
    sys::jstring,
};
use serde_json::Value;
use std::{
    sync::{
        LazyLock,
        atomic::{AtomicBool, Ordering},
    },
    time::{Duration, SystemTime, UNIX_EPOCH},
};

static RT: LazyLock<tokio::runtime::Runtime> =
    LazyLock::new(|| tokio::runtime::Builder::new_multi_thread().worker_threads(1).enable_all().build().expect("runtime"));
static HTTP: LazyLock<reqwest::Client> = LazyLock::new(reqwest::Client::new);
static STOP: AtomicBool = AtomicBool::new(false);
const MODES: [&str; 4] = ["call", "text", "both", "none"];

fn string(env: &mut JNIEnv, s: &JString) -> String {
    env.get_string(s).map(Into::into).unwrap_or_default()
}

fn now_ms() -> u64 {
    SystemTime::now().duration_since(UNIX_EPOCH).unwrap().as_millis() as u64
}

/// Listens until `stop`: rings for fresh calls (ring.rs) and passes contact-mode changes on (`Native.onMode`).
/// Reconnects with backoff; after a drop it catches up on what it missed.
#[unsafe(no_mangle)]
pub extern "system" fn Java_fyi_mani_callme_Native_listen(mut env: JNIEnv, class: JClass, ctx: JObject, topic: JString) {
    let topic = string(&mut env, &topic);
    STOP.store(false, Ordering::Relaxed);
    let (tx, rx) = std::sync::mpsc::channel::<Value>();
    // The network runs on the runtime; the Java callbacks run here, on the service's own thread.
    let task = RT.spawn(async move {
        let (mut last_id, mut backoff) = (None::<String>, 2u64);
        while !STOP.load(Ordering::Relaxed) {
            let since = last_id.clone().unwrap_or_else(|| "75s".into());
            let r = HTTP.get(format!("https://ntfy.sh/{topic}/json?since={since}")).timeout(Duration::from_secs(24 * 3600)).send().await;
            if let Ok(r) = r.and_then(|r| r.error_for_status()) {
                crate::android::log("listening for calls");
                backoff = 2;
                let mut body = r.bytes_stream();
                let mut line = Vec::new();
                use futures_util::StreamExt;
                // ntfy sends a keepalive every ~45 s: silence longer than a minute = a dead link.
                while let Ok(Some(Ok(chunk))) = tokio::time::timeout(Duration::from_secs(60), body.next()).await {
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
                    if STOP.load(Ordering::Relaxed) {
                        return;
                    }
                }
            }
            if STOP.load(Ordering::Relaxed) {
                return;
            }
            tokio::time::sleep(Duration::from_secs(backoff)).await;
            backoff = (backoff * 2).min(60);
        }
    });
    while !STOP.load(Ordering::Relaxed) {
        let Ok(msg) = rx.recv_timeout(Duration::from_secs(1)) else { continue };
        let at = msg["time"].as_u64().unwrap_or(0) * 1000;
        let text = msg["message"].as_str().unwrap_or("").trim().to_string();
        let link = msg["click"].as_str().unwrap_or("").to_string();
        let res = match msg["title"].as_str() {
            Some("Contact mode") if MODES.contains(&text.as_str()) => env.new_string(&text).and_then(|m| {
                let sig = "(Landroid/content/Context;Ljava/lang/String;J)V";
                env.call_static_method(&class, "onMode", sig, &[JValue::Object(&ctx), JValue::Object(&m), JValue::Long(at as i64)]).map(drop)
            }),
            // Claude keeps a call open ~75 s; anything older has already given up.
            Some("Claude is calling") if now_ms().saturating_sub(at) < 75_000 && link.starts_with("https://") => {
                crate::ring::ring(&mut env, &ctx, &link, &text)
            }
            _ => continue,
        };
        if res.is_err() && env.exception_check().unwrap_or(false) {
            let _ = env.exception_clear();
        }
    }
    task.abort();
}

/// Ends `listen` (within a second).
#[unsafe(no_mangle)]
pub extern "system" fn Java_fyi_mani_callme_Native_stop(_env: JNIEnv, _class: JClass) {
    STOP.store(true, Ordering::Relaxed);
}

/// Tells Claude the call was declined or hung up, so it stops waiting (in the background).
#[unsafe(no_mangle)]
pub extern "system" fn Java_fyi_mani_callme_Native_hangUp(mut env: JNIEnv, _class: JClass, link: JString) {
    let link = string(&mut env, &link);
    if let Some((base, query)) = link.split_once("/?").filter(|_| link.starts_with("https://")) {
        let url = format!("{base}/decline?{query}");
        RT.spawn(async move {
            let _ = HTTP.post(url).timeout(Duration::from_secs(10)).send().await;
        });
    }
}

/// Publishes the contact mode chosen on the phone, so the PC agents follow it (in the background).
#[unsafe(no_mangle)]
pub extern "system" fn Java_fyi_mani_callme_Native_publishMode(mut env: JNIEnv, _class: JClass, topic: JString, mode: JString) {
    let (topic, mode) = (string(&mut env, &topic), string(&mut env, &mode));
    RT.spawn(async move {
        let req = HTTP.post(format!("https://ntfy.sh/{topic}")).header("Title", "Contact mode").header("Priority", "1").body(mode);
        let _ = req.timeout(Duration::from_secs(10)).send().await;
    });
}

/// The latest contact mode set from anywhere in the last 12 h, as "mode atMs", or null. Blocks: call off the UI thread.
#[unsafe(no_mangle)]
pub extern "system" fn Java_fyi_mani_callme_Native_latestMode(mut env: JNIEnv, _class: JClass, topic: JString) -> jstring {
    let topic = string(&mut env, &topic);
    let latest = RT.block_on(async {
        let r = HTTP.get(format!("https://ntfy.sh/{topic}/json?poll=1&since=12h")).timeout(Duration::from_secs(8)).send().await.ok()?;
        let text = r.text().await.ok()?;
        text.lines()
            .filter_map(|l| serde_json::from_str::<Value>(l).ok())
            .filter(|m| m["title"] == "Contact mode" && MODES.contains(&m["message"].as_str().unwrap_or("").trim()))
            .last()
            .map(|m| format!("{} {}", m["message"].as_str().unwrap_or("").trim(), m["time"].as_u64().unwrap_or(0) * 1000))
    });
    match latest.and_then(|s| env.new_string(s).ok()) {
        Some(s) => s.into_raw(),
        None => std::ptr::null_mut(),
    }
}

/// Rings for a call (also the home screen's test ring).
#[unsafe(no_mangle)]
pub extern "system" fn Java_fyi_mani_callme_Native_ring(mut env: JNIEnv, _class: JClass, ctx: JObject, link: JString, reason: JString) {
    let (link, reason) = (string(&mut env, &link), string(&mut env, &reason));
    if crate::ring::ring(&mut env, &ctx, &link, &reason).is_err() && env.exception_check().unwrap_or(false) {
        let _ = env.exception_clear();
    }
}

/// Stops ringing (answered, declined, or the call screen was touched).
#[unsafe(no_mangle)]
pub extern "system" fn Java_fyi_mani_callme_Native_stopRinging(mut env: JNIEnv, _class: JClass, ctx: JObject, handled: jni::sys::jboolean) {
    if crate::ring::stop(&mut env, &ctx, handled != 0).is_err() && env.exception_check().unwrap_or(false) {
        let _ = env.exception_clear();
    }
}
