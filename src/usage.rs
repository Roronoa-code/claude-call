use serde_json::Value;
use std::{
    collections::HashSet,
    path::{Path, PathBuf},
    time::{Duration, SystemTime},
};

struct Price {
    input: f64,
    output: f64,
    cache_write: f64,
    cache_read: f64,
}

fn price(model: &str) -> Price {
    match model {
        m if m.contains("haiku") => Price { input: 0.10, output: 0.50, cache_write: 0.125, cache_read: 0.01 },
        m if m.contains("sonnet") => Price { input: 2.0, output: 10.0, cache_write: 2.5, cache_read: 0.20 },
        m if m.contains("fable") || m.contains("mythos") => Price { input: 10.0, output: 50.0, cache_write: 12.5, cache_read: 0.25 },
        _ => Price { input: 4.0, output: 20.0, cache_write: 5.0, cache_read: 0.20 },
    }
}

fn projects() -> Option<PathBuf> {
    std::env::var_os("USERPROFILE").or_else(|| std::env::var_os("HOME")).map(|h| Path::new(&h).join(".claude").join("projects"))
}

pub fn find(said: &str) -> Option<PathBuf> {
    let dirs: Vec<PathBuf> = std::fs::read_dir(projects()?).ok()?.flatten().map(|e| e.path()).filter(|p| p.is_dir()).collect();
    // The JSONL is raw: the needle stops at the first character JSON would escape.
    let needle: String = said.chars().take_while(|c| !matches!(c, '"' | '\\') && !c.is_control()).take(40).collect();
    if needle.len() < 8 {
        return None;
    }
    let holds = |p: &PathBuf| std::fs::read_to_string(p).is_ok_and(|s| s.contains(&needle));
    // Runs started from inside another session inherit CLAUDE_CODE_SESSION_ID.
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
    (n("input_tokens") * p.input + n("output_tokens") * p.output + n("cache_creation_input_tokens") * p.cache_write + n("cache_read_input_tokens") * p.cache_read) / 1e6
}

pub fn label(dollars: f64) -> String {
    if dollars < 0.005 { "<$0.01".into() } else { format!("${dollars:.2}") }
}
