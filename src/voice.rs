//! Voice and hearing (the GPU server voice/voice_server.py), the ElevenLabs fallback voice, ringing through ntfy,
//! and the user's contact mode.
use crate::{cfg, dir, log, now_iso};
use serde_json::{Value, json};
use std::{sync::LazyLock, time::Duration};

pub static HTTP: LazyLock<reqwest::Client> = LazyLock::new(reqwest::Client::new);

// ---- GPU voice + hearing: the user's own voice cloned by Qwen3-TTS, and Whisper large. One instance is shared by
// all sessions and exits itself after 30 idle minutes.
const QWEN: &str = "http://127.0.0.1:47811";

pub async fn qwen_up() -> bool {
    !cfg().no_gpu
        && HTTP.get(format!("{QWEN}/health")).timeout(Duration::from_secs(2)).send().await.is_ok_and(|r| r.status().is_success())
}

static LOADING: tokio::sync::Mutex<()> = tokio::sync::Mutex::const_new(());

/// Starts the GPU server if needed (40-70 s); false on machines without it (e.g. the laptop).
pub async fn load_qwen() -> bool {
    let _one_at_a_time = LOADING.lock().await;
    if qwen_up().await {
        return true;
    }
    let voice = dir().join("voice");
    let pythonw = voice.join(".venv/Scripts/pythonw.exe");
    if cfg().no_gpu || !voice.join("voice.json").exists() || !pythonw.exists() {
        return false;
    }
    let out = std::fs::File::create(voice.join("voice_server.log"));
    let mut cmd = std::process::Command::new(pythonw);
    cmd.args(["voice_server.py", "47811"]).current_dir(&voice);
    if let Ok(out) = out {
        cmd.stdout(out.try_clone().unwrap()).stderr(out);
    }
    if let Err(e) = crate::spawn_hidden(&mut cmd) {
        log(format!("gpu voice did not start: {e}"));
        return false;
    }
    for _ in 0..180 {
        tokio::time::sleep(Duration::from_secs(1)).await;
        if qwen_up().await {
            return true;
        }
    }
    log("gpu voice did not start; see voice/voice_server.log");
    false
}

/// Calls the GPU server; if it has exited (idle timeout), starts it again once.
pub async fn gpu(route: &str, body: Vec<u8>, context: Option<&str>) -> Option<reqwest::Response> {
    for _ in 0..2 {
        if !load_qwen().await {
            return None;
        }
        let mut req = HTTP.post(format!("{QWEN}{route}")).body(body.clone());
        if let Some(c) = context {
            req = req.query(&[("context", c)]);
        }
        match req.send().await {
            Ok(r) if r.status().is_success() => return Some(r),
            Ok(r) => {
                log(format!("gpu {route} {}", r.status()));
                return None;
            }
            Err(_) => {} // it exited between the health check and the call: start it again
        }
    }
    None
}

fn f32_bytes(pcm: &[f32]) -> Vec<u8> {
    pcm.iter().flat_map(|x| x.to_le_bytes()).collect()
}

/// Speech-to-text, prompted with what Claude just said so names are heard right.
/// ponytail: no CPU fallback (the old small-Whisper path); without the GPU server the user types instead.
pub async fn hear(pcm: &[f32], context: &str) -> String {
    match gpu("/stt", f32_bytes(pcm), Some(context)).await {
        Some(r) => r.text().await.unwrap_or_default().trim().to_string(),
        None => {
            log("no hearing: the GPU voice server is not available");
            String::new()
        }
    }
}

/// Live captions: GPU only and never starts it; None = skipped.
pub async fn hear_quick(pcm: &[f32], context: &str) -> Option<String> {
    if !qwen_up().await {
        return None;
    }
    Some(gpu("/stt", f32_bytes(pcm), Some(context)).await?.text().await.ok()?.trim().to_string())
}

/// Whisper invents these on near-silence.
pub fn is_noise(text: &str) -> bool {
    let t = text.trim().to_lowercase();
    t.is_empty()
        || (t.starts_with('[') && t.ends_with(']'))
        || (t.starts_with('(') && t.ends_with(')'))
        || ["you", "thank you", "thank you.", "thanks for watching", "thanks for watching!"].contains(&t.as_str())
        || t.chars().all(|c| c == '.')
}

pub struct Audio {
    pub data: Vec<u8>,
    pub kind: &'static str,
}

/// ElevenLabs, for machines without a GPU (e.g. the laptop): the user's own ElevenLabs voice, as 24 kHz WAV.
/// Needs ELEVENLABS_API_KEY and ELEVENLABS_VOICE_ID.
async fn eleven(text: &str) -> Option<Audio> {
    let key = std::env::var("ELEVENLABS_API_KEY").ok()?;
    let voice = std::env::var("ELEVENLABS_VOICE_ID").ok()?;
    let model = std::env::var("ELEVENLABS_MODEL").unwrap_or_else(|_| "eleven_flash_v2_5".into());
    let r = HTTP
        .post(format!("https://api.elevenlabs.io/v1/text-to-speech/{voice}?output_format=pcm_24000"))
        .header("xi-api-key", key)
        .header("content-type", "application/json")
        .body(json!({ "text": crate::speech::without_tags(text), "model_id": model }).to_string())
        .send()
        .await
        .ok()?;
    if !r.status().is_success() {
        log(format!("elevenlabs {} {}", r.status(), r.text().await.unwrap_or_default().chars().take(200).collect::<String>()));
        return None;
    }
    Some(Audio { data: wav(&r.bytes().await.ok()?, 24_000), kind: "audio/wav" })
}

/// A whole-file line: the cloned voice if the GPU server is up, else ElevenLabs; None = the phone's own voice.
pub async fn synth(text: &str) -> Option<Audio> {
    if qwen_up().await {
        if let Some(r) = gpu("/tts", text.as_bytes().to_vec(), None).await {
            if let Ok(b) = r.bytes().await {
                return Some(Audio { data: b.to_vec(), kind: "audio/wav" });
            }
        }
    }
    eleven(text).await
}

// ---- Ringing: a generic alert on the private ntfy topic; the phone app rings on the title "Claude is calling". ----
pub async fn ring(link: &str, reason: Option<&str>) {
    let agent = &cfg().agent;
    // ponytail: message text is not sent through ntfy (public server); only a generic alert + private link.
    let body = match reason {
        Some(r) => format!("{agent} needs you: {r}"),
        None => format!("{agent} needs to talk to you"),
    };
    let r = HTTP
        .post(format!("https://ntfy.sh/{}", cfg().topic))
        .header("Title", "Claude is calling")
        .header("Priority", "5")
        .header("Tags", "telephone_receiver")
        .header("Click", link)
        .header("Actions", format!("view, Answer, {link}"))
        .body(body)
        .send()
        .await;
    if let Err(e) = r {
        log(format!("ntfy failed {e}"));
    }
}

// ---- Contact mode, shared by every agent (Discord Input MCP reads the same file):
// "call" = calls only, "text" = Discord only, "both", "none" = don't contact.
pub const MODES: [&str; 4] = ["call", "text", "both", "none"];

pub fn read_status() -> Value {
    std::fs::read_to_string(&cfg().status_path)
        .ok()
        .and_then(|s| serde_json::from_str(&s).ok())
        .unwrap_or_else(|| json!({ "mode": "both", "updatedAt": "1970-01-01T00:00:00.000Z", "by": "default" }))
}

pub fn write_status(mode: &str, updated_at: String, by: &str) {
    let s = json!({ "mode": mode, "updatedAt": updated_at, "by": by });
    let _ = std::fs::write(&cfg().status_path, serde_json::to_string_pretty(&s).unwrap());
}

fn iso_ms(s: &str) -> u64 {
    humantime::parse_rfc3339_weak(s).ok().and_then(|t| t.duration_since(std::time::UNIX_EPOCH).ok()).map_or(0, |d| d.as_millis() as u64)
}

/// The last 12 hours of messages on the ntfy topic (what ntfy keeps), one JSON object per line.
async fn ntfy_recent() -> Option<String> {
    let r = HTTP
        .get(format!("https://ntfy.sh/{}/json?poll=1&since=12h", cfg().topic))
        .timeout(Duration::from_secs(4))
        .send()
        .await
        .ok()?;
    r.text().await.ok()
}

/// The phone app's Call button publishes "Call request"; one from the last 5 minutes means the user asked for this call.
fn call_requested_in(lines: &str, now_secs: u64) -> bool {
    lines
        .lines()
        .filter_map(|l| serde_json::from_str::<Value>(l).ok())
        .any(|m| m["event"] == "message" && m["title"] == "Call request" && m["time"].as_u64().is_some_and(|t| t + 300 >= now_secs))
}

pub async fn call_requested() -> bool {
    if cfg().test_status {
        return false; // tests never read the real topic
    }
    let now = std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).map_or(0, |d| d.as_secs());
    ntfy_recent().await.is_some_and(|t| call_requested_in(&t, now))
}

/// The phone app publishes its setting on the ntfy topic (title "Contact mode"); take it if it's newer than the file.
pub async fn contact_mode() -> String {
    let file = read_status();
    let mode = file["mode"].as_str().unwrap_or("both").to_string();
    if cfg().test_status {
        return mode;
    }
    let poll = async {
        ntfy_recent()
            .await?
            .lines()
            .filter_map(|l| serde_json::from_str::<Value>(l).ok())
            .filter(|m| m["event"] == "message" && m["title"] == "Contact mode")
            .filter(|m| MODES.contains(&m["message"].as_str().unwrap_or("").trim()))
            .last()
    };
    if let Some(latest) = poll.await {
        let new = latest["message"].as_str().unwrap_or("").trim();
        let at = latest["time"].as_u64().unwrap_or(0) * 1000;
        // >= : ntfy times are whole seconds, so two changes in one second tie; the later message wins.
        if at >= iso_ms(file["updatedAt"].as_str().unwrap_or("")) && new != mode {
            let when = std::time::UNIX_EPOCH + Duration::from_millis(at);
            write_status(new, humantime::format_rfc3339_millis(when).to_string(), "phone app");
            return new.to_string();
        }
    }
    mode
}

pub async fn set_contact_mode(mode: &str) {
    write_status(mode, now_iso(), "claude");
    // Publish too, so the phone app shows the new mode. Never from tests: they must not change the real phone.
    if !cfg().test_status {
        let _ = HTTP
            .post(format!("https://ntfy.sh/{}", cfg().topic))
            .header("Title", "Contact mode")
            .header("Priority", "1")
            .body(mode.to_string())
            .send()
            .await;
    }
}

/// 16-bit mono PCM as a WAV file (what the phone plays).
fn wav(pcm: &[u8], rate: u32) -> Vec<u8> {
    let n = pcm.len() as u32;
    let mut w = Vec::with_capacity(44 + pcm.len());
    for part in [&b"RIFF"[..], &(36 + n).to_le_bytes(), b"WAVEfmt ", &16u32.to_le_bytes(), &1u16.to_le_bytes(), &1u16.to_le_bytes()] {
        w.extend_from_slice(part);
    }
    for part in [rate.to_le_bytes(), (rate * 2).to_le_bytes()] {
        w.extend_from_slice(&part);
    }
    for part in [&2u16.to_le_bytes()[..], &16u16.to_le_bytes(), b"data", &n.to_le_bytes(), pcm] {
        w.extend_from_slice(part);
    }
    w
}

#[cfg(test)]
mod tests {
    #[test]
    fn wav_header() {
        let w = super::wav(&[1, 0, 2, 0], 24_000);
        assert_eq!((&w[0..4], &w[8..16], w.len()), (&b"RIFF"[..], &b"WAVEfmt "[..], 48));
        assert_eq!(u32::from_le_bytes(w[24..28].try_into().unwrap()), 24_000);
        assert_eq!(&w[36..40], b"data");
    }

    #[test]
    fn call_request_freshness() {
        let at = |title: &str, time: u64| format!("{{\"event\":\"message\",\"time\":{time},\"title\":\"{title}\",\"message\":\"claude\"}}\n");
        let now = 1_000_000;
        assert!(super::call_requested_in(&(at("Contact mode", now) + &at("Call request", now - 299)), now));
        assert!(!super::call_requested_in(&at("Call request", now - 301), now));
        assert!(!super::call_requested_in(&(at("Contact mode", now) + "{\"event\":\"keepalive\",\"time\":1000000}\nnot json\n"), now));
    }
}
