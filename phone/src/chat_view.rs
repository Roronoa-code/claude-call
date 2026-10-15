//! What the Chats screens show beyond the store itself (inbox.rs show): a waiting question's answers and my last
//! message for a list card, who matters now (the character at the top of the list, with one line on where it stands),
//! and the list's groups.
use crate::{ChatRow, chats};

/// A waiting question's answers.
pub fn choices(c: &chats::Chat, now: i64) -> Vec<slint::SharedString> {
    if chats::ask_state(c, now) != "waiting" {
        return vec![];
    }
    c.lines.iter().find(|l| !l.me).map_or(vec![], |l| l.choices.iter().map(|s| s.into()).collect())
}

/// My last message in the chat (a card's title), or its title.
pub fn topic(c: &chats::Chat) -> String {
    c.lines.iter().rev().find(|l| l.me && !l.text.is_empty()).map_or_else(|| c.title.clone(), |l| l.text.clone())
}

/// Who matters now: the chat being worked on, then a question waiting, then the newest; and one line on where it
/// stands.
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

/// The list in three groups, each headed by its first row: what needs Mani (a question, a new reply), what the PC is
/// working on, and the rest; newest first within each. The chat standing at the top (`hero`) heads no group: the list
/// doesn't show it again.
pub fn arrange(mut rows: Vec<ChatRow>, hero: &str) -> Vec<ChatRow> {
    let group = |r: &ChatRow| if r.asks || r.unread { 0 } else if r.busy { 1 } else { 2 };
    rows.sort_by_key(group); // stable: newest first stays within each group
    let mut last = None;
    for r in rows.iter_mut().filter(|r| r.id != hero) {
        let g = group(r);
        if last != Some(g) {
            r.head = ["Needs you", "Working on it", "Earlier"][g].into();
        }
        last = Some(g);
    }
    rows
}
