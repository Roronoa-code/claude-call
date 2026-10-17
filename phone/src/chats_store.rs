use super::*;

impl Store {
    pub fn load(path: &Path) -> Option<Self> {
        match std::fs::read(path) {
            Ok(b) => serde_json::from_slice(&b).ok(),
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => Some(Self::default()),
            Err(_) => None,
        }
    }

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

    pub fn add_mine(&mut self, chat: &str, text: &str, media: Option<Media>) -> Option<String> {
        let mut text = cut(text.trim(), MAX_TEXT);
        while serde_json::to_string(text).ok()?.len() > MAX_BODY - 500 {
            text = &text[..text.char_indices().last()?.0];
        }
        let c = self.get_mut(chat).filter(|_| !text.is_empty() || media.is_some())?;
        let (at, msg) = (now(), id("m"));
        if c.title.is_empty() {
            c.title = match (text, &media) {
                ("", Some(m)) => if m.video() { "Video" } else { "Photo" }.into(),
                _ => text.lines().next().unwrap_or("").chars().take(120).collect(),
            };
        }
        c.updated = at;
        let media = media.into_iter().map(|m| Media { part: 1, url: String::new(), ..m }).collect();
        c.lines.push(Line { id: msg.clone(), me: true, text: text.into(), at, since: at, first: at, state: State::Sending, media, ..Line::default() });
        Some(msg)
    }

    pub fn send(&mut self, chat: &str, text: &str, with_pick: bool) -> Option<String> {
        let media = self.get(chat)?.pick.clone().filter(|_| with_pick);
        let msg = self.add_mine(chat, text, media)?;
        let c = self.get_mut(chat)?;
        (c.draft, c.pick) = (String::new(), None);
        Some(msg)
    }

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
        self.picked(chat);
        self.get_mut(chat).map(|c| c.pick = m).is_some()
    }

    pub fn picking_for(&mut self, chat: &str) {
        self.picking = chat.into();
    }

    pub fn picked(&mut self, chat: &str) {
        if self.picking == chat {
            self.picking.clear();
        }
    }

    pub fn uses(&self, path: &str) -> bool {
        self.chats.iter().any(|c| c.pick.as_ref().is_some_and(|m| m.path == path) || c.lines.iter().flat_map(|l| &l.media).any(|m| m.path == path))
    }

    pub fn drop_empty(&mut self, chat: &str) -> bool {
        let empty = self.get(chat).is_some_and(|c| c.kind == Kind::Chat && c.lines.is_empty() && c.draft.trim().is_empty() && c.pick.is_none() && c.deleted == 0);
        empty && self.picking != chat && self.remove(chat).is_some()
    }

    pub fn body(&self, chat: &str, msg: &str, key: &str) -> Option<String> {
        let c = self.get(chat)?;
        let l = c.lines.iter().find(|l| l.me && l.id == msg)?;
        let model = c.model.as_deref();
        let att = l.media.first();
        if att.is_some_and(|m| m.url.is_empty()) {
            return None;
        }
        let sig = sign(key, c, l);
        let mut v = json!({"v": 1, "chat": chat, "msg": msg, "agent": c.agent, "model": model, "ts": l.at, "text": l.text, "sig": sig});
        if let Some(m) = att {
            v["att"] = json!({"url": m.url, "name": m.name, "type": m.mime, "size": m.size, "sha256": m.sha256});
        }
        Some(v.to_string())
    }

    pub fn to_upload(&self, chat: &str, msg: &str) -> Option<Media> {
        let l = self.get(chat)?.lines.iter().find(|l| l.me && l.id == msg)?;
        l.media.first().filter(|m| m.url.is_empty()).cloned()
    }

    pub fn uploaded(&mut self, chat: &str, msg: &str, url: &str) {
        if let Some(m) = self.get_mut(chat).and_then(|c| c.lines.iter_mut().find(|l| l.me && l.id == msg)).and_then(|l| l.media.first_mut()) {
            (m.url, m.uploaded) = (url.into(), now());
        }
    }

    pub fn sent(&mut self, chat: &str, msg: &str, ok: bool) {
        if let Some(l) = self.get_mut(chat).and_then(|c| c.lines.iter_mut().find(|l| l.me && l.id == msg && l.state == State::Sending)) {
            (l.state, l.since) = (if ok { State::Sent } else { State::NotSent }, now());
        }
    }

    pub fn retry(&mut self, chat: &str, msg: &str, by_hand: bool) -> Option<String> {
        let c = self.get_mut(chat)?;
        let l = if msg.is_empty() { c.lines.iter_mut().rev().find(|l| l.me) } else { c.lines.iter_mut().find(|l| l.me && l.id == msg) }?;
        if !matches!(l.state, State::Sending | State::NotSent) {
            return None;
        }
        let now = now();
        (l.state, l.since, l.at) = (State::Sending, now, now);
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

    pub fn due(&self, now: i64) -> Vec<(String, String)> {
        let clock_went_back = |since: i64| since > now;
        let due = |l: &Line| !l.mirrored && unsent(l, now) == Unsent::Waiting && (now >= l.since + backoff(l.tries) || clock_went_back(l.since));
        self.chats.iter().filter(|c| c.deleted == 0).flat_map(|c| c.lines.iter().filter(|l| due(l)).map(|l| (c.id.clone(), l.id.clone()))).collect()
    }

    pub fn stalled(&self, now: i64) -> Vec<(String, String)> {
        let mut out = vec![];
        for c in self.chats.iter().filter(|c| c.deleted == 0) {
            for l in c.lines.iter().filter(|l| !l.me && l.state == State::Partial && (15_000..30 * 60_000).contains(&(now - l.at))) {
                if c.lines.iter().any(|m| m.me && !m.mirrored && m.id == l.id) {
                    out.push((c.id.clone(), l.id.clone()));
                }
            }
        }
        out
    }

    pub fn unanswered(&self, now: i64) -> Vec<(String, String, i64)> {
        let mut out = vec![];
        for c in self.chats.iter().filter(|c| c.deleted == 0 && ask_state(c, now) == Ask::Sending) {
            for l in c.lines.iter().filter(|l| l.me && !l.mirrored && l.state == State::Sent && now - l.since >= 60_000) {
                if !c.lines.iter().any(|x| !x.me && x.id == l.id) {
                    out.push((c.id.clone(), l.id.clone(), l.since));
                }
            }
        }
        out
    }

    pub fn downloads(&self) -> Vec<Media> {
        let mut out: Vec<Media> = vec![];
        for m in self.chats.iter().flat_map(|c| &c.lines).flat_map(|l| &l.media).filter(|m| m.state == MediaState::Loading) {
            if !out.iter().any(|o| o.url == m.url) {
                out.push(m.clone());
            }
        }
        out
    }

    pub fn media_done(&mut self, url: &str, path: Option<&str>) {
        for m in self.chats.iter_mut().flat_map(|c| &mut c.lines).flat_map(|l| &mut l.media).filter(|m| m.url == url) {
            (m.state, m.path) = (if path.is_some() { MediaState::Ready } else { MediaState::Failed }, path.unwrap_or("").into());
        }
    }

    pub fn refetch(&mut self, url: &str) -> bool {
        let mut any = false;
        for m in self.chats.iter_mut().flat_map(|c| &mut c.lines).flat_map(|l| &mut l.media).filter(|m| m.url == url && m.state == MediaState::Failed) {
            m.state = MediaState::Loading;
            any = true;
        }
        any
    }

    pub fn delete(&mut self, chat: &str, now: i64) -> bool {
        self.get_mut(chat).filter(|c| c.deleted == 0).map(|c| c.deleted = now).is_some()
    }

    pub fn undelete(&mut self, chat: &str) -> bool {
        self.get_mut(chat).filter(|c| c.deleted != 0).map(|c| c.deleted = 0).is_some()
    }

    pub fn purge(&mut self, before: i64) -> Vec<Chat> {
        let ids: Vec<String> = self.chats.iter().filter(|c| c.deleted != 0 && c.deleted <= before).map(|c| c.id.clone()).collect();
        ids.iter().filter_map(|id| self.remove(id)).map(|(_, c)| c).collect()
    }

    pub fn undoable(&self) -> Option<&Chat> {
        self.chats.iter().filter(|c| c.deleted != 0).max_by_key(|c| c.deleted)
    }

    pub fn remove(&mut self, chat: &str) -> Option<(usize, Chat)> {
        let i = self.chats.iter().position(|c| c.id == chat)?;
        let gone = self.chats.remove(i);
        if gone.kind == Kind::Chat {
            self.gone.push(format!("{chat}/"));
        } else {
            self.gone.extend(gone.lines.iter().filter(|l| !l.me).map(|l| format!("{}/{}", gone.id, l.id)));
        }
        let extra = self.gone.len().saturating_sub(GONE_KEEP);
        self.gone.drain(..extra);
        Some((i, gone))
    }

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

    pub fn needing(&self) -> usize {
        self.chats.iter().filter(|c| c.unread > 0 && c.deleted == 0).count()
    }

    pub fn answer(&mut self, chat: &str, text: &str, now: i64) -> Option<Answered> {
        let c = self.get(chat).filter(|c| c.deleted == 0)?.clone();
        let open = c.kind == Kind::Chat || (c.kind == Kind::Ask && ask_state(&c, now) == Ask::Waiting);
        if open && room(text) < 0 {
            let d = &mut self.get_mut(chat)?.draft;
            *d = if d.trim().is_empty() { text.trim().into() } else { format!("{}\n\n{}", d.trim_end(), text.trim()) };
            return Some(Answered { chat: c, msg: None, too_long: true });
        }
        let msg = if open { self.add_mine(chat, text, None) } else { None };
        if msg.is_some() {
            self.mark_read(chat);
        }
        Some(Answered { chat: c, msg, too_long: false })
    }

    pub fn apply_mine(&mut self, v: &Value) -> Option<()> {
        let id = |s: &str| (1..=40).contains(&s.len()) && s.chars().all(|c| c.is_ascii_alphanumeric() || c == '_' || c == '-');
        let (chat, msg, agent, text, at) = (v["chat"].as_str()?, v["msg"].as_str()?, v["agent"].as_str()?, v["text"].as_str()?, v["ts"].as_i64()?);
        let model = match v.get("model") {
            None | Some(Value::Null) => None,
            Some(Value::String(m)) if m.len() <= 100 && m.chars().all(|c| c.is_ascii_alphanumeric() || "._-".contains(c)) => Some(m.clone()),
            _ => return None,
        };
        let media = match v.get("att") {
            None => vec![],
            Some(a) => vec![Media::from(a, 1)?],
        };
        if v["v"] != 1 || !id(chat) || !id(msg) || !matches!(agent, "claude" | "codex") || at <= 0
            || text.len() > MAX_TEXT || (text.trim().is_empty() && media.is_empty()) || self.gone.iter().any(|g| *g == format!("{chat}/")) {
            return None;
        }
        if self.get(chat).is_some_and(|c| c.agent != agent || c.model != model) { return None }
        if self.get(chat).is_none() {
            // Ask replies need their original question; do not turn them into ordinary chats.
            if !chat.starts_with('c') { return None }
            let title = if text.trim().is_empty() { if media[0].video() { "Video" } else { "Photo" }.into() }
                else { text.lines().next().unwrap_or("").chars().take(120).collect() };
            self.chats.push(Chat { id: chat.into(), agent: agent.into(), model, title, ..Chat::default() });
        }
        let c = self.get_mut(chat)?;
        if c.lines.iter().any(|l| l.me && l.id == msg) { return None }
        c.updated = c.updated.max(at);
        let line = Line { id: msg.into(), me: true, mirrored: true, text: text.into(), at, since: at, first: at, state: State::Sent, media, ..Line::default() };
        c.lines.push(line);
        Some(())
    }

    pub fn apply_reply(&mut self, v: &Value, open: Option<&str>) -> Option<(String, String, String)> {
        self.reply_at(v, open, now())
    }

    pub fn reply_at(&mut self, v: &Value, open: Option<&str>, now: i64) -> Option<(String, String, String)> {
        let (chat, re, state) = (v["chat"].as_str()?, v["re"].as_str()?, v["state"].as_str()?);
        let c = self.get_mut(chat).filter(|_| v["v"] == 1)?;
        let mine = c.lines.iter().position(|l| l.me && l.id == re)?;
        let state = wire::<State>(state);
        let Some(state @ (State::Done | State::Failed | State::Stopped | State::Missed)) = state else {
            let l = &mut c.lines[mine];
            let from: &[State] = if state == Some(State::Working) {
                &[State::Sending, State::Sent, State::NotSent, State::Queued]
            } else {
                &[State::Sending, State::Sent, State::NotSent]
            };
            if let Some(state @ (State::Working | State::Queued)) = state.filter(|_| from.contains(&l.state)) {
                (l.state, l.since) = (state, now);
            }
            return None;
        };
        let at = c.lines.iter().position(|l| !l.me && l.id == re);
        if at.is_some_and(|i| c.lines[i].state != State::Partial) {
            return None;
        }
        let i = at.unwrap_or_else(|| {
            c.lines.push(Line { id: re.into(), at: now, state, ..Line::default() });
            c.lines.len() - 1
        });
        let l = &mut c.lines[i];
        if state == State::Done {
            if !file_part(l, v) {
                return None;
            }
        } else {
            (l.state, l.text) = (state, v["text"].as_str().unwrap_or("").into());
        }
        l.at = now;
        let shown = shown(l);
        c.lines[mine].state = State::Answered;
        c.updated = c.updated.max(now);
        if open == Some(chat) {
            return None;
        }
        c.unread += 1;
        (c.deleted == 0).then(|| (c.id.clone(), label(c), shown))
    }

    pub fn apply_receipt(&mut self, v: &Value) -> bool {
        let (Some(chat), Some(re), Some(new)) = (v["chat"].as_str(), v["re"].as_str(), v["state"].as_str().and_then(wire::<Seen>)) else { return false };
        let Some(l) = self.get_mut(chat).filter(|_| v["v"] == 1).and_then(|c| c.lines.iter_mut().find(|l| l.me && l.id == re)) else {
            return false;
        };
        if new <= l.seen {
            return false;
        }
        l.seen = new;
        if matches!(l.state, State::Sending | State::NotSent) {
            (l.state, l.since) = (State::Sent, now());
        }
        true
    }

    pub fn apply_note(&mut self, v: &Value, open: Option<&str>) -> Option<(String, String, String)> {
        let id = |s: &str| (1..=40).contains(&s.len()) && s.chars().all(|c| c.is_ascii_alphanumeric() || c == '_' || c == '-');
        let (chat, note, title) = (v["chat"].as_str()?, v["note"].as_str()?, v["title"].as_str()?);
        let kind = wire::<Kind>(v["kind"].as_str()?);
        let choices = v["choices"].as_array()?.iter().map(|c| c.as_str().map(String::from)).collect::<Option<Vec<_>>>()?;
        let (ts, part, parts) = (v["ts"].as_i64()?, v["part"].as_u64()?, v["parts"].as_u64()?);
        let ok = v["v"] == 1
            && id(chat)
            && id(note)
            && title.chars().count() <= 100
            && matches!(kind, Some(Kind::Ask | Kind::Note))
            && choices.len() <= 6
            && choices.iter().all(|c| (1..=80).contains(&c.chars().count()) && !c.chars().any(|ch| ch < ' '))
            && v["text"].is_string()
            && (1..=parts).contains(&part)
            && parts <= 50;
        if !ok || self.gone.iter().any(|g| *g == format!("{chat}/{note}")) {
            return None;
        }
        let now = ts;
        if self.get(chat).is_none() {
            let c = Chat { id: chat.into(), agent: "claude".into(), title: title.into(), kind: kind?, asked: ts, updated: now, ..Chat::default() };
            self.chats.push(c);
        }
        let c = self.get_mut(chat)?;
        let at = c.lines.iter().position(|l| !l.me && l.id == note);
        if at.is_some_and(|i| c.lines[i].state != State::Partial) {
            return None;
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
        c.updated = c.updated.max(now);
        if open == Some(chat) {
            return None;
        }
        c.unread += 1;
        (c.deleted == 0).then(|| (c.id.clone(), label(c), shown))
    }
}
