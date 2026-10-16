//! In-app chats on the phone: the store file, sending, filing what the PC sends (replies, and threads it starts), and
//! the Chats models of the home screen. The rules themselves (signing, replies, notes, unread) are in chats.rs.
use crate::{
    ChatMsg, ChatRow, Chats, HomeScreen, android, attach, chat_alert, chat_view,
    chats::{self, Store},
    jx,
    net, settings,
};
use jni::{
    JNIEnv,
    errors::Result,
    objects::{GlobalRef, JObject},
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

/// A tapped notification asks the home screen to open this chat.
pub static OPEN_CHAT: Mutex<Option<String>> = Mutex::new(None);
/// The chat on screen right now.
static VIEWING: Mutex<Option<String>> = Mutex::new(None);
/// Moves on whenever the store changes, so the home screen redraws.
static VERSION: AtomicU64 = AtomicU64::new(0);
static PATH: OnceLock<PathBuf> = OnceLock::new();
/// The store as last read from or written to its file (only this process writes it), so it is parsed once, not every
/// second; one reader or writer at a time (the listener and the home screen both use it).
static STORE: Mutex<Option<Store>> = Mutex::new(None);
/// The home screen, to ask whether it is in front.
static HOME: Mutex<Option<GlobalRef>> = Mutex::new(None);
#[path = "inbox_send.rs"]
mod send;
pub(crate) use send::{auto, post};
use send::fetch_media;
/// How long Undo stays.
const UNDO_FOR: Duration = Duration::from_secs(5);
thread_local! {
    /// The open chat's lines, kept and changed in place: a new model would rebuild every row, and each would rise in
    /// again as if it had just arrived.
    static MSGS: std::rc::Rc<VecModel<ChatMsg>> = std::rc::Rc::new(VecModel::default());
    /// The list's rows, kept in place the same way: a rebuilt row would cut short a row sliding out or opening back in.
    static ROWS: std::rc::Rc<VecModel<ChatRow>> = std::rc::Rc::new(VecModel::default());
    /// Lets the deleted chats go for good (and their files) once Undo is over.
    static UNDO_TIMER: slint::Timer = slint::Timer::default();
    /// The text box's latest text (chat, text), kept a moment after the last key and then saved with its chat.
    static PENDING: RefCell<Option<(String, String)>> = const { RefCell::new(None) };
    static DRAFT_TIMER: slint::Timer = slint::Timer::default();
    /// The photo open full screen (it is decoded again, sharper, once zoomed).
    static FULL: RefCell<String> = const { RefCell::new(String::new()) };
}

/// The store changed: the home screen redraws.
pub fn changed() {
    VERSION.fetch_add(1, Relaxed);
}

/// Where photos and videos are kept (filesDir/media), both the PC's and mine.
pub fn media_dir() -> Option<PathBuf> {
    PATH.get().map(|p| p.with_file_name("media"))
}

/// Finds the store file (filesDir/chats.json). Also for a pick that comes back after Android closed the app (attach.rs).
pub(crate) fn path(env: &mut JNIEnv, ctx: &JObject) -> Result<()> {
    if PATH.get().is_none() {
        let dir = env.call_method(ctx, "getFilesDir", "()Ljava/io/File;", &[])?.l()?;
        let dir = env.call_method(&dir, "getAbsolutePath", "()Ljava/lang/String;", &[])?.l()?;
        let _ = PATH.set(PathBuf::from(jx::string(env, dir)?.unwrap_or_default()).join("chats.json"));
    }
    Ok(())
}

/// The stored chats, read from the file the first time (None while it can't be read: never saved over).
fn store<'a>(cache: &'a mut Option<Store>) -> Option<&'a mut Store> {
    if cache.is_none() {
        let Some(s) = Store::load(PATH.get()?) else {
            android::log("chats: chats.json unreadable; left as it is");
            return None;
        };
        *cache = Some(s);
    }
    cache.as_mut()
}

/// Runs `f` on the stored chats and saves them if it changed anything. None when that couldn't be saved: then nothing
/// changed at all (what is typed or picked stays, and the caller says so).
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

/// Reads the stored chats (nothing is saved).
fn read<R>(f: impl FnOnce(&Store) -> R) -> Option<R> {
    let mut cache = STORE.lock().unwrap();
    store(&mut cache).map(|s| f(s))
}

/// How many chats need me, leaving out `read_now` (being read right now), and the one with the newest news (the
/// notifications' summary says both).
pub(crate) fn needing(read_now: &str) -> Option<(usize, Option<chats::Chat>)> {
    read(|s| {
        let need: Vec<_> = s.chats.iter().filter(|c| c.unread > 0 && c.deleted == 0 && c.id != read_now).collect();
        (need.len(), need.iter().max_by_key(|c| c.updated).map(|c| (*c).clone()))
    })
}

/// PREVIEW (fake_chat.rs): the chat on screen right now, if any.
pub(crate) fn viewing() -> Option<String> {
    VIEWING.lock().unwrap().clone()
}

/// Whether the home screen is in front and focused (a chat open behind the lock screen is not being read).
fn focused(env: &mut JNIEnv) -> bool {
    let home = HOME.lock().unwrap().clone();
    home.is_some_and(|h| env.call_method(&h, "hasWindowFocus", "()Z", &[]).and_then(|v| v.z()).unwrap_or(false))
}

/// The PC's reply arrived over ntfy: file it, and notify unless that chat is being looked at.
pub fn reply_arrived(env: &mut JNIEnv, ctx: &JObject, body: &str) -> Result<()> {
    arrived(env, ctx, body, "reply", chats::reply_signed, Store::apply_reply)
}

/// A thread the PC started arrived ("Chat note": an agent's question, or an update): file it (the thread opens if it
/// is new), and notify like a reply.
pub fn note_arrived(env: &mut JNIEnv, ctx: &JObject, body: &str) -> Result<()> {
    arrived(env, ctx, body, "note", chats::note_signed, Store::apply_note)
}

/// The PC's read receipt for one of my messages ("Chat receipt": delivered or read): checked like a reply, then the
/// status line under my last message moves on. Never notifies.
pub fn receipt_arrived(env: &mut JNIEnv, ctx: &JObject, body: &str) -> Result<()> {
    arrived(env, ctx, body, "receipt", chats::receipt_signed, |s, v, _| {
        s.apply_receipt(v);
        None
    })
}

type Apply = fn(&mut Store, &Value, Option<&str>) -> Option<(String, String, String)>;

/// Checks the PC's signature, files the message, fetches its files and notifies, unless that chat is being looked at.
fn arrived(env: &mut JNIEnv, ctx: &JObject, body: &str, what: &str, signed: fn(&str, &Value) -> bool, apply: Apply) -> Result<()> {
    path(env, ctx)?;
    let Ok(v) = serde_json::from_str::<Value>(body) else { return Ok(()) };
    let Some(key) = settings::chat_key(env, ctx)? else { return Ok(()) };
    if !signed(&key, &v) {
        android::log(&format!("chats: {what} with a missing or wrong signature dropped"));
        return Ok(());
    }
    let open = if focused(env) { VIEWING.lock().unwrap().clone() } else { None };
    let note = update(|s| apply(s, &v, open.as_deref())).flatten();
    changed();
    fetch_media();
    let Some((chat, name, text)) = note else { return Ok(()) };
    chat_alert::notify(env, ctx, &chat, &name, &text)
}

/// Saves what was typed in a text box (kept a moment after the last key: keep_draft).
fn save_draft() {
    let Some((chat, text)) = PENDING.with(|p| p.borrow_mut().take()) else { return };
    if update(|s| s.set_draft(&chat, &text)).is_none() {
        // couldn't be saved: tried again next time (unless the box changed meanwhile)
        PENDING.with(|p| {
            p.borrow_mut().get_or_insert((chat, text));
        });
    }
}

/// The text box changed: saved with its chat a moment after the last key (and at once when the chat closes).
fn keep_draft(chat: &str, text: &str) {
    if chat.is_empty() {
        return;
    }
    PENDING.with(|p| *p.borrow_mut() = Some((chat.into(), text.into())));
    DRAFT_TIMER.with(|t| t.start(slint::TimerMode::SingleShot, Duration::from_millis(600), save_draft));
}

/// Redraws the Chats screen from the store: the list, the open chat's bubbles and status, and marks the open chat
/// read when it is really in front.
fn show(ui: &HomeScreen) {
    let c = ui.global::<Chats>();
    let cur = c.get_current().to_string();
    *VIEWING.lock().unwrap() = (c.get_open() && !cur.is_empty()).then(|| cur.clone());
    let seen = android::with(|env, act| {
        let f = focused(env);
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
    // The last line in one line, in plain words (as its notification says it).
    let preview = |c: &chats::Chat| {
        let last = c.lines.iter().rev().find(|l| l.me || l.state != "partial");
        chats::brief(&last.map_or("New chat".to_string(), chats::shown)).replace('\n', " ")
    };
    let rows: Vec<ChatRow> = all
        .iter()
        // a thread the PC started shows once its first message is whole
        .filter(|c| c.kind.is_empty() || c.lines.iter().any(|l| l.me || l.state != "partial"))
        .map(|c| ChatRow {
            id: c.id.clone().into(),
            name: chats::label(c).into(),
            face: chats::face(&c.agent, c.model.as_deref()).into(),
            preview: preview(c).into(),
            time: chats::ago(now - c.updated).into(),
            unread: c.unread > 0,
            busy: chats::waiting(c),
            asks: chats::ask_state(c, now) == "waiting",
            choices: match chat_view::choices(c, now) {
                v if v.is_empty() => ModelRc::default(),
                v => ModelRc::new(VecModel::from(v)),
            },
            topic: chat_view::topic(c).into(),
            count: c.unread as i32,
            head: Default::default(),
            draft: chats::draft_line(c, now).into(),
            think: chats::think(c, now).into(),
            mine_last: c.lines.iter().rev().find(|l| l.me || l.state != "partial").is_some_and(|l| l.me),
        })
        .collect();
    let (hero, line) = chat_view::hero(&rows);
    let rows = chat_view::arrange(rows);
    ROWS.with(|model| {
        // gone chats out; then row by row: the same chat changes in place, a chat that moved or came back goes in
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
            // "Opus" from "Claude · Opus"; a PC-started thread keeps its whole name ("Deploy · Notebook")
            let short = if chat.kind.is_empty() { name.rsplit(" · ").next().unwrap_or(&name) } else { &name }.to_string();
            c.set_hint(if ask == "waiting" { "Type your answer".into() } else { format!("Message {short}").into() });
            c.set_short(short.into());
            c.set_name(name.into());
            c.set_face(chats::face(&chat.agent, chat.model.as_deref()).into());
            c.set_empty(msgs.is_empty());
            c.set_waiting(chats::waiting(chat));
            c.set_replies(chat.lines.iter().filter(|l| !l.me && l.state != "partial").count() as i32);
            // A PC-started thread is named after its task, so the line under the name says where it stands instead.
            c.set_title(match (chat.kind.as_str(), ask) {
                ("", _) => chat.title.clone().into(),
                (_, "waiting") => "Waiting for your answer".into(),
                (_, "sending") => "Waiting for the PC".into(),
                (_, "answered") => "Answered".into(),
                (_, "expired") => "Expired".into(),
                _ => "From your PC".into(),
            });
            c.set_ask(ask.into());
            c.set_readonly(chat.kind == "note");
            // The PC takes only the first answer to a question: once one is on its way (or nobody waits), no text box.
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
        // The closed chat's lines stay: its screen is still on its way back into its row (home.slint).
        None => c.set_current("".into()),
    }
}

/// Just the small grey status line and the PC's word in the header (they change with time, not with the store).
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
    if ask != c.get_ask().as_str() {
        changed(); // a question just expired (or its answer was taken) while open: redraw
    }
    // "Opus is thinking · 3 min" (the time from the header's "Thinking · 3 min")
    let thinking = if waiting { format!("{} is thinking{}", c.get_short(), think.strip_prefix("Thinking").unwrap_or("")) } else { String::new() };
    // set only what changed (each set redraws)
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

/// Whether chat is set up (the PC gave its key): read at the start and whenever the app comes back.
fn ready(ui: &HomeScreen) {
    let c = ui.global::<Chats>();
    let ready = crate::fake_chat::on() || android::with(|env, act| settings::chat_key(env, act)).flatten().is_some();
    if c.get_ready() != ready {
        c.set_ready(ready);
    }
}

/// A chat was left (closed, or another opened over it): what was typed is saved, and a chat I opened and left empty
/// goes quietly (no Undo: nothing was lost). The list is still covered by the closing chat, so nothing jumps.
fn leave(chat: &str) {
    save_draft();
    if update(|s| s.drop_empty(chat)) == Some(true) {
        changed();
    }
}

/// Keeps the home screen's models in step with the store; called from the home timer.
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
            c.set_from_id("".into()); // opened from a notification: no card to carry a character from or back to
            c.set_current(id.into());
        }
        let (open, current) = (c.get_open(), c.get_current().to_string());
        if current != self.current && !self.current.is_empty() {
            leave(&self.current);
        }
        let version = VERSION.load(Relaxed);
        let second = self.last.is_none_or(|t| t.elapsed() > Duration::from_secs(1));
        let focus = second && android::with(|env, _| Ok(focused(env))).unwrap_or(false);
        let back = focus && !self.focus; // the app came back (or just started)
        if back || self.last.is_none() {
            ready(ui);
        }
        // the list's ages ("3 min") move on while it is in front
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

/// Deletes a chat for UNDO_FOR (its notification goes): hidden, with "Chat deleted · Undo"; replies that come meanwhile
/// are kept with it. Each deleted chat keeps its own Undo time; Undo brings back the newest.
fn delete(ui: &HomeScreen, chat: &str) {
    let c = ui.global::<Chats>();
    if update(|s| s.delete(chat, chats::now())) != Some(true) {
        c.set_leaving("".into()); // not deleted (the store can't be saved): the row comes back
        show(ui);
        return;
    }
    android::with(|env, act| chat_alert::cancel(env, act, chat));
    if c.get_current() == chat {
        c.set_current("".into());
    }
    changed();
    show(ui);
    undo_timer();
}

/// Lets each deleted chat go for good once its Undo is over.
fn undo_timer() {
    UNDO_TIMER.with(|t| {
        t.start(slint::TimerMode::Repeated, Duration::from_millis(250), || {
            forget(chats::now() - UNDO_FOR.as_millis() as i64);
            if read(|s| s.undoable().is_none()).unwrap_or(true) {
                UNDO_TIMER.with(slint::Timer::stop);
            }
        })
    });
}

/// Undo: the newest deleted chat back, with anything that came for it meanwhile; its row opens back in.
fn undo(ui: &HomeScreen) {
    let c = ui.global::<Chats>();
    let Some(Some(id)) = read(|s| s.undoable().map(|c| c.id.clone())) else { return };
    if update(|s| s.undelete(&id)) != Some(true) {
        return;
    }
    c.set_leaving("".into());
    c.set_returning(id.into());
    changed();
    show(ui);
    fetch_media(); // anything that was still on its way
}

/// Undo is over for the chats deleted at or before `before`: they go for good, with their photos and videos unless
/// another chat (or a pick) uses them.
fn forget(before: i64) {
    let Some((n, files)) = update(|s| {
        let gone = s.purge(before);
        (gone.len(), gone.iter().flat_map(|c| s.unused_files(c)).collect::<Vec<_>>())
    }) else {
        return;
    };
    if n == 0 {
        return;
    }
    changed(); // the Undo bar goes (or moves on to the one deleted before)
    let Some(dir) = media_dir() else { return };
    for name in files {
        let _ = std::fs::remove_file(dir.join(name));
    }
}

/// Connects the Chats callbacks of the home screen.
pub fn wire(ui: &HomeScreen) -> Tracker {
    android::with(|env, act| {
        path(env, act)?;
        *HOME.lock().unwrap() = Some(env.new_global_ref(act)?);
        Ok(())
    });
    // deleted before: gone once their Undo is over (the screen may be made again a moment after a delete)
    forget(chats::now() - UNDO_FOR.as_millis() as i64);
    if read(|s| s.undoable().is_some()).unwrap_or(false) {
        undo_timer();
    }
    // and chats left empty when it closed (they go when left, but the app may have been closed in one; not one the
    // photo picker is open for: its pick lands there, attach.rs)
    update(|s| {
        let empty: Vec<String> = s.chats.iter().map(|c| c.id.clone()).collect();
        empty.iter().for_each(|id| {
            s.drop_empty(id);
        });
    });
    // PREVIEW: the PC played here with debug.callme.fake (the test emulator only; fake_chat.rs)
    crate::fake_chat::set(crate::stage::prop(c"debug.callme.fake", 2) > 0);
    let c = ui.global::<Chats>();
    MSGS.with(|m| c.set_msgs(ModelRc::from(m.clone())));
    ROWS.with(|m| c.set_rows(ModelRc::from(m.clone())));
    let current = |ui: &HomeScreen| ui.global::<Chats>().get_current().to_string();
    let tick = || {
        android::with(crate::ring::tick);
    };
    // The message couldn't be kept to send: what is typed stays, and the thread says so for a moment.
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
            let picked = read(|s| s.get(&chat).is_some_and(|c| c.pick.is_some())).unwrap_or(false);
            if text.is_empty() && !picked {
                return false;
            }
            let ok = android::with(|env, act| post(env, act, &chat, |s| s.send(&chat, text))).unwrap_or(false);
            if ok {
                // what was typed went with it: an older keystroke must not bring it back
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
            android::with(|env, act| {
                let i = jx::view_intent(env, "android.intent.action.VIEW", &link)?;
                jx::start_activity(env, act, &i)
            });
        }
    });
    c.on_tick(tick);
    // A long-press on the thread: my message, a code block alone, or the PC's whole answer as plain text.
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
    // A tapped answer to an agent's question: its text, as an ordinary message (what is picked stays picked).
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
    // "Send again" under the PC's failed answer: my message, as a new one.
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
        android::with(|env, act| env.call_method(act, "clearField", "()V", &[]).map(drop));
    });
    c.on_attach({
        let ui = ui.as_weak();
        move || {
            let Some(ui) = ui.upgrade() else { return };
            let chat = current(&ui);
            if crate::fake_chat::on() {
                crate::fake_chat::pick(&chat);
            } else {
                android::with(|env, act| attach::open(env, act, &chat));
            }
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
    // Zoomed in on the full-screen photo: decoded again at up to 4096 px, so it stays crisp (once it has settled).
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
    fetch_media(); // anything still loading when the app last stopped
    Tracker { version: u64::MAX, ..Tracker::default() } // draws once at the start
}
