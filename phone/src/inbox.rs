use crate::{
    ChatMsg, ChatRow, Chats, HomeScreen, android, attach, chat_alert, chat_view,
    chats::{self, Store},
    jx::{self, JNIEnv, JObject, Result},
    net, settings,
};
use serde_json::Value;
use slint::{ComponentHandle, Model, ModelRc, VecModel};
use std::{
    cell::RefCell,
    path::PathBuf,
    sync::{
        Mutex, OnceLock,
        atomic::{AtomicU64, Ordering::Relaxed},
    },
    time::{Duration, Instant},
};

pub static OPEN_CHAT: Mutex<Option<String>> = Mutex::new(None);
static VIEWING: Mutex<Option<String>> = Mutex::new(None);
static VERSION: AtomicU64 = AtomicU64::new(0);
static PATH: OnceLock<PathBuf> = OnceLock::new();
static STORE: Mutex<Option<Store>> = Mutex::new(None);
#[path = "inbox_send.rs"]
mod send;
pub(crate) use send::{auto, post};
use send::fetch_media;
#[path = "inbox_undo.rs"]
mod undo;
use undo::{delete, forget, undo, undo_timer};
const UNDO_FOR: Duration = Duration::from_secs(5);
thread_local! {
    /// A new Slint model rebuilds every row, and each would rise in again as if it had just arrived.
    static MSGS: std::rc::Rc<VecModel<ChatMsg>> = std::rc::Rc::new(VecModel::default());
    static ROWS: std::rc::Rc<VecModel<ChatRow>> = std::rc::Rc::new(VecModel::default());
    static UNDO_TIMER: slint::Timer = slint::Timer::default();
    static PENDING: RefCell<Option<(String, String)>> = const { RefCell::new(None) };
    static DRAFT_TIMER: slint::Timer = slint::Timer::default();
    static FULL: RefCell<String> = const { RefCell::new(String::new()) };
}

pub fn changed() {
    VERSION.fetch_add(1, Relaxed);
}

pub fn media_dir() -> Option<PathBuf> {
    PATH.get().map(|p| p.with_file_name("media"))
}

pub(crate) fn path(env: &mut JNIEnv, ctx: &JObject) -> Result<()> {
    if PATH.get().is_none() {
        let _ = PATH.set(android::files_dir(env, ctx)?.join("chats.json"));
    }
    Ok(())
}

fn store<'a>(cache: &'a mut Option<Store>) -> Option<&'a mut Store> {
    if cache.is_none() {
        let Some(s) = Store::load(PATH.get()?) else {
            static SAID: std::sync::atomic::AtomicBool = std::sync::atomic::AtomicBool::new(false);
            if !SAID.swap(true, Relaxed) {
                android::log("chats: chats.json unreadable; left as it is");
            }
            return None;
        };
        *cache = Some(s);
    }
    cache.as_mut()
}

pub(crate) fn update<R>(f: impl FnOnce(&mut Store) -> R) -> Option<R> {
    let mut cache = STORE.lock().unwrap();
    let st = store(&mut cache)?;
    let mut next = st.clone();
    let r = f(&mut next);
    if next != *st {
        if let Err(e) = next.save(PATH.get()?) {
            android::log(&format!("chats: {e}"));
            return None;
        }
        *st = next;
    }
    Some(r)
}

fn read<R>(f: impl FnOnce(&Store) -> R) -> Option<R> {
    let mut cache = STORE.lock().unwrap();
    store(&mut cache).map(|s| f(s))
}

pub(crate) fn needing(read_now: &str) -> Option<(usize, Option<chats::Chat>)> {
    read(|s| {
        let need: Vec<_> = s.chats.iter().filter(|c| c.unread > 0 && c.deleted == 0 && c.id != read_now).collect();
        (need.len(), need.iter().max_by_key(|c| c.updated).map(|c| (*c).clone()))
    })
}

pub(crate) fn saved_draft(chat: &str) -> Option<String> {
    read(|s| s.get(chat).map(|c| c.draft.clone())).flatten()
}


pub fn mine_arrived(env: &mut JNIEnv, ctx: &JObject, body: &str) -> Result<()> {
    arrived(env, ctx, body, false, "message", chats::mine_signed, |s, v, _| {
        s.apply_mine(v);
        None
    })
}

pub fn reply_arrived(env: &mut JNIEnv, ctx: &JObject, body: &str, at: i64, notify: bool) -> Result<()> {
    arrived(env, ctx, body, notify, "reply", chats::reply_signed, |s, v, open| s.reply_at(v, open, at))
}

pub fn note_arrived(env: &mut JNIEnv, ctx: &JObject, body: &str, notify: bool) -> Result<()> {
    arrived(env, ctx, body, notify, "note", chats::note_signed, Store::apply_note)
}

pub fn receipt_arrived(env: &mut JNIEnv, ctx: &JObject, body: &str) -> Result<()> {
    arrived(env, ctx, body, false, "receipt", chats::receipt_signed, |s, v, _| {
        s.apply_receipt(v);
        None
    })
}

fn arrived(env: &mut JNIEnv, ctx: &JObject, body: &str, notify: bool, what: &str, signed: fn(&str, &Value) -> bool,
    apply: impl FnOnce(&mut Store, &Value, Option<&str>) -> Option<(String, String, String)>) -> Result<()> {
    path(env, ctx)?;
    let Ok(v) = serde_json::from_str::<Value>(body) else { return Ok(()) };
    let Some(key) = settings::chat_key(env, ctx)? else { return Ok(()) };
    if !signed(&key, &v) {
        android::log(&format!("chats: {what} with a missing or wrong signature dropped"));
        return Ok(());
    }
    let open = if android::focused(env) { VIEWING.lock().unwrap().clone() } else { None };
    let note = update(|s| apply(s, &v, open.as_deref())).flatten();
    changed();
    fetch_media();
    if !notify { return Ok(()) }
    let Some((chat, name, text)) = note else { return Ok(()) };
    chat_alert::notify(env, ctx, &chat, &name, &text)
}

fn save_draft() {
    let Some((chat, text)) = PENDING.with(|p| p.borrow_mut().take()) else { return };
    if update(|s| s.set_draft(&chat, &text)).is_none() {
        PENDING.with(|p| {
            p.borrow_mut().get_or_insert((chat, text));
        });
    }
}

fn keep_draft(chat: &str, text: &str) {
    if chat.is_empty() {
        return;
    }
    PENDING.with(|p| *p.borrow_mut() = Some((chat.into(), text.into())));
    DRAFT_TIMER.with(|t| t.start(slint::TimerMode::SingleShot, Duration::from_millis(600), save_draft));
}

fn show(ui: &HomeScreen) {
    let c = ui.global::<Chats>();
    let cur = c.get_current().to_string();
    *VIEWING.lock().unwrap() = (c.get_open() && !cur.is_empty()).then(|| cur.clone());
    let seen = android::with(|env, act| {
        let f = android::focused(env);
        if f && !cur.is_empty() {
            chat_alert::cancel(env, act, &cur)?;
        }
        Ok(f)
    })
    .unwrap_or(false);
    save_draft();
    if seen {
        update(|st| st.mark_read(&cur));
    }
    let Some(st) = read(Store::clone) else { return };
    let now = chats::now();
    let mut all: Vec<_> = st.chats.iter().filter(|c| c.deleted == 0).collect();
    all.sort_by_key(|c| -c.updated);
    let preview = |c: &chats::Chat| {
        let last = c.lines.iter().rev().find(|l| l.me || l.state != chats::State::Partial);
        chats::brief(&last.map_or("New chat".to_string(), chats::shown)).replace('\n', " ")
    };
    let rows: Vec<ChatRow> = all
        .iter()
        .filter(|c| c.kind == chats::Kind::Chat || c.lines.iter().any(|l| l.me || l.state != chats::State::Partial))
        .map(|c| ChatRow {
            id: c.id.clone().into(),
            name: chats::label(c).into(),
            face: chats::face(&c.agent, c.model.as_deref()).into(),
            preview: preview(c).into(),
            time: chats::ago(now - c.updated).into(),
            unread: c.unread > 0,
            busy: chats::waiting(c),
            asks: chats::ask_state(c, now) == chats::Ask::Waiting,
            choices: match chat_view::choices(c, now) {
                v if v.is_empty() => ModelRc::default(),
                v => ModelRc::new(VecModel::from(v)),
            },
            topic: chat_view::topic(c).into(),
            count: c.unread as i32,
            head: Default::default(),
            draft: chats::draft_line(c, now).into(),
            think: chats::think(c, now).into(),
            mine_last: c.lines.iter().rev().find(|l| l.me || l.state != chats::State::Partial).is_some_and(|l| l.me),
        })
        .collect();
    let (hero, line) = chat_view::hero(&rows);
    let rows = chat_view::arrange(rows);
    ROWS.with(|model| {
        for i in (0..model.row_count()).rev() {
            if !rows.iter().any(|r| model.row_data(i).is_some_and(|m| m.id == r.id)) {
                model.remove(i);
            }
        }
        for (i, r) in rows.iter().enumerate() {
            match model.row_data(i) {
                Some(m) if m.id == r.id => {
                    if m != *r {
                        model.set_row_data(i, r.clone());
                    }
                }
                _ => {
                    if let Some(j) = (i + 1..model.row_count()).find(|&j| model.row_data(j).is_some_and(|m| m.id == r.id)) {
                        model.remove(j);
                    }
                    model.insert(i, r.clone());
                }
            }
        }
        while model.row_count() > rows.len() {
            model.remove(model.row_count() - 1);
        }
    });
    c.set_unread(all.iter().any(|c| c.unread > 0));
    c.set_hero(hero);
    c.set_hero_line(line.into());
    c.set_deleted(st.undoable().map_or(String::new(), |c| c.id.clone()).into());
    match st.get(&cur).filter(|c| c.deleted == 0) {
        Some(chat) => {
            let ask = chats::ask_state(chat, now);
            let msgs = chat_view::thread(chat, ask, now);
            let name = chats::label(chat);
            let short = if chat.kind == chats::Kind::Chat { name.rsplit(" · ").next().unwrap_or(&name) } else { &name }.to_string();
            c.set_hint(if ask == chats::Ask::Waiting { "Type your answer".into() } else { format!("Message {short}").into() });
            c.set_short(short.into());
            c.set_name(name.into());
            c.set_face(chats::face(&chat.agent, chat.model.as_deref()).into());
            c.set_empty(msgs.is_empty());
            c.set_waiting(chats::waiting(chat));
            c.set_replies(chat.lines.iter().filter(|l| !l.me && l.state != chats::State::Partial).count() as i32);
            c.set_title(match (chat.kind, ask) {
                (chats::Kind::Chat, _) => chat.title.clone().into(),
                (_, chats::Ask::Waiting) => "Waiting for your answer".into(),
                (_, chats::Ask::Sending) => "Waiting for the PC".into(),
                (_, chats::Ask::Answered) => "Answered".into(),
                (_, chats::Ask::Expired) => "Expired".into(),
                _ => "From your PC".into(),
            });
            c.set_ask(ask.as_str().into());
            let ghosts = chat_view::choices(chat, now);
            if !c.get_ghosts().iter().eq(ghosts.iter().cloned()) {
                c.set_ghosts(ModelRc::new(VecModel::from(ghosts)));
            }
            c.set_readonly(chat.kind == chats::Kind::Note);
            c.set_ended(chats::ended(ask).into());
            c.set_draft(chat.draft.clone().into());
            MSGS.with(|model| {
                for (i, m) in msgs.iter().enumerate() {
                    if i >= model.row_count() {
                        model.push(m.clone());
                    } else if model.row_data(i).as_ref() != Some(m) {
                        model.set_row_data(i, m.clone());
                    }
                }
                while model.row_count() > msgs.len() {
                    model.remove(model.row_count() - 1);
                }
            });
            status(ui);
            attach::show(ui, &cur, chat.pick.as_ref());
        }
        None => c.set_current("".into()),
    }
}

fn status(ui: &HomeScreen) {
    let c = ui.global::<Chats>();
    let cur = c.get_current().to_string();
    let now = chats::now();
    let Some(Some((line, ask, think, waiting))) =
        read(|st| st.get(&cur).map(|chat| (chats::status(chat, now), chats::ask_state(chat, now), chats::think(chat, now), chats::waiting(chat))))
    else {
        return;
    };
    let (text, action) = line;
    if ask.as_str() != c.get_ask().as_str() {
        changed();
    }
    let thinking = if waiting { format!("{} is thinking{}", c.get_short(), think.strip_prefix("Thinking").unwrap_or("")) } else { String::new() };
    if c.get_status() != text {
        c.set_status(text.into());
    }
    if c.get_action() != action {
        c.set_action(action.into());
    }
    if c.get_think() != think.as_str() {
        c.set_think(think.into());
    }
    if c.get_thinking() != thinking.as_str() {
        c.set_thinking(thinking.into());
    }
}

fn ready(ui: &HomeScreen) {
    let c = ui.global::<Chats>();
    let ready = android::with(|env, act| settings::chat_key(env, act)).flatten().is_some();
    if c.get_ready() != ready {
        c.set_ready(ready);
    }
}

fn leave(chat: &str) {
    save_draft();
    if update(|s| s.drop_empty(chat)) == Some(true) {
        changed();
    }
}

#[derive(Default)]
pub struct Tracker {
    version: u64,
    open: bool,
    current: String,
    focus: bool,
    last: Option<Instant>,
    rows: Option<Instant>,
}

impl Tracker {
    pub fn tick(&mut self, ui: &HomeScreen) {
        let c = ui.global::<Chats>();
        if let Some(id) = OPEN_CHAT.lock().unwrap().take() {
            c.set_picker(false);
            c.set_open(true);
            c.set_from_id("".into());
            c.set_current(id.into());
        }
        let (open, current) = (c.get_open(), c.get_current().to_string());
        if current != self.current && !self.current.is_empty() {
            leave(&self.current);
        }
        let version = VERSION.load(Relaxed);
        let second = self.last.is_none_or(|t| t.elapsed() > Duration::from_secs(1));
        let focus = second && android::with(|env, _| Ok(android::focused(env))).unwrap_or(false);
        let back = focus && !self.focus;
        if back || self.last.is_none() {
            ready(ui);
        }
        let ages = open && current.is_empty() && self.rows.is_some_and(|t| t.elapsed() > Duration::from_secs(30));
        if version != self.version || open != self.open || current != self.current || back || ages {
            (self.version, self.open, self.current) = (version, open, current);
            show(ui);
            self.rows = Some(Instant::now());
        } else if second {
            status(ui);
        }
        if second {
            (self.last, self.focus) = (Some(Instant::now()), focus);
            android::with(auto);
        }
    }
}

pub fn wire(ui: &HomeScreen) -> Tracker {
    android::with(|env, act| {
        path(env, act)?;
        android::keep_home(env, act)
    });
    forget(chats::now() - UNDO_FOR.as_millis() as i64);
    if read(|s| s.undoable().is_some()).unwrap_or(false) {
        undo_timer();
    }
    update(|s| {
        let empty: Vec<String> = s.chats.iter().map(|c| c.id.clone()).collect();
        empty.iter().for_each(|id| {
            s.drop_empty(id);
        });
    });
    let c = ui.global::<Chats>();
    MSGS.with(|m| c.set_msgs(ModelRc::from(m.clone())));
    ROWS.with(|m| c.set_rows(ModelRc::from(m.clone())));
    let current = |ui: &HomeScreen| ui.global::<Chats>().get_current().to_string();
    let tick = || {
        android::with(crate::ring::tick);
    };
    let trouble = |ui: &HomeScreen| ui.global::<Chats>().set_trouble("Not saved. Try again.".into());
    c.on_open_chat({
        let ui = ui.as_weak();
        move |id| {
            if let Some(ui) = ui.upgrade() {
                ui.global::<Chats>().set_current(id);
                show(&ui);
            }
        }
    });
    c.on_new_chat({
        let ui = ui.as_weak();
        move |agent, model| {
            let Some(ui) = ui.upgrade() else { return };
            let model = (!model.is_empty()).then(|| model.to_string());
            if let Some(id) = update(|s| s.new_chat(&agent, model.as_deref())) {
                ui.global::<Chats>().set_current(id.into());
                show(&ui);
            }
        }
    });
    c.on_send_composed({
        let ui = ui.as_weak();
        move |draft, word, at| {
            let Some(ui) = ui.upgrade() else { return false };
            let text = chats::composed(&draft, &word, at.max(0) as usize);
            let text = text.trim();
            let chat = current(&ui);
            let with_pick = !attach::failed(&chat);
            let picked = with_pick && read(|s| s.get(&chat).is_some_and(|c| c.pick.is_some())).unwrap_or(false);
            if text.is_empty() && !picked {
                return false;
            }
            let ok = android::with(|env, act| post(env, act, &chat, |s| s.send(&chat, text, with_pick))).unwrap_or(false);
            if ok {
                PENDING.with(|p| p.borrow_mut().take_if(|(c, _)| *c == chat));
                tick();
            } else {
                trouble(&ui);
            }
            show(&ui);
            ok
        }
    });
    c.on_keep_draft(|chat, text| keep_draft(&chat, &text));
    c.on_room(|text| chats::room(&text).clamp(i32::MIN as i64, i32::MAX as i64) as i32);
    c.on_open_link(|link| {
        if link.starts_with("https://") || link.starts_with("http://") {
            android::with(|env, act| jx::open_link(env, act, &link));
        }
    });
    c.on_tick(tick);
    c.on_copy({
        let ui = ui.as_weak();
        move |line, me, n| {
            let Some(ui) = ui.upgrade() else { return };
            let chat = current(&ui);
            let Some(Some(text)) = read(|s| s.get(&chat).and_then(|c| chats::copy_text(c, &line, me, n.max(0) as usize))) else { return };
            android::with(|env, act| {
                jx::copy(env, act, &text)?;
                crate::ring::tick(env, act)
            });
        }
    });
    c.on_choose({
        let ui = ui.as_weak();
        move |text| {
            let Some(ui) = ui.upgrade() else { return };
            let chat = current(&ui);
            match android::with(|env, act| post(env, act, &chat, |s| s.add_mine(&chat, &text, None))) {
                Some(true) => tick(),
                _ => trouble(&ui),
            }
            show(&ui);
        }
    });
    c.on_stop({
        let ui = ui.as_weak();
        move || {
            let Some(ui) = ui.upgrade() else { return };
            let chat = current(&ui);
            if android::with(|env, act| post(env, act, &chat, |s| s.add_mine(&chat, "stop", None))) != Some(true) {
                trouble(&ui);
            }
            show(&ui);
        }
    });
    c.on_again({
        let ui = ui.as_weak();
        move |line| {
            let Some(ui) = ui.upgrade() else { return };
            let chat = current(&ui);
            match android::with(|env, act| post(env, act, &chat, |s| s.again(&chat, &line))) {
                Some(true) => tick(),
                _ => trouble(&ui),
            }
            show(&ui);
        }
    });
    c.on_refetch(|url| {
        if update(|s| s.refetch(&url)) == Some(true) {
            changed();
            fetch_media();
        }
    });
    c.on_clear_field(|| {
        android::clear_field();
    });
    c.on_attach({
        let ui = ui.as_weak();
        move || {
            let Some(ui) = ui.upgrade() else { return };
            let chat = current(&ui);
            android::with(|env, act| attach::open(env, act, &chat));
        }
    });
    c.on_unpick({
        let ui = ui.as_weak();
        move || {
            if let Some(ui) = ui.upgrade() {
                attach::clear(&current(&ui));
            }
        }
    });
    c.on_retry({
        let ui = ui.as_weak();
        move |msg| {
            let Some(ui) = ui.upgrade() else { return };
            let chat = current(&ui);
            android::with(|env, act| post(env, act, &chat, |s| s.retry(&chat, &msg, true)));
            show(&ui);
        }
    });
    c.on_delete_chat({
        let ui = ui.as_weak();
        move |id| {
            if let Some(ui) = ui.upgrade() {
                delete(&ui, &id);
            }
        }
    });
    c.on_undo({
        let ui = ui.as_weak();
        move || {
            if let Some(ui) = ui.upgrade() {
                undo(&ui);
            }
        }
    });
    c.on_open_media({
        let ui = ui.as_weak();
        move |path, kind| {
            let Some(ui) = ui.upgrade() else { return };
            if kind == "video" {
                android::with(|env, act| attach::play(env, act, &path));
            } else if let Some((pic, _)) = attach::picture(&path, false, 2048) {
                FULL.with(|f| *f.borrow_mut() = path.to_string());
                let c = ui.global::<Chats>();
                c.set_full(pic);
                c.set_viewing(true);
            }
        }
    });
    c.on_sharpen({
        let ui = ui.as_weak();
        move || {
            let Some(ui) = ui.upgrade() else { return };
            let path = FULL.with(|f| std::mem::take(&mut *f.borrow_mut()));
            if let Some((pic, _)) = (!path.is_empty()).then(|| attach::picture(&path, false, 4096)).flatten() {
                ui.global::<Chats>().set_full(pic);
            }
        }
    });
    fetch_media();
    Tracker { version: u64::MAX, ..Tracker::default() }
}

#[cfg(all(test, windows))]
mod tests {
    use super::*;

    #[test]
    #[ignore = "requires fresh isolated CALLME_DESK_DATA and CALLME_CONFIG with chatKey test-key"]
    fn signed_cached_conversation_is_saved_without_notifications() {
        let dir = PathBuf::from(std::env::var_os("CALLME_DESK_DATA").expect("isolated data directory"));
        assert!(std::env::var_os("CALLME_CONFIG").is_some());
        assert!(!dir.join("chats.json").exists(), "never overwrite an existing history fixture");
        android::with(|env, ctx| {
            assert_eq!(settings::chat_key(env, ctx)?.as_deref(), Some("test-key"));
            let message = serde_json::json!({"v":1,"chat":"c1","msg":"m1","agent":"claude","model":"haiku-5.5","ts":1700000000000i64,
                "text":"héllo 👋","sig":"ab529206c0c5da8e9c12efc38dac2b71aacc25df75a33f8d9c1b230be8b2f44e"});
            let mut forged = message.clone();
            forged["text"] = serde_json::json!("forged message");
            mine_arrived(env, ctx, &forged.to_string())?;
            assert_eq!(read(|s| s.chats.len()), Some(0));
            mine_arrived(env, ctx, &message.to_string())?;
            mine_arrived(env, ctx, &message.to_string())?;
            let reply = serde_json::json!({"v":1,"chat":"c1","re":"m1","state":"done","text":"héllo 👋","part":1,"parts":1,
                "sig":"260ceed955ae169bfb04cb73eff4e7d7a7faf95ca2abf61ab69fd69e8f1100be"});
            reply_arrived(env, ctx, &reply.to_string(), 1700000001000, false)?;
            Ok(())
        }).expect("receive path");
        let saved = Store::load(&dir.join("chats.json")).unwrap();
        let chat = saved.get("c1").unwrap();
        assert_eq!(chat.lines.len(), 2);
        assert!(chat.lines[0].mirrored);
        assert_eq!(chat.lines[0].state, chats::State::Answered);
        assert_eq!(chat.lines[1].state, chats::State::Done);
        assert_eq!(chat.lines[1].at, 1700000001000);
    }
}
