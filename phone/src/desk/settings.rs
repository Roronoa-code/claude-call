use crate::jx::{JNIEnv, JObject, Result};
use serde_json::{Map, Value, json};
use std::sync::{LazyLock, Mutex};

pub const MODES: [&str; 6] = ["call", "text", "both", "none", "wake", "sleep"];

static PREFS: LazyLock<Mutex<Map<String, Value>>> = LazyLock::new(|| Mutex::new(read(&file()).unwrap_or_default()));

fn file() -> std::path::PathBuf {
    crate::android::data_dir().join("prefs.json")
}

fn read(path: &std::path::Path) -> Option<Map<String, Value>> {
    serde_json::from_slice(&std::fs::read(path).ok()?).ok()
}

pub fn get(key: &str) -> Option<Value> {
    PREFS.lock().unwrap().get(key).cloned()
}

pub fn put(pairs: &[(&str, Value)]) {
    let mut p = PREFS.lock().unwrap();
    for (k, v) in pairs {
        p.insert(k.to_string(), v.clone());
    }
    let tmp = file().with_extension("tmp");
    if std::fs::write(&tmp, Value::Object(p.clone()).to_string()).is_ok() {
        let _ = std::fs::rename(tmp, file());
    }
}

fn config(key: &str) -> Option<String> {
    let path = std::env::var("CALLME_CONFIG").unwrap_or_else(|_| r"B:\Projects\CallMe\config.json".into());
    let text = std::fs::read_to_string(path).ok()?;
    let v: Value = serde_json::from_str(text.trim_start_matches('\u{feff}')).ok()?;
    v[key].as_str().map(|s| s.trim().to_string()).filter(|s| !s.is_empty())
}

pub fn topic(_: &mut JNIEnv, _: &JObject) -> Result<String> {
    config("topic").ok_or_else(|| "no topic in config.json".into())
}

pub fn chat_key(_: &mut JNIEnv, _: &JObject) -> Result<Option<String>> {
    Ok(config("chatKey"))
}

pub fn cursor(_: &mut JNIEnv, _: &JObject) -> Result<i64> {
    Ok(get("ntfyCursor").and_then(|v| v.as_i64()).unwrap_or(0))
}

pub fn set_cursor(env: &mut JNIEnv, ctx: &JObject, secs: i64) -> Result<()> {
    if secs > cursor(env, ctx)? {
        put(&[("ntfyCursor", json!(secs))]);
    }
    Ok(())
}

pub fn mode(_: &mut JNIEnv, _: &JObject) -> Result<String> {
    Ok(get("mode").and_then(|v| v.as_str().map(String::from)).unwrap_or_else(|| "both".into()))
}

fn mode_changed_at() -> i64 {
    get("modeChangedAt").and_then(|v| v.as_i64()).unwrap_or(0)
}

pub fn choose(env: &mut JNIEnv, ctx: &JObject, m: &str) -> Result<()> {
    if !MODES.contains(&m) || mode(env, ctx)? == m {
        return Ok(());
    }
    put(&[("mode", json!(m)), ("modeChangedAt", json!(crate::jx::now_ms()))]);
    crate::net::publish_mode(topic(env, ctx)?, m.to_string());
    Ok(())
}

pub fn received(_: &mut JNIEnv, _: &JObject, m: &str, at: i64) -> Result<()> {
    if MODES.contains(&m) && at >= mode_changed_at() - 2_000 {
        put(&[("mode", json!(m)), ("modeChangedAt", json!(at))]);
    }
    Ok(())
}

pub fn hinted(_: &mut JNIEnv, _: &JObject) -> Result<bool> {
    Ok(get("faceHint").is_some())
}

pub fn set_hinted(_: &mut JNIEnv, _: &JObject) -> Result<()> {
    put(&[("faceHint", json!("1"))]);
    Ok(())
}

pub fn missing(_: &mut JNIEnv, _: &JObject) -> Result<String> {
    Ok(String::new())
}

pub fn setup_line(_: &mut JNIEnv, _: &JObject) -> Result<String> {
    Ok(String::new())
}

pub fn set_up(_: &mut JNIEnv, _: &JObject) -> Result<()> {
    Ok(())
}

#[cfg(test)]
mod tests {
    #[test]
    fn prefs_round_trip() {
        let dir = std::env::temp_dir().join(format!("callme-prefs-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        unsafe { std::env::set_var("CALLME_DESK_DATA", &dir) };
        super::put(&[("mode", serde_json::json!("text"))]);
        assert_eq!(super::read(&dir.join("prefs.json")).unwrap()["mode"], "text");
        let _ = std::fs::remove_dir_all(dir);
    }
}
