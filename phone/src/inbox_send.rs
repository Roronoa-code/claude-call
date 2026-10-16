use super::*;

static FETCHING: Mutex<Vec<String>> = Mutex::new(Vec::new());
static SENDING: Mutex<Vec<String>> = Mutex::new(Vec::new());
static ASKED: Mutex<Vec<(String, Instant)>> = Mutex::new(Vec::new());
static WAITING: AtomicU64 = AtomicU64::new(0);
static AUTO_AT: Mutex<Option<Instant>> = Mutex::new(None);

pub(crate) fn post(env: &mut JNIEnv, ctx: &JObject, chat: &str, f: impl FnOnce(&mut Store) -> Option<String>) -> Result<bool> {
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

pub fn auto(env: &mut JNIEnv, ctx: &JObject) -> Result<()> {
    let Ok(mut last) = AUTO_AT.try_lock() else { return Ok(()) };
    if last.is_some_and(|t| t.elapsed() < Duration::from_millis(900)) {
        return Ok(());
    }
    *last = Some(Instant::now());
    let now = chats::now();
    path(env, ctx)?;
    let Some((due, stalled, waiting)) = read(|s| {
        let waiting = s.chats.iter().flat_map(|c| &c.lines).filter(|l| chats::unsent(l, now) == chats::Unsent::Waiting).count() as u64;
        let parts = s.stalled(now).into_iter().map(|(c, m)| (c, m, 60_000));
        let taken = s.unanswered(now).into_iter().map(|(c, m, since)| (c, m, ((now - since) / 4).clamp(60_000, 30 * 60_000)));
        (s.due(now), parts.chain(taken).collect::<Vec<_>>(), waiting)
    }) else {
        return Ok(());
    };
    if WAITING.swap(waiting, Relaxed) != waiting {
        changed();
    }
    let busy = SENDING.lock().unwrap().clone();
    let due: Vec<_> = due.into_iter().filter(|(_, m)| !busy.contains(m)).collect();
    let stalled: Vec<_> = {
        let mut asked = ASKED.lock().unwrap();
        asked.retain(|(_, t)| t.elapsed() < Duration::from_secs(30 * 60));
        let wait = |ms: i64| Duration::from_millis(ms as u64);
        let s: Vec<_> = stalled.into_iter().filter(|(_, m, w)| !asked.iter().any(|(a, t)| a == m && t.elapsed() < wait(*w))).collect();
        asked.retain(|(a, _)| !s.iter().any(|(_, m, _)| m == a));
        asked.extend(s.iter().map(|(_, m, _)| (m.clone(), Instant::now())));
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
    for (chat, msg, _) in stalled {
        if let Some(Some(body)) = read(|s| s.body(&chat, &msg, &key)) {
            let topic = topic.clone();
            net::RT.spawn(async move { net::publish_chat(&topic, body).await });
        }
    }
    Ok(())
}
