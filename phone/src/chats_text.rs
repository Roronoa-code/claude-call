//! The PC's answers as the thread and notifications show them: markdown split into blocks, made safe for Slint,
//! as plain text to copy, and in plain words for a notification (chats.rs).
use super::{Chat, shown};

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

/// A block's one link address ("" with none, or with more than one): then a tap anywhere on its words opens it, and a
/// long-press copies it like any other.
pub fn only_link(text: &str) -> String {
    let t = inline(text);
    // a bare address runs to its ">" (it may hold brackets: a(b)/c); a markdown one to its ")"
    let bare = t.match_indices("<http").map(|(i, _)| &t[i + 1..]).map(|s| &s[..s.find('>').unwrap_or(s.len())]);
    let named = t.match_indices("](").map(|(i, _)| &t[i + 2..]).map(|s| &s[..s.find(')').unwrap_or(s.len())]);
    let mut all = bare.chain(named);
    match (all.next(), all.next()) {
        // only a web address opens (open-link takes nothing else): a stray "](" leaves the words as they were
        (Some(l), None) if l.starts_with("https://") || l.starts_with("http://") => l.to_string(),
        _ => String::new(),
    }
}

/// What a notification (and the chat list's preview) says for a message: plain words, one line per block, no markdown marks (`**`, backticks,
/// a link's address), a code block as "(code)".
pub fn brief(text: &str) -> String {
    let mut out: Vec<String> = vec![];
    for b in blocks(text) {
        let t = match b.kind {
            "code" => "(code)".to_string(),
            _ => {
                let t = unlinked(&b.text).replace("**", "").replace('`', "").replace('\n', " ");
                if b.kind == "li" { format!("{} {t}", b.mark) } else { t }
            }
        };
        if !(t == "(code)" && out.last().is_some_and(|l| l == "(code)")) {
            out.push(t);
        }
    }
    out.join("\n")
}

/// "[words](address)" as just the words.
fn unlinked(text: &str) -> String {
    let mut out = String::with_capacity(text.len());
    let mut rest = text;
    while let Some(open) = rest.find('[') {
        let after = &rest[open + 1..];
        match after.find("](").map(|m| (m, after[m + 2..].find(')'))) {
            Some((m, Some(close))) if !after[..m].contains('[') => {
                out.push_str(&rest[..open]);
                out.push_str(&after[..m]);
                rest = &after[m + 2 + close + 1..];
            }
            _ => {
                out.push_str(&rest[..=open]);
                rest = after;
            }
        }
    }
    out.push_str(rest);
    out
}
