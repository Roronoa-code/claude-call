//! What the call has used so far, for the meter on the call screen: the agent's own transcript plus its helpers'
//! (Sonnet / Opus subagents), priced per model at API rates.
use serde_json::Value;
use std::{
    collections::HashSet,
    path::{Path, PathBuf},
    time::{Duration, SystemTime},
};

/// $ per million tokens: input, output, cache write, cache read.
fn price(model: &str) -> [f64; 4] {
    match model {
        m if m.contains("haiku") => [0.10, 0.50, 0.125, 0.01],
        m if m.contains("sonnet") => [2.0, 10.0, 2.5, 0.20],
        m if m.contains("fable") || m.contains("mythos") => [10.0, 50.0, 12.5, 0.25],
        _ => [4.0, 20.0, 5.0, 0.20], // Opus, or unknown: priced high so the meter never under-reports
    }
}

fn projects() -> Option<PathBuf> {
    std::env::var_os("USERPROFILE").or_else(|| std::env::var_os("HOME")).map(|h| Path::new(&h).join(".claude").join("projects"))
}

/// The agent's transcript: by session id when Claude Code passes it, else the transcript written in the last two
/// minutes that holds the line just said (the call_user message).
pub fn find(said: &str) -> Option<PathBuf> {
    let dirs: Vec<PathBuf> = std::fs::read_dir(projects()?).ok()?.flatten().map(|e| e.path()).filter(|p| p.is_dir()).collect();
    // Up to the first character JSON would escape, so the needle matches the raw file.
    let needle: String = said.chars().take_while(|c| !matches!(c, '"' | '\\') && !c.is_control()).take(40).collect();
    if needle.len() < 8 {
        return None;
    }
    let holds = |p: &PathBuf| std::fs::read_to_string(p).is_ok_and(|s| s.contains(&needle));
    // The id is inherited by runs started from inside another session, so it must hold the line too.
    if let Ok(id) = std::env::var("CLAUDE_CODE_SESSION_ID") {
        if let Some(p) = dirs.iter().map(|d| d.join(format!("{id}.jsonl"))).find(|p| holds(p)) {
            return Some(p);
        }
    }
    let recent = SystemTime::now() - Duration::from_secs(120);
    dirs.iter()
        .flat_map(|d| std::fs::read_dir(d).into_iter().flatten().flatten().map(|e| e.path()))
        .filter(|p| p.extension().is_some_and(|e| e == "jsonl"))
        .filter(|p| std::fs::metadata(p).and_then(|m| m.modified()).is_ok_and(|t| t > recent))
        .find(|p| holds(p))
}

/// Dollars used since `since` (RFC 3339, the transcripts' own format) in the transcript and its subagents'.
pub fn cost(main: &Path, since: &str) -> f64 {
    let mut files = vec![main.to_path_buf()];
    if let Ok(rd) = std::fs::read_dir(main.with_extension("").join("subagents")) {
        files.extend(rd.flatten().map(|e| e.path()).filter(|p| p.extension().is_some_and(|e| e == "jsonl")));
    }
    let mut seen = HashSet::new(); // a reply is written once per content block, each with the same usage
    let mut total = 0.0;
    for f in files {
        let Ok(text) = std::fs::read_to_string(&f) else { continue };
        for line in text.lines() {
            let Ok(j) = serde_json::from_str::<Value>(line) else { continue };
            let m = &j["message"];
            if j["type"] != "assistant" || j["timestamp"].as_str().is_none_or(|t| t < since) {
                continue;
            }
            if !seen.insert(m["id"].as_str().unwrap_or_default().to_string() + &f.to_string_lossy()) {
                continue;
            }
            total += line_cost(m);
        }
    }
    total
}

fn line_cost(m: &Value) -> f64 {
    let u = &m["usage"];
    let n = |k: &str| u[k].as_f64().unwrap_or(0.0);
    let p = price(m["model"].as_str().unwrap_or(""));
    (n("input_tokens") * p[0] + n("output_tokens") * p[1] + n("cache_creation_input_tokens") * p[2] + n("cache_read_input_tokens") * p[3]) / 1e6
}

/// "$0.04", or "<$0.01" for a call that has barely started.
pub fn label(dollars: f64) -> String {
    if dollars < 0.005 { "<$0.01".into() } else { format!("${dollars:.2}") }
}

#[cfg(test)]
mod tests {
    use serde_json::json;

    #[test]
    fn prices_each_model() {
        let m = |model: &str| json!({ "model": model, "usage": { "input_tokens": 1_000_000, "output_tokens": 1_000_000, "cache_read_input_tokens": 1_000_000 } });
        assert!((super::line_cost(&m("claude-haiku-5-5")) - 0.61).abs() < 1e-9);
        assert!((super::line_cost(&m("claude-sonnet-5-5")) - 12.2).abs() < 1e-9);
        assert!((super::line_cost(&m("claude-opus-5-5")) - 24.2).abs() < 1e-9);
        assert_eq!(super::label(0.001), "<$0.01");
        assert_eq!(super::label(0.256), "$0.26");
    }

    #[test]
    fn counts_each_reply_once_and_only_since_the_call() {
        let dir = std::env::temp_dir().join(format!("callme-usage-{}", std::process::id()));
        std::fs::create_dir_all(dir.join("s").join("subagents")).unwrap();
        let line = |id: &str, t: &str, model: &str| json!({ "type": "assistant", "timestamp": t, "message": { "id": id, "model": model, "usage": { "output_tokens": 100_000 } } }).to_string();
        let main = [line("a", "2026-10-14T20:00:00.000Z", "claude-haiku-5-5"), line("b", "2026-10-14T21:00:00.000Z", "claude-haiku-5-5"), line("b", "2026-10-14T21:00:00.000Z", "claude-haiku-5-5")];
        std::fs::write(dir.join("s.jsonl"), main.join("\n")).unwrap();
        std::fs::write(dir.join("s").join("subagents").join("h.jsonl"), line("c", "2026-10-14T21:01:00.000Z", "claude-sonnet-5-5")).unwrap();
        let c = super::cost(&dir.join("s.jsonl"), "2026-10-14T20:30:00.000Z");
        let _ = std::fs::remove_dir_all(&dir);
        assert!((c - (0.05 + 1.0)).abs() < 1e-9, "{c}"); // b once (Haiku) + c (Sonnet); a is before the call
    }
}
