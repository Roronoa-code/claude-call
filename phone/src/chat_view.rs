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

/// My last message in the chat (a card's title), or its title, or "New chat" (the list's preview of an empty one).
pub fn topic(c: &chats::Chat) -> String {
    match c.lines.iter().rev().find(|l| l.me && !l.text.is_empty()) {
        Some(l) => l.text.clone(),
        None if c.title.trim().is_empty() => "New chat".into(),
        None => c.title.clone(),
    }
}

/// Who matters now: the chat being worked on, then a question waiting, then the newest; and a short line on where it
/// stands (never the reply itself): "Has a question for you", "Working on “…”", "Replied to “…”", "Done: …".
pub fn hero(rows: &[ChatRow]) -> (ChatRow, String) {
    let pick = rows.iter().find(|r| r.busy).or_else(|| rows.iter().find(|r| r.asks)).or_else(|| rows.first());
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
