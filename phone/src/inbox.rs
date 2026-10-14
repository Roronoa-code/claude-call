//! In-app chats on the phone: the store file, sending, notifications for the PC's replies, and the Chats models of
//! the home screen. The rules themselves (signing, replies, unread) are in chats.rs.
use crate::{
    Chats, ChatRow, HomeScreen, Msg, android,
    chats::{self, Store},
    jx::{self, s, service},
    net, ring, settings,
};
use jni::{
    JNIEnv,
    errors::Result,
    objects::{GlobalRef, JObject, JValue},
};
use serde_json::Value;
use slint::{ComponentHandle, ModelRc, VecModel};
use std::{
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
/// One reader or writer of the file at a time (the listener and the home screen both use it).
static LOCK: Mutex<()> = Mutex::new(());
/// The home screen, to ask whether it is in front.
static HOME: Mutex<Option<GlobalRef>> = Mutex::new(None);

/// Finds the store file (filesDir/chats.json).
fn path(env: &mut JNIEnv, ctx: &JObject) -> Result<()> {
    if PATH.get().is_none() {
        let dir = env.call_method(ctx, "getFilesDir", "()Ljava/io/File;", &[])?.l()?;
        let dir = env.call_method(&dir, "getAbsolutePath", "()Ljava/lang/String;", &[])?.l()?;
        let _ = PATH.set(PathBuf::from(jx::string(env, dir)?.unwrap_or_default()).join("chats.json"));
    }
    Ok(())
}

/// Runs `f` on the stored chats and saves them.
fn update<R>(f: impl FnOnce(&mut Store) -> R) -> Option<R> {
    let path = PATH.get()?;
    let _lock = LOCK.lock().unwrap();
    let Some(mut store) = Store::load(path) else {
        android::log("chats: chats.json unreadable; left as it is");
        return None;
    };
    let r = f(&mut store);
    if let Err(e) = store.save(path) {
        android::log(&format!("chats: {e}"));
    }
    Some(r)
}

/// Whether the home screen is in front and focused (a chat open behind the lock screen is not being read).
fn focused(env: &mut JNIEnv) -> bool {
    let home = HOME.lock().unwrap().clone();
    home.is_some_and(|h| env.call_method(&h, "hasWindowFocus", "()Z", &[]).and_then(|v| v.z()).unwrap_or(false))
}

fn notification_id(chat: &str) -> i32 {
    100 + (chat.bytes().fold(0u32, |h, b| h.wrapping_mul(31).wrapping_add(b as u32)) & 0xf_ffff) as i32
}

/// The PC's reply arrived over ntfy: file it, and notify unless that chat is being looked at.
pub fn reply_arrived(env: &mut JNIEnv, ctx: &JObject, body: &str) -> Result<()> {
    path(env, ctx)?;
    let Ok(v) = serde_json::from_str::<Value>(body) else { return Ok(()) };
    let Some(key) = settings::chat_key(env, ctx)? else { return Ok(()) };
    if !chats::reply_signed(&key, &v) {
        android::log("chats: reply with a missing or wrong signature dropped");
        return Ok(());
    }
    let open = if focused(env) { VIEWING.lock().unwrap().clone() } else { None };
    let note = update(|s| s.apply_reply(&v, open.as_deref())).flatten();
    VERSION.fetch_add(1, Relaxed);
    let Some((chat, name, text)) = note else { return Ok(()) };

    let nm = service(env, ctx, "notification")?;
    ring::channel(env, &nm, "chat_replies", "Chat replies", 4, false)?; // IMPORTANCE_HIGH
    let home = jx::intent(env, ctx, "MainActivity")?;
    env.call_method(&home, "addFlags", "(I)Landroid/content/Intent;", &[JValue::Int(0x0400_0000 | 0x2000_0000 | 0x1000_0000)])?; // CLEAR_TOP | SINGLE_TOP | NEW_TASK
    let c = s(env, &chat)?;
    jx::put(env, &home, "chat", JValue::Object(&c))?;
    let id = notification_id(&chat);
    let tap = jx::pending(env, ctx, "getActivity", id, &home)?;
    let channel = s(env, "chat_replies")?;
    let b = env.new_object("android/app/Notification$Builder", "(Landroid/content/Context;Ljava/lang/String;)V", &[JValue::Object(ctx), JValue::Object(&channel)])?;
    let icon = env.get_static_field("android/R$drawable", "sym_action_chat", "I")?.i()?;
    let builder = "Landroid/app/Notification$Builder;";
    jx::call(env, &b, "setSmallIcon", &format!("(I){builder}"), &[JValue::Int(icon)])?;
    ring::set_text(env, &b, "setContentTitle", &name)?;
    ring::set_text(env, &b, "setContentText", &text)?;
    let big = env.new_object("android/app/Notification$BigTextStyle", "()V", &[])?;
    let t = s(env, &text)?;
    env.call_method(&big, "bigText", "(Ljava/lang/CharSequence;)Landroid/app/Notification$BigTextStyle;", &[JValue::Object(&t)])?;
    jx::call(env, &b, "setStyle", &format!("(Landroid/app/Notification$Style;){builder}"), &[JValue::Object(&big)])?;
    jx::call(env, &b, "setContentIntent", &format!("(Landroid/app/PendingIntent;){builder}"), &[JValue::Object(&tap)])?;
    jx::call(env, &b, "setAutoCancel", &format!("(Z){builder}"), &[JValue::Bool(1)])?;
    let cat = s(env, "msg")?;
    jx::call(env, &b, "setCategory", &format!("(Ljava/lang/String;){builder}"), &[JValue::Object(&cat)])?;
    let quiet = matches!(settings::mode(env, ctx)?.as_str(), "none" | "sleep");
    jx::call(env, &b, "setSilent", &format!("(Z){builder}"), &[JValue::Bool(quiet as u8)])?;
    let n = env.call_method(&b, "build", "()Landroid/app/Notification;", &[])?.l()?;
    env.call_method(&nm, "notify", "(ILandroid/app/Notification;)V", &[JValue::Int(id), JValue::Object(&n)])?;
    Ok(())
}

/// Adds or retries a message of mine (`f` says which) and posts it, signed.
fn post(env: &mut JNIEnv, ctx: &JObject, chat: &str, f: impl FnOnce(&mut Store) -> Option<String>) -> Result<()> {
    let Some(key) = settings::chat_key(env, ctx)? else { return Ok(()) };
    let topic = settings::topic(env, ctx)?;
    path(env, ctx)?;
    let out = update(|s| {
        let msg = f(s)?;
        Some((s.body(chat, &msg, &key)?, msg))
    })
    .flatten();
    VERSION.fetch_add(1, Relaxed);
    if let Some((body, msg)) = out {
        let chat = chat.to_string();
        net::publish_chat(topic, body, move |ok| {
            update(|s| s.sent(&chat, &msg, ok));
            VERSION.fetch_add(1, Relaxed);
        });
    }
    Ok(())
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
            let nm = service(env, act, "notification")?;
            env.call_method(&nm, "cancel", "(I)V", &[JValue::Int(notification_id(&cur))])?;
        }
        Ok(f)
    })
    .unwrap_or(false);
    let Some(st) = update(|st| {
        if seen {
            st.mark_read(&cur);
        }
        st.clone()
    }) else {
        return;
    };
    let now = chats::now();
    let mut all: Vec<_> = st.chats.iter().collect();
    all.sort_by_key(|c| -c.updated);
    let preview = |c: &chats::Chat| {
        let last = c.lines.iter().rev().find(|l| l.me || l.state != "partial");
        last.map_or("New chat".to_string(), |l| if l.me { l.text.clone() } else { chats::shown(l) }).replace('\n', " ")
    };
    let rows: Vec<ChatRow> = all
        .iter()
        .map(|c| ChatRow {
            id: c.id.clone().into(),
            name: chats::name(&c.agent, c.model.as_deref()).into(),
            preview: preview(c).into(),
            time: chats::ago(now - c.updated).into(),
            unread: c.unread > 0,
        })
        .collect();
    c.set_rows(ModelRc::new(VecModel::from(rows)));
    c.set_unread(st.chats.iter().any(|c| c.unread > 0));
    match st.get(&cur) {
        Some(chat) => {
            let msgs: Vec<Msg> = chat
                .lines
                .iter()
                .filter(|l| l.me || l.state != "partial")
                .map(|l| Msg { me: l.me, text: if l.me { l.text.clone() } else { chats::shown(l) }.into(), lit: "".into() })
                .collect();
            c.set_name(chats::name(&chat.agent, chat.model.as_deref()).into());
            c.set_title(chat.title.clone().into());
            c.set_msgs(ModelRc::new(VecModel::from(msgs)));
            status(ui);
        }
        None => {
            c.set_current("".into());
            c.set_msgs(ModelRc::default());
        }
    }
}

/// Just the small grey status line (it changes with time, not with the store), and whether chat is set up.
fn status(ui: &HomeScreen) {
    let c = ui.global::<Chats>();
    let cur = c.get_current().to_string();
    if let Some(path) = PATH.get() {
        let _lock = LOCK.lock().unwrap();
        let (text, action) = Store::load(path).unwrap_or_default().get(&cur).map_or(("", ""), |chat| chats::status(chat, chats::now()));
        if c.get_status() != text {
            c.set_status(text.into());
        }
        if c.get_action() != action {
            c.set_action(action.into());
        }
    }
    let ready = android::with(|env, act| settings::chat_key(env, act)).flatten().is_some();
    if c.get_ready() != ready {
        c.set_ready(ready);
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
}

impl Tracker {
    pub fn tick(&mut self, ui: &HomeScreen) {
        let c = ui.global::<Chats>();
        if let Some(id) = OPEN_CHAT.lock().unwrap().take() {
            c.set_picker(false);
            c.set_open(true);
            c.set_current(id.into());
        }
        let (version, open, current) = (VERSION.load(Relaxed), c.get_open(), c.get_current().to_string());
        let second = self.last.is_none_or(|t| t.elapsed() > Duration::from_secs(1));
        let focus = second && android::with(|env, _| Ok(focused(env))).unwrap_or(false);
        if version != self.version || open != self.open || current != self.current || (focus && !self.focus) {
            (self.version, self.open, self.current) = (version, open, current);
            show(ui);
        } else if second {
            status(ui);
        }
        if second {
            (self.last, self.focus) = (Some(Instant::now()), focus);
        }
    }
}

/// Connects the Chats callbacks of the home screen.
pub fn wire(ui: &HomeScreen) -> Tracker {
    android::with(|env, act| {
        path(env, act)?;
        *HOME.lock().unwrap() = Some(env.new_global_ref(act)?);
        Ok(())
    });
    let c = ui.global::<Chats>();
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
    c.on_send({
        let ui = ui.as_weak();
        move |text| {
            let Some(ui) = ui.upgrade() else { return };
            let chat = ui.global::<Chats>().get_current().to_string();
            android::with(|env, act| post(env, act, &chat, |s| s.add_mine(&chat, &text)));
            show(&ui);
        }
    });
    c.on_retry({
        let ui = ui.as_weak();
        move || {
            let Some(ui) = ui.upgrade() else { return };
            let chat = ui.global::<Chats>().get_current().to_string();
            android::with(|env, act| post(env, act, &chat, |s| s.retry(&chat)));
            show(&ui);
        }
    });
    Tracker { version: u64::MAX, ..Tracker::default() } // draws once at the start
}
