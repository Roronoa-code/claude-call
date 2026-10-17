use hmac::{Hmac, Mac};
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};
use sha2::Sha256;
use std::{
    collections::hash_map::RandomState,
    hash::{BuildHasher, Hasher},
    path::Path,
};

#[path = "chats_store.rs"]
mod store;
#[path = "chats_text.rs"]
mod text;
pub use text::{blocks, brief, composed, copy_text, has_link, inline, only_link};

/// The whole signed message must stay under ntfy's 4096 bytes.
const MAX_TEXT: usize = 2800;
const MAX_BODY: usize = 4000;

#[derive(Serialize, Deserialize, Clone, Copy, Default, Debug, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum State {
    #[default]
    #[serde(rename = "")]
    Unset,
    Sending,
    Sent,
    NotSent,
    Working,
    Queued,
    Answered,
    Partial,
    Done,
    Failed,
    Stopped,
    Missed,
}

#[derive(Serialize, Deserialize, Clone, Copy, Default, Debug, PartialEq, Eq, PartialOrd, Ord)]
#[serde(rename_all = "snake_case")]
pub enum Seen {
    #[default]
    #[serde(rename = "")]
    Unseen,
    Delivered,
    Read,
}

#[derive(Serialize, Deserialize, Clone, Copy, Default, Debug, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum MediaState {
    #[default]
    #[serde(rename = "")]
    Unset,
    Loading,
    Ready,
    Failed,
}

#[derive(Serialize, Deserialize, Clone, Copy, Default, Debug, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum Kind {
    #[default]
    #[serde(rename = "")]
    Chat,
    Ask,
    Note,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Ask {
    NotAsked,
    Waiting,
    Sending,
    Answered,
    Expired,
}

impl Ask {
    pub fn as_str(self) -> &'static str {
        match self {
            Ask::NotAsked => "",
            Ask::Waiting => "waiting",
            Ask::Sending => "sending",
            Ask::Answered => "answered",
            Ask::Expired => "expired",
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Unsent {
    No,
    Waiting,
    Failed,
}

pub fn wire<T: serde::de::DeserializeOwned>(s: &str) -> Option<T> {
    serde_json::from_value(Value::from(s)).ok()
}

#[derive(Serialize, Deserialize, Clone, Default, Debug, PartialEq)]
#[serde(default)]
pub struct Line {
    pub id: String,
    pub me: bool,
    #[serde(skip_serializing_if = "std::ops::Not::not")]
    pub mirrored: bool,
    pub text: String,
    pub at: i64,
    pub since: i64,
    pub state: State,
    pub parts: Vec<Option<String>>,
    pub media: Vec<Media>,
    pub choices: Vec<String>,
    pub seen: Seen,
    pub first: i64,
    pub tries: u32,
}

/// ntfy.sh holds attachments up to 15 MB.
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
const HEIF: [&str; 2] = ["image/heic", "image/heif"];

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
    pub state: MediaState,
    pub uploaded: i64,
}

/// ntfy keeps an attachment 3 hours.
pub const NTFY_FILE_TTL: i64 = 3 * 3600 * 1000;
const LINK_KEEP: i64 = NTFY_FILE_TTL - 3600 * 1000;

impl Media {
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
            state: MediaState::Loading,
            ..Media::default()
        })
    }

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
                state: MediaState::Ready,
                ..Media::default()
            }),
        }
    }

    /// Samsung's Gallery may name no type, or a wrong one.
    pub fn kind(stated: &str, bytes: &[u8]) -> Option<&'static str> {
        if let Some(m) = MIMES.iter().map(|(m, _)| *m).chain(HEIF).find(|m| *m == stated) {
            return Some(m);
        }
        let at = |i: usize, b: &[u8]| bytes.get(i..i + b.len()) == Some(b);
        let brand = bytes.get(8..12).unwrap_or(b"");
        match () {
            _ if at(0, b"\xff\xd8\xff") => Some("image/jpeg"),
            _ if at(0, b"\x89PNG") => Some("image/png"),
            _ if at(0, b"GIF8") => Some("image/gif"),
            _ if at(0, b"RIFF") && at(8, b"WEBP") => Some("image/webp"),
            _ if at(0, b"\x1a\x45\xdf\xa3") => Some("video/webm"),
            _ if !at(4, b"ftyp") => None,
            _ if brand == b"qt  " => Some("video/quicktime"),
            _ if [&b"heic"[..], b"heix", b"hevc", b"hevx", b"mif1", b"msf1", b"avif"].contains(&brand) => Some("image/heif"),
            _ => Some("video/mp4"),
        }
    }

    pub fn heif(mime: &str) -> bool {
        HEIF.contains(&mime)
    }

    pub fn video(&self) -> bool {
        self.mime.starts_with("video/")
    }

    pub fn file_name(&self) -> String {
        let ext = MIMES.iter().find(|(m, _)| *m == self.mime).map_or("bin", |(_, e)| e);
        format!("{}.{ext}", self.sha256)
    }

    pub fn matches(&self, bytes: &[u8]) -> bool {
        use sha2::Digest;
        bytes.len() as u64 == self.size && hex(&Sha256::digest(bytes)) == self.sha256
    }
}

fn hex(b: &[u8]) -> String {
    b.iter().map(|b| format!("{b:02x}")).collect()
}

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
#[serde(default)]
pub struct Chat {
    pub id: String,
    pub agent: String,
    pub model: Option<String>,
    pub title: String,
    pub updated: i64,
    pub unread: u32,
    pub lines: Vec<Line>,
    pub kind: Kind,
    pub asked: i64,
    pub draft: String,
    pub pick: Option<Media>,
    pub deleted: i64,
}

#[derive(Serialize, Deserialize, Clone, Default, Debug, PartialEq)]
#[serde(default)]
pub struct Store {
    pub chats: Vec<Chat>,
    pub gone: Vec<String>,
    pub picking: String,
}

const GONE_KEEP: usize = 500;
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

fn id(prefix: &str) -> String {
    let r = RandomState::new().build_hasher().finish() % 36u64.pow(4);
    format!("{prefix}{}{}", base36(now() as u64, 0), base36(r, 4))
}

pub fn sign(key: &str, c: &Chat, l: &Line) -> String {
    let mut mac = Hmac::<Sha256>::new_from_slice(key.as_bytes()).expect("any key length");
    let att = l.media.first();
    let v = if att.is_some() { "v2" } else { "v1" };
    let (chat, msg, agent, model, ts, text) = (&c.id, &l.id, &c.agent, c.model.as_deref().unwrap_or(""), l.at, &l.text);
    mac.update(format!("{v}\n{chat}\n{msg}\n{agent}\n{model}\n{ts}\n{text}").as_bytes());
    if let Some(m) = att {
        mac.update(format!("\n{}\n{}\n{}\n{}\n{}", m.url, m.name, m.mime, m.size, m.sha256).as_bytes());
    }
    hex(&mac.finalize().into_bytes())
}

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

/// Anyone who learns the topic can post to it.
pub fn mine_signed(key: &str, v: &Value) -> bool {
    let s = |k: &str| v[k].as_str().unwrap_or("");
    let base = format!("{}\n{}\n{}\n{}\n{}\n{}", s("chat"), s("msg"), s("agent"), s("model"), v["ts"].as_i64().unwrap_or(-1), s("text"));
    match att_signed(v) {
        Ok(Some(att)) => verify(key, v, &format!("v2\n{base}{att}")),
        Ok(None) => verify(key, v, &format!("v1\n{base}")),
        Err(()) => false,
    }
}

pub fn reply_signed(key: &str, v: &Value) -> bool {
    let (n, s) = (|k: &str| v[k].as_i64().unwrap_or(-1), |k: &str| v[k].as_str().unwrap_or(""));
    let base = format!("{}\n{}\n{}\n{}\n{}\n{}", s("chat"), s("re"), s("state"), s("text"), n("part"), n("parts"));
    match att_signed(v) {
        Ok(Some(att)) => verify(key, v, &format!("v2-reply\n{base}{att}")),
        Ok(None) => verify(key, v, &format!("v1-reply\n{base}")),
        Err(()) => false,
    }
}

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

pub fn receipt_signed(key: &str, v: &Value) -> bool {
    let s = |k: &str| v[k].as_str().unwrap_or("");
    verify(key, v, &format!("v1-receipt
{}
{}
{}", s("chat"), s("re"), s("state")))
}

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

pub fn label(c: &Chat) -> String {
    match c.kind {
        Kind::Chat => name(&c.agent, c.model.as_deref()),
        _ if c.title.trim().is_empty() => "Claude".into(),
        _ => c.title.clone(),
    }
}

pub fn ask_state(c: &Chat, now: i64) -> Ask {
    let reply = |id: &str| c.lines.iter().find(|l| !l.me && l.id == id).map_or(State::Unset, |l| l.state);
    let answers = || c.lines.iter().filter(|l| l.me && unsent(l, now) != Unsent::Failed && reply(&l.id) != State::Failed);
    match c.kind {
        Kind::Ask if answers().any(|l| reply(&l.id) == State::Done) => Ask::Answered,
        Kind::Ask if answers().any(|l| reply(&l.id) == State::Missed) => Ask::Expired,
        Kind::Ask if now - c.asked > ASK_FOR => Ask::Expired,
        Kind::Ask if answers().next().is_some() => Ask::Sending,
        Kind::Ask => Ask::Waiting,
        _ => Ask::NotAsked,
    }
}

pub fn open_choices(c: &Chat, now: i64) -> &[String] {
    match c.lines.iter().find(|l| !l.me) {
        Some(l) if ask_state(c, now) == Ask::Waiting => &l.choices,
        _ => &[],
    }
}

pub fn ended(ask: Ask) -> &'static str {
    match ask {
        Ask::Sending => "Waiting for the PC to take your answer…",
        Ask::Answered => "You've answered this. Start a new chat to say more.",
        Ask::Expired => "This question has expired.",
        _ => "",
    }
}

pub fn draft_line(c: &Chat, now: i64) -> String {
    match (c.draft.trim(), &c.pick) {
        _ if c.kind != Kind::Chat && ask_state(c, now) != Ask::Waiting => String::new(),
        ("", Some(m)) => if m.video() { "Video" } else { "Photo" }.to_string(),
        (t, _) => t.split_whitespace().collect::<Vec<_>>().join(" "),
    }
}

pub const AUTO_FOR: i64 = 25 * 60_000;

pub fn unsent(l: &Line, now: i64) -> Unsent {
    let limit = if l.media.is_empty() { 15_000 } else { 120_000 };
    let stuck = l.state == State::NotSent || l.state == State::Sending && now - l.since >= limit;
    let first = if l.first > 0 { l.first } else { l.at };
    match () {
        _ if !l.me || !stuck => Unsent::No,
        _ if now - first < AUTO_FOR => Unsent::Waiting,
        _ => Unsent::Failed,
    }
}

fn backoff(tries: u32) -> i64 {
    15_000 << tries.min(3)
}

pub fn face(agent: &str, model: Option<&str>) -> &'static str {
    crate::cast::Who::named(&format!("{agent} {}", model.unwrap_or(""))).face()
}

pub fn waiting(c: &Chat) -> bool {
    c.lines.iter().rev().find(|l| l.me).is_some_and(|l| l.state == State::Working)
}

pub fn think(c: &Chat, now: i64) -> String {
    match c.lines.iter().rev().find(|l| l.me) {
        Some(l) if l.state == State::Working && now - l.since >= 60_000 => format!("Thinking · {}", ago(now - l.since)),
        Some(l) if l.state == State::Working => "Thinking".into(),
        Some(l) if l.state == State::Queued => "Queued".into(),
        _ => String::new(),
    }
}

pub fn ago(ms: i64) -> String {
    match ms / 1000 {
        s if s < 60 => "now".into(),
        s if s < 3600 => format!("{} min", s / 60),
        s if s < 86400 => format!("{} h", s / 3600),
        s => format!("{} d", s / 86400),
    }
}

pub fn shown(l: &Line) -> String {
    match (l.text.as_str(), l.state) {
        ("", _) if !l.media.is_empty() => sent(&l.media),
        ("", State::Failed) => "Something went wrong.".into(),
        ("", State::Stopped) => "Stopped.".into(),
        ("", State::Missed) => "No agent was free to take this.".into(),
        ("", _) => "Done.".into(),
        (t, _) => t.into(),
    }
}

pub fn status(c: &Chat, now: i64) -> (&'static str, &'static str) {
    let Some(l) = c.lines.iter().rev().find(|l| l.me) else { return ("", "") };
    let partial = c.lines.iter().any(|x| !x.me && x.id == l.id && x.state == State::Partial && now - x.at > 3000);
    let working = l.state == State::Working;
    match (l.state, unsent(l, now)) {
        (_, Unsent::Waiting) => ("Waiting for connection…", ""),
        (_, Unsent::Failed) => ("Failed · Tap to retry", "retry"),
        (State::Sending, _) => ("Sending…", ""),
        _ if partial => ("Receiving reply…", if working { "stop" } else { "" }),
        (State::Queued, _) => ("Queued", "stop"),
        (State::Sent, _) if l.seen == Seen::Unseen && now - l.since > 60_000 => ("Sent · the PC hasn't picked it up yet", ""),
        (State::Sent | State::Working, _) => {
            let seen = match l.seen {
                Seen::Read => "Read",
                _ if l.seen == Seen::Delivered || working => "Delivered",
                _ => "Sent",
            };
            (seen, if working { "stop" } else { "" })
        }
        _ => ("", ""),
    }
}

pub fn room(text: &str) -> i64 {
    let t = text.trim();
    let json = serde_json::to_string(t).map_or(0, |j| j.len() - 2);
    (MAX_TEXT as i64 - t.len() as i64).min((MAX_BODY - 500) as i64 - 2 - json as i64)
}

fn cut(s: &str, n: usize) -> &str {
    let mut n = n.min(s.len());
    while !s.is_char_boundary(n) {
        n -= 1;
    }
    &s[..n]
}

fn file_part(l: &mut Line, v: &Value) -> bool {
    let n = v["parts"].as_u64().unwrap_or(1).clamp(1, 50) as usize;
    let p = (v["part"].as_u64().unwrap_or(1) as usize).clamp(1, n) - 1;
    l.state = State::Partial;
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
    l.state = State::Done;
    true
}

#[derive(Clone, Default, Debug, PartialEq)]
pub struct QuickAnswers {
    pub choices: Vec<String>,
    pub reply: bool,
    pub open: bool,
}

pub struct AfterAnswer {
    pub sent: bool,
    pub note: Option<&'static str>,
    pub answers: QuickAnswers,
}

pub struct Answered {
    pub chat: Chat,
    pub msg: Option<String>,
    pub too_long: bool,
}

/// Android shows at most three notification buttons.
pub fn quick_answers(c: &Chat, now: i64) -> QuickAnswers {
    let (choices, reply, open) = match c.kind {
        Kind::Chat => (vec![], true, false),
        Kind::Ask if ask_state(c, now) == Ask::Waiting => {
            let choices = open_choices(c, now).to_vec();
            match choices.len() {
                0..=2 => (choices, true, false),
                3 => (choices, false, false),
                _ => (vec![], true, true),
            }
        }
        _ => (vec![], false, false),
    };
    QuickAnswers { choices, reply, open }
}

pub fn after_answer(c: &Chat, tried: bool, kept: bool, now: i64) -> AfterAnswer {
    let sent = tried && kept;
    let note = match ask_state(c, now) {
        _ if tried && !kept => Some("Not sent. Try again"),
        Ask::Answered | Ask::Sending if !sent => Some("Already answered"),
        Ask::Expired => Some("This question has closed"),
        _ => None,
    };
    let answers = if !sent || c.kind == Kind::Chat { quick_answers(c, now) } else { QuickAnswers::default() };
    AfterAnswer { sent, note, answers }
}

#[cfg(test)]
#[path = "chats_tests.rs"]
mod tests;

#[cfg(test)]
#[path = "chats_sync_tests.rs"]
mod sync_tests;
