//! The in-app chats and the rules for them, with no Android in it: the store (a JSON file), signing what the phone
//! sends, and applying the PC's replies. See inbox.rs for the part that talks to Android.
use hmac::{Hmac, Mac};
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};
use sha2::Sha256;
use std::{
    collections::hash_map::RandomState,
    hash::{BuildHasher, Hasher},
    path::Path,
};

/// What the phone lets you type, in bytes; the whole signed message must stay under ntfy's 4096.
const MAX_TEXT: usize = 2800;
const MAX_BODY: usize = 4000;

/// One bubble. Mine: `id` is the message id and `state` how it is going (sending, sent, not_sent, working, answered).
/// The PC's: `id` is the message it answers, `state` is done / failed / stopped / missed (partial while parts are
/// still missing), `parts` the pieces of a long answer by number.
#[derive(Serialize, Deserialize, Clone, Default, Debug, PartialEq)]
#[serde(default)] // fields added later must not make older files unreadable
pub struct Line {
    pub id: String,
    pub me: bool,
    pub text: String,
    pub at: i64,
    pub since: i64, // when `state` last changed (ms)
    pub state: String,
    pub parts: Vec<Option<String>>,
}

#[derive(Serialize, Deserialize, Clone, Default, Debug, PartialEq)]
#[serde(default)] // fields added later must not make older files unreadable
pub struct Chat {
    pub id: String,
    pub agent: String,
    pub model: Option<String>,
    pub title: String,
    pub updated: i64,
    pub unread: u32,
    pub lines: Vec<Line>,
}

#[derive(Serialize, Deserialize, Clone, Default, Debug, PartialEq)]
#[serde(default)] // fields added later must not make older files unreadable
pub struct Store {
    pub chats: Vec<Chat>,
}

pub fn now() -> i64 {
    std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).unwrap().as_millis() as i64
}

fn base36(mut n: u64, min: usize) -> String {
    let mut s = vec![];
    while n > 0 || s.len() < min {
        s.push(b"0123456789abcdefghijklmnopqrstuvwxyz"[(n % 36) as usize]);
        n /= 36;
    }
    s.reverse();
    String::from_utf8(s).unwrap()
}

/// "c" or "m", the time in base 36 and 4 random characters.
fn id(prefix: &str) -> String {
    let r = RandomState::new().build_hasher().finish() % 36u64.pow(4);
    format!("{prefix}{}{}", base36(now() as u64, 0), base36(r, 4))
}

/// "v1", the ids, the agent, the model, the time and the text, signed with the shared key (lowercase hex).
pub fn sign(key: &str, chat: &str, msg: &str, agent: &str, model: Option<&str>, ts: i64, text: &str) -> String {
    let mut mac = Hmac::<Sha256>::new_from_slice(key.as_bytes()).expect("any key length");
    mac.update(format!("v1\n{chat}\n{msg}\n{agent}\n{}\n{ts}\n{text}", model.unwrap_or("")).as_bytes());
    mac.finalize().into_bytes().iter().map(|b| format!("{b:02x}")).collect()
}

/// "Claude · Haiku", "Codex".
pub fn name(agent: &str, model: Option<&str>) -> String {
    match (agent, model) {
        ("codex", _) => "Codex".into(),
        (_, Some(m)) if !m.is_empty() => {
            let mut w = m.split('-').next().unwrap_or(m).chars();
            format!("Claude · {}{}", w.next().map(|c| c.to_uppercase().to_string()).unwrap_or_default(), w.as_str())
        }
        _ => "Claude".into(),
    }
}

/// How long ago, short.
pub fn ago(ms: i64) -> String {
    match ms / 1000 {
        s if s < 60 => "now".into(),
        s if s < 3600 => format!("{} min", s / 60),
        s if s < 86400 => format!("{} h", s / 3600),
        s => format!("{} d", s / 86400),
    }
}

/// What a bubble from the PC says.
pub fn shown(l: &Line) -> String {
    match (l.text.as_str(), l.state.as_str()) {
        ("", "failed") => "Something went wrong.".into(),
        ("", "stopped") => "Stopped.".into(),
        ("", "missed") => "No agent was free to take this.".into(),
        ("", _) => "Done.".into(),
        (t, _) => t.into(),
    }
}

/// The small grey line under my last message, and what tapping it does ("stop", "retry" or nothing).
pub fn status(c: &Chat, now: i64) -> (&'static str, &'static str) {
    let Some(l) = c.lines.iter().rev().find(|l| l.me) else { return ("", "") };
    let age = now - l.since;
    match l.state.as_str() {
        "sending" if age < 15_000 => ("Sending…", ""),
        "sending" | "not_sent" => ("Not sent · tap to retry", "retry"),
        "sent" if age > 60_000 => ("PC hasn't picked this up yet", ""),
        "sent" => ("Sent", ""),
        "working" => ("Working…", "stop"),
        _ => ("", ""),
    }
}

fn cut(s: &str, n: usize) -> &str {
    let mut n = n.min(s.len());
    while !s.is_char_boundary(n) {
        n -= 1;
    }
    &s[..n]
}

impl Store {
    /// None when the file is there but can't be read: the caller must not save over it (that wiped every chat).
    pub fn load(path: &Path) -> Option<Self> {
        match std::fs::read(path) {
            Ok(b) => serde_json::from_slice(&b).ok(),
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => Some(Self::default()),
            Err(_) => None,
        }
    }

    /// Writes beside the file and renames over it, so a crash never leaves half a file.
    pub fn save(&self, path: &Path) -> std::io::Result<()> {
        let tmp = path.with_extension("tmp");
        std::fs::write(&tmp, serde_json::to_vec(self)?)?;
        std::fs::rename(tmp, path)
    }

    pub fn get(&self, chat: &str) -> Option<&Chat> {
        self.chats.iter().find(|c| c.id == chat)
    }

    fn get_mut(&mut self, chat: &str) -> Option<&mut Chat> {
        self.chats.iter_mut().find(|c| c.id == chat)
    }

    pub fn new_chat(&mut self, agent: &str, model: Option<&str>) -> String {
        let c = Chat { id: id("c"), agent: agent.into(), model: model.map(Into::into), updated: now(), ..Chat::default() };
        self.chats.push(c.clone());
        c.id
    }

    /// Adds my message (sending) and returns its id; None when there is nothing to send.
    pub fn add_mine(&mut self, chat: &str, text: &str) -> Option<String> {
        let mut text = cut(text.trim(), MAX_TEXT);
        // Quotes and backslashes grow when escaped; keep the whole signed message under MAX_BODY.
        while serde_json::to_string(text).ok()?.len() > MAX_BODY - 500 {
            text = &text[..text.char_indices().last()?.0];
        }
        let c = self.get_mut(chat).filter(|_| !text.is_empty())?;
        let (at, msg) = (now(), id("m"));
        if c.title.is_empty() {
            c.title = text.chars().take(40).collect();
        }
        c.updated = at;
        c.lines.push(Line { id: msg.clone(), me: true, text: text.into(), at, since: at, state: "sending".into(), ..Line::default() });
        Some(msg)
    }

    /// The signed message for one of my lines; a retry sends the very same one.
    pub fn body(&self, chat: &str, msg: &str, key: &str) -> Option<String> {
        let c = self.get(chat)?;
        let l = c.lines.iter().find(|l| l.me && l.id == msg)?;
        let model = c.model.as_deref();
        let sig = sign(key, chat, msg, &c.agent, model, l.at, &l.text);
        Some(json!({"v": 1, "chat": chat, "msg": msg, "agent": c.agent, "model": model, "ts": l.at, "text": l.text, "sig": sig}).to_string())
    }

    /// The send finished (or failed). Only a message still "sending" changes: a reply may already have come.
    pub fn sent(&mut self, chat: &str, msg: &str, ok: bool) {
        if let Some(l) = self.get_mut(chat).and_then(|c| c.lines.iter_mut().find(|l| l.me && l.id == msg && l.state == "sending")) {
            (l.state, l.since) = (if ok { "sent" } else { "not_sent" }.into(), now());
        }
    }

    /// Back to "sending" for the last of my messages that did not go; returns its id.
    pub fn retry(&mut self, chat: &str) -> Option<String> {
        let l = self.get_mut(chat)?.lines.iter_mut().rev().find(|l| l.me)?;
        if !matches!(l.state.as_str(), "sending" | "not_sent") {
            return None;
        }
        // A fresh time, so a retry long after the first try isn't turned away as too old (same id: no double run).
        (l.state, l.since, l.at) = ("sending".into(), now(), now());
        Some(l.id.clone())
    }

    pub fn mark_read(&mut self, chat: &str) {
        if let Some(c) = self.get_mut(chat) {
            c.unread = 0;
        }
    }

    /// The PC's reply. Safe to apply twice (the phone replays what it missed). Returns (chat, name, text) when it is
    /// new and `open` (the chat being looked at) isn't that chat: that is what to notify.
    pub fn apply_reply(&mut self, v: &Value, open: Option<&str>) -> Option<(String, String, String)> {
        let (chat, re, state) = (v["chat"].as_str()?, v["re"].as_str()?, v["state"].as_str()?);
        let c = self.get_mut(chat).filter(|_| v["v"] == 1)?;
        let mine = c.lines.iter().position(|l| l.me && l.id == re)?;
        let now = now();
        if state == "working" {
            let l = &mut c.lines[mine];
            if matches!(l.state.as_str(), "sending" | "sent" | "not_sent") {
                (l.state, l.since) = ("working".into(), now);
            }
            return None;
        }
        let text = v["text"].as_str().unwrap_or("");
        let at = c.lines.iter().position(|l| !l.me && l.id == re);
        if at.is_some_and(|i| c.lines[i].state != "partial") {
            return None; // already have the final answer
        }
        let i = at.unwrap_or_else(|| {
            c.lines.push(Line { id: re.into(), at: now, state: state.into(), ..Line::default() });
            c.lines.len() - 1
        });
        let l = &mut c.lines[i];
        if state == "done" {
            let n = v["parts"].as_u64().unwrap_or(1).clamp(1, 50) as usize;
            let p = (v["part"].as_u64().unwrap_or(1) as usize).clamp(1, n) - 1;
            l.state = "partial".into();
            l.parts.resize(n, None);
            l.parts[p] = Some(text.into());
            if l.parts.iter().any(Option::is_none) {
                return None;
            }
            l.text = l.parts.drain(..).flatten().collect();
            l.state = "done".into();
        } else {
            (l.state, l.text) = (state.into(), text.into());
        }
        l.at = now;
        let shown = shown(l);
        c.lines[mine].state = "answered".into();
        c.updated = now;
        if open == Some(chat) {
            return None;
        }
        c.unread += 1;
        Some((c.id.clone(), name(&c.agent, c.model.as_deref()), shown))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn store() -> (Store, String, String) {
        let mut s = Store::default();
        let chat = s.new_chat("claude", Some("haiku-5.5"));
        let msg = s.add_mine(&chat, "hello").unwrap();
        (s, chat, msg)
    }

    fn reply(chat: &str, re: &str, state: &str, text: &str, part: u32, parts: u32) -> Value {
        json!({"v": 1, "chat": chat, "re": re, "state": state, "text": text, "part": part, "parts": parts})
    }

    #[test]
    fn save_load_round_trip() {
        let (s, ..) = store();
        let path = std::env::temp_dir().join(format!("callme-chats-{}.json", std::process::id()));
        s.save(&path).unwrap();
        assert_eq!(Store::load(&path), Some(s));
        std::fs::remove_file(path).unwrap();
        assert_eq!(Store::load(Path::new("no/such/file")), Some(Store::default()));
    }

    #[test]
    fn working_then_done_in_parts() {
        let (mut s, c, m) = store();
        assert!(s.apply_reply(&reply(&c, &m, "working", "", 1, 1), None).is_none());
        assert_eq!(s.get(&c).unwrap().lines[0].state, "working");
        for (p, t) in [(2, "world "), (1, "hello "), (2, "world "), (3, "!")] {
            let r = s.apply_reply(&reply(&c, &m, "done", t, p, 3), None);
            assert_eq!(r.is_some(), p == 3, "notifies once, when whole");
        }
        assert!(s.apply_reply(&reply(&c, &m, "done", "!", 3, 3), None).is_none(), "replay");
        let chat = s.get(&c).unwrap();
        assert_eq!((chat.lines[1].text.as_str(), chat.lines[1].state.as_str()), ("hello world !", "done"));
        assert_eq!((chat.lines[0].state.as_str(), chat.unread, chat.lines.len()), ("answered", 1, 2));
        assert_eq!(status(chat, now()), ("", ""));
    }

    #[test]
    fn unread_only_when_not_open_and_mark_read() {
        let (mut s, c, m) = store();
        let m2 = s.add_mine(&c, "again").unwrap();
        assert!(s.apply_reply(&reply(&c, &m, "done", "a", 1, 1), Some(&c)).is_none());
        assert_eq!(s.get(&c).unwrap().unread, 0);
        let (id, name, text) = s.apply_reply(&reply(&c, &m2, "failed", "", 1, 1), Some("other")).unwrap();
        assert_eq!((id, name.as_str(), text.as_str()), (c.clone(), "Claude · Haiku", "Something went wrong."));
        assert_eq!(s.get(&c).unwrap().unread, 1);
        s.mark_read(&c);
        assert_eq!(s.get(&c).unwrap().unread, 0);
        assert!(s.apply_reply(&reply("nope", &m, "done", "a", 1, 1), None).is_none(), "unknown chat");
    }

    #[test]
    fn signature_matches_shared_vector() {
        let sig = sign("test-key", "c1", "m1", "claude", Some("haiku-5.5"), 1700000000000, "héllo 👋");
        assert_eq!(sig, "ab529206c0c5da8e9c12efc38dac2b71aacc25df75a33f8d9c1b230be8b2f44e");
    }

    #[test]
    fn body_fits_at_the_cap_and_retry_resends_the_same() {
        let (mut s, c, _) = store();
        for text in ["x".repeat(5000), "é".repeat(5000), "\"".repeat(5000)] {
            let m = s.add_mine(&c, &text).unwrap();
            let body = s.body(&c, &m, "k").unwrap();
            assert!(body.len() < 4000, "{} bytes", body.len());
            let v: Value = serde_json::from_str(&body).unwrap();
            assert!(v["text"].as_str().unwrap().len() <= 2800);
        }
        let m = s.add_mine(&c, "retry me").unwrap();
        let first = s.body(&c, &m, "k");
        s.sent(&c, &m, false);
        assert_eq!(status(s.get(&c).unwrap(), now()).1, "retry");
        assert_eq!(s.retry(&c), Some(m.clone()));
        assert_eq!(s.body(&c, &m, "k"), first);
        assert!(s.add_mine(&c, "  ").is_none());
    }
}
