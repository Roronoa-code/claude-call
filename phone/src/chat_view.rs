//! What the Chats screens show beyond the store itself (inbox.rs show): the open chat's pieces, a waiting question's
//! answers and my last message for a list card, who matters now (the character at the top of the list, with one line on where it stands),
//! and the list's groups.
use crate::{
    ChatMsg, ChatRow,
    chats::{self, Media},
    attach::thumb,
};

/// How long the PC's photo and video links work (ntfy keeps a file 3 hours), with a margin.
const LINK_FOR: i64 = (3 * 60 - 10) * 60_000;

/// A waiting question's answers.
pub fn choices(c: &chats::Chat, now: i64) -> Vec<slint::SharedString> {
    if chats::ask_state(c, now) != "waiting" {
        return vec![];
    }
    c.lines.iter().find(|l| !l.me).map_or(vec![], |l| l.choices.iter().map(|s| s.into()).collect())
}

/// My last message in the chat (a card's title), or its title, or "New chat" (the list's preview of an empty one).
pub fn topic(c: &chats::Chat) -> String {
    match c.lines.iter().rev().find(|l| l.me && !l.text.is_empty()) {
        Some(l) => l.text.clone(),
        None if c.title.trim().is_empty() => "New chat".into(),
        None => c.title.clone(),
    }
}

/// Who matters now: a question waiting for Mani, then the chat being worked on, then the newest; and a short line on where it
/// stands (never the reply itself): "Has a question for you", "Working on “…”", "Replied to “…”", "Done: …".
pub fn hero(rows: &[ChatRow]) -> (ChatRow, String) {
    let pick = rows.iter().find(|r| r.asks).or_else(|| rows.iter().find(|r| r.busy)).or_else(|| rows.first());
    let Some(r) = pick else { return (ChatRow::default(), String::new()) };
    let asked = short(&r.topic, 44);
    let mine = r.topic != r.name && r.topic != "New chat"; // a PC-started thread (Updates) has nothing of mine
    let line = if r.asks {
        "Has a question for you".to_string()
    } else if r.busy {
        format!("Working on \u{201c}{asked}\u{201d}")
    } else if r.topic == "New chat" {
        "Ready when you are".into()
    } else if !mine {
        if r.unread { "Has something new for you".into() } else { "All caught up".into() }
    } else if r.unread {
        format!("Replied to \u{201c}{asked}\u{201d}")
    } else if r.think == "Queued" {
        format!("Queued: \u{201c}{asked}\u{201d}")
    } else if r.preview == r.topic {
        // my message is the last word: not answered yet (sending, sent, or the PC hasn't picked it up)
        format!("Sent \u{201c}{asked}\u{201d}")
    } else {
        format!("Done: {asked}")
    };
    (r.clone(), line)
}

/// `s` in at most `n` characters, ending in "…" when cut (at a word where it can).
fn short(s: &str, n: usize) -> String {
    let s = s.split_whitespace().collect::<Vec<_>>().join(" ");
    if s.chars().count() <= n {
        return s;
    }
    let cut: String = s.chars().take(n).collect();
    let cut = cut.rsplit_once(' ').map_or(cut.as_str(), |(a, _)| a).trim_end_matches([',', '.', ':', ';']);
    format!("{cut}\u{2026}")
}

/// The list in three groups, each headed by its first row: what needs Mani (a question, a new reply), what the PC is
/// working on, and the rest; newest first within each.
pub fn arrange(mut rows: Vec<ChatRow>) -> Vec<ChatRow> {
    let group = |r: &ChatRow| if r.asks || r.unread { 0 } else if r.busy { 1 } else { 2 };
    rows.sort_by_key(group); // stable: newest first stays within each group
    let mut last = None;
    for r in rows.iter_mut() {
        let g = group(r);
        if last != Some(g) {
            r.head = ["Needs you", "Working on it", "Earlier"][g].into();
        }
        last = Some(g);
    }
    rows
}

/// The bubble for a photo or video: its thumbnail once it is here, else a grey note. One of the PC's that couldn't be
/// fetched can be tapped to fetch it again while its link still works (`at`: when it came).
fn media_msg(me: bool, m: &Media, at: i64, now: i64) -> ChatMsg {
    let what = if m.video() { "video" } else { "photo" };
    let note = |text: String| ChatMsg { me, text: text.into(), kind: "note".into(), ..ChatMsg::default() };
    match m.state.as_str() {
        "loading" => note(format!("Downloading {what}…")),
        "ready" => {
            match thumb(m) {
                // A video whose frame can't be read still gets its card (and plays).
                None if !m.video() => note(format!("Couldn't show this {what}")),
                t => {
                    let (pic, ratio) = t.unwrap_or_else(|| (slint::Image::default(), 9.0 / 16.0));
                    ChatMsg { me, kind: if m.video() { "video" } else { "image" }.into(), pic, ratio, path: m.path.clone().into(), ..ChatMsg::default() }
                }
            }
        }
        _ if !me && now - at < LINK_FOR => {
            ChatMsg { mark: "refetch".into(), path: m.url.clone().into(), ..note(format!("Couldn't load this {what} · Tap to retry")) }
        }
        _ => note(format!("Couldn't load this {what}")),
    }
}

/// The open chat as the thread lays it out: my messages as bubbles (a photo or video above its text; an earlier one
/// that didn't go says so under it), the PC's as blocks of text (chats::blocks) then its files, each with the room
/// above it (`gap`), its place in its message (`n`, for the arrival's stagger) and its message's id (`line`, for
/// copying). The first block of an answer that is the newest line is the `anchor`. An agent's question (`ask`:
/// chats::ask_state) offers its answers under it as "choice" buttons (`mark` "on": the one I sent), and says "Sending
/// answer…" or "Expired" (a "tag") once nobody waits for it. An answer the PC failed or missed, to my last message,
/// offers "Send again".
pub fn thread(chat: &chats::Chat, ask: &str, now: i64) -> Vec<ChatMsg> {
    let lines: Vec<_> = chat.lines.iter().filter(|l| l.me || l.state != "partial").collect();
    let question = lines.iter().position(|l| !l.me).filter(|_| ask != "");
    let sent = |c: &str| chat.lines.iter().any(|l| l.me && chats::unsent(l, now) != "failed" && l.text == c);
    let last_mine = chat.lines.iter().rev().find(|l| l.me).map(|l| l.id.as_str());
    let mut msgs: Vec<ChatMsg> = vec![];
    for (i, l) in lines.iter().enumerate() {
        let mut items: Vec<ChatMsg> = vec![];
        if l.me {
            items.extend(l.media.iter().map(|m| media_msg(true, m, l.at, now)));
            if !l.text.is_empty() || l.media.is_empty() {
                items.push(ChatMsg { me: true, text: chats::shown(l).into(), ..ChatMsg::default() });
            }
            // my last message says how it is going in the status line; an earlier one that didn't go, here
            match chats::unsent(l, now) {
                "" => {}
                _ if Some(l.id.as_str()) == last_mine => {}
                "failed" => items.push(ChatMsg { me: true, text: "Not sent · Tap to retry".into(), kind: "unsent".into(), mark: "retry".into(), ..ChatMsg::default() }),
                _ => items.push(ChatMsg { me: true, text: "Waiting for connection…".into(), kind: "unsent".into(), ..ChatMsg::default() }),
            }
        } else {
            if l.text.is_empty() && l.media.is_empty() {
                // "Something went wrong.", "Stopped." and the like: a quiet line, not an answer
                items.push(ChatMsg { text: chats::shown(l).into(), kind: "note".into(), ..ChatMsg::default() });
            }
            for b in chats::blocks(&l.text) {
                let body = match b.kind {
                    "code" => slint::StyledText::default(),
                    "h" => styled(&format!("**{}**", b.text.replace("**", ""))),
                    _ => styled(&b.text),
                };
                let links = b.kind != "code" && chats::has_link(&b.text);
                let text = if b.kind == "code" { soft_breaks(&b.text) } else { b.text.clone() };
                items.push(ChatMsg { text: text.into(), kind: b.kind.into(), body, mark: b.mark.into(), level: b.level as i32, links, ..ChatMsg::default() });
            }
            items.extend(l.media.iter().map(|m| media_msg(false, m, l.at, now)));
            if question == Some(i) {
                let choice = |c: &String| ChatMsg { text: c.into(), kind: "choice".into(), mark: if sent(c) { "on" } else { "" }.into(), ..ChatMsg::default() };
                items.extend(l.choices.iter().map(choice));
                match ask {
                    "expired" => items.push(ChatMsg { text: "Expired".into(), kind: "tag".into(), ..ChatMsg::default() }),
                    "sending" => items.push(ChatMsg { text: "Sending answer…".into(), kind: "tag".into(), ..ChatMsg::default() }),
                    _ => {}
                }
            }
            if chat.kind.is_empty() && i + 1 == lines.len() && matches!(l.state.as_str(), "failed" | "missed") && Some(l.id.as_str()) == last_mine {
                items.push(ChatMsg { text: "Send again".into(), kind: "again".into(), ..ChatMsg::default() });
            }
        }
        let mine_before = msgs.last().map(|m| m.me);
        let count = items.len();
        for (n, mut m) in items.into_iter().enumerate() {
            m.last = n + 1 == count;
            m.line = l.id.clone().into();
            let prev = msgs.last().map_or(String::new(), |p| p.kind.to_string());
            m.n = n as i32;
            m.gap = match (mine_before, n) {
                (None, 0) => 0.0,
                (Some(true), 0) if l.me => 8.0, // my messages in a row
                (Some(_), 0) => 26.0, // the other side speaks, or the PC's next answer
                _ => match (prev.as_str(), m.kind.as_str()) {
                    (_, "h") => 22.0,
                    ("h", _) => 8.0,
                    ("li", "li") | ("choice", "choice") => 8.0,
                    (_, "tag") => 10.0,
                    (_, "unsent") => 2.0,
                    _ if l.me => 6.0, // my photo, then its text
                    _ => 14.0,
                },
            };
            m.anchor = n == 0 && !l.me && i + 1 == lines.len();
            msgs.push(m);
        }
    }
    msgs
}

/// Inline markdown for the thread; text Slint's parser can't take shows as it is.
fn styled(text: &str) -> slint::StyledText {
    slint::StyledText::from_markdown(&chats::inline(text)).unwrap_or_else(|_| slint::StyledText::from_plain_text(text))
}

/// A code block's text with a zero-width break after `_ . / : (` inside long words, so a line that has to wrap breaks
/// there ("BOOT_ / COMPLETED") rather than mid-word.
pub fn soft_breaks(code: &str) -> String {
    let mut out = String::with_capacity(code.len() + 16);
    for (i, word) in code.split(' ').enumerate() {
        if i > 0 {
            out.push(' ');
        }
        if word.chars().count() < 16 {
            out.push_str(word);
            continue;
        }
        for c in word.chars() {
            out.push(c);
            if matches!(c, '_' | '.' | '/' | ':' | '(') {
                out.push('\u{200b}');
            }
        }
    }
    out
}

#[cfg(test)]
mod tests {
    #[test]
    fn short_cuts_at_a_word() {
        assert_eq!(super::short("why is the release build slow on the laptop today", 30), "why is the release build slow\u{2026}");
        assert_eq!(super::short("short one", 30), "short one");
    }

    #[test]
    fn breaks_only_long_words_at_natural_points() {
        assert_eq!(super::soft_breaks("a.b BOOT_COMPLETED_NOW"), "a.b BOOT_\u{200b}COMPLETED_\u{200b}NOW");
    }
}
