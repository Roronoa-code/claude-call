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
    pub choices: Vec<String>, // an agent's question: the answers it offers as buttons
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
    /// "" a chat I started; a thread the PC started: "ask" (an agent's question, its own thread) or "note" (updates,
    /// read-only). `asked`: when the PC sent it (ms, the PC's clock).
    pub kind: String,
    pub asked: i64,
}

#[derive(Serialize, Deserialize, Clone, Default, Debug, PartialEq)]
#[serde(default)] // fields added later must not make older files unreadable
pub struct Store {
    pub chats: Vec<Chat>,
    /// "chat/line" of the PC's messages in PC-started threads deleted here, so their late parts and resends are
    /// dropped instead of opening the thread again (the newest GONE_KEEP).
    pub gone: Vec<String>,
}

const GONE_KEEP: usize = 500;
/// An agent's question nobody answered in a day is closed on the PC.
pub const ASK_FOR: i64 = 24 * 3600 * 1000;

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

/// `data` signed with the key as v["sig"] says (64 hex), checked in constant time.
fn verify(key: &str, v: &Value, data: &str) -> bool {
    let sig = v["sig"].as_str().unwrap_or("");
    let Some(want) = (0..sig.len()).step_by(2).map(|i| sig.get(i..i + 2).and_then(|h| u8::from_str_radix(h, 16).ok())).collect::<Option<Vec<u8>>>()
    else {
        return false;
    };
    let mut mac = Hmac::<Sha256>::new_from_slice(key.as_bytes()).expect("any key length");
    mac.update(data.as_bytes());
    sig.len() == 64 && mac.verify_slice(&want).is_ok()
}

/// What a part's file adds to its signed string (url, name, type, size, sha256, each after a "\n"): None without a
/// file, Err when "att" is there but not an object (never valid).
fn att_signed(v: &Value) -> Result<Option<String>, ()> {
    match v["att"].as_object() {
        Some(a) => {
            let f = |k: &str| a.get(k).and_then(Value::as_str).unwrap_or("");
            let size = a.get("size").and_then(Value::as_u64).map_or("-1".into(), |n| n.to_string());
            Ok(Some(format!("\n{}\n{}\n{}\n{size}\n{}", f("url"), f("name"), f("type"), f("sha256"))))
        }
        None if v.get("att").is_some() => Err(()),
        None => Ok(None),
    }
}

/// Whether a "Chat reply" really came from the PC: signed with the same key (constant-time check). A fake reply
/// from anyone who learns the topic would otherwise show as a notification and push out the real answer.
pub fn reply_signed(key: &str, v: &Value) -> bool {
    let (n, s) = (|k: &str| v[k].as_i64().unwrap_or(-1), |k: &str| v[k].as_str().unwrap_or(""));
    let base = format!("{}\n{}\n{}\n{}\n{}\n{}", s("chat"), s("re"), s("state"), s("text"), n("part"), n("parts"));
    // A part with a file ("att") is signed as v2, over the file's fields too.
    match att_signed(v) {
        Ok(Some(att)) => verify(key, v, &format!("v2-reply\n{base}{att}")),
        Ok(None) => verify(key, v, &format!("v1-reply\n{base}")),
        Err(()) => false,
    }
}

/// Whether a "Chat note" (a thread the PC starts: an agent's question or an update) really came from the PC: signed
/// like a reply over its own fields, the choices joined with U+001F ("v1-note", or "v2-note" with a file).
pub fn note_signed(key: &str, v: &Value) -> bool {
    let (n, s) = (|k: &str| v[k].as_i64().unwrap_or(-1), |k: &str| v[k].as_str().unwrap_or(""));
    let Some(choices) = v["choices"].as_array().and_then(|a| a.iter().map(Value::as_str).collect::<Option<Vec<_>>>()) else {
        return false;
    };
    let (chat, note, title, kind, text) = (s("chat"), s("note"), s("title"), s("kind"), s("text"));
    let base = format!("{chat}\n{note}\n{title}\n{kind}\n{}\n{}\n{text}\n{}\n{}", choices.join("\u{1f}"), n("ts"), n("part"), n("parts"));
    match att_signed(v) {
        Ok(Some(att)) => verify(key, v, &format!("v2-note\n{base}{att}")),
        Ok(None) => verify(key, v, &format!("v1-note\n{base}")),
        Err(()) => false,
    }
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

/// The chat's name: the agent's ("Claude · Haiku"), or a PC-started thread's own ("Weather app", "Updates").
pub fn label(c: &Chat) -> String {
    match c.kind.as_str() {
        "" => name(&c.agent, c.model.as_deref()),
        _ if c.title.trim().is_empty() => "Claude".into(),
        _ => c.title.clone(),
    }
}

/// An agent's question: "waiting" for my answer, "answered" or "expired" (unanswered after ASK_FOR); "" for any
/// other chat. A message of mine answers it unless it didn't go or the PC turned it away as failed (a photo it
/// couldn't get: the question still waits).
pub fn ask_state(c: &Chat, now: i64) -> &'static str {
    let failed = |id: &str| c.lines.iter().any(|l| !l.me && l.id == id && l.state == "failed");
    match c.kind.as_str() {
        "ask" if c.lines.iter().any(|l| l.me && l.state != "not_sent" && !failed(&l.id)) => "answered",
        "ask" if now - c.asked > ASK_FOR => "expired",
        "ask" => "waiting",
        _ => "",
    }
}

/// Its character (actor.slint): clawd, haiku, sonnet, opus or null.
pub fn face(agent: &str, model: Option<&str>) -> &'static str {
    crate::cast::Who::named(&format!("{agent} {}", model.unwrap_or(""))).face()
}

/// Whether the PC is still on my last message (sent, or being worked on).
pub fn waiting(c: &Chat) -> bool {
    c.lines.iter().rev().find(|l| l.me).is_some_and(|l| matches!(l.state.as_str(), "sending" | "sent" | "working"))
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

/// One piece of the PC's answer as the thread lays it out: kind "p" (a paragraph), "h" (a heading), "li" (a list
/// item: `mark` "1." or "•", `level` 0 or 1 for a nested one), "quote", or "code" (`mark` its language, if given).
/// `text` keeps its inline markdown (bold, `code`, links); a code block's is its lines exactly.
#[derive(Debug, PartialEq, Clone)]
pub struct Block {
    pub kind: &'static str,
    pub text: String,
    pub mark: String,
    pub level: u8,
}

/// Splits an answer's markdown into blocks, line by line: ``` fences, # headings, - * + and 1. 1) lists, > quotes;
/// a blank line ends a paragraph, a rule (---) is dropped, and anything else is text. An unclosed fence runs to the end.
pub fn blocks(text: &str) -> Vec<Block> {
    let b = |kind, text: &str, mark: &str, level| Block { kind, text: text.into(), mark: mark.into(), level };
    // an open fence: its language and lines so far
    let (mut out, mut code, mut open): (Vec<Block>, Option<(&str, Vec<&str>)>, bool) = (vec![], None, false);
    let fence = |(lang, lines): (&str, Vec<&str>)| b("code", &lines.join("
"), lang, 0);
    for line in text.lines() {
        let t = line.trim_start();
        if let Some((_, lines)) = code.as_mut() {
            if t.starts_with("```") {
                out.extend(code.take().map(fence));
            } else {
                lines.push(line.trim_end());
            }
            continue;
        }
        let indent = line.len() - t.len();
        let level = u8::from(indent >= 2);
        let hashes = t.chars().take_while(|&c| c == '#').count();
        let digits = t.chars().take_while(char::is_ascii_digit).count();
        if let Some(lang) = t.strip_prefix("```") {
            code = Some((lang.trim(), vec![]));
            open = false;
        } else if t.is_empty() {
            open = false;
        } else if (1..=6).contains(&hashes) && t[hashes..].starts_with(' ') {
            out.push(b("h", t[hashes..].trim(), "", 0));
            open = false;
        } else if let Some(rest) = ["- ", "* ", "+ "].iter().find_map(|p| t.strip_prefix(p)) {
            out.push(b("li", rest.trim(), "•", level));
            open = true;
        } else if (1..=3).contains(&digits) && (t[digits..].starts_with(". ") || t[digits..].starts_with(") ")) {
            out.push(b("li", t[digits + 2..].trim(), &format!("{}.", &t[..digits]), level));
            open = true;
        } else if t.len() >= 3 && ["-", "*", "_"].iter().any(|r| t.chars().all(|c| c.to_string() == *r || c == ' ')) {
            open = false;
        } else {
            let (kind, t) = match t.strip_prefix('>') {
                Some(q) => ("quote", q.trim()),
                None => ("p", t.trim_end()),
            };
            match out.last_mut() {
                // a line run on from the one before (a soft break): same paragraph, item or quote
                Some(last) if open && (last.kind == kind || last.kind == "li" && kind == "p") => {
                    last.text.push('\n');
                    last.text.push_str(t);
                }
                _ => out.push(b(kind, t, "", 0)),
            }
            open = true;
        }
    }
    out.extend(code.map(fence));
    out
}

/// Inline markdown made safe for Slint's parser: a "<" outside `code` is written as an entity, so `Vec<u8>` or
/// "a < b" stay text instead of being read as HTML (which fails the whole block).
pub fn inline(text: &str) -> String {
    text.split('`').enumerate().map(|(i, part)| if i % 2 == 0 { part.replace('<', "&lt;") } else { part.to_string() }).collect::<Vec<_>>().join("`")
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

    /// Takes a chat out (the PC's late replies to it then find nothing and are dropped); returns its place and the
    /// chat, for Undo.
    pub fn remove(&mut self, chat: &str) -> Option<(usize, Chat)> {
        let i = self.chats.iter().position(|c| c.id == chat)?;
        let gone = self.chats.remove(i);
        // a thread the PC started: its notes' late parts and resends must not open it again
        if !gone.kind.is_empty() {
            self.gone.extend(gone.lines.iter().filter(|l| !l.me).map(|l| format!("{}/{}", gone.id, l.id)));
            let extra = self.gone.len().saturating_sub(GONE_KEEP);
            self.gone.drain(..extra);
        }
        Some((i, gone))
    }

    /// Undo: the removed chat back where it was, exactly as it was.
    pub fn restore(&mut self, at: usize, chat: Chat) {
        if self.get(&chat.id).is_none() {
            self.gone.retain(|g| !chat.lines.iter().any(|l| *g == format!("{}/{}", chat.id, l.id)));
            self.chats.insert(at.min(self.chats.len()), chat);
        }
    }

    /// A removed chat's saved files that no chat left uses, by file name (they all live in one folder, named by their
    /// sha256, so the same file sent twice is shared).
    pub fn unused_files(&self, gone: &Chat) -> Vec<String> {
        let file = |m: &Media| Path::new(&m.path).file_name().and_then(|n| n.to_str()).unwrap_or("").to_string();
        let used: std::collections::HashSet<String> = self.chats.iter().flat_map(|c| &c.lines).flat_map(|l| &l.media).map(file).collect();
        let mut out: Vec<String> = gone.lines.iter().flat_map(|l| &l.media).map(file).filter(|f| !f.is_empty() && !used.contains(f)).collect();
        out.sort();
        out.dedup();
        out
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
            if !file_part(l, v) {
                return None;
            }
        } else {
            (l.state, l.text) = (state.into(), v["text"].as_str().unwrap_or("").into());
        }
        l.at = now;
        let shown = shown(l);
        c.lines[mine].state = "answered".into();
        c.updated = now;
        if open == Some(chat) {
            return None;
        }
        c.unread += 1;
        Some((c.id.clone(), label(c), shown))
    }

    /// A thread the PC started ("Chat note"): an agent's question (kind "ask", in its own thread "ask-<id>") or an
    /// update ("updates", kind "note"). Makes the thread when it is new, then files the note like a reply, part by
    /// part with its files; safe to apply twice. Dropped for a thread deleted here (late parts and resends). Returns
    /// what to notify, as apply_reply does.
    pub fn apply_note(&mut self, v: &Value, open: Option<&str>) -> Option<(String, String, String)> {
        let id = |s: &str| (1..=40).contains(&s.len()) && s.chars().all(|c| c.is_ascii_alphanumeric() || c == '_' || c == '-');
        let (chat, note, title, kind) = (v["chat"].as_str()?, v["note"].as_str()?, v["title"].as_str()?, v["kind"].as_str()?);
        let choices = v["choices"].as_array()?.iter().map(|c| c.as_str().map(String::from)).collect::<Option<Vec<_>>>()?;
        let (ts, part, parts) = (v["ts"].as_i64()?, v["part"].as_u64()?, v["parts"].as_u64()?);
        // Lengths counted loosely, in characters (the PC counts UTF-16 units, so whatever it sends fits), never bytes.
        let ok = v["v"] == 1
            && id(chat)
            && id(note)
            && title.chars().count() <= 100
            && matches!(kind, "ask" | "note")
            && choices.len() <= 6
            && choices.iter().all(|c| (1..=80).contains(&c.chars().count()) && !c.chars().any(|ch| ch < ' '))
            && v["text"].is_string()
            && (1..=parts).contains(&part)
            && parts <= 50;
        if !ok || self.gone.iter().any(|g| *g == format!("{chat}/{note}")) {
            return None;
        }
        let now = now();
        if self.get(chat).is_none() {
            let c = Chat { id: chat.into(), agent: "claude".into(), title: title.into(), kind: kind.into(), asked: ts, updated: now, ..Chat::default() };
            self.chats.push(c);
        }
        let c = self.get_mut(chat)?;
        let at = c.lines.iter().position(|l| !l.me && l.id == note);
        if at.is_some_and(|i| c.lines[i].state != "partial") {
            return None; // already whole
        }
        let i = at.unwrap_or_else(|| {
            c.lines.push(Line { id: note.into(), at: now, choices, ..Line::default() });
            c.lines.len() - 1
        });
        let l = &mut c.lines[i];
        if !file_part(l, v) {
            return None;
        }
        l.at = now;
        let shown = shown(l);
        c.updated = now;
        if open == Some(chat) {
            return None;
        }
        c.unread += 1;
        Some((c.id.clone(), label(c), shown))
    }
}

/// Files one part of the PC's message: its text, and its file when that is safe (an unsafe or malformed one is left
/// out; its part still counts). True once every part is here: the line is "done", its text joined.
fn file_part(l: &mut Line, v: &Value) -> bool {
    let n = v["parts"].as_u64().unwrap_or(1).clamp(1, 50) as usize;
    let p = (v["part"].as_u64().unwrap_or(1) as usize).clamp(1, n) - 1;
    l.state = "partial".into();
    l.parts.resize(n, None);
    l.parts[p] = Some(v["text"].as_str().unwrap_or("").into());
    if let Some(m) = Media::from(&v["att"], p + 1).filter(|m| !l.media.iter().any(|x| x.part == m.part)) {
        l.media.push(m);
        l.media.sort_by_key(|m| m.part);
    }
    if l.parts.iter().any(Option::is_none) {
        return false;
    }
    l.text = l.parts.drain(..).flatten().collect();
    l.state = "done".into();
    true
}

#[cfg(test)]
#[path = "chats_tests.rs"]
mod tests;
