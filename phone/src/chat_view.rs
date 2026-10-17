use crate::{
    ChatMsg, ChatRow,
    chats::{self, Media},
    attach::thumb,
};

const LINK_FOR: i64 = chats::NTFY_FILE_TTL - 10 * 60_000;

pub fn choices(c: &chats::Chat, now: i64) -> Vec<slint::SharedString> {
    chats::open_choices(c, now).iter().map(|s| s.into()).collect()
}

pub fn topic(c: &chats::Chat) -> String {
    match c.lines.iter().rev().find(|l| l.me && !l.text.is_empty()) {
        Some(l) => l.text.clone(),
        None if c.title.trim().is_empty() => "New chat".into(),
        None => c.title.clone(),
    }
}

pub fn hero(rows: &[ChatRow]) -> (ChatRow, String) {
    let pick = rows.iter().find(|r| r.asks).or_else(|| rows.iter().find(|r| r.busy)).or_else(|| rows.first());
    let Some(r) = pick else { return (ChatRow::default(), String::new()) };
    let asked = short(&r.topic, 44);
    let mine = r.topic != r.name && r.topic != "New chat";
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
    } else if r.mine_last {
        format!("Sent \u{201c}{asked}\u{201d}")
    } else {
        format!("Done: {asked}")
    };
    (r.clone(), line)
}

fn short(s: &str, n: usize) -> String {
    let s = s.split_whitespace().collect::<Vec<_>>().join(" ");
    if s.chars().count() <= n {
        return s;
    }
    let cut: String = s.chars().take(n).collect();
    let cut = cut.rsplit_once(' ').map_or(cut.as_str(), |(a, _)| a).trim_end_matches([',', '.', ':', ';']);
    format!("{cut}\u{2026}")
}

pub fn arrange(mut rows: Vec<ChatRow>) -> Vec<ChatRow> {
    let group = |r: &ChatRow| if r.asks || r.unread { 0 } else if r.busy { 1 } else { 2 };
    rows.sort_by_key(group);
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

fn media_msg(me: bool, m: &Media, at: i64, now: i64) -> ChatMsg {
    let what = if m.video() { "video" } else { "photo" };
    let note = |text: String| ChatMsg { me, text: text.into(), kind: "note".into(), ..ChatMsg::default() };
    match m.state {
        chats::MediaState::Loading => note(format!("Downloading {what}…")),
        chats::MediaState::Ready => {
            match thumb(m) {
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

pub fn thread(chat: &chats::Chat, ask: chats::Ask, now: i64) -> Vec<ChatMsg> {
    let lines: Vec<_> = chat.lines.iter().filter(|l| l.me || l.state != chats::State::Partial).collect();
    let question = lines.iter().position(|l| !l.me).filter(|_| ask != chats::Ask::NotAsked);
    let sent = |c: &str| chat.lines.iter().any(|l| l.me && chats::unsent(l, now) != chats::Unsent::Failed && l.text == c);
    let last_mine = chat.lines.iter().rev().find(|l| l.me).map(|l| l.id.as_str());
    let mut msgs: Vec<ChatMsg> = vec![];
    for (i, l) in lines.iter().enumerate() {
        let mut items: Vec<ChatMsg> = vec![];
        if l.me {
            items.extend(l.media.iter().map(|m| media_msg(true, m, l.at, now)));
            if !l.text.is_empty() || l.media.is_empty() {
                items.push(ChatMsg { me: true, text: chats::shown(l).into(), ..ChatMsg::default() });
            }
            match chats::unsent(l, now) {
                chats::Unsent::No => {}
                _ if Some(l.id.as_str()) == last_mine => {}
                chats::Unsent::Failed => items.push(ChatMsg { me: true, text: "Not sent · Tap to retry".into(), kind: "unsent".into(), mark: "retry".into(), ..ChatMsg::default() }),
                _ => items.push(ChatMsg { me: true, text: "Waiting for connection…".into(), kind: "unsent".into(), ..ChatMsg::default() }),
            }
        } else {
            if l.text.is_empty() && l.media.is_empty() {
                items.push(ChatMsg { text: chats::shown(l).into(), kind: "note".into(), ..ChatMsg::default() });
            }
            for b in chats::blocks(&l.text) {
                let body = match b.kind {
                    "code" => slint::StyledText::default(),
                    "h" => styled(&format!("**{}**", b.text.replace("**", ""))),
                    _ => styled(&b.text),
                };
                let links = b.kind != "code" && chats::has_link(&b.text);
                let link = if links { chats::only_link(&b.text) } else { String::new() };
                let text = if b.kind == "code" { soft_breaks(&b.text) } else { b.text.clone() };
                let (kind, mark, level) = (b.kind.into(), b.mark.into(), b.level as i32);
                items.push(ChatMsg { text: text.into(), kind, body, mark, level, links, link: link.into(), ..ChatMsg::default() });
            }
            items.extend(l.media.iter().map(|m| media_msg(false, m, l.at, now)));
            if question == Some(i) {
                let choice = |c: &String| ChatMsg { text: c.into(), kind: "choice".into(), mark: if sent(c) { "on" } else { "" }.into(), ..ChatMsg::default() };
                items.extend(l.choices.iter().map(choice));
                match ask {
                    chats::Ask::Expired => items.push(ChatMsg { text: "Expired".into(), kind: "tag".into(), ..ChatMsg::default() }),
                    chats::Ask::Sending => items.push(ChatMsg { text: "Waiting for the PC".into(), kind: "tag".into(), ..ChatMsg::default() }),
                    _ => {}
                }
            }
            if chat.kind == chats::Kind::Chat && i + 1 == lines.len() && matches!(l.state, chats::State::Failed | chats::State::Missed) && Some(l.id.as_str()) == last_mine {
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
                (Some(true), 0) if l.me => 8.0,
                (Some(_), 0) => 26.0,
                _ => match (prev.as_str(), m.kind.as_str()) {
                    (_, "h") => 22.0,
                    ("h", _) => 8.0,
                    ("li", "li") | ("choice", "choice") => 8.0,
                    (_, "tag") => 10.0,
                    (_, "unsent") => 2.0,
                    _ if l.me => 6.0,
                    _ => 14.0,
                },
            };
            m.anchor = n == 0 && !l.me && i + 1 == lines.len();
            msgs.push(m);
        }
    }
    msgs
}

fn styled(text: &str) -> slint::StyledText {
    slint::StyledText::from_markdown(&chats::inline(text)).unwrap_or_else(|_| slint::StyledText::from_plain_text(text))
}

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
