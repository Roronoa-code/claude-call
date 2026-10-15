//! PREVIEW (branch preview/chats-4, not for main): the chats directions picked with `adb shell setprop
//! debug.callme.chats <n>` and a restart (0 or unset: today's screens; 1 Portraits, 2 Companion, 3 Pages). What the
//! directions need beyond today's models: a speaker line over each answer, which piece ends its message, a waiting
//! question's answers in the list, and the list's live character.
use crate::{ChatMsg, ChatRow, chats};
use std::sync::OnceLock;

static LOOK: OnceLock<usize> = OnceLock::new();

/// The direction showing (read once: a change needs a restart, like the other previews).
pub fn look() -> usize {
    *LOOK.get_or_init(|| crate::stage::prop(c"debug.callme.chats", 3))
}

/// Whether each answer opens with its speaker (portrait, name, when): Portraits and Pages.
fn speakers() -> bool {
    matches!(look(), 1 | 3)
}

/// The figure's scale in the small live texture: today's header size, or the list's portrait size for the
/// directions (still.slint Bust: k 0.33 fills a 52px seat).
pub fn small_k() -> f32 {
    if look() == 0 { 0.21 } else { 0.33 }
}

/// The speaker line opening the PC's message `l` in `chat` (Portraits, Pages), else None.
pub fn who(chat: &chats::Chat, l: &chats::Line, now: i64) -> Option<ChatMsg> {
    if !speakers() {
        return None;
    }
    let name = chats::label(chat);
    let short = if chat.kind.is_empty() { name.rsplit(" · ").next().unwrap_or(&name).to_string() } else { name.clone() };
    Some(ChatMsg { text: short.into(), kind: "who".into(), mark: chats::ago(now - l.at).into(), ..ChatMsg::default() })
}

/// A waiting question's answers, to answer it from the list.
pub fn choices(c: &chats::Chat, now: i64) -> Vec<slint::SharedString> {
    if chats::ask_state(c, now) != "waiting" {
        return vec![];
    }
    c.lines.iter().find(|l| !l.me).map_or(vec![], |l| l.choices.iter().map(|s| s.into()).collect())
}

/// My last message in the chat (Pages titles a chat with it), or its title.
pub fn topic(c: &chats::Chat) -> String {
    c.lines.iter().rev().find(|l| l.me && !l.text.is_empty()).map_or_else(|| c.title.clone(), |l| l.text.clone())
}

/// Companion's hero: the chat that matters now (being worked on, then a question waiting, then the newest) and one
/// line on where it stands.
pub fn hero(rows: &[ChatRow]) -> (ChatRow, String) {
    let pick = rows.iter().find(|r| r.busy).or_else(|| rows.iter().find(|r| r.asks)).or_else(|| rows.first());
    let Some(r) = pick else { return (ChatRow::default(), String::new()) };
    let line = if r.busy {
        format!("Working on \u{201c}{}\u{201d}", r.topic)
    } else if r.asks {
        "Has a question for you".to_string()
    } else if r.unread {
        "Replied to you".to_string()
    } else {
        r.preview.to_string()
    };
    (r.clone(), line)
}
