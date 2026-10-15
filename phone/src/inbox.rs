//! In-app chats on the phone: the store file, sending, notifications for the PC's replies, and the Chats models of
//! the home screen. The rules themselves (signing, replies, unread) are in chats.rs.
use crate::{
    ChatMsg, Chats, ChatRow, HomeScreen, android,
    chats::{self, Media, Store},
    jx::{self, s, service},
    net, ring, settings,
};
use jni::{
    JNIEnv,
    errors::Result,
    objects::{GlobalRef, JByteArray, JObject, JValue},
};
use serde_json::Value;
use slint::{ComponentHandle, ModelRc, VecModel};
use std::{
    cell::RefCell,
    collections::HashMap,
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
/// Photo and video links being fetched right now, so each is fetched once.
static FETCHING: Mutex<Vec<String>> = Mutex::new(Vec::new());
thread_local! {
    /// Thumbnails already drawn, by file (None: it couldn't be read).
    static THUMBS: RefCell<HashMap<String, Option<(slint::Image, f32)>>> = RefCell::new(HashMap::new());
}

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
    fetch_media();
    let Some((chat, name, text)) = note else { return Ok(()) };

    let nm = service(env, ctx, "notification")?;
    // Quiet modes post on a silent low-importance channel (Notification.Builder has no public setSilent: calling it
    // threw, so no reply was ever shown).
    let quiet = matches!(settings::mode(env, ctx)?.as_str(), "none" | "sleep");
    let channel_id = if quiet { "chat_replies_quiet" } else { "chat_replies" };
    if quiet {
        ring::channel(env, &nm, channel_id, "Chat replies (quiet)", 2, true)?; // IMPORTANCE_LOW
    } else {
        ring::channel(env, &nm, channel_id, "Chat replies", 4, false)?; // IMPORTANCE_HIGH
    }
    let home = jx::intent(env, ctx, "MainActivity")?;
    env.call_method(&home, "addFlags", "(I)Landroid/content/Intent;", &[JValue::Int(0x0400_0000 | 0x2000_0000 | 0x1000_0000)])?; // CLEAR_TOP | SINGLE_TOP | NEW_TASK
    let c = s(env, &chat)?;
    jx::put(env, &home, "chat", JValue::Object(&c))?;
    let id = notification_id(&chat);
    let tap = jx::pending(env, ctx, "getActivity", id, &home)?;
    let channel = s(env, channel_id)?;
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

/// Fetches every photo and video still loading, in the background, right away (ntfy keeps them only 3 hours). Each
/// is kept only if its size and sha256 are what the PC signed for, as filesDir/media/<sha256>.<ext>.
fn fetch_media() {
    let Some(path) = PATH.get() else { return };
    let dir = path.with_file_name("media");
    let todo = {
        let _lock = LOCK.lock().unwrap();
        Store::load(path).map(|s| s.downloads()).unwrap_or_default()
    };
    for m in todo {
        {
            let mut busy = FETCHING.lock().unwrap();
            if busy.contains(&m.url) {
                continue;
            }
            busy.push(m.url.clone());
        }
        let dir = dir.clone();
        net::RT.spawn(async move {
            let saved = net::fetch(&m.url, m.size).await.filter(|b| m.matches(b)).and_then(|b| {
                std::fs::create_dir_all(&dir).ok()?;
                let file = dir.join(m.file_name());
                let tmp = file.with_extension("part");
                std::fs::write(&tmp, b).and_then(|_| std::fs::rename(&tmp, &file)).ok()?;
                Some(file.to_string_lossy().into_owned())
            });
            if saved.is_none() {
                android::log("chats: a photo or video couldn't be fetched");
            }
            update(|s| s.media_done(&m.url, saved.as_deref()));
            FETCHING.lock().unwrap().retain(|u| *u != m.url);
            VERSION.fetch_add(1, Relaxed);
        });
    }
}

/// A photo, or a video's first frame, at most about `max` pixels on its longer side, as (picture, height / width).
fn picture(path: &str, video: bool, max: i32) -> Option<(slint::Image, f32)> {
    let (w, h, px) = android::with(|env, _| env.with_local_frame(16, |env| bitmap(env, path, video, max))).flatten()?;
    let buf = slint::SharedPixelBuffer::<slint::Rgba8Pixel>::clone_from_slice(&px, w, h);
    Some((slint::Image::from_rgba8_premultiplied(buf), h as f32 / w as f32))
}

/// Decodes with Android (any format it knows; a video through MediaMetadataRetriever) into RGBA bytes.
fn bitmap(env: &mut JNIEnv, path: &str, video: bool, max: i32) -> Result<Option<(u32, u32, Vec<u8>)>> {
    let p = s(env, path)?;
    let bmp = if video {
        let r = env.new_object("android/media/MediaMetadataRetriever", "()V", &[])?;
        env.call_method(&r, "setDataSource", "(Ljava/lang/String;)V", &[JValue::Object(&p)])?;
        let sig = "(JIII)Landroid/graphics/Bitmap;"; // OPTION_CLOSEST_SYNC, scaled to fit max x max
        let frame = env.call_method(&r, "getScaledFrameAtTime", sig, &[JValue::Long(0), JValue::Int(2), JValue::Int(max), JValue::Int(max)])?.l()?;
        env.call_method(&r, "release", "()V", &[])?;
        frame
    } else {
        let o = env.new_object("android/graphics/BitmapFactory$Options", "()V", &[])?;
        let sig = "(Ljava/lang/String;Landroid/graphics/BitmapFactory$Options;)Landroid/graphics/Bitmap;";
        env.set_field(&o, "inJustDecodeBounds", "Z", JValue::Bool(1))?;
        env.call_static_method("android/graphics/BitmapFactory", "decodeFile", sig, &[JValue::Object(&p), JValue::Object(&o)])?;
        let big = env.get_field(&o, "outWidth", "I")?.i()?.max(env.get_field(&o, "outHeight", "I")?.i()?);
        let mut sample = 1;
        while big / (sample * 2) >= max {
            sample *= 2;
        }
        env.set_field(&o, "inJustDecodeBounds", "Z", JValue::Bool(0))?;
        env.set_field(&o, "inSampleSize", "I", JValue::Int(sample))?;
        env.call_static_method("android/graphics/BitmapFactory", "decodeFile", sig, &[JValue::Object(&p), JValue::Object(&o)])?.l()?
    };
    if bmp.is_null() {
        return Ok(None);
    }
    // A plain ARGB_8888 copy (video frames can come as RGB_565): its bytes are RGBA, premultiplied.
    let cfg = env.get_static_field("android/graphics/Bitmap$Config", "ARGB_8888", "Landroid/graphics/Bitmap$Config;")?.l()?;
    let soft = env.call_method(&bmp, "copy", "(Landroid/graphics/Bitmap$Config;Z)Landroid/graphics/Bitmap;", &[JValue::Object(&cfg), JValue::Bool(0)])?.l()?;
    env.call_method(&bmp, "recycle", "()V", &[])?;
    if soft.is_null() {
        return Ok(None);
    }
    let (w, h) = (env.call_method(&soft, "getWidth", "()I", &[])?.i()?, env.call_method(&soft, "getHeight", "()I", &[])?.i()?);
    let buf = env.call_static_method("java/nio/ByteBuffer", "allocate", "(I)Ljava/nio/ByteBuffer;", &[JValue::Int(w * h * 4)])?.l()?;
    env.call_method(&soft, "copyPixelsToBuffer", "(Ljava/nio/Buffer;)V", &[JValue::Object(&buf)])?;
    env.call_method(&soft, "recycle", "()V", &[])?;
    let bytes = env.call_method(&buf, "array", "()[B", &[])?.l()?;
    Ok(Some((w as u32, h as u32, env.convert_byte_array(JByteArray::from(bytes))?)))
}

/// Plays a video the PC sent in the phone's video player, through the app's own content link (MediaProvider in
/// Shell.kt) with a one-time read grant.
fn play(env: &mut JNIEnv, act: &JObject, path: &str) -> Result<()> {
    let name = std::path::Path::new(path).file_name().and_then(|n| n.to_str()).unwrap_or("");
    let mime = match name.rsplit('.').next() {
        Some("mov") => "video/quicktime",
        Some("webm") => "video/webm",
        _ => "video/mp4",
    };
    let i = jx::view_intent(env, "android.intent.action.VIEW", &format!("content://{}.media/{name}", jx::PKG))?;
    let uri = env.call_method(&i, "getData", "()Landroid/net/Uri;", &[])?.l()?;
    let t = s(env, mime)?;
    env.call_method(&i, "setDataAndType", "(Landroid/net/Uri;Ljava/lang/String;)Landroid/content/Intent;", &[JValue::Object(&uri), JValue::Object(&t)])?;
    env.call_method(&i, "addFlags", "(I)Landroid/content/Intent;", &[JValue::Int(1)])?; // FLAG_GRANT_READ_URI_PERMISSION
    jx::start_activity(env, act, &i)
}

/// The bubble for a photo or video: its thumbnail once it is here, else a grey note.
fn media_msg(me: bool, m: &Media) -> ChatMsg {
    let what = if m.video() { "video" } else { "photo" };
    let note = |text: String| ChatMsg { me, text: text.into(), kind: "note".into(), ..ChatMsg::default() };
    match m.state.as_str() {
        "loading" => note(format!("Downloading {what}…")),
        "ready" => {
            let thumb = THUMBS.with(|t| t.borrow_mut().entry(m.path.clone()).or_insert_with(|| picture(&m.path, m.video(), 720)).clone());
            match thumb {
                // A video whose frame can't be read still gets its card (and plays).
                None if !m.video() => note(format!("Couldn't show this {what}")),
                t => {
                    let (pic, ratio) = t.unwrap_or_else(|| (slint::Image::default(), 9.0 / 16.0));
                    ChatMsg { me, kind: if m.video() { "video" } else { "image" }.into(), pic, ratio, path: m.path.clone().into(), ..ChatMsg::default() }
                }
            }
        }
        _ => note(format!("Couldn't load this {what}")),
    }
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
            let mut msgs: Vec<ChatMsg> = vec![];
            for l in chat.lines.iter().filter(|l| l.me || l.state != "partial") {
                // A reply of only files has no text bubble, just theirs.
                if l.me || !l.text.is_empty() || l.media.is_empty() {
                    let text = if l.me { l.text.clone() } else { chats::shown(l) };
                    msgs.push(ChatMsg { me: l.me, text: text.into(), ..ChatMsg::default() });
                }
                msgs.extend(l.media.iter().map(|m| media_msg(l.me, m)));
            }
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
    c.on_open_media({
        let ui = ui.as_weak();
        move |path, kind| {
            let Some(ui) = ui.upgrade() else { return };
            if kind == "video" {
                android::with(|env, act| play(env, act, &path));
            } else if let Some((pic, _)) = picture(&path, false, 2048) {
                let c = ui.global::<Chats>();
                c.set_full(pic);
                c.set_viewing(true);
            }
        }
    });
    fetch_media(); // anything still loading when the app last stopped
    Tracker { version: u64::MAX, ..Tracker::default() } // draws once at the start
}
