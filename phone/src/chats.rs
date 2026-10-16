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
    text.split('`').enumerate().map(|(i, part)| if i % 2 == 0 { links(&part.replace('<', "&lt;")) } else { part.to_string() }).collect::<Vec<_>>().join("`")
}

/// A bare web address (at the start, after a space or "(", not already in a markdown link) as a link: <https://…>.
/// Punctuation that ends a sentence stays outside it.
fn links(text: &str) -> String {
    let mut out = String::with_capacity(text.len());
    let mut rest = text;
    while let Some(at) = ["https://", "http://"].iter().filter_map(|p| rest.find(p)).min() {
        let (before, from) = rest.split_at(at);
        out.push_str(before);
        let len = from.find(|c: char| c.is_whitespace() || c == '>' || c == '"').unwrap_or(from.len());
        let url = from[..len].trim_end_matches(['.', ',', ';', ':', '!', '?', ')', '\'', '*', '_']);
        let mut back = out.chars().rev();
        let bare = match (back.next(), back.next()) {
            (None, _) => true,
            (Some('('), b) => b != Some(']'), // "(see https://…)", not a markdown link's "](https://…)"
            (Some(c), _) => c.is_whitespace(),
        };
        if bare && url.len() > 8 {
            out.push('<');
            out.push_str(url);
            out.push('>');
            rest = &from[url.len()..];
        } else {
            out.push_str(&from[..len]);
            rest = &from[len..];
        }
    }
    out.push_str(rest);
    out
}

/// What the field shows: its text with the keyboard's word in progress put in where the cursor is (`at`, a byte
/// offset into `draft`).
pub fn composed(draft: &str, word: &str, at: usize) -> String {
    let mut at = at.min(draft.len());
    while !draft.is_char_boundary(at) {
        at -= 1;
    }
    format!("{}{word}{}", &draft[..at], &draft[at..])
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

/// An answer's text as plain text to copy: blocks one per paragraph, list items with their marks, code exactly as
/// written, links as "text (address)", no bold or code marks.
pub fn plain(text: &str) -> String {
    let flat = |t: &str| {
        let t = t.replace("**", "").replace('`', "");
        // [text](address) -> text (address)
        let mut out = String::new();
        let mut rest = t.as_str();
        while let Some(open) = rest.find('[') {
            let Some(mid) = rest[open..].find("](").map(|i| open + i) else { break };
            let Some(end) = rest[mid..].find(')').map(|i| mid + i) else { break };
            out.push_str(&rest[..open]);
            out.push_str(&format!("{} ({})", &rest[open + 1..mid], &rest[mid + 2..end]));
            rest = &rest[end + 1..];
        }
        out + rest
    };
    let parts: Vec<String> = blocks(text)
        .iter()
        .map(|b| match b.kind {
            "code" => b.text.clone(),
            "li" => format!("{}{} {}", "  ".repeat(b.level as usize), b.mark, flat(&b.text)),
            _ => flat(&b.text),
        })
        .collect();
    parts.join("\n\n")
}

/// What a long-press on a piece of the thread copies: my message's text; a code block of the PC's alone (piece `n`
/// of its message, exactly as written); anything else of the PC's: its whole message as plain text.
pub fn copy_text(c: &Chat, line: &str, me: bool, n: usize) -> Option<String> {
    let l = c.lines.iter().find(|l| l.me == me && l.id == line)?;
    let text = match blocks(&l.text).get(n) {
        _ if me => l.text.clone(),
        Some(b) if b.kind == "code" => b.text.clone(),
        _ if l.text.is_empty() => shown(l),
        _ => plain(&l.text),
    };
    (!text.is_empty()).then_some(text)
}

/// Whether a block's text has a link in it (its long-press then stays off the words, so the link still opens).
pub fn has_link(text: &str) -> bool {
    let t = inline(text);
    t.contains("<http") || t.contains("](")
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
        c.lines.push(Line { id: msg.clone(), me: true, text: text.into(), at, since: at, first: at, state: "sending".into(), media, ..Line::default() });
        Some(msg)
    }

    /// Sends what is in a chat's text box: `text` with the photo or video picked there; its draft and pick go once
    /// the message is in (the caller saves both in one write, so nothing is lost if that fails).
    pub fn send(&mut self, chat: &str, text: &str) -> Option<String> {
        let media = self.get(chat)?.pick.clone();
        let msg = self.add_mine(chat, text, media)?;
        let c = self.get_mut(chat)?;
        (c.draft, c.pick) = (String::new(), None);
        Some(msg)
    }

    /// Sends one of my messages again as a new message (its text and file): after the PC failed or missed it.
    pub fn again(&mut self, chat: &str, msg: &str) -> Option<String> {
        let l = self.get(chat)?.lines.iter().find(|l| l.me && l.id == msg)?.clone();
        self.add_mine(chat, &l.text, l.media.first().cloned())
    }

    pub fn set_draft(&mut self, chat: &str, text: &str) {
        if let Some(c) = self.get_mut(chat).filter(|c| c.draft != text) {
            c.draft = text.into();
        }
    }

    pub fn set_pick(&mut self, chat: &str, m: Option<Media>) -> bool {
        self.get_mut(chat).map(|c| c.pick = m).is_some()
    }

    /// Whether any chat uses the file at `path` (a message's, or one picked to send).
    pub fn uses(&self, path: &str) -> bool {
        self.chats.iter().any(|c| c.pick.as_ref().is_some_and(|m| m.path == path) || c.lines.iter().flat_map(|l| &l.media).any(|m| m.path == path))
    }

    /// A chat I opened and left with nothing in it (no lines, no draft, no pick): taken out quietly.
    pub fn drop_empty(&mut self, chat: &str) -> bool {
        let empty = self.get(chat).is_some_and(|c| c.kind.is_empty() && c.lines.is_empty() && c.draft.trim().is_empty() && c.pick.is_none() && c.deleted == 0);
        empty && self.remove(chat).is_some()
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

    /// Back to "sending" for one of my messages that did not go (`msg`, or "" for my last one); returns its id. Tapped
    /// (`by_hand`), it gets a fresh AUTO_FOR of retrying by itself; else it counts one more try for the backoff.
    pub fn retry(&mut self, chat: &str, msg: &str, by_hand: bool) -> Option<String> {
        let c = self.get_mut(chat)?;
        let l = if msg.is_empty() { c.lines.iter_mut().rev().find(|l| l.me) } else { c.lines.iter_mut().find(|l| l.me && l.id == msg) }?;
        if !matches!(l.state.as_str(), "sending" | "not_sent") {
            return None;
        }
        // A fresh time, so a retry long after the first try isn't turned away as too old (same id: no double run), and
        // its file goes up again (ntfy keeps a file only 3 hours).
        let now = now();
        (l.state, l.since, l.at) = ("sending".into(), now, now);
        if l.first == 0 {
            l.first = now;
        }
        if by_hand {
            (l.first, l.tries) = (now, 0);
        } else {
            l.tries += 1;
        }
        l.media.iter_mut().for_each(|m| m.url.clear());
        Some(l.id.clone())
    }

    /// My messages that didn't go and are due to be tried again by themselves (chat, message).
    pub fn due(&self, now: i64) -> Vec<(String, String)> {
        let due = |l: &Line| unsent(l, now) == "waiting" && now >= l.since + backoff(l.tries);
        self.chats.iter().filter(|c| c.deleted == 0).flat_map(|c| c.lines.iter().filter(|l| due(l)).map(|l| (c.id.clone(), l.id.clone()))).collect()
    }

    /// My messages whose answer is missing a part (the PC is asked again: it replays a recorded answer), from 15 s
    /// after its first part came until 30 min.
    pub fn stalled(&self, now: i64) -> Vec<(String, String)> {
        let mut out = vec![];
        for c in &self.chats {
            for l in c.lines.iter().filter(|l| !l.me && l.state == "partial" && (15_000..30 * 60_000).contains(&(now - l.at))) {
                if c.lines.iter().any(|m| m.me && m.id == l.id) {
                    out.push((c.id.clone(), l.id.clone()));
                }
            }
        }
        out
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

    /// A photo or video that couldn't be fetched, fetched again (tapped, while its link still works).
    pub fn refetch(&mut self, url: &str) -> bool {
        let mut any = false;
        for m in self.chats.iter_mut().flat_map(|c| &mut c.lines).flat_map(|l| &mut l.media).filter(|m| m.url == url && m.state == "failed") {
            m.state = "loading".into();
            any = true;
        }
        any
    }

    /// Deletes a chat for now: hidden, its replies still land, until Undo (undelete) or purge.
    pub fn delete(&mut self, chat: &str, now: i64) -> bool {
        self.get_mut(chat).filter(|c| c.deleted == 0).map(|c| c.deleted = now).is_some()
    }

    pub fn undelete(&mut self, chat: &str) -> bool {
        self.get_mut(chat).filter(|c| c.deleted != 0).map(|c| c.deleted = 0).is_some()
    }

    /// The chats deleted at or before `before`, gone for good (their Undo is over); returned for their files.
    pub fn purge(&mut self, before: i64) -> Vec<Chat> {
        let ids: Vec<String> = self.chats.iter().filter(|c| c.deleted != 0 && c.deleted <= before).map(|c| c.id.clone()).collect();
        ids.iter().filter_map(|id| self.remove(id)).map(|(_, c)| c).collect()
    }

    /// The chat Undo brings back: the newest still deleted.
    pub fn undoable(&self) -> Option<&Chat> {
        self.chats.iter().filter(|c| c.deleted != 0).max_by_key(|c| c.deleted)
    }

    /// Takes a chat out (the PC's late replies to it then find nothing and are dropped); returns its place and the
    /// chat.
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

    /// A removed chat's saved files (its messages' and its pick) that no chat left uses, by file name (they all live
    /// in one folder, named by their sha256, so the same file sent twice is shared).
    pub fn unused_files(&self, gone: &Chat) -> Vec<String> {
        let file = |m: &Media| Path::new(&m.path).file_name().and_then(|n| n.to_str()).unwrap_or("").to_string();
        let all = |c: &Chat| c.lines.iter().flat_map(|l| &l.media).chain(&c.pick).map(file).collect::<Vec<_>>();
        let used: std::collections::HashSet<String> = self.chats.iter().flat_map(all).collect();
        let mut out: Vec<String> = all(gone).into_iter().filter(|f| !f.is_empty() && !used.contains(f)).collect();
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
        // Progress, not an answer: queued (behind another job in the chat), then working. Any state this phone doesn't
        // know is not taken as an answer either (it once showed as a finished "Done.").
        if !matches!(state, "done" | "failed" | "stopped" | "missed") {
            let l = &mut c.lines[mine];
            let from: &[&str] = if state == "working" { &["sending", "sent", "not_sent", "queued"] } else { &["sending", "sent", "not_sent"] };
            if matches!(state, "working" | "queued") && from.contains(&l.state.as_str()) {
                (l.state, l.since) = (state.into(), now);
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
        // a chat deleted a moment ago keeps what comes (Undo brings it back with it), without a notification
        (c.deleted == 0).then(|| (c.id.clone(), label(c), shown))
    }

    /// The PC's read receipt for one of my messages: only ever moves forward (delivered, then read); a receipt for a
    /// message not here, a lower or unknown one changes nothing. True when it changed something.
    pub fn apply_receipt(&mut self, v: &Value) -> bool {
        let rank = |s: &str| ["", "delivered", "read"].iter().position(|r| *r == s);
        let (Some(chat), Some(re), Some(new)) = (v["chat"].as_str(), v["re"].as_str(), v["state"].as_str().and_then(rank)) else { return false };
        let Some(l) = self.get_mut(chat).filter(|_| v["v"] == 1).and_then(|c| c.lines.iter_mut().find(|l| l.me && l.id == re)) else {
            return false;
        };
        if new <= rank(&l.seen).unwrap_or(0) {
            return false;
        }
        l.seen = v["state"].as_str().unwrap_or("").into();
        // the PC has it, so it went, whatever the send thought
        if matches!(l.state.as_str(), "sending" | "not_sent") {
            (l.state, l.since) = ("sent".into(), now());
        }
        true
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
        // a chat deleted a moment ago keeps what comes (Undo brings it back with it), without a notification
        (c.deleted == 0).then(|| (c.id.clone(), label(c), shown))
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
