//! Chat notifications: one per chat (a reply, an agent's question or an update), grouped, and gone once read. Each is
//! a conversation with the agent (its face as the sender's picture). A chat of mine and a waiting question can be
//! answered from the notification itself (Reply, or the question's choices): ChatAction (Shell.kt, not exported, so
//! only this app's own buttons reach it) sends that as the app does, signed (inbox.rs post).
use crate::{
    chats::{self, Chat},
    inbox,
    jx::{self, s, service},
    ring, settings,
};
use jni::{
    JNIEnv,
    errors::Result,
    objects::{JObject, JObjectArray, JValue},
};

fn notification_id(chat: &str) -> i32 {
    100 + (chat.bytes().fold(0u32, |h, b| h.wrapping_mul(31).wrapping_add(b as u32)) & 0xf_ffff) as i32
}

/// Chat notifications are their own group, with a summary that opens the chat list. Left ungrouped, Android bundled
/// them with "Ready for Claude's calls", and a tap on the bundle opened the home screen instead of the chat.
const GROUP: &str = "chats";
const SUMMARY_ID: i32 = 99; // chat notifications are 100 and up
/// Where Reply's typed words come back (RemoteInput).
const TYPED: &str = "reply";
/// A choice button's intent action: only it may carry a "choice" (Reply's intent is mutable, so extras can be added).
const CHOICE: &str = "fyi.mani.callme.CHOICE";
const BUILDER: &str = "Landroid/app/Notification$Builder;";
/// What the summary last said (its count and line): it is posted again only when that changes (Android drops a
/// burst of updates, which could take a real one with it).
static SUMMED: std::sync::Mutex<Option<(usize, String)>> = std::sync::Mutex::new(None);

/// A chat was read: its notification goes, and the summary too once no chat notification is left (while some are,
/// its count is brought up to date).
pub fn cancel(env: &mut JNIEnv, ctx: &JObject, chat: &str) -> Result<()> {
    let nm = service(env, ctx, "notification")?;
    env.call_method(&nm, "cancel", "(I)V", &[JValue::Int(notification_id(chat))])?;
    let all = JObjectArray::from(env.call_method(&nm, "getActiveNotifications", "()[Landroid/service/notification/StatusBarNotification;", &[])?.l()?);
    for i in 0..env.get_array_length(&all)? {
        let n = env.get_object_array_element(&all, i)?;
        if env.call_method(&n, "getId", "()I", &[])?.i()? > SUMMARY_ID {
            return recount(env, ctx, chat);
        }
    }
    env.call_method(&nm, "cancel", "(I)V", &[JValue::Int(SUMMARY_ID)])?;
    *SUMMED.lock().unwrap() = None;
    Ok(())
}

/// The summary posted again, quietly, with how many chats need me now (it stays as it is when none do: cancelling it
/// would take the other chats' notifications with it).
fn recount(env: &mut JNIEnv, ctx: &JObject, read: &str) -> Result<()> {
    let Some((need, Some(c))) = inbox::needing(read) else { return Ok(()) };
    let last = c.lines.iter().rev().find(|l| !l.me && l.state != "partial");
    let name = chats::label(&c);
    let a = Alert {
        chat: &c.id,
        name: &name,
        face: chats::face(&c.agent, c.model.as_deref()),
        text: chats::brief(&last.map(chats::shown).unwrap_or_default()),
        at: last.map_or(c.updated, |l| l.at),
        mine: None,
        note: None,
        answers: (vec![], false, false),
        updates: c.kind == "note",
        alert: false,
    };
    if SUMMED.lock().unwrap().as_ref() == Some(&(need, summary_line(&a))) {
        return Ok(()); // nothing to change
    }
    summary(env, ctx, &a, need)
}

/// The summary's line: who, and the first line of what they said.
fn summary_line(a: &Alert) -> String {
    format!("{}: {}", a.name, a.text.lines().next().unwrap_or(""))
}

/// One chat's notification.
struct Alert<'a> {
    chat: &'a str,
    name: &'a str,         // who
    face: &'static str,    // their face (chats::face)
    text: String,          // what they said, in plain words
    at: i64,               // when (ms)
    mine: Option<&'a str>, // my answer, sent from the notification
    note: Option<&'a str>, // a word on why nothing went ("Already answered")
    answers: (Vec<String>, bool, bool), // chats::quick_answers
    updates: bool,         // the Updates thread: its own quieter channel
    alert: bool,           // something new (sound, a peek); false: the same one changed quietly
}

/// Shows what came in a chat (`name`: who, `text`: what, markdown); a tap opens that chat.
pub fn notify(env: &mut JNIEnv, ctx: &JObject, chat: &str, name: &str, text: &str) -> Result<()> {
    let now = chats::now();
    let (c, need) = inbox::update(|s| (s.get(chat).cloned(), s.needing())).unwrap_or((None, 1));
    let c = c.unwrap_or_default();
    let a = Alert {
        chat,
        name,
        face: chats::face(&c.agent, c.model.as_deref()),
        text: chats::brief(text),
        at: now,
        mine: None,
        note: None,
        answers: chats::quick_answers(&c, now),
        updates: c.kind == "note",
        alert: true,
    };
    show(env, ctx, &a)?;
    summary(env, ctx, &a, need.max(1))
}

/// A button on a chat's notification (ChatAction): Reply's typed words, or a question's choice, sent as my message,
/// unless the question was answered or closed meanwhile. The notification then shows what went, or why nothing did.
/// Android leaves Reply's spinner until the notification is posted again, so a failure on the way still answers it:
/// "Not sent", or, once the message is kept to send (only showing it failed), the notification goes.
pub fn acted(env: &mut JNIEnv, ctx: &JObject, intent: &JObject) -> Result<()> {
    let Some(chat) = jx::extra(env, intent, "chat")? else { return Ok(()) };
    let mut kept = false;
    if jx::ok(env, |env| answer(env, ctx, intent, &chat, &mut kept)).is_some() {
        return Ok(());
    }
    if !kept && jx::ok(env, |env| show(env, ctx, &not_sent(&chat, chats::now()))).is_some() {
        return Ok(());
    }
    let nm = service(env, ctx, "notification")?;
    env.call_method(&nm, "cancel", "(I)V", &[JValue::Int(notification_id(&chat))]).map(drop)
}

/// Nothing went: said in place of the spinner Reply leaves (a tap opens the chat).
fn not_sent(chat: &str, now: i64) -> Alert<'_> {
    Alert {
        chat,
        name: "Claude",
        face: "clawd",
        text: "Not sent. Open the chat to try again".into(),
        at: now,
        mine: None,
        note: None,
        answers: (vec![], false, false),
        updates: false,
        alert: false,
    }
}

/// acted's work; `kept` turns true once my message is saved to send.
fn answer(env: &mut JNIEnv, ctx: &JObject, intent: &JObject, chat: &str, kept: &mut bool) -> Result<()> {
    let action = env.call_method(intent, "getAction", "()Ljava/lang/String;", &[])?.l()?;
    let choice = if jx::string(env, action)?.as_deref() == Some(CHOICE) { jx::extra(env, intent, "choice")? } else { None };
    let typed_words = choice.is_none();
    let text = match choice {
        Some(c) => c,
        None => typed(env, intent)?.unwrap_or_default(),
    };
    let text = text.trim().to_string();
    let now = chats::now();
    // None: nothing could be read (the store, or chat isn't set up); Some(None): the chat is gone or deleted
    let mut seen: Option<Option<(Chat, bool, bool)>> = None;
    *kept = inbox::post(env, ctx, chat, |s| {
        let r = s.answer(chat, &text, now);
        let msg = r.as_ref().and_then(|(_, m, _)| m.clone());
        seen = Some(r.map(|(c, m, long)| (c, m.is_some(), long)));
        msg
    })?;
    let (c, tried, long) = match seen {
        Some(Some(v)) => v,
        Some(None) => {
            let nm = service(env, ctx, "notification")?;
            return env.call_method(&nm, "cancel", "(I)V", &[JValue::Int(notification_id(chat))]).map(drop);
        }
        None => {
            keep_typed(env, ctx, typed_words, &text);
            return show(env, ctx, &not_sent(chat, now));
        }
    };
    let last = c.lines.iter().rev().find(|l| !l.me && l.state != "partial");
    let (sent, note, answers) = chats::after_answer(&c, tried, *kept, now);
    // "in the chat" only once it really was saved there (else the words would be lost with this notification)
    let kept_long = long && inbox::saved_draft(chat).is_some_and(|d| d.ends_with(text.as_str()));
    // typed words kept nowhere (the store couldn't be written) go on the clipboard, and the note says so
    let lost = (long && !kept_long) || (tried && !*kept);
    let note = match () {
        _ if kept_long => Some("Too long to send from here. It's in the chat"),
        _ if lost && keep_typed(env, ctx, typed_words, &text) => Some("Not sent. Copied, so you can paste it"),
        _ if long => Some("Not sent. Open the chat to try again"),
        _ => note,
    };
    let a = Alert {
        chat,
        name: &chats::label(&c),
        face: chats::face(&c.agent, c.model.as_deref()),
        text: chats::brief(&last.map(chats::shown).unwrap_or_default()),
        at: last.map_or(now, |l| l.at),
        mine: sent.then_some(text.as_str()),
        note,
        answers,
        updates: c.kind == "note",
        alert: false,
    };
    show(env, ctx, &a)?;
    if sent {
        recount(env, ctx, chat)?; // answered from here: this chat no longer counts
    }
    Ok(())
}

/// Typed words that couldn't be kept in the app, put on the clipboard (Android's Reply box has emptied); true if so.
fn keep_typed(env: &mut JNIEnv, ctx: &JObject, typed: bool, text: &str) -> bool {
    typed && !text.is_empty() && jx::ok(env, |env| jx::copy(env, ctx, text)).is_some()
}

/// Reply's typed words, from the intent the notification filled in.
fn typed(env: &mut JNIEnv, intent: &JObject) -> Result<Option<String>> {
    let sig = "(Landroid/content/Intent;)Landroid/os/Bundle;";
    let b = env.call_static_method("android/app/RemoteInput", "getResultsFromIntent", sig, &[JValue::Object(intent)])?.l()?;
    if b.is_null() {
        return Ok(None);
    }
    let k = s(env, TYPED)?;
    let t = env.call_method(&b, "getCharSequence", "(Ljava/lang/String;)Ljava/lang/CharSequence;", &[JValue::Object(&k)])?.l()?;
    if t.is_null() {
        return Ok(None);
    }
    let t = env.call_method(&t, "toString", "()Ljava/lang/String;", &[])?.l()?;
    jx::string(env, t)
}

/// The channel for a chat: quiet modes post silently (Notification.Builder has no public setSilent: calling it threw,
/// so no reply was ever shown); updates are routine, so they never sound or peek.
fn channel<'a>(env: &mut JNIEnv<'a>, ctx: &JObject, nm: &JObject, updates: bool) -> Result<JObject<'a>> {
    let quiet = matches!(settings::mode(env, ctx)?.as_str(), "none" | "sleep");
    let id = if quiet {
        ring::channel(env, nm, "chat_replies_quiet", "Chat replies (quiet)", 2, true)?; // IMPORTANCE_LOW
        "chat_replies_quiet"
    } else if updates {
        ring::channel(env, nm, "chat_updates", "Updates from the PC", 2, true)?;
        "chat_updates"
    } else {
        ring::channel(env, nm, "chat_replies", "Chat replies", 4, false)?; // IMPORTANCE_HIGH
        "chat_replies"
    };
    s(env, id)
}

/// A notification for one chat or the summary, with the app's chat glyph and the group.
fn builder<'a>(env: &mut JNIEnv<'a>, ctx: &JObject, nm: &JObject, updates: bool) -> Result<JObject<'a>> {
    let ch = channel(env, ctx, nm, updates)?;
    let b = env.new_object("android/app/Notification$Builder", "(Landroid/content/Context;Ljava/lang/String;)V", &[JValue::Object(ctx), JValue::Object(&ch)])?;
    let icon = jx::res(env, ctx, "drawable", "ic_chat_small")?;
    jx::call(env, &b, "setSmallIcon", &format!("(I){BUILDER}"), &[JValue::Int(icon)])?;
    let group = s(env, GROUP)?;
    jx::call(env, &b, "setGroup", &format!("(Ljava/lang/String;){BUILDER}"), &[JValue::Object(&group)])?;
    Ok(b)
}

/// Opens the app at a chat ("" = the chat list).
fn open<'a>(env: &mut JNIEnv<'a>, ctx: &JObject, chat: &str, code: i32) -> Result<JObject<'a>> {
    let home = jx::intent(env, ctx, "MainActivity")?;
    env.call_method(&home, "addFlags", "(I)Landroid/content/Intent;", &[JValue::Int(0x0400_0000 | 0x2000_0000 | 0x1000_0000)])?; // CLEAR_TOP | SINGLE_TOP | NEW_TASK
    let c = s(env, chat)?;
    jx::put(env, &home, "chat", JValue::Object(&c))?;
    jx::pending(env, ctx, "getActivity", code, &home)
}

fn show(env: &mut JNIEnv, ctx: &JObject, a: &Alert) -> Result<()> {
    let nm = service(env, ctx, "notification")?;
    let id = notification_id(a.chat);
    let b = builder(env, ctx, &nm, a.updates)?;
    ring::set_text(env, &b, "setContentTitle", a.name)?;
    ring::set_text(env, &b, "setContentText", a.mine.map_or(a.text.clone(), |m| format!("You: {m}")).as_str())?;
    let tap = open(env, ctx, a.chat, id)?;
    jx::call(env, &b, "setContentIntent", &format!("(Landroid/app/PendingIntent;){BUILDER}"), &[JValue::Object(&tap)])?;
    jx::call(env, &b, "setAutoCancel", &format!("(Z){BUILDER}"), &[JValue::Bool(1)])?;
    jx::call(env, &b, "setOnlyAlertOnce", &format!("(Z){BUILDER}"), &[JValue::Bool(u8::from(!a.alert))])?;
    jx::call(env, &b, "setWhen", &format!("(J){BUILDER}"), &[JValue::Long(a.at)])?;
    let cat = s(env, "msg")?;
    jx::call(env, &b, "setCategory", &format!("(Ljava/lang/String;){BUILDER}"), &[JValue::Object(&cat)])?;
    if let Some(n) = a.note {
        ring::set_text(env, &b, "setSubText", n)?;
    }
    // The conversation: the agent's message, and my answer under it once sent from here.
    let you = {
        let p = env.new_object("android/app/Person$Builder", "()V", &[])?;
        let n = s(env, "You")?;
        env.call_method(&p, "setName", "(Ljava/lang/CharSequence;)Landroid/app/Person$Builder;", &[JValue::Object(&n)])?;
        env.call_method(&p, "build", "()Landroid/app/Person;", &[])?.l()?
    };
    let them = person(env, a.name, a.face)?;
    let style = env.new_object("android/app/Notification$MessagingStyle", "(Landroid/app/Person;)V", &[JValue::Object(&you)])?;
    let add = "(Ljava/lang/CharSequence;JLandroid/app/Person;)Landroid/app/Notification$MessagingStyle;";
    let t = s(env, &a.text)?;
    env.call_method(&style, "addMessage", add, &[JValue::Object(&t), JValue::Long(a.at), JValue::Object(&them)])?;
    if let Some(m) = a.mine {
        let (t, none) = (s(env, m)?, JObject::null()); // no sender: the user
        env.call_method(&style, "addMessage", add, &[JValue::Object(&t), JValue::Long(jx::now_ms()), JValue::Object(&none)])?;
    }
    jx::call(env, &b, "setStyle", &format!("(Landroid/app/Notification$Style;){BUILDER}"), &[JValue::Object(&style)])?;
    // The buttons, at most three (chats::quick_answers): choices, Reply, Open.
    let (choices, reply, more) = &a.answers;
    for (k, c) in choices.iter().enumerate() {
        let i = jx::intent(env, ctx, "ChatAction")?;
        let what = s(env, CHOICE)?;
        env.call_method(&i, "setAction", "(Ljava/lang/String;)Landroid/content/Intent;", &[JValue::Object(&what)])?;
        let (chat, choice) = (s(env, a.chat)?, s(env, c)?);
        jx::put(env, &i, "chat", JValue::Object(&chat))?;
        jx::put(env, &i, "choice", JValue::Object(&choice))?;
        let p = jx::pending(env, ctx, "getBroadcast", id * 8 + k as i32, &i)?;
        let act = action(env, c, &p, None)?;
        jx::call(env, &b, "addAction", &format!("(Landroid/app/Notification$Action;){BUILDER}"), &[JValue::Object(&act)])?;
    }
    if *reply {
        let i = jx::intent(env, ctx, "ChatAction")?;
        let chat = s(env, a.chat)?;
        jx::put(env, &i, "chat", JValue::Object(&chat))?;
        // Mutable, so Android can put the typed words in; explicit (one class of this app), so nothing else can.
        let sig = "(Landroid/content/Context;ILandroid/content/Intent;I)Landroid/app/PendingIntent;";
        let flags = 0x0200_0000 | 0x0800_0000; // FLAG_MUTABLE | FLAG_UPDATE_CURRENT
        let p = env.call_static_method("android/app/PendingIntent", "getBroadcast", sig, &[JValue::Object(ctx), JValue::Int(id * 8 + 3), JValue::Object(&i), JValue::Int(flags)])?.l()?;
        let input = {
            let k = s(env, TYPED)?;
            let rb = env.new_object("android/app/RemoteInput$Builder", "(Ljava/lang/String;)V", &[JValue::Object(&k)])?;
            let l = s(env, "Reply")?;
            env.call_method(&rb, "setLabel", "(Ljava/lang/CharSequence;)Landroid/app/RemoteInput$Builder;", &[JValue::Object(&l)])?;
            env.call_method(&rb, "build", "()Landroid/app/RemoteInput;", &[])?.l()?
        };
        let act = action(env, "Reply", &p, Some(&input))?;
        jx::call(env, &b, "addAction", &format!("(Landroid/app/Notification$Action;){BUILDER}"), &[JValue::Object(&act)])?;
    }
    if *more {
        let act = action(env, "Open", &tap, None)?;
        jx::call(env, &b, "addAction", &format!("(Landroid/app/Notification$Action;){BUILDER}"), &[JValue::Object(&act)])?;
    }
    let n = env.call_method(&b, "build", "()Landroid/app/Notification;", &[])?.l()?;
    env.call_method(&nm, "notify", "(ILandroid/app/Notification;)V", &[JValue::Int(id), JValue::Object(&n)])?;
    Ok(())
}

/// The group's summary: how many chats need you; a tap opens the chat list. Silent (the message itself alerts), and
/// it stays when tapped (cancelling it would clear every chat's notification); reading the last chat removes it.
fn summary(env: &mut JNIEnv, ctx: &JObject, a: &Alert, need: usize) -> Result<()> {
    let nm = service(env, ctx, "notification")?;
    let b = builder(env, ctx, &nm, a.updates)?;
    let title = if need == 1 { "1 chat needs you".to_string() } else { format!("{need} chats need you") };
    ring::set_text(env, &b, "setContentTitle", &title)?;
    ring::set_text(env, &b, "setContentText", &summary_line(a))?;
    let tap = open(env, ctx, "", SUMMARY_ID)?;
    jx::call(env, &b, "setContentIntent", &format!("(Landroid/app/PendingIntent;){BUILDER}"), &[JValue::Object(&tap)])?;
    jx::call(env, &b, "setGroupSummary", &format!("(Z){BUILDER}"), &[JValue::Bool(1)])?;
    jx::call(env, &b, "setGroupAlertBehavior", &format!("(I){BUILDER}"), &[JValue::Int(2)])?; // GROUP_ALERT_CHILDREN
    let n = env.call_method(&b, "build", "()Landroid/app/Notification;", &[])?.l()?;
    env.call_method(&nm, "notify", "(ILandroid/app/Notification;)V", &[JValue::Int(SUMMARY_ID), JValue::Object(&n)])?;
    *SUMMED.lock().unwrap() = Some((need, summary_line(a)));
    Ok(())
}

/// A notification button; with `input`, it opens a text box in the notification (Reply).
fn action<'a>(env: &mut JNIEnv<'a>, label: &str, p: &JObject, input: Option<&JObject>) -> Result<JObject<'a>> {
    let (none, t) = (JObject::null(), s(env, label)?);
    let ab = env.new_object(
        "android/app/Notification$Action$Builder",
        "(Landroid/graphics/drawable/Icon;Ljava/lang/CharSequence;Landroid/app/PendingIntent;)V",
        &[JValue::Object(&none), JValue::Object(&t), JValue::Object(p)],
    )?;
    if let Some(i) = input {
        env.call_method(&ab, "addRemoteInput", "(Landroid/app/RemoteInput;)Landroid/app/Notification$Action$Builder;", &[JValue::Object(i)])?;
        env.call_method(&ab, "setSemanticAction", "(I)Landroid/app/Notification$Action$Builder;", &[JValue::Int(1)])?; // SEMANTIC_ACTION_REPLY
        env.call_method(&ab, "setAllowGeneratedReplies", "(Z)Landroid/app/Notification$Action$Builder;", &[JValue::Bool(0)])?;
    }
    env.call_method(&ab, "build", "()Landroid/app/Notification$Action;", &[])?.l()
}

/// The agent as the conversation's other person: its name and its face (the chat list's still), framed on the body.
fn person<'a>(env: &mut JNIEnv<'a>, name: &str, face: &str) -> Result<JObject<'a>> {
    let png: &[u8] = match face {
        "haiku" => include_bytes!("../ui/faces/haiku.png"),
        "sonnet" => include_bytes!("../ui/faces/sonnet.png"),
        "opus" => include_bytes!("../ui/faces/opus.png"),
        "null" => include_bytes!("../ui/faces/null.png"),
        _ => include_bytes!("../ui/faces/clawd.png"),
    };
    let p = env.new_object("android/app/Person$Builder", "()V", &[])?;
    let n = s(env, name)?;
    env.call_method(&p, "setName", "(Ljava/lang/CharSequence;)Landroid/app/Person$Builder;", &[JValue::Object(&n)])?;
    let bytes = env.byte_array_from_slice(png)?;
    let bmp = env.call_static_method("android/graphics/BitmapFactory", "decodeByteArray", "([BII)Landroid/graphics/Bitmap;", &[JValue::Object(&bytes), JValue::Int(0), JValue::Int(png.len() as i32)])?.l()?;
    if !bmp.is_null() {
        // Android rounds it to a circle: a square on the body (the stills stand low in their frame).
        let w = env.call_method(&bmp, "getWidth", "()I", &[])?.i()?;
        let h = env.call_method(&bmp, "getHeight", "()I", &[])?.i()?;
        let side = (w.min(h) as f32 * 0.85) as i32;
        let x = ((w - side) / 2).clamp(0, w - side);
        let y = ((h as f32 * 0.6) as i32 - side / 2).clamp(0, h - side);
        let sig = "(Landroid/graphics/Bitmap;IIII)Landroid/graphics/Bitmap;";
        let sq = env.call_static_method("android/graphics/Bitmap", "createBitmap", sig, &[JValue::Object(&bmp), JValue::Int(x), JValue::Int(y), JValue::Int(side), JValue::Int(side)])?.l()?;
        let icon = env.call_static_method("android/graphics/drawable/Icon", "createWithBitmap", "(Landroid/graphics/Bitmap;)Landroid/graphics/drawable/Icon;", &[JValue::Object(&sq)])?.l()?;
        env.call_method(&p, "setIcon", "(Landroid/graphics/drawable/Icon;)Landroid/app/Person$Builder;", &[JValue::Object(&icon)])?;
    }
    env.call_method(&p, "build", "()Landroid/app/Person;", &[])?.l()
}
