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

#[path = "chats_store.rs"]
mod store;
#[path = "chats_text.rs"]
mod text;
pub use text::{blocks, brief, composed, copy_text, has_link, inline};
#[cfg(test)]
use text::Block;

/// What the phone lets you type, in bytes; the whole signed message must stay under ntfy's 4096.
const MAX_TEXT: usize = 2800;
const MAX_BODY: usize = 4000;

/// One bubble. Mine: `id` is the message id, `state` how it is going (sending, sent, not_sent, working, answered) and
/// `seen` the PC's read receipt ("", "delivered", "read").
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
    pub seen: String,
    /// Mine: when it was first sent (a retry gives `at` a fresh time; 0 in files from before: `at`), and how many times
    /// it has been retried by itself since it last failed for good (auto-retry's backoff).
    pub first: i64,
    pub tries: u32,
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
const HEIF: [&str; 2] = ["image/heic", "image/heif"];

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
    /// Mine: when its upload finished (ms), so a retry re-sends the same link while ntfy still holds the file.
    pub uploaded: i64,
}

/// How long a retry re-sends my file's link instead of uploading it again (ntfy keeps a file 3 hours).
const LINK_KEEP: i64 = 2 * 3600 * 1000;

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

    /// What a picked file is: the type it says it is when the phone sends that (or HEIC / HEIF, made a JPEG first),
    /// else what its first bytes say (Samsung's Gallery may name no type, or a wrong one). None: not one it can send.
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

    /// A photo type Android decodes but the PC isn't sent as is: made a JPEG on the phone first (attach.rs).
    pub fn heif(mime: &str) -> bool {
        HEIF.contains(&mime)
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
    /// What is typed in its text box and the photo or video picked to go with it, kept with the chat (and the file).
    pub draft: String,
    pub pick: Option<Media>,
    /// When it was deleted (ms): hidden, but kept (replies still land in it) until Undo is over; 0 = not deleted.
    pub deleted: i64,
}

#[derive(Serialize, Deserialize, Clone, Default, Debug, PartialEq)]
#[serde(default)] // fields added later must not make older files unreadable
pub struct Store {
    pub chats: Vec<Chat>,
    /// "chat/line" of the PC's messages in PC-started threads deleted here, so their late parts and resends are
    /// dropped instead of opening the thread again (the newest GONE_KEEP).
    pub gone: Vec<String>,
    /// The chat the photo picker is open for: it stays, empty or not, if Android closes the app meanwhile, and the
    /// pick still lands in it (attach.rs). "" when no picker is open.
    pub picking: String,
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

/// Whether a "Chat receipt" really came from the PC: "v1-receipt", the chat, the message and "delivered" / "read",
/// signed like a reply (its own prefix, so a reply's signature never passes as a receipt's).
pub fn receipt_signed(key: &str, v: &Value) -> bool {
    let s = |k: &str| v[k].as_str().unwrap_or("");
    verify(key, v, &format!("v1-receipt
{}
{}
{}", s("chat"), s("re"), s("state")))
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

/// An agent's question: "waiting" for my answer, "sending" (my answer is on its way: not yet taken by the PC),
/// "answered" or "expired" (unanswered after ASK_FOR); "" for any other chat. A message of mine answers it unless it
/// failed for good or the PC turned it away as failed (a photo it couldn't get: the question still waits).
pub fn ask_state(c: &Chat, now: i64) -> &'static str {
    let failed = |id: &str| c.lines.iter().any(|l| !l.me && l.id == id && l.state == "failed");
    let answers = || c.lines.iter().filter(|l| l.me && unsent(l, now) != "failed" && !failed(&l.id));
    // taken: the PC said so (a receipt, working, its reply), or "sent" a minute ago (a PC without receipts)
    let taken = |l: &Line| !l.seen.is_empty() || !matches!(l.state.as_str(), "sending" | "not_sent" | "sent") || now - l.since > 60_000;
    match c.kind.as_str() {
        "ask" if answers().any(|l| unsent(l, now) == "" && taken(l)) => "answered",
        "ask" if answers().next().is_some() => "sending",
        "ask" if now - c.asked > ASK_FOR => "expired",
        "ask" => "waiting",
        _ => "",
    }
}

/// What stands where a question's text box was ("": the box). The PC takes only the first answer, so once one is on
/// its way nothing more can be typed; if it fails for good the question waits again and the box comes back.
pub fn ended(ask: &str) -> &'static str {
    match ask {
        "sending" => "Sending your answer…",
        "answered" => "You've answered this. Start a new chat to say more.",
        "expired" => "This question has expired.",
        _ => "",
    }
}

/// How long my message keeps retrying by itself after it failed to go (the PC takes messages up to 30 min old).
pub const AUTO_FOR: i64 = 25 * 60_000;

/// One of my messages that did not go: "waiting" (it retries by itself, AUTO_FOR from when it was first sent) or
/// "failed" (Tap to retry); "" when it went or is still going.
pub fn unsent(l: &Line, now: i64) -> &'static str {
    let limit = if l.media.is_empty() { 15_000 } else { 120_000 }; // a photo or video goes up first, which takes longer
    let stuck = l.state == "not_sent" || l.state == "sending" && now - l.since >= limit;
    let first = if l.first > 0 { l.first } else { l.at };
    match () {
        _ if !l.me || !stuck => "",
        _ if now - first < AUTO_FOR => "waiting",
        _ => "failed",
    }
}

/// When a message that did not go is tried again by itself: 15 s, 30 s, 60 s, then every 2 min.
fn backoff(tries: u32) -> i64 {
    15_000 << tries.min(3)
}

/// Its character (actor.slint): clawd, haiku, sonnet, opus or null.
pub fn face(agent: &str, model: Option<&str>) -> &'static str {
    crate::cast::Who::named(&format!("{agent} {}", model.unwrap_or(""))).face()
}

/// Whether the PC is working on my last message (it said so: sent or queued is not thinking yet).
pub fn waiting(c: &Chat) -> bool {
    c.lines.iter().rev().find(|l| l.me).is_some_and(|l| l.state == "working")
}

/// Where the PC is with my last message, in the list's words: "Thinking", "Thinking · 3 min" after a minute,
/// "Queued" (behind another job in the chat); "" otherwise.
pub fn think(c: &Chat, now: i64) -> String {
    match c.lines.iter().rev().find(|l| l.me) {
        Some(l) if l.state == "working" && now - l.since >= 60_000 => format!("Thinking · {}", ago(now - l.since)),
        Some(l) if l.state == "working" => "Thinking".into(),
        Some(l) if l.state == "queued" => "Queued".into(),
        _ => String::new(),
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
    // its answer has started to come but a part is missing (the PC is asked again: inbox.rs)
    let partial = c.lines.iter().any(|x| !x.me && x.id == l.id && x.state == "partial" && now - x.at > 3000);
    match (l.state.as_str(), unsent(l, now)) {
        (_, "waiting") => ("Waiting for connection…", ""),
        (_, "failed") => ("Failed · Tap to retry", "retry"),
        ("sending", _) => ("Sending…", ""),
        _ if partial => ("Receiving reply…", if l.state == "working" { "stop" } else { "" }),
        ("queued", _) => ("Queued", "stop"),
        ("sent", _) if l.seen.is_empty() && now - l.since > 60_000 => ("Sent · the PC hasn't picked it up yet", ""),
        ("sent" | "working", _) => {
            // the PC working on it proves it arrived, even from a PC that sends no receipts
            let seen = match l.seen.as_str() {
                "read" => "Read",
                _ if l.seen == "delivered" || l.state == "working" => "Delivered",
                _ => "Sent",
            };
            (seen, if l.state == "working" { "stop" } else { "" })
        }
        _ => ("", ""),
    }
}

/// How much more fits in a message (the field shows it near the end): the text's bytes against MAX_TEXT, and its
/// escaped size against what the signed message leaves room for. Negative: that much too long.
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

/// What a chat's notification can answer with (Android shows at most three buttons): (its choices, Reply, Open).
/// A chat of mine: Reply. A waiting question: its choices when they fit, Reply beside one or two; more than three:
/// Reply and Open (the choices are in the app). Updates, and a question answered or closed: nothing.
pub fn quick_answers(c: &Chat, now: i64) -> (Vec<String>, bool, bool) {
    match c.kind.as_str() {
        "" => (vec![], true, false),
        "ask" if ask_state(c, now) == "waiting" => {
            let choices = c.lines.iter().find(|l| !l.me).map_or(vec![], |l| l.choices.clone());
            match choices.len() {
                0..=2 => (choices, true, false),
                3 => (choices, false, false),
                _ => (vec![], true, true),
            }
        }
        _ => (vec![], false, false),
    }
}

/// What a chat's notification shows once answered from there (`c`: the chat as it was; `tried`: my answer was added;
/// `kept`: and saved to send): (my answer shown as sent, a word on why nothing went, the buttons left). An answer that
/// couldn't be saved went nowhere: it says so and keeps the buttons to try again.
pub fn after_answer(c: &Chat, tried: bool, kept: bool, now: i64) -> (bool, Option<&'static str>, (Vec<String>, bool, bool)) {
    let sent = tried && kept;
    let note = match ask_state(c, now) {
        _ if tried && !kept => Some("Not sent. Try again"),
        "answered" | "sending" if !sent => Some("Already answered"),
        "expired" => Some("This question has closed"),
        _ => None,
    };
    // after answering a question nothing is left to tap; a chat of mine keeps Reply
    let answers = if !sent || c.kind.is_empty() { quick_answers(c, now) } else { (vec![], false, false) };
    (sent, note, answers)
}

#[cfg(test)]
mod alert_tests {
    use super::{Chat, Line, quick_answers};

    #[test]
    fn notification_buttons_fit_three() {
        let ask = |n: usize| Chat {
            kind: "ask".into(),
            asked: 1_000,
            lines: vec![Line { choices: (0..n).map(|i| format!("c{i}")).collect(), ..Line::default() }],
            ..Chat::default()
        };
        assert_eq!(quick_answers(&Chat::default(), 2_000), (vec![], true, false));
        assert_eq!(quick_answers(&ask(0), 2_000), (vec![], true, false));
        assert_eq!(quick_answers(&ask(2), 2_000), (vec!["c0".into(), "c1".into()], true, false));
        assert_eq!(quick_answers(&ask(3), 2_000).0.len(), 3);
        assert_eq!(quick_answers(&ask(3), 2_000).1, false);
        assert_eq!(quick_answers(&ask(5), 2_000), (vec![], true, true));
        assert_eq!(quick_answers(&ask(2), 1_000 + super::ASK_FOR + 1), (vec![], false, false)); // closed
        let mut answered = ask(2);
        answered.lines.push(Line { me: true, state: "sent".into(), ..Line::default() });
        assert_eq!(quick_answers(&answered, 2_000), (vec![], false, false));
        assert_eq!(quick_answers(&Chat { kind: "note".into(), ..Chat::default() }, 2_000), (vec![], false, false));
    }

    #[test]
    fn notifications_read_as_plain_words() {
        assert_eq!(super::brief("**Done.** See `main.rs`"), "Done. See main.rs");
        assert_eq!(super::brief("Fixed:\n```rust\nfn a() {}\n```\n```\nb\n```\nThat's it"), "Fixed:\n(code)\nThat's it");
        assert_eq!(super::brief("# Plan\n- one\n2. two"), "Plan\n• one\n2. two");
        assert_eq!(super::brief("see [the docs](https://x.y/a) and [b] (c)"), "see the docs and [b] (c)");
        assert_eq!(super::brief("a\nb"), "a b");
        assert_eq!(super::brief("Done."), "Done.");
    }
}

#[cfg(test)]
#[path = "chats_tests.rs"]
mod tests;
