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
    pub media: Vec<Media>, // photos and videos sent with the PC's answer, in part order
}

/// The biggest photo or video the phone fetches (the PC sends up to this; ntfy.sh holds up to 15 MB).
pub const MEDIA_MAX: u64 = 10 * 1024 * 1024;
const MIMES: [(&str, &str); 7] = [
    ("image/png", "png"),
    ("image/jpeg", "jpg"),
    ("image/webp", "webp"),
    ("image/gif", "gif"),
    ("video/mp4", "mp4"),
    ("video/quicktime", "mov"),
    ("video/webm", "webm"),
];

/// A photo or video from the PC: where to fetch it (an ntfy attachment, gone after 3 hours), what it must be, and
/// `state` loading / ready (saved at `path`) / failed.
#[derive(Serialize, Deserialize, Clone, Default, Debug, PartialEq)]
#[serde(default)]
pub struct Media {
    pub part: usize,
    pub url: String,
    pub name: String,
    pub mime: String,
    pub size: u64,
    pub sha256: String,
    pub path: String,
    pub state: String,
}

impl Media {
    /// The file a signed reply part names, when it is safe to fetch: an https ntfy.sh file link, a known photo or
    /// video type, at most MEDIA_MAX, with a sha256 to check it against.
    fn from(v: &Value, part: usize) -> Option<Self> {
        let (url, name, mime, sha) = (v["url"].as_str()?, v["name"].as_str()?, v["type"].as_str()?, v["sha256"].as_str()?);
        let size = v["size"].as_u64()?;
        let file = url.strip_prefix("https://ntfy.sh/file/")?;
        let ok = !file.is_empty()
            && file.chars().all(|c| c.is_ascii_alphanumeric() || "._-".contains(c))
            && MIMES.iter().any(|(m, _)| *m == mime)
            && (1..=MEDIA_MAX).contains(&size)
            && sha.len() == 64
            && sha.chars().all(|c| c.is_ascii_hexdigit());
        ok.then(|| Media {
            part,
            url: url.into(),
            name: name.into(),
            mime: mime.into(),
            size,
            sha256: sha.to_ascii_lowercase(),
            state: "loading".into(),
            ..Media::default()
        })
    }

    /// A photo or video picked on the phone to send: a known type, at most MEDIA_MAX. Its name says only what it is
    /// ("photo.jpg"); the caller saves it at `path` (file_name()). Err: why it can't be sent, to show.
    pub fn picked(bytes: &[u8], mime: &str) -> Result<Self, String> {
        use sha2::Digest;
        let what = if mime.starts_with("video/") { "video" } else { "photo" };
        let Some((mime, ext)) = MIMES.iter().find(|(m, _)| *m == mime) else {
            return Err(format!("Only photos and videos can be sent ({})", if mime.is_empty() { "unknown type" } else { mime }));
        };
        match bytes.len() as u64 {
            0 => Err(format!("That {what} is empty")),
            n if n > MEDIA_MAX => Err(format!("That {what} is over 10 MB")),
            size => Ok(Media {
                name: format!("{what}.{ext}"),
                mime: mime.to_string(),
                size,
                sha256: hex(&Sha256::digest(bytes)),
                state: "ready".into(),
                ..Media::default()
            }),
        }
    }

    pub fn video(&self) -> bool {
        self.mime.starts_with("video/")
    }

    /// Its file name on the phone: the hash and the type's extension (the PC's name never becomes a path).
    pub fn file_name(&self) -> String {
        let ext = MIMES.iter().find(|(m, _)| *m == self.mime).map_or("bin", |(_, e)| e);
        format!("{}.{ext}", self.sha256)
    }

    /// Whether downloaded bytes are exactly the file the PC signed for.
    pub fn matches(&self, bytes: &[u8]) -> bool {
        use sha2::Digest;
        bytes.len() as u64 == self.size && hex(&Sha256::digest(bytes)) == self.sha256
    }
}

fn hex(b: &[u8]) -> String {
    b.iter().map(|b| format!("{b:02x}")).collect()
}

/// "Sent a photo", "Sent 2 videos", "Sent a photo and a video".
fn sent(media: &[Media]) -> String {
    let v = media.iter().filter(|m| m.video()).count();
    let say = |n: usize, what: &str| match n {
        0 => None,
        1 => Some(format!("a {what}")),
        n => Some(format!("{n} {what}s")),
    };
    let both: Vec<_> = [say(media.len() - v, "photo"), say(v, "video")].into_iter().flatten().collect();
    format!("Sent {}", both.join(" and "))
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

/// "v1", the ids, the agent, the model, the time and the text, signed with the shared key (lowercase hex). With a
/// photo or video it is "v2" and its link, name, type, size and sha256 follow, so a v1 signature never vouches for a file.
#[allow(clippy::too_many_arguments)]
pub fn sign(key: &str, chat: &str, msg: &str, agent: &str, model: Option<&str>, ts: i64, text: &str, att: Option<&Media>) -> String {
    let mut mac = Hmac::<Sha256>::new_from_slice(key.as_bytes()).expect("any key length");
    let v = if att.is_some() { "v2" } else { "v1" };
    mac.update(format!("{v}\n{chat}\n{msg}\n{agent}\n{}\n{ts}\n{text}", model.unwrap_or("")).as_bytes());
    if let Some(m) = att {
        mac.update(format!("\n{}\n{}\n{}\n{}\n{}", m.url, m.name, m.mime, m.size, m.sha256).as_bytes());
    }
    hex(&mac.finalize().into_bytes())
}

/// Whether a "Chat reply" really came from the PC: signed with the same key (constant-time check). A fake reply
/// from anyone who learns the topic would otherwise show as a notification and push out the real answer.
pub fn reply_signed(key: &str, v: &Value) -> bool {
    let sig = v["sig"].as_str().unwrap_or("");
    let Some(want) = (0..sig.len()).step_by(2).map(|i| sig.get(i..i + 2).and_then(|h| u8::from_str_radix(h, 16).ok())).collect::<Option<Vec<u8>>>()
    else {
        return false;
    };
    let (n, s) = (|k: &str| v[k].as_i64().unwrap_or(-1), |k: &str| v[k].as_str().unwrap_or(""));
    let mut mac = Hmac::<Sha256>::new_from_slice(key.as_bytes()).expect("any key length");
    let base = format!("{}\n{}\n{}\n{}\n{}\n{}", s("chat"), s("re"), s("state"), s("text"), n("part"), n("parts"));
    // A part with a file ("att") is signed as v2, over the file's fields too.
    let data = match v["att"].as_object() {
        Some(a) => {
            let f = |k: &str| a.get(k).and_then(Value::as_str).unwrap_or("");
            let size = a.get("size").and_then(Value::as_u64).map_or("-1".into(), |n| n.to_string());
            format!("v2-reply\n{base}\n{}\n{}\n{}\n{size}\n{}", f("url"), f("name"), f("type"), f("sha256"))
        }
        None if v.get("att").is_some() => return false,
        None => format!("v1-reply\n{base}"),
    };
    mac.update(data.as_bytes());
    sig.len() == 64 && mac.verify_slice(&want).is_ok()
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
        ("", _) if !l.media.is_empty() => sent(&l.media),
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
        // A photo or video goes up first, which takes longer.
        "sending" if age < if l.media.is_empty() { 15_000 } else { 120_000 } => ("Sending…", ""),
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

    /// Adds my message (sending), with a photo or video picked to go with it, and returns its id; None when there is
    /// nothing to send.
    pub fn add_mine(&mut self, chat: &str, text: &str, media: Option<Media>) -> Option<String> {
        let mut text = cut(text.trim(), MAX_TEXT);
        // Quotes and backslashes grow when escaped; keep the whole signed message under MAX_BODY.
        while serde_json::to_string(text).ok()?.len() > MAX_BODY - 500 {
            text = &text[..text.char_indices().last()?.0];
        }
        let c = self.get_mut(chat).filter(|_| !text.is_empty() || media.is_some())?;
        let (at, msg) = (now(), id("m"));
        if c.title.is_empty() {
            // The first line; the header elides it with "…" to fit.
            c.title = match (text, &media) {
                ("", Some(m)) => if m.video() { "Video" } else { "Photo" }.into(),
                _ => text.lines().next().unwrap_or("").chars().take(120).collect(),
            };
        }
        c.updated = at;
        let media = media.into_iter().map(|m| Media { part: 1, url: String::new(), ..m }).collect();
        c.lines.push(Line { id: msg.clone(), me: true, text: text.into(), at, since: at, state: "sending".into(), media, ..Line::default() });
        Some(msg)
    }

    /// The signed message for one of my lines; a retry sends the very same one. None while its file has no link yet.
    pub fn body(&self, chat: &str, msg: &str, key: &str) -> Option<String> {
        let c = self.get(chat)?;
        let l = c.lines.iter().find(|l| l.me && l.id == msg)?;
        let model = c.model.as_deref();
        let att = l.media.first();
        if att.is_some_and(|m| m.url.is_empty()) {
            return None;
        }
        let sig = sign(key, chat, msg, &c.agent, model, l.at, &l.text, att);
        let mut v = json!({"v": 1, "chat": chat, "msg": msg, "agent": c.agent, "model": model, "ts": l.at, "text": l.text, "sig": sig});
        if let Some(m) = att {
            v["att"] = json!({"url": m.url, "name": m.name, "type": m.mime, "size": m.size, "sha256": m.sha256});
        }
        Some(v.to_string())
    }

    /// The photo or video of my message still to upload (it has no link yet).
    pub fn to_upload(&self, chat: &str, msg: &str) -> Option<Media> {
        let l = self.get(chat)?.lines.iter().find(|l| l.me && l.id == msg)?;
        l.media.first().filter(|m| m.url.is_empty()).cloned()
    }

    /// Its upload finished: the ntfy link the PC fetches it from.
    pub fn uploaded(&mut self, chat: &str, msg: &str, url: &str) {
        if let Some(m) = self.get_mut(chat).and_then(|c| c.lines.iter_mut().find(|l| l.me && l.id == msg)).and_then(|l| l.media.first_mut()) {
            m.url = url.into();
        }
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
        // A fresh time, so a retry long after the first try isn't turned away as too old (same id: no double run), and
        // its file goes up again (ntfy keeps a file only 3 hours).
        (l.state, l.since, l.at) = ("sending".into(), now(), now());
        l.media.iter_mut().for_each(|m| m.url.clear());
        Some(l.id.clone())
    }

    /// Photos and videos still to fetch (each link once).
    pub fn downloads(&self) -> Vec<Media> {
        let mut out: Vec<Media> = vec![];
        for m in self.chats.iter().flat_map(|c| &c.lines).flat_map(|l| &l.media).filter(|m| m.state == "loading") {
            if !out.iter().any(|o| o.url == m.url) {
                out.push(m.clone());
            }
        }
        out
    }

    /// A fetch finished: saved at `path`, or failed (None).
    pub fn media_done(&mut self, url: &str, path: Option<&str>) {
        for m in self.chats.iter_mut().flat_map(|c| &mut c.lines).flat_map(|l| &mut l.media).filter(|m| m.url == url) {
            (m.state, m.path) = (if path.is_some() { "ready" } else { "failed" }.into(), path.unwrap_or("").into());
        }
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
            // An unsafe or malformed file is left out; its part still counts.
            if let Some(m) = Media::from(&v["att"], p + 1).filter(|m| !l.media.iter().any(|x| x.part == m.part)) {
                l.media.push(m);
                l.media.sort_by_key(|m| m.part);
            }
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
        let msg = s.add_mine(&chat, "hello", None).unwrap();
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
        let m2 = s.add_mine(&c, "again", None).unwrap();
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
    fn reply_signature_matches_shared_vector() {
        let good = json!({"v": 1, "chat": "c1", "re": "m1", "state": "done", "text": "héllo 👋", "part": 1, "parts": 1,
            "sig": "260ceed955ae169bfb04cb73eff4e7d7a7faf95ca2abf61ab69fd69e8f1100be"});
        assert!(reply_signed("test-key", &good));
        let mut forged = good.clone();
        forged["text"] = json!("pay me");
        assert!(!reply_signed("test-key", &forged));
        assert!(!reply_signed("other-key", &good));
        let mut unsigned = good.clone();
        unsigned.as_object_mut().unwrap().remove("sig");
        assert!(!reply_signed("test-key", &unsigned));
    }

    fn att() -> Value {
        json!({"url": "https://ntfy.sh/file/abc123.jpg", "name": "pic é.jpg", "type": "image/jpeg", "size": 1234, "sha256": "ab".repeat(32)})
    }

    #[test]
    fn file_reply_signature_matches_shared_vector() {
        let good = json!({"v": 1, "chat": "c1", "re": "m1", "state": "done", "text": "", "part": 2, "parts": 2, "att": att(),
            "sig": "54b592109f701bf43a7e16049916d3ef5d0974179f835a7081500f2baa4d3d86"});
        assert!(reply_signed("test-key", &good));
        for (k, bad) in [("url", json!("https://ntfy.sh/file/evil.jpg")), ("name", json!("x")), ("type", json!("video/mp4")), ("size", json!(1)), ("sha256", json!("cd".repeat(32)))] {
            let mut forged = good.clone();
            forged["att"][k] = bad;
            assert!(!reply_signed("test-key", &forged), "{k}");
        }
        let mut stripped = good.clone();
        stripped.as_object_mut().unwrap().remove("att");
        assert!(!reply_signed("test-key", &stripped), "a v2 sig is not a v1 sig");
        let mut odd = good.clone();
        odd["att"] = json!("https://ntfy.sh/file/abc123.jpg");
        assert!(!reply_signed("test-key", &odd));
    }

    #[test]
    fn only_safe_files_are_fetched() {
        assert!(Media::from(&att(), 1).is_some());
        for (k, bad) in [
            ("url", json!("http://ntfy.sh/file/abc.jpg")),
            ("url", json!("https://ntfy.sh.evil.com/file/abc.jpg")),
            ("url", json!("https://ntfy.sh/file/../x")),
            ("url", json!("https://ntfy.sh/file/")),
            ("type", json!("application/pdf")),
            ("size", json!(MEDIA_MAX + 1)),
            ("size", json!(0)),
            ("sha256", json!("zz".repeat(32))),
        ] {
            let mut a = att();
            a[k] = bad;
            assert!(Media::from(&a, 1).is_none(), "{a}");
        }
        let m = Media::from(&json!({"url": "https://ntfy.sh/file/x.mp4", "name": "../../a.exe", "type": "video/mp4", "size": 3,
            "sha256": "BA7816BF8F01CFEA414140DE5DAE2223B00361A396177A9CB410FF61F20015AD"}), 1).unwrap();
        assert_eq!(m.file_name(), "ba7816bf8f01cfea414140de5dae2223b00361a396177a9cb410ff61f20015ad.mp4");
        assert!(m.matches(b"abc") && !m.matches(b"abd") && !m.matches(b"abcd"));
    }

    #[test]
    fn text_then_files_in_parts() {
        let (mut s, c, m) = store();
        let video = json!({"url": "https://ntfy.sh/file/v.mp4", "name": "clip.mp4", "type": "video/mp4", "size": 9, "sha256": "cd".repeat(32)});
        let mut r3 = reply(&c, &m, "done", "", 3, 3);
        r3["att"] = video;
        assert!(s.apply_reply(&r3, None).is_none());
        assert_eq!(s.downloads().len(), 1, "fetch starts before the answer is whole");
        let mut r2 = reply(&c, &m, "done", "", 2, 3);
        r2["att"] = att();
        assert!(s.apply_reply(&r2, None).is_none());
        assert!(s.apply_reply(&r2, None).is_none(), "a repeated part is not added twice");
        let (_, _, note) = s.apply_reply(&reply(&c, &m, "done", "Here.", 1, 3), None).unwrap();
        assert_eq!(note, "Here.");
        let l = &s.get(&c).unwrap().lines[1];
        assert_eq!((l.text.as_str(), l.media.iter().map(|m| m.name.as_str()).collect::<Vec<_>>()), ("Here.", vec!["pic é.jpg", "clip.mp4"]));
        assert_eq!(s.downloads().len(), 2);
        s.media_done("https://ntfy.sh/file/v.mp4", Some("/data/v.mp4"));
        s.media_done("https://ntfy.sh/file/abc123.jpg", None);
        let l = &s.get(&c).unwrap().lines[1];
        assert_eq!((l.media[0].state.as_str(), l.media[1].state.as_str(), l.media[1].path.as_str()), ("failed", "ready", "/data/v.mp4"));
        assert!(s.downloads().is_empty());
    }

    #[test]
    fn files_only_reply_says_what_was_sent() {
        let (mut s, c, m) = store();
        let mut r = reply(&c, &m, "done", "", 1, 1);
        r["att"] = att();
        assert_eq!(s.apply_reply(&r, None).unwrap().2, "Sent a photo");
        let line = |mimes: &[&str]| Line {
            state: "done".into(),
            media: mimes.iter().map(|m| Media { mime: m.to_string(), ..Media::default() }).collect(),
            ..Line::default()
        };
        assert_eq!(shown(&line(&["video/mp4"])), "Sent a video");
        assert_eq!(shown(&line(&["image/png", "image/gif"])), "Sent 2 photos");
        assert_eq!(shown(&line(&["image/png", "video/webm", "video/mp4"])), "Sent a photo and 2 videos");
        let mut bad = reply(&c, &m, "done", "", 1, 1);
        bad["att"] = json!({"url": "https://example.com/x.png"});
        let m2 = s.add_mine(&c, "again", None).unwrap();
        bad["re"] = json!(m2);
        assert_eq!(s.apply_reply(&bad, None).unwrap().2, "Done.", "an unsafe file is dropped");
    }

    #[test]
    fn signature_matches_shared_vector() {
        let sig = sign("test-key", "c1", "m1", "claude", Some("haiku-5.5"), 1700000000000, "héllo 👋", None);
        assert_eq!(sig, "ab529206c0c5da8e9c12efc38dac2b71aacc25df75a33f8d9c1b230be8b2f44e");
    }

    #[test]
    fn file_signature_matches_shared_vector() {
        let m = Media { url: "https://ntfy.sh/file/abc123.jpg".into(), name: "pic é.jpg".into(), mime: "image/jpeg".into(), size: 1234, sha256: "ab".repeat(32), ..Media::default() };
        let sig = |m: &Media| sign("test-key", "c1", "m1", "claude", Some("haiku-5.5"), 1700000000000, "héllo 👋", Some(m));
        assert_eq!(sig(&m), "f61fe75b634ec0074a302a1f927b60591ae6d5a1028b6cd8a4203befa10d0cf5");
        assert_ne!(sig(&Media { size: 1, ..m.clone() }), sig(&m));
    }

    #[test]
    fn picked_files_are_checked() {
        let m = Media::picked(b"abc", "image/jpeg").unwrap();
        assert_eq!((m.name.as_str(), m.size, m.state.as_str()), ("photo.jpg", 3, "ready"));
        assert_eq!(m.sha256, "ba7816bf8f01cfea414140de5dae2223b00361a396177a9cb410ff61f20015ad");
        assert_eq!(Media::picked(b"x", "video/quicktime").unwrap().name, "video.mov");
        assert_eq!(Media::picked(b"x", "image/heic").unwrap_err(), "Only photos and videos can be sent (image/heic)");
        assert_eq!(Media::picked(b"", "video/mp4").unwrap_err(), "That video is empty");
        assert_eq!(Media::picked(&vec![0; MEDIA_MAX as usize + 1], "image/png").unwrap_err(), "That photo is over 10 MB");
    }

    #[test]
    fn a_photo_goes_up_first_then_the_signed_message_carries_it() {
        let (mut s, c, _) = store();
        let c2 = s.new_chat("codex", None);
        let pic = Media { path: "/data/x.jpg".into(), ..Media::picked(b"abc", "image/jpeg").unwrap() };
        let m = s.add_mine(&c2, "", Some(pic)).unwrap();
        assert_eq!(s.get(&c2).unwrap().title, "Photo");
        assert_eq!(shown(&s.get(&c2).unwrap().lines[0]), "Sent a photo");
        assert!(s.body(&c2, &m, "k").is_none(), "not before the upload");
        assert_eq!(s.to_upload(&c2, &m).unwrap().path, "/data/x.jpg");
        s.uploaded(&c2, &m, "https://ntfy.sh/file/up.jpg");
        assert!(s.to_upload(&c2, &m).is_none());
        let v: Value = serde_json::from_str(&s.body(&c2, &m, "k").unwrap()).unwrap();
        assert_eq!(v["att"], json!({"url": "https://ntfy.sh/file/up.jpg", "name": "photo.jpg", "type": "image/jpeg", "size": 3,
            "sha256": "ba7816bf8f01cfea414140de5dae2223b00361a396177a9cb410ff61f20015ad"}));
        let l = &s.get(&c2).unwrap().lines[0];
        assert_eq!(v["sig"], sign("k", &c2, &m, "codex", None, l.at, "", l.media.first()).as_str());
        s.sent(&c2, &m, false);
        assert_eq!(s.retry(&c2), Some(m.clone()));
        assert!(s.to_upload(&c2, &m).is_some(), "a retry uploads again");
        assert!(s.downloads().is_empty(), "my own file is never fetched");
        let m2 = s.add_mine(&c, &"\"".repeat(5000), Some(Media::picked(b"abc", "video/webm").unwrap())).unwrap();
        s.uploaded(&c, &m2, &format!("https://ntfy.sh/file/{}.webm", "a".repeat(20)));
        assert!(s.body(&c, &m2, "k").unwrap().len() < 4000, "the longest text and a file still fit");
    }

    #[test]
    fn body_fits_at_the_cap_and_retry_resends_the_same() {
        let (mut s, c, _) = store();
        for text in ["x".repeat(5000), "é".repeat(5000), "\"".repeat(5000)] {
            let m = s.add_mine(&c, &text, None).unwrap();
            let body = s.body(&c, &m, "k").unwrap();
            assert!(body.len() < 4000, "{} bytes", body.len());
            let v: Value = serde_json::from_str(&body).unwrap();
            assert!(v["text"].as_str().unwrap().len() <= 2800);
        }
        let m = s.add_mine(&c, "retry me", None).unwrap();
        let first = s.body(&c, &m, "k");
        s.sent(&c, &m, false);
        assert_eq!(status(s.get(&c).unwrap(), now()).1, "retry");
        assert_eq!(s.retry(&c), Some(m.clone()));
        assert_eq!(s.body(&c, &m, "k"), first);
        assert!(s.add_mine(&c, "  ", None).is_none());
    }
}
