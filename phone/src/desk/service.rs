use std::sync::{Mutex, atomic::Ordering};

static ACTIVE: Mutex<Option<String>> = Mutex::new(None);
static RINGING: Mutex<Option<String>> = Mutex::new(None);

pub fn active_link() -> Option<String> {
    ACTIVE.lock().unwrap().clone()
}

pub fn start() {
    let Some(topic) = crate::android::with(|env, act| crate::settings::topic(env, act)) else { return };
    let generation = crate::net::GEN.load(Ordering::Relaxed);
    std::thread::Builder::new().name("ntfy".into()).spawn(move || {
        let (mut env, act) = (crate::jx::JNIEnv(std::marker::PhantomData), crate::jx::JObject(std::marker::PhantomData));
        crate::net::listen(&mut env, &act, topic, generation);
    }).ok();
}

pub fn watch(link: String) {
    *ACTIVE.lock().unwrap() = Some(link.clone());
    *RINGING.lock().unwrap() = Some(link.clone());
    crate::net::RT.spawn(async move {
        let (mut v, mut wait) = (-1i64, 2u64);
        while RINGING.lock().unwrap().as_deref() == Some(link.as_str()) {
            let Some(url) = crate::ringwatch::state_url(&link, v) else { return };
            match crate::net::HTTP.get(url).timeout(std::time::Duration::from_secs(30)).send().await.and_then(|r| r.error_for_status()) {
                Ok(r) => {
                    let Ok(st) = r.json::<serde_json::Value>().await else { continue };
                    wait = 2;
                    v = st["v"].as_i64().unwrap_or(v);
                    if crate::ringwatch::taken(&st) && RINGING.lock().unwrap().as_deref() == Some(link.as_str()) {
                        let _ = slint::invoke_from_event_loop(crate::desk_app::close_call);
                        return;
                    }
                }
                Err(_) => {
                    tokio::time::sleep(std::time::Duration::from_secs(wait)).await;
                    wait = (wait * 2).min(30);
                }
            }
        }
    });
}

pub fn answered_here() {
    *RINGING.lock().unwrap() = None;
}

pub fn ended() {
    *RINGING.lock().unwrap() = None;
    *ACTIVE.lock().unwrap() = None;
}
