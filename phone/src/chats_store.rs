//! The store's rules (chats.rs): my messages and their sending, drafts and picks, deletes and Undo, and filing what
//! the PC sends (replies, receipts, the threads it starts).
use super::*;

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

    pub(super) fn get_mut(&mut self, chat: &str) -> Option<&mut Chat> {
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

    /// What is picked in a chat's box (None: nothing); the picker opened for it is done.
    pub fn set_pick(&mut self, chat: &str, m: Option<Media>) -> bool {
        self.picked(chat);
        self.get_mut(chat).map(|c| c.pick = m).is_some()
    }

    /// The photo picker opened for `chat`: until it answers, the chat stays even if it is empty (Store::picking).
    pub fn picking_for(&mut self, chat: &str) {
        self.picking = chat.into();
    }

    /// The picker opened for `chat` answered (or was closed).
    pub fn picked(&mut self, chat: &str) {
        if self.picking == chat {
            self.picking.clear();
        }
    }

    /// Whether any chat uses the file at `path` (a message's, or one picked to send).
    pub fn uses(&self, path: &str) -> bool {
        self.chats.iter().any(|c| c.pick.as_ref().is_some_and(|m| m.path == path) || c.lines.iter().flat_map(|l| &l.media).any(|m| m.path == path))
    }

    /// A chat I opened and left with nothing in it (no lines, no draft, no pick): taken out quietly.
    pub fn drop_empty(&mut self, chat: &str) -> bool {
        let empty = self.get(chat).is_some_and(|c| c.kind.is_empty() && c.lines.is_empty() && c.draft.trim().is_empty() && c.pick.is_none() && c.deleted == 0);
        empty && self.picking != chat && self.remove(chat).is_some()
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
            (m.url, m.uploaded) = (url.into(), now());
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
        // A fresh time, so a retry long after the first try isn't turned away as too old (same id: no double run). Its
        // file keeps the link it went up to while ntfy still holds it (3 hours); after that it goes up again.
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
        l.media.iter_mut().filter(|m| now - m.uploaded > LINK_KEEP).for_each(|m| m.url.clear());
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
        for c in self.chats.iter().filter(|c| c.deleted == 0) {
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

    /// How many chats have something new (a deleted one, its Undo still showing, doesn't count).
    pub fn needing(&self) -> usize {
        self.chats.iter().filter(|c| c.unread > 0 && c.deleted == 0).count()
    }

    /// Answers a chat from its notification (Reply's words or a question's choice): my message, and the chat read.
    /// Nothing for a chat that is gone or deleted (its Undo still showing); nothing added to a question answered or
    /// closed meanwhile. Too long to send (room): nothing goes, nothing is cut; the words wait in the chat's text box
    /// (after anything already typed there) to be shortened. Returns the chat as it was, my message's id when it was
    /// added, and whether it was too long.
    pub fn answer(&mut self, chat: &str, text: &str, now: i64) -> Option<(Chat, Option<String>, bool)> {
        let c = self.get(chat).filter(|c| c.deleted == 0)?.clone();
        let open = c.kind.is_empty() || (c.kind == "ask" && ask_state(&c, now) == "waiting");
        if open && room(text) < 0 {
            let d = &mut self.get_mut(chat)?.draft;
            *d = if d.trim().is_empty() { text.trim().into() } else { format!("{}\n\n{}", d.trim_end(), text.trim()) };
            return Some((c, None, true));
        }
        let msg = if open { self.add_mine(chat, text, None) } else { None };
        if msg.is_some() {
            self.mark_read(chat); // answered from the notification: nothing new left in it
        }
        Some((c, msg, false))
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
