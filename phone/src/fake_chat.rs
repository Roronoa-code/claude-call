//! PREVIEW (branch preview/chats-2, not for main): with `debug.callme.fake` set (the test emulator only), a chat's PC is
//! played here on the phone, so the chat screens can be shown in every state without anything reaching the PC: a
//! message goes "Sent", then "Working…", then a canned answer arrives; Stop stops it; + picks filesDir/pick.jpg
//! instead of opening the system picker. Nothing is published, uploaded or asked of the PC.
use crate::{
    attach,
    chats::{self, Store},
    inbox,
};
use serde_json::json;
use std::{
    sync::atomic::{AtomicBool, AtomicUsize, Ordering::Relaxed},
    thread,
    time::Duration,
};

static ON: AtomicBool = AtomicBool::new(false);
static N: AtomicUsize = AtomicUsize::new(0);

pub fn set(on: bool) {
    ON.store(on, Relaxed);
}

pub fn on() -> bool {
    ON.load(Relaxed)
}

const ANSWERS: [&str; 2] = [
    "Found it. The laptop rebuilds every crate because `CARGO_TARGET_DIR` points at the USB disk, so nothing stays \
     warm between builds.\n\n\
     **What I'd change**\n\n\
     1. Point the build back at the fast drive in `.cargo/config.toml`:\n\
     ```toml\n[build]\ntarget-dir = \"B:/Projects/CallMe/target\"\n```\n\
     2. Run `cargo clean -p callme-phone` once, so the old half-built files go.\n\
     3. Rebuild. A release build should take about **2 min** instead of 9.\n\n\
     Want me to make the change?",
    "Done. `config.toml` now points at B:, and a clean release build took **2 min 04 s**.",
];

/// My message, added as `add` says, then played out: sent, worked on, answered (or stopped, for "stop"). True once
/// it is kept.
pub fn post(chat: &str, add: impl FnOnce(&mut Store) -> Option<String>) -> bool {
    let Some(msg) = inbox::update(add).flatten() else { return false };
    inbox::changed();
    let chat = chat.to_string();
    thread::spawn(move || {
        let reply = |state: &str, text: &str, re: &str| {
            let open = inbox::viewing();
            let v = json!({ "v": 1, "chat": chat, "re": re, "state": state, "text": text, "part": 1, "parts": 1 });
            inbox::update(|s| s.apply_reply(&v, open.as_deref()));
            inbox::changed();
        };
        thread::sleep(Duration::from_millis(700));
        inbox::update(|s| s.sent(&chat, &msg, true));
        inbox::changed();
        let stop = inbox::update(|s| s.get(&chat).and_then(|c| c.lines.iter().rev().find(|l| l.me && l.id == msg)).is_some_and(|l| l.text == "stop"));
        if stop == Some(true) {
            // the message it was working on is stopped; the "stop" itself needs no answer
            let working = inbox::update(|s| {
                let c = s.get(&chat)?;
                c.lines.iter().rev().find(|l| l.me && l.id != msg && l.state == "working").map(|l| l.id.clone())
            });
            thread::sleep(Duration::from_millis(600));
            if let Some(Some(w)) = working {
                reply("stopped", "", &w);
            }
            inbox::update(|s| {
                if let Some(l) = s.chats.iter_mut().find(|c| c.id == chat).and_then(|c| c.lines.iter_mut().find(|l| l.me && l.id == msg)) {
                    l.state = "answered".into();
                }
            });
            inbox::changed();
            return;
        }
        thread::sleep(Duration::from_millis(900));
        reply("working", "", &msg);
        thread::sleep(Duration::from_millis(5200));
        let n = N.fetch_add(1, Relaxed);
        // still being worked on (not stopped meanwhile): the answer
        let working = inbox::update(|s| s.get(&chat).and_then(|c| c.lines.iter().find(|l| l.me && l.id == msg)).is_some_and(|l| l.state == "working"));
        if working == Some(true) {
            reply("done", ANSWERS[n % ANSWERS.len()], &msg);
        }
    });
    true
}

/// + : filesDir/pick.jpg as if picked in the system picker (a moment "Getting it ready…" first).
pub fn pick(chat: &str) {
    let Some(dir) = inbox::media_dir() else { return };
    attach::set_loading(chat);
    let chat = chat.to_string();
    thread::spawn(move || {
        thread::sleep(Duration::from_millis(900));
        let file = dir.with_file_name("pick.jpg");
        let got = std::fs::read(&file).map_err(|_| "Couldn't read that file".to_string());
        let m = got.and_then(|b| {
            let m = chats::Media::picked(&b, "image/jpeg")?;
            let path = dir.join(m.file_name());
            std::fs::create_dir_all(&dir).and_then(|_| std::fs::write(&path, &b)).map_err(|_| "Couldn't save that file".to_string())?;
            Ok(chats::Media { path: path.to_string_lossy().into_owned(), ..m })
        });
        attach::set_result(&chat, m);
    });
}
