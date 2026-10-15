//! In-app chats on the phone: the store file, sending, filing what the PC sends (replies, and threads it starts), and
//! the Chats models of the home screen. The rules themselves (signing, replies, notes, unread) are in chats.rs.
use crate::{
    ChatMsg, ChatRow, Chats, HomeScreen, android, attach, chat_alert, chat_view,
    chats::{self, Media, Store},
    jx::{self, s},
    net, settings,
};
use jni::{
    JNIEnv,
    errors::Result,
    objects::{GlobalRef, JByteArray, JObject, JValue},
};
use serde_json::Value;
use slint::{ComponentHandle, Model, ModelRc, VecModel};
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
/// The chat deleted last, with its place in the store, while "Chat deleted · Undo" can still bring it back.
static DELETED: Mutex<Option<(usize, chats::Chat)>> = Mutex::new(None);
/// How long Undo stays.
const UNDO_FOR: Duration = Duration::from_secs(5);
thread_local! {
    /// Thumbnails already drawn, by file (None: it couldn't be read).
    static THUMBS: RefCell<HashMap<String, Option<(slint::Image, f32)>>> = RefCell::new(HashMap::new());
    /// The open chat's lines, kept and changed in place: a new model would rebuild every row, and each would rise in
    /// again as if it had just arrived.
    static MSGS: std::rc::Rc<VecModel<ChatMsg>> = std::rc::Rc::new(VecModel::default());
    /// The list's rows, kept in place the same way: a rebuilt row would cut short a row sliding out or opening back in.
    static ROWS: std::rc::Rc<VecModel<ChatRow>> = std::rc::Rc::new(VecModel::default());
    /// Hides the Undo bar and lets the deleted chat's files go.
    static UNDO_TIMER: slint::Timer = slint::Timer::default();
}

/// The store changed: the home screen redraws.
pub fn changed() {
    VERSION.fetch_add(1, Relaxed);
}

/// Where photos and videos are kept (filesDir/media), both the PC's and mine.
pub fn media_dir() -> Option<PathBuf> {
    PATH.get().map(|p| p.with_file_name("media"))
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
pub(crate) fn update<R>(f: impl FnOnce(&mut Store) -> R) -> Option<R> {
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

/// Adds or retries a message of mine (`f` says which) and sends it in the background.
fn post(env: &mut JNIEnv, ctx: &JObject, chat: &str, f: impl FnOnce(&mut Store) -> Option<String>) -> Result<()> {
    if crate::fake_chat::on() {
        path(env, ctx)?;
        crate::fake_chat::post(chat, f);
        return Ok(());
    }
    let Some(key) = settings::chat_key(env, ctx)? else { return Ok(()) };
    let topic = settings::topic(env, ctx)?;
    path(env, ctx)?;
    let msg = update(f).flatten();
    changed();
    let Some(msg) = msg else { return Ok(()) };
    let chat = chat.to_string();
    net::RT.spawn(async move {
        let ok = deliver(&topic, &key, &chat, &msg).await;
        update(|s| s.sent(&chat, &msg, ok));
        changed();
    });
    Ok(())
}

/// Uploads my message's photo or video first (a retry uploads it again), then posts the signed message.
async fn deliver(topic: &str, key: &str, chat: &str, msg: &str) -> bool {
    if let Some(m) = update(|s| s.to_upload(chat, msg)).flatten() {
        let Some(bytes) = std::fs::read(&m.path).ok() else { return false };
        let Some(url) = net::upload(topic, &m.name, bytes).await else {
            android::log("chats: a photo or video couldn't be uploaded");
            return false;
        };
        update(|s| s.uploaded(chat, msg, &url));
    }
    let Some(body) = update(|s| s.body(chat, msg, key)).flatten() else { return false };
    net::publish_chat(topic, body).await
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
            // its chat was deleted while it came: the file goes too (Undo fetches it again)
            let unused = update(|s| {
                s.media_done(&m.url, saved.as_deref());
                saved.as_ref().is_some_and(|p| !s.chats.iter().flat_map(|c| &c.lines).flat_map(|l| &l.media).any(|x| x.path == *p))
            });
            if let (Some(true), Some(p)) = (unused, &saved) {
                if !attach::holds(p) {
                    let _ = std::fs::remove_file(p);
                }
            }
            FETCHING.lock().unwrap().retain(|u| *u != m.url);
            changed();
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

/// A file's thumbnail, drawn once (a photo, or a video's first frame).
pub fn thumb(m: &Media) -> Option<(slint::Image, f32)> {
    THUMBS.with(|t| t.borrow_mut().entry(m.path.clone()).or_insert_with(|| picture(&m.path, m.video(), 720)).clone())
}

/// The bubble for a photo or video: its thumbnail once it is here, else a grey note.
fn media_msg(me: bool, m: &Media) -> ChatMsg {
    let what = if m.video() { "video" } else { "photo" };
    let note = |text: String| ChatMsg { me, text: text.into(), kind: "note".into(), ..ChatMsg::default() };
    match m.state.as_str() {
        "loading" => note(format!("Downloading {what}…")),
        "ready" => {
            match thumb(m) {
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

/// The open chat as the thread lays it out: my messages as bubbles (a photo or video above its text), the PC's as
/// blocks of text (chats::blocks) then its files, each with the room above it (`gap`) and its place in its message
/// (`n`, for the arrival's stagger). The first block of an answer that is the newest line is the `anchor`. An agent's
/// question (`ask`: chats::ask_state) offers its answers under it as "choice" buttons (`mark` "on": the one I sent),
/// and says "Expired" (a "tag") once nobody waits for it.
fn thread(chat: &chats::Chat, ask: &str) -> Vec<ChatMsg> {
    let lines: Vec<_> = chat.lines.iter().filter(|l| l.me || l.state != "partial").collect();
    let question = lines.iter().position(|l| !l.me).filter(|_| ask != "");
    let sent = |c: &str| chat.lines.iter().any(|l| l.me && l.state != "not_sent" && l.text == c);
    let mut msgs: Vec<ChatMsg> = vec![];
    for (i, l) in lines.iter().enumerate() {
        let mut items: Vec<ChatMsg> = vec![];
        if l.me {
            items.extend(l.media.iter().map(|m| media_msg(true, m)));
            if !l.text.is_empty() || l.media.is_empty() {
                items.push(ChatMsg { me: true, text: chats::shown(l).into(), ..ChatMsg::default() });
            }
        } else {
            if l.text.is_empty() && l.media.is_empty() {
                // "Something went wrong.", "Stopped." and the like: a quiet line, not an answer
                items.push(ChatMsg { text: chats::shown(l).into(), kind: "note".into(), ..ChatMsg::default() });
            }
            for b in chats::blocks(&l.text) {
                let body = match b.kind {
                    "code" => slint::StyledText::default(),
                    "h" => styled(&format!("**{}**", b.text.replace("**", ""))),
                    _ => styled(&b.text),
                };
                let text = if b.kind == "code" { chat_view::soft_breaks(&b.text) } else { b.text.clone() };
                items.push(ChatMsg { text: text.into(), kind: b.kind.into(), body, mark: b.mark.into(), level: b.level as i32, ..ChatMsg::default() });
            }
            items.extend(l.media.iter().map(|m| media_msg(false, m)));
            if question == Some(i) {
                let choice = |c: &String| ChatMsg { text: c.into(), kind: "choice".into(), mark: if sent(c) { "on" } else { "" }.into(), ..ChatMsg::default() };
                items.extend(l.choices.iter().map(choice));
                if ask == "expired" {
                    items.push(ChatMsg { text: "Expired".into(), kind: "tag".into(), ..ChatMsg::default() });
                }
            }
        }
        let mine_before = msgs.last().map(|m| m.me);
        let count = items.len();
        for (n, mut m) in items.into_iter().enumerate() {
            m.last = n + 1 == count;
            let prev = msgs.last().map_or(String::new(), |p| p.kind.to_string());
            m.n = n as i32;
            m.gap = match (mine_before, n) {
                (None, 0) => 0.0,
                (Some(true), 0) if l.me => 8.0, // my messages in a row
                (Some(_), 0) => 26.0, // the other side speaks, or the PC's next answer
                _ => match (prev.as_str(), m.kind.as_str()) {
                    (_, "h") => 22.0,
                    ("h", _) => 8.0,
                    ("li", "li") | ("choice", "choice") => 8.0,
                    (_, "tag") => 10.0,
                    _ if l.me => 6.0, // my photo, then its text
                    _ => 14.0,
                },
            };
            m.anchor = n == 0 && !l.me && i + 1 == lines.len();
            msgs.push(m);
        }
    }
    msgs
}

/// Inline markdown for the thread; text Slint's parser can't take shows as it is.
fn styled(text: &str) -> slint::StyledText {
    slint::StyledText::from_markdown(&chats::inline(text)).unwrap_or_else(|_| slint::StyledText::from_plain_text(text))
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
    // The last line in one line, without its markdown marks.
    let preview = |c: &chats::Chat| {
        let last = c.lines.iter().rev().find(|l| l.me || l.state != "partial");
        let t = last.map_or("New chat".to_string(), chats::shown);
        let t: Vec<_> = chats::blocks(&t).iter().map(|b| b.text.replace("**", "").replace('`', "")).collect();
        t.join(" ").replace('\n', " ")
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
        })
        .collect();
    let (hero, line) = chat_view::hero(&rows);
    let rows = chat_view::arrange(rows, &hero.id);
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
    c.set_unread(st.chats.iter().any(|c| c.unread > 0));
    c.set_hero(hero);
    c.set_hero_line(line.into());
    match st.get(&cur) {
        Some(chat) => {
            let ask = chats::ask_state(chat, now);
            let msgs = thread(chat, ask);
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
                (_, "answered") => "Answered".into(),
                (_, "expired") => "Expired".into(),
                _ => "From your PC".into(),
            });
            c.set_ask(ask.into());
            c.set_readonly(chat.kind == "note");
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
            attach::show(ui);
        }
        // The closed chat's lines stay: its screen is still on its way back into its row (home.slint).
        None => c.set_current("".into()),
    }
}

/// Just the small grey status line (it changes with time, not with the store), and whether chat is set up.
fn status(ui: &HomeScreen) {
    let c = ui.global::<Chats>();
    let cur = c.get_current().to_string();
    if let Some(path) = PATH.get() {
        let _lock = LOCK.lock().unwrap();
        let store = Store::load(path).unwrap_or_default();
        let (text, action) = store.get(&cur).map_or(("", ""), |chat| chats::status(chat, chats::now()));
        if store.get(&cur).is_some_and(|chat| chats::ask_state(chat, chats::now()) != c.get_ask().as_str()) {
            changed(); // a question just expired while open: redraw
        }
        if c.get_status() != text {
            c.set_status(text.into());
        }
        if c.get_action() != action {
            c.set_action(action.into());
        }
    }
    let ready = crate::fake_chat::on() || android::with(|env, act| settings::chat_key(env, act)).flatten().is_some();
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

/// Deletes a chat (its notification goes too) and shows "Chat deleted · Undo" for UNDO_FOR. The PC's late replies to
/// it are dropped, as for any chat the phone doesn't have (chats.rs apply_reply).
fn delete(ui: &HomeScreen, chat: &str) {
    forget(); // the one deleted before can't come back any more
    let c = ui.global::<Chats>();
    let Some(Some(gone)) = update(|s| s.remove(chat)) else {
        c.set_leaving("".into()); // not deleted (the store can't be read): the row comes back
        show(ui);
        return;
    };
    *DELETED.lock().unwrap() = Some(gone);
    android::with(|env, act| chat_alert::cancel(env, act, chat));
    if c.get_current() == chat {
        c.set_current("".into());
    }
    c.set_deleted(chat.into());
    changed();
    show(ui);
    let ui = ui.as_weak();
    UNDO_TIMER.with(|t| {
        t.start(slint::TimerMode::SingleShot, UNDO_FOR, move || {
            if let Some(ui) = ui.upgrade() {
                ui.global::<Chats>().set_deleted("".into());
            }
            forget();
        })
    });
}

/// Undo: the deleted chat back where it was, exactly as it was; its row opens back in.
fn undo(ui: &HomeScreen) {
    UNDO_TIMER.with(slint::Timer::stop);
    let c = ui.global::<Chats>();
    c.set_deleted("".into());
    let Some((at, chat)) = DELETED.lock().unwrap().take() else { return };
    let id = chat.id.clone();
    if update(|s| s.restore(at, chat.clone())).is_none() {
        *DELETED.lock().unwrap() = Some((at, chat)); // kept: its files stay until it can be put back or is forgotten
        return;
    }
    c.set_leaving("".into());
    c.set_returning(id.into());
    changed();
    show(ui);
    fetch_media(); // anything that was still on its way
}

/// Undo is over: the deleted chat's photos and videos go, unless another chat (or what is picked to send) uses them.
// shortcut: if the app is killed during the Undo seconds its files stay behind in filesDir/media; sweep unused files
// there on start if that ever matters.
fn forget() {
    let Some((_, gone)) = DELETED.lock().unwrap().take() else { return };
    let Some(dir) = media_dir() else { return };
    for name in update(|s| s.unused_files(&gone)).unwrap_or_default() {
        let f = dir.join(name);
        if !attach::holds(&f.to_string_lossy()) {
            let _ = std::fs::remove_file(f);
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
    // PREVIEW: the PC played here with debug.callme.fake (the test emulator only; fake_chat.rs)
    crate::fake_chat::set(crate::stage::prop(c"debug.callme.fake", 2) > 0);
    let c = ui.global::<Chats>();
    MSGS.with(|m| c.set_msgs(ModelRc::from(m.clone())));
    ROWS.with(|m| c.set_rows(ModelRc::from(m.clone())));
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
            let media = attach::take();
            android::with(|env, act| post(env, act, &chat, |s| s.add_mine(&chat, &text, media)));
            show(&ui);
        }
    });
    // A tapped answer to an agent's question: its text, as an ordinary message (what is picked stays picked).
    c.on_choose({
        let ui = ui.as_weak();
        move |text| {
            let Some(ui) = ui.upgrade() else { return };
            let chat = ui.global::<Chats>().get_current().to_string();
            android::with(|env, act| post(env, act, &chat, |s| s.add_mine(&chat, &text, None)));
            show(&ui);
        }
    });
    c.on_stop({
        let ui = ui.as_weak();
        move || {
            let Some(ui) = ui.upgrade() else { return };
            let chat = ui.global::<Chats>().get_current().to_string();
            android::with(|env, act| post(env, act, &chat, |s| s.add_mine(&chat, "stop", None)));
            show(&ui);
        }
    });
    c.on_clear_field(|| {
        android::with(|env, act| env.call_method(act, "clearField", "()V", &[]).map(drop));
    });
    c.on_attach(|| {
        if crate::fake_chat::on() {
            crate::fake_chat::pick();
        } else {
            android::with(attach::open);
        }
    });
    c.on_unpick(attach::clear);
    c.on_retry({
        let ui = ui.as_weak();
        move || {
            let Some(ui) = ui.upgrade() else { return };
            let chat = ui.global::<Chats>().get_current().to_string();
            android::with(|env, act| post(env, act, &chat, |s| s.retry(&chat)));
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
