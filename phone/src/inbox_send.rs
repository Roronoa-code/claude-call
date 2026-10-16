//! Sending my messages (and trying again by themselves), and fetching the PC's photos and videos (inbox.rs).
use super::*;

/// Photo and video links being fetched right now, so each is fetched once.
static FETCHING: Mutex<Vec<String>> = Mutex::new(Vec::new());
/// My messages being sent right now (never tried again by themselves meanwhile).
static SENDING: Mutex<Vec<String>> = Mutex::new(Vec::new());
/// My messages whose answer is missing a part, and when the PC was last asked for it again.
static ASKED: Mutex<Vec<(String, i64)>> = Mutex::new(Vec::new());
/// How many of my messages are waiting to be tried again (when it changes, the thread redraws their lines).
static WAITING: AtomicU64 = AtomicU64::new(0);
/// When my messages were last looked at for trying again (ms). The home screen and the listener both ask every second;
/// one at a time, at most once a second, so one message is never sent twice at once.
static AUTO_AT: Mutex<i64> = Mutex::new(0);

/// Adds or retries a message of mine (`f` says which) and sends it in the background. True once it is kept (saved)
/// to send.
pub(crate) fn post(env: &mut JNIEnv, ctx: &JObject, chat: &str, f: impl FnOnce(&mut Store) -> Option<String>) -> Result<bool> {
    if crate::fake_chat::on() {
        path(env, ctx)?;
        return Ok(crate::fake_chat::post(chat, f));
    }
    let Some(key) = settings::chat_key(env, ctx)? else { return Ok(false) };
    let topic = settings::topic(env, ctx)?;
    path(env, ctx)?;
    let msg = update(f).flatten();
    changed();
    let Some(msg) = msg else { return Ok(false) };
    SENDING.lock().unwrap().push(msg.clone());
    let chat = chat.to_string();
    net::RT.spawn(async move {
        let ok = deliver(&topic, &key, &chat, &msg).await;
        update(|s| s.sent(&chat, &msg, ok));
        SENDING.lock().unwrap().retain(|m| *m != msg);
        changed();
    });
    Ok(true)
}

/// Uploads my message's photo or video first (a retry uploads it again), then posts the signed message.
async fn deliver(topic: &str, key: &str, chat: &str, msg: &str) -> bool {
    if let Some(m) = read(|s| s.to_upload(chat, msg)).flatten() {
        let Some(bytes) = std::fs::read(&m.path).ok() else { return false };
        let Some(url) = net::upload(topic, &m.name, bytes).await else {
            android::log("chats: a photo or video couldn't be uploaded");
            return false;
        };
        update(|s| s.uploaded(chat, msg, &url));
    }
    let Some(body) = read(|s| s.body(chat, msg, key)).flatten() else { return false };
    net::publish_chat(topic, body).await
}

/// Fetches every photo and video still loading, in the background, right away (ntfy keeps them only 3 hours). Each
/// is kept only if its size and sha256 are what the PC signed for, as filesDir/media/<sha256>.<ext>.
pub(super) fn fetch_media() {
    let Some(path) = PATH.get() else { return };
    let dir = path.with_file_name("media");
    let todo = read(Store::downloads).unwrap_or_default();
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
                saved.as_ref().is_some_and(|p| !s.uses(p))
            });
            if let (Some(true), Some(p)) = (unused, &saved) {
                let _ = std::fs::remove_file(p);
            }
            FETCHING.lock().unwrap().retain(|u| *u != m.url);
            changed();
        });
    }
}

/// Every second: my messages that didn't go are tried again by themselves (with a growing wait), and an answer missing
/// a part is asked for again (the PC sends a recorded answer again for the same message, never runs it twice). Called
/// by the home screen and by the listener (net.rs), so a Reply sent from a notification is retried with the app closed.
pub fn auto(env: &mut JNIEnv, ctx: &JObject) -> Result<()> {
    if crate::fake_chat::on() {
        return Ok(());
    }
    let mut last = AUTO_AT.lock().unwrap(); // held to the end: one caller at a time
    let now = chats::now();
    if now - *last < 900 {
        return Ok(());
    }
    *last = now;
    path(env, ctx)?;
    let Some((due, stalled, waiting)) = read(|s| {
        let waiting = s.chats.iter().flat_map(|c| &c.lines).filter(|l| chats::unsent(l, now) == "waiting").count() as u64;
        (s.due(now), s.stalled(now), waiting)
    }) else {
        return Ok(());
    };
    if WAITING.swap(waiting, Relaxed) != waiting {
        changed(); // one started or stopped waiting: its line under it changes
    }
    let busy = SENDING.lock().unwrap().clone();
    let due: Vec<_> = due.into_iter().filter(|(_, m)| !busy.contains(m)).collect();
    let stalled: Vec<_> = {
        let mut asked = ASKED.lock().unwrap();
        asked.retain(|(_, t)| now - t < 30 * 60_000);
        let s: Vec<_> = stalled.into_iter().filter(|(_, m)| !asked.iter().any(|(a, t)| a == m && now - t < 60_000)).collect();
        asked.retain(|(a, _)| !s.iter().any(|(_, m)| m == a));
        asked.extend(s.iter().map(|(_, m)| (m.clone(), now)));
        s
    };
    if due.is_empty() && stalled.is_empty() {
        return Ok(());
    }
    for (chat, msg) in &due {
        post(env, ctx, chat, |s| s.retry(chat, msg, false))?;
    }
    if stalled.is_empty() {
        return Ok(());
    }
    let Some(key) = settings::chat_key(env, ctx)? else { return Ok(()) };
    let topic = settings::topic(env, ctx)?;
    for (chat, msg) in stalled {
        // the very message it answered, unchanged: the PC knows it and only replays its answer
        if let Some(Some(body)) = read(|s| s.body(&chat, &msg, &key)) {
            let topic = topic.clone();
            net::RT.spawn(async move { net::publish_chat(&topic, body).await });
        }
    }
    Ok(())
}
