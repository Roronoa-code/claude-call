//! The chats rules' tests (chats.rs): the store, signatures (with the vectors the PC shares), replies and notes.
use super::*;

fn store() -> (Store, String, String) {
    let mut s = Store::default();
    let chat = s.new_chat("claude", Some("haiku-5.5"));
    let msg = s.add_mine(&chat, "hello", None).unwrap();
    (s, chat, msg)
}

fn reply(chat: &str, re: &str, state: &str, text: &str, part: u32, parts: u32) -> Value {
    json!({"v": 1, "chat": chat, "re": re, "state": state, "text": text, "part": part, "parts": parts})
}

#[test]
fn save_load_round_trip() {
    let (s, ..) = store();
    let path = std::env::temp_dir().join(format!("callme-chats-{}.json", std::process::id()));
    s.save(&path).unwrap();
    assert_eq!(Store::load(&path), Some(s));
    std::fs::remove_file(path).unwrap();
    assert_eq!(Store::load(Path::new("no/such/file")), Some(Store::default()));
}

#[test]
fn working_then_done_in_parts() {
    let (mut s, c, m) = store();
    assert!(s.apply_reply(&reply(&c, &m, "working", "", 1, 1), None).is_none());
    assert_eq!(s.get(&c).unwrap().lines[0].state, "working");
    for (p, t) in [(2, "world "), (1, "hello "), (2, "world "), (3, "!")] {
        let r = s.apply_reply(&reply(&c, &m, "done", t, p, 3), None);
        assert_eq!(r.is_some(), p == 3, "notifies once, when whole");
    }
    assert!(s.apply_reply(&reply(&c, &m, "done", "!", 3, 3), None).is_none(), "replay");
    let chat = s.get(&c).unwrap();
    assert_eq!((chat.lines[1].text.as_str(), chat.lines[1].state.as_str()), ("hello world !", "done"));
    assert_eq!((chat.lines[0].state.as_str(), chat.unread, chat.lines.len()), ("answered", 1, 2));
    assert_eq!(status(chat, now()), ("", ""));
}

#[test]
fn unread_only_when_not_open_and_mark_read() {
    let (mut s, c, m) = store();
    let m2 = s.add_mine(&c, "again", None).unwrap();
    assert!(s.apply_reply(&reply(&c, &m, "done", "a", 1, 1), Some(&c)).is_none());
    assert_eq!(s.get(&c).unwrap().unread, 0);
    let (id, name, text) = s.apply_reply(&reply(&c, &m2, "failed", "", 1, 1), Some("other")).unwrap();
    assert_eq!((id, name.as_str(), text.as_str()), (c.clone(), "Claude · Haiku", "Something went wrong."));
    assert_eq!(s.get(&c).unwrap().unread, 1);
    s.mark_read(&c);
    assert_eq!(s.get(&c).unwrap().unread, 0);
    assert!(s.apply_reply(&reply("nope", &m, "done", "a", 1, 1), None).is_none(), "unknown chat");
}

#[test]
fn reply_signature_matches_shared_vector() {
    let good = json!({"v": 1, "chat": "c1", "re": "m1", "state": "done", "text": "héllo 👋", "part": 1, "parts": 1,
        "sig": "260ceed955ae169bfb04cb73eff4e7d7a7faf95ca2abf61ab69fd69e8f1100be"});
    assert!(reply_signed("test-key", &good));
    let mut forged = good.clone();
    forged["text"] = json!("pay me");
    assert!(!reply_signed("test-key", &forged));
    assert!(!reply_signed("other-key", &good));
    let mut unsigned = good.clone();
    unsigned.as_object_mut().unwrap().remove("sig");
    assert!(!reply_signed("test-key", &unsigned));
}

fn att() -> Value {
    json!({"url": "https://ntfy.sh/file/abc123.jpg", "name": "pic é.jpg", "type": "image/jpeg", "size": 1234, "sha256": "ab".repeat(32)})
}

#[test]
fn file_reply_signature_matches_shared_vector() {
    let good = json!({"v": 1, "chat": "c1", "re": "m1", "state": "done", "text": "", "part": 2, "parts": 2, "att": att(),
        "sig": "54b592109f701bf43a7e16049916d3ef5d0974179f835a7081500f2baa4d3d86"});
    assert!(reply_signed("test-key", &good));
    for (k, bad) in [("url", json!("https://ntfy.sh/file/evil.jpg")), ("name", json!("x")), ("type", json!("video/mp4")), ("size", json!(1)), ("sha256", json!("cd".repeat(32)))] {
        let mut forged = good.clone();
        forged["att"][k] = bad;
        assert!(!reply_signed("test-key", &forged), "{k}");
    }
    let mut stripped = good.clone();
    stripped.as_object_mut().unwrap().remove("att");
    assert!(!reply_signed("test-key", &stripped), "a v2 sig is not a v1 sig");
    let mut odd = good.clone();
    odd["att"] = json!("https://ntfy.sh/file/abc123.jpg");
    assert!(!reply_signed("test-key", &odd));
}

#[test]
fn only_safe_files_are_fetched() {
    assert!(Media::from(&att(), 1).is_some());
    for (k, bad) in [
        ("url", json!("http://ntfy.sh/file/abc.jpg")),
        ("url", json!("https://ntfy.sh.evil.com/file/abc.jpg")),
        ("url", json!("https://ntfy.sh/file/../x")),
        ("url", json!("https://ntfy.sh/file/")),
        ("type", json!("application/pdf")),
        ("size", json!(MEDIA_MAX + 1)),
        ("size", json!(0)),
        ("sha256", json!("zz".repeat(32))),
    ] {
        let mut a = att();
        a[k] = bad;
        assert!(Media::from(&a, 1).is_none(), "{a}");
    }
    let m = Media::from(&json!({"url": "https://ntfy.sh/file/x.mp4", "name": "../../a.exe", "type": "video/mp4", "size": 3,
        "sha256": "BA7816BF8F01CFEA414140DE5DAE2223B00361A396177A9CB410FF61F20015AD"}), 1).unwrap();
    assert_eq!(m.file_name(), "ba7816bf8f01cfea414140de5dae2223b00361a396177a9cb410ff61f20015ad.mp4");
    assert!(m.matches(b"abc") && !m.matches(b"abd") && !m.matches(b"abcd"));
}

#[test]
fn text_then_files_in_parts() {
    let (mut s, c, m) = store();
    let video = json!({"url": "https://ntfy.sh/file/v.mp4", "name": "clip.mp4", "type": "video/mp4", "size": 9, "sha256": "cd".repeat(32)});
    let mut r3 = reply(&c, &m, "done", "", 3, 3);
    r3["att"] = video;
    assert!(s.apply_reply(&r3, None).is_none());
    assert_eq!(s.downloads().len(), 1, "fetch starts before the answer is whole");
    let mut r2 = reply(&c, &m, "done", "", 2, 3);
    r2["att"] = att();
    assert!(s.apply_reply(&r2, None).is_none());
    assert!(s.apply_reply(&r2, None).is_none(), "a repeated part is not added twice");
    let (_, _, note) = s.apply_reply(&reply(&c, &m, "done", "Here.", 1, 3), None).unwrap();
    assert_eq!(note, "Here.");
    let l = &s.get(&c).unwrap().lines[1];
    assert_eq!((l.text.as_str(), l.media.iter().map(|m| m.name.as_str()).collect::<Vec<_>>()), ("Here.", vec!["pic é.jpg", "clip.mp4"]));
    assert_eq!(s.downloads().len(), 2);
    s.media_done("https://ntfy.sh/file/v.mp4", Some("/data/v.mp4"));
    s.media_done("https://ntfy.sh/file/abc123.jpg", None);
    let l = &s.get(&c).unwrap().lines[1];
    assert_eq!((l.media[0].state.as_str(), l.media[1].state.as_str(), l.media[1].path.as_str()), ("failed", "ready", "/data/v.mp4"));
    assert!(s.downloads().is_empty());
}

#[test]
fn files_only_reply_says_what_was_sent() {
    let (mut s, c, m) = store();
    let mut r = reply(&c, &m, "done", "", 1, 1);
    r["att"] = att();
    assert_eq!(s.apply_reply(&r, None).unwrap().2, "Sent a photo");
    let line = |mimes: &[&str]| Line {
        state: "done".into(),
        media: mimes.iter().map(|m| Media { mime: m.to_string(), ..Media::default() }).collect(),
        ..Line::default()
    };
    assert_eq!(shown(&line(&["video/mp4"])), "Sent a video");
    assert_eq!(shown(&line(&["image/png", "image/gif"])), "Sent 2 photos");
    assert_eq!(shown(&line(&["image/png", "video/webm", "video/mp4"])), "Sent a photo and 2 videos");
    let mut bad = reply(&c, &m, "done", "", 1, 1);
    bad["att"] = json!({"url": "https://example.com/x.png"});
    let m2 = s.add_mine(&c, "again", None).unwrap();
    bad["re"] = json!(m2);
    assert_eq!(s.apply_reply(&bad, None).unwrap().2, "Done.", "an unsafe file is dropped");
}

#[test]
fn answers_split_into_blocks() {
    let k = |v: Vec<Block>| v.iter().map(|b| (b.kind, b.text.clone(), b.mark.clone(), b.level)).collect::<Vec<_>>();
    let s = |x: &str| x.to_string();
    let text = "Two things:\nmore of it\n\n## Fix\n1. Set **it** back\n   still the same item\n2) Run `cargo`\n  - nested\n\n---\n> quoted\n> on\n```rust\nfn a() {\n\n    b();\n}\n```\nEnd.\n```sh\nopen fence";
    assert_eq!(k(blocks(text)), vec![
        ("p", s("Two things:\nmore of it"), s(""), 0),
        ("h", s("Fix"), s(""), 0),
        ("li", s("Set **it** back\nstill the same item"), s("1."), 0),
        ("li", s("Run `cargo`"), s("2."), 0),
        ("li", s("nested"), s("•"), 1),
        ("quote", s("quoted\non"), s(""), 0),
        ("code", s("fn a() {\n\n    b();\n}"), s("rust"), 0),
        ("p", s("End."), s(""), 0),
        ("code", s("open fence"), s("sh"), 0),
    ]);
    assert_eq!(k(blocks("*not a list* but text")), vec![("p", s("*not a list* but text"), s(""), 0)]);
    assert_eq!(k(blocks("2024. was a year")), vec![("p", s("2024. was a year"), s(""), 0)]);
    assert!(blocks("").is_empty() && blocks("```\n```")[0].text.is_empty());
    assert_eq!(inline("Vec<u8> and `Vec<u8>` and a<b"), "Vec&lt;u8> and `Vec<u8>` and a&lt;b");
}

#[test]
fn delete_undo_keeps_late_replies_and_purge_frees_files() {
    let (mut s, c, m) = store();
    let pic = |path: &str| Media { path: path.into(), state: "ready".into(), ..Media::default() };
    s.get_mut(&c).unwrap().lines[0].media = vec![pic("/m/shared.jpg"), pic("/m/only.jpg")];
    s.get_mut(&c).unwrap().pick = Some(pic("/m/picked.jpg"));
    let other = s.new_chat("codex", None);
    let m2 = s.add_mine(&other, "hi", Some(pic("/other/spelling/shared.jpg"))).unwrap();
    s.apply_reply(&reply(&c, &m, "working", "", 1, 1), None);

    assert!(s.delete(&c, 1000));
    assert!(!s.delete(&c, 2000), "once");
    assert_eq!(s.undoable().unwrap().id, c);
    // the PC answers while Undo is still up: kept (no notification), and back with Undo
    assert!(s.apply_reply(&reply(&c, &m, "done", "late", 1, 1), None).is_none());
    assert!(s.undelete(&c));
    assert_eq!(s.get(&c).unwrap().lines[1].text, "late");
    assert_eq!(s.get(&c).unwrap().unread, 1);

    // two deletes in a row: each keeps its own time; the first going for good leaves the second undoable
    assert!(s.delete(&c, 1000) && s.delete(&other, 4000));
    assert_eq!(s.undoable().unwrap().id, other, "Undo brings back the newest");
    let gone = s.purge(999);
    assert!(gone.is_empty(), "not before its time");
    let gone = s.purge(1000);
    assert_eq!(gone.len(), 1);
    assert_eq!((s.get(&c), s.undoable().map(|c| c.id.clone())), (None, Some(other.clone())));
    assert_eq!(s.unused_files(&gone[0]), vec!["only.jpg".to_string(), "picked.jpg".to_string()], "a file the other chat uses stays");
    // gone for good: its replies are dropped
    assert!(s.apply_reply(&reply(&c, &m, "done", "later", 1, 1), None).is_none());
    assert!(s.get(&c).is_none());
    assert!(s.undelete(&other));
    assert!(s.apply_reply(&reply(&other, &m2, "done", "ok", 1, 1), None).is_some());
}

#[test]
fn signature_matches_shared_vector() {
    let sig = sign("test-key", "c1", "m1", "claude", Some("haiku-5.5"), 1700000000000, "héllo 👋", None);
    assert_eq!(sig, "ab529206c0c5da8e9c12efc38dac2b71aacc25df75a33f8d9c1b230be8b2f44e");
}

#[test]
fn file_signature_matches_shared_vector() {
    let m = Media { url: "https://ntfy.sh/file/abc123.jpg".into(), name: "pic é.jpg".into(), mime: "image/jpeg".into(), size: 1234, sha256: "ab".repeat(32), ..Media::default() };
    let sig = |m: &Media| sign("test-key", "c1", "m1", "claude", Some("haiku-5.5"), 1700000000000, "héllo 👋", Some(m));
    assert_eq!(sig(&m), "f61fe75b634ec0074a302a1f927b60591ae6d5a1028b6cd8a4203befa10d0cf5");
    assert_ne!(sig(&Media { size: 1, ..m.clone() }), sig(&m));
}

#[test]
fn picked_files_are_checked() {
    let m = Media::picked(b"abc", "image/jpeg").unwrap();
    assert_eq!((m.name.as_str(), m.size, m.state.as_str()), ("photo.jpg", 3, "ready"));
    assert_eq!(m.sha256, "ba7816bf8f01cfea414140de5dae2223b00361a396177a9cb410ff61f20015ad");
    assert_eq!(Media::picked(b"x", "video/quicktime").unwrap().name, "video.mov");
    assert_eq!(Media::picked(b"x", "image/heic").unwrap_err(), "Only photos and videos can be sent (image/heic)");
    assert_eq!(Media::picked(b"", "video/mp4").unwrap_err(), "That video is empty");
    assert_eq!(Media::picked(&vec![0; MEDIA_MAX as usize + 1], "image/png").unwrap_err(), "That photo is over 10 MB");
}

#[test]
fn a_photo_goes_up_first_then_the_signed_message_carries_it() {
    let (mut s, c, _) = store();
    let c2 = s.new_chat("codex", None);
    let pic = Media { path: "/data/x.jpg".into(), ..Media::picked(b"abc", "image/jpeg").unwrap() };
    let m = s.add_mine(&c2, "", Some(pic)).unwrap();
    assert_eq!(s.get(&c2).unwrap().title, "Photo");
    assert_eq!(shown(&s.get(&c2).unwrap().lines[0]), "Sent a photo");
    assert!(s.body(&c2, &m, "k").is_none(), "not before the upload");
    assert_eq!(s.to_upload(&c2, &m).unwrap().path, "/data/x.jpg");
    s.uploaded(&c2, &m, "https://ntfy.sh/file/up.jpg");
    assert!(s.to_upload(&c2, &m).is_none());
    let v: Value = serde_json::from_str(&s.body(&c2, &m, "k").unwrap()).unwrap();
    assert_eq!(v["att"], json!({"url": "https://ntfy.sh/file/up.jpg", "name": "photo.jpg", "type": "image/jpeg", "size": 3,
        "sha256": "ba7816bf8f01cfea414140de5dae2223b00361a396177a9cb410ff61f20015ad"}));
    let l = &s.get(&c2).unwrap().lines[0];
    assert_eq!(v["sig"], sign("k", &c2, &m, "codex", None, l.at, "", l.media.first()).as_str());
    s.sent(&c2, &m, false);
    assert_eq!(s.retry(&c2, "", true), Some(m.clone()));
    assert!(s.to_upload(&c2, &m).is_none(), "a retry re-sends the link it went up to");
    assert_eq!(serde_json::from_str::<Value>(&s.body(&c2, &m, "k").unwrap()).unwrap()["att"]["url"], "https://ntfy.sh/file/up.jpg");
    assert!(s.downloads().is_empty(), "my own file is never fetched");
    let m2 = s.add_mine(&c, &"\"".repeat(5000), Some(Media::picked(b"abc", "video/webm").unwrap())).unwrap();
    s.uploaded(&c, &m2, &format!("https://ntfy.sh/file/{}.webm", "a".repeat(20)));
    assert!(s.body(&c, &m2, "k").unwrap().len() < 4000, "the longest text and a file still fit");
}

#[test]
fn body_fits_at_the_cap_and_retry_resends_the_same() {
    let (mut s, c, _) = store();
    for text in ["x".repeat(5000), "é".repeat(5000), "\"".repeat(5000)] {
        let m = s.add_mine(&c, &text, None).unwrap();
        let body = s.body(&c, &m, "k").unwrap();
        assert!(body.len() < 4000, "{} bytes", body.len());
        let v: Value = serde_json::from_str(&body).unwrap();
        assert!(v["text"].as_str().unwrap().len() <= 2800);
    }
    let m = s.add_mine(&c, "retry me", None).unwrap();
    let first = s.body(&c, &m, "k");
    s.sent(&c, &m, false);
    assert_eq!(unsent(s.get(&c).unwrap().lines.last().unwrap(), now()), "waiting");
    assert_eq!(s.retry(&c, "", true), Some(m.clone()));
    assert_eq!(s.body(&c, &m, "k"), first);
    assert!(s.add_mine(&c, "  ", None).is_none());
}

fn ask_vector() -> Value {
    json!({"v": 1, "chat": "ask-6f1c2b3a-0000-4000-8000-000000000001", "note": "6f1c2b3a-0000-4000-8000-000000000001",
        "title": "Weather app", "kind": "ask", "choices": ["Blue", "Keep it"], "ts": 1700000000000i64, "text": "héllo 👋",
        "part": 1, "parts": 1, "sig": "bd9b36964a1a593f6a39d11df5d889d7b9e97f80ea5f9209ac4008c63f9a468e"})
}

fn file_note_vector() -> Value {
    json!({"v": 1, "chat": "updates", "note": "n1", "title": "Updates", "kind": "note", "choices": [], "ts": 1700000000000i64,
        "text": "", "part": 2, "parts": 2, "att": att(), "sig": "e3db6014eef9413d2f43f8cd247c9159839a383b6934ee1fa32bf9d82eed3ef2"})
}

#[test]
fn note_signatures_match_shared_vectors() {
    for good in [ask_vector(), file_note_vector()] {
        assert!(note_signed("test-key", &good), "{good}");
        assert!(!note_signed("other-key", &good));
        assert!(!reply_signed("test-key", &good), "a note's signature is not a reply's");
    }
    for (k, bad) in [
        ("title", json!("Weather")),
        ("kind", json!("note")),
        ("choices", json!(["Keep it", "Blue"])),
        ("choices", json!("Blue")),
        ("ts", json!(1700000000001i64)),
        ("text", json!("hello")),
        ("part", json!(2)),
    ] {
        let mut forged = ask_vector();
        forged[k] = bad;
        assert!(!note_signed("test-key", &forged), "{k}");
    }
    let mut forged = file_note_vector();
    forged["att"]["size"] = json!(1);
    assert!(!note_signed("test-key", &forged));
    let mut stripped = file_note_vector();
    stripped.as_object_mut().unwrap().remove("att");
    assert!(!note_signed("test-key", &stripped), "a v2 sig is not a v1 sig");
    let mut odd = file_note_vector();
    odd["att"] = json!("https://ntfy.sh/file/abc123.jpg");
    assert!(!note_signed("test-key", &odd));
}

fn note(chat: &str, id: &str, kind: &str, choices: &[&str], text: &str, part: u32, parts: u32) -> Value {
    let title = if chat == "updates" { "Updates" } else { "Wéather app ☀️" };
    json!({"v": 1, "chat": chat, "note": id, "title": title, "kind": kind, "choices": choices, "ts": now() - 1000, "text": text,
        "part": part, "parts": parts})
}

#[test]
fn an_ask_opens_its_own_thread_once() {
    let mut s = Store::default();
    let ask = note("ask-r1", "r1", "ask", &["Blue 💙", "Keep it as it is, thanks"], "Blue or keep it? Ünïcode ✓", 1, 1);
    let (id, name, text) = s.apply_note(&ask, None).unwrap();
    assert_eq!((id.as_str(), name.as_str(), text.as_str()), ("ask-r1", "Wéather app ☀️", "Blue or keep it? Ünïcode ✓"));
    assert!(s.apply_note(&ask, None).is_none(), "a repeat changes nothing");
    let c = s.get("ask-r1").unwrap();
    assert_eq!((c.agent.as_str(), c.model.as_deref(), c.kind.as_str(), c.unread, c.lines.len()), ("claude", None, "ask", 1, 1));
    assert_eq!(label(c), "Wéather app ☀️");
    assert_eq!(c.lines[0].choices, vec!["Blue 💙".to_string(), "Keep it as it is, thanks".into()]);
    assert_eq!(ask_state(c, now()), "waiting");
    assert_eq!(ask_state(c, now() + ASK_FOR + 5000), "expired");
    // the answer is an ordinary signed message (agent claude, no model); the PC's "Got it." files as a reply
    let m = s.add_mine("ask-r1", "Blue 💙", None).unwrap();
    let v: Value = serde_json::from_str(&s.body("ask-r1", &m, "k").unwrap()).unwrap();
    assert_eq!((v["agent"].as_str(), v["model"].is_null(), v["chat"].as_str()), (Some("claude"), true, Some("ask-r1")));
    assert_eq!(ask_state(s.get("ask-r1").unwrap(), now()), "sending", "until the PC takes it");
    assert_eq!(s.get("ask-r1").unwrap().title, "Wéather app ☀️", "the title stays the task's");
    let got = s.apply_reply(&reply("ask-r1", &m, "done", "Got it.", 1, 1), None).unwrap();
    assert_eq!((got.1.as_str(), got.2.as_str()), ("Wéather app ☀️", "Got it."));
    assert_eq!(ask_state(s.get("ask-r1").unwrap(), now() + ASK_FOR + 5000), "answered", "an answered question never expires");
    // seen while open: no notification, nothing unread
    assert!(s.apply_note(&note("ask-r2", "r2", "ask", &[], "Anything else?", 1, 1), Some("ask-r2")).is_none());
    assert_eq!(s.get("ask-r2").unwrap().unread, 0);
}

#[test]
fn a_failed_or_unsent_answer_leaves_the_question_waiting() {
    let mut s = Store::default();
    s.apply_note(&note("ask-r1", "r1", "ask", &["Yes"], "Ship it?", 1, 1), None);
    let m = s.add_mine("ask-r1", "Yes", None).unwrap();
    s.sent("ask-r1", &m, false);
    s.chats[0].lines[1].first -= AUTO_FOR;
    assert_eq!(ask_state(s.get("ask-r1").unwrap(), now()), "waiting", "not sent, for good");
    s.retry("ask-r1", "", true);
    s.sent("ask-r1", &m, true);
    s.apply_receipt(&receipt("ask-r1", &m, "delivered"));
    assert_eq!(ask_state(s.get("ask-r1").unwrap(), now()), "answered");
    s.apply_reply(&reply("ask-r1", &m, "failed", "I couldn't get the photo you sent. Send it again.", 1, 1), None);
    assert_eq!(ask_state(s.get("ask-r1").unwrap(), now()), "waiting", "the PC couldn't take it");
    assert_eq!(ask_state(&Chat::default(), now()), "", "not a question");
}

#[test]
fn notes_file_parts_and_files_like_replies() {
    let mut s = Store::default();
    let mut p2 = note("updates", "n1", "note", &[], "", 2, 3);
    p2["att"] = att();
    let mut p3 = note("updates", "n1", "note", &[], "", 3, 3);
    p3["att"] = json!({"url": "https://ntfy.sh/file/v.mp4", "name": "clip.mp4", "type": "video/mp4", "size": 9, "sha256": "cd".repeat(32)});
    assert!(s.apply_note(&p3, None).is_none());
    assert!(s.apply_note(&p2, None).is_none());
    assert!(s.apply_note(&p2, None).is_none(), "a repeated part is not added twice");
    assert_eq!(s.downloads().len(), 2, "files are fetched before the note is whole");
    let (_, name, text) = s.apply_note(&note("updates", "n1", "note", &[], "Build done ✓", 1, 3), None).unwrap();
    assert_eq!((name.as_str(), text.as_str()), ("Updates", "Build done ✓"));
    let c = s.get("updates").unwrap();
    assert_eq!((c.kind.as_str(), c.lines[0].state.as_str(), c.lines[0].media.len()), ("note", "done", 2));
    assert!(s.apply_note(&p3, None).is_none(), "a replayed part changes nothing");
    // the next update lands below the first, in the same thread; a photo alone says so
    let mut only = note("updates", "n2", "note", &[], "", 1, 1);
    only["att"] = json!({"url": "https://ntfy.sh/file/p.png", "name": "p.png", "type": "image/png", "size": 5, "sha256": "ef".repeat(32)});
    assert_eq!(s.apply_note(&only, None).unwrap().2, "Sent a photo");
    let c = s.get("updates").unwrap();
    assert_eq!((c.lines.len(), c.lines[1].id.as_str(), c.unread, ask_state(c, now())), (2, "n2", 2, ""));
    let mut bad = note("updates", "n3", "note", &[], "", 1, 1);
    bad["att"] = json!({"url": "https://example.com/x.png", "name": "x", "type": "image/png", "size": 5, "sha256": "ef".repeat(32)});
    assert_eq!(s.apply_note(&bad, None).unwrap().2, "Done.", "an unsafe file is left out");
}

#[test]
fn bad_notes_are_dropped_but_long_unicode_fits() {
    let mut s = Store::default();
    // counted in characters, never bytes: a 100-character title of 200 bytes and a choice of 80 emoji (320 bytes) pass
    let mut v = note("ask-x", "x", "ask", &[], "q", 1, 1);
    (v["title"], v["choices"]) = (json!("é".repeat(100)), json!(["😀".repeat(80)]));
    assert!(s.apply_note(&v, None).is_some());
    for (k, bad) in [
        ("v", json!(2)),
        ("chat", json!("a/b")),
        ("chat", json!("x".repeat(41))),
        ("note", json!("")),
        ("title", json!("t".repeat(101))),
        ("kind", json!("call")),
        ("choices", json!(["a", "b", "c", "d", "e", "f", "g"])),
        ("choices", json!([""])),
        ("choices", json!(["two\nlines"])),
        ("choices", json!(["Blue\u{1f}Keep it"])), // joined, it would sign the same as two choices
        ("choices", json!("Blue")),
        ("ts", json!("soon")),
        ("text", json!(null)),
        ("part", json!(0)),
        ("part", json!(3)),
        ("parts", json!(51)),
    ] {
        let mut v = note("ask-y", "y", "ask", &["Blue"], "q", 1, 2);
        v[k] = bad;
        assert!(s.apply_note(&v, None).is_none(), "{k}");
    }
    assert!(s.get("ask-y").is_none());
}

#[test]
fn a_deleted_thread_stays_deleted_and_undo_brings_it_back() {
    let (mut s, mine, _) = store();
    let ask = note("ask-r1", "r1", "ask", &["Yes"], "Ship it?", 1, 2);
    s.apply_note(&ask, None);
    s.apply_note(&note("updates", "n1", "note", &[], "Built.", 1, 1), None);
    let before = s.clone();
    assert!(s.delete("ask-r1", 1));
    s.undelete("ask-r1");
    assert_eq!(s, before, "undo puts it back exactly");
    s.delete("ask-r1", 1);
    assert_eq!(s.purge(1).len(), 1);
    // its last part and a resend of the whole note arrive late: dropped, the thread doesn't come back
    assert!(s.apply_note(&note("ask-r1", "r1", "ask", &["Yes"], "!", 2, 2), None).is_none());
    assert!(s.apply_note(&ask, None).is_none());
    assert!(s.get("ask-r1").is_none());
    // updates: the old notes' replays are dropped, a new update opens the thread again
    s.remove("updates").unwrap();
    assert!(s.apply_note(&note("updates", "n1", "note", &[], "Built.", 1, 1), None).is_none());
    assert!(s.apply_note(&note("updates", "n2", "note", &[], "Deployed.", 1, 1), None).is_some());
    assert_eq!(s.get("updates").unwrap().lines.len(), 1);
    // my own chats leave no trace
    s.remove(&mine).unwrap();
    assert_eq!(s.gone, vec!["ask-r1/r1".to_string(), "updates/n1".to_string()]);
}

fn receipt(chat: &str, re: &str, state: &str) -> Value {
    json!({"v": 1, "chat": chat, "re": re, "state": state})
}

#[test]
fn receipt_vectors_match_the_pc() {
    // PROTOCOL-asks.md, "Read receipts" (key test-key)
    for (state, sig) in [
        ("delivered", "b7f7d1d56b961afaca9ca2cd7a80538ab72c9f9ac076c0256be198fe8d1cd283"),
        ("read", "c30a0bb0ca6ba10dacd0112fc1bff039a760aa0117c06d289ab01af1495471f5"),
    ] {
        let mut good = receipt("c1", "m1", state);
        good["sig"] = sig.into();
        assert!(receipt_signed("test-key", &good), "{state}");
        assert!(!receipt_signed("other-key", &good));
        assert!(!reply_signed("test-key", &good), "a receipt's signature is not a reply's");
        for (k, v) in [("chat", "c2"), ("re", "m2"), ("state", if state == "read" { "delivered" } else { "read" })] {
            let mut forged = good.clone();
            forged[k] = v.into();
            assert!(!receipt_signed("test-key", &forged), "{k}");
        }
        let mut upper = good.clone();
        upper["sig"] = sig.to_uppercase().into();
        assert!(receipt_signed("test-key", &upper), "hex in either case, like replies");
        good.as_object_mut().unwrap().remove("sig");
        assert!(!receipt_signed("test-key", &good));
    }
}

#[test]
fn receipts_step_the_status_forward_only() {
    let (mut s, c, m) = store();
    let line = |s: &Store| s.get(&c).unwrap().lines[0].clone();
    let shows = |s: &Store| status(s.get(&c).unwrap(), now());
    assert_eq!(shows(&s), ("Sending…", ""));
    s.sent(&c, &m, true);
    assert_eq!(shows(&s), ("Sent", ""));
    assert!(s.apply_receipt(&receipt(&c, &m, "delivered")));
    assert_eq!(shows(&s), ("Delivered", ""));
    assert!(!s.apply_receipt(&receipt(&c, &m, "delivered")), "replay");
    assert!(s.apply_reply(&reply(&c, &m, "working", "", 1, 1), None).is_none());
    assert_eq!(shows(&s), ("Delivered", "stop"), "working: Stop still offered");
    assert!(s.apply_receipt(&receipt(&c, &m, "read")));
    assert_eq!(shows(&s), ("Read", "stop"));
    assert!(!s.apply_receipt(&receipt(&c, &m, "delivered")), "late, lower receipt");
    assert_eq!(line(&s).seen, "read");
    // unknown chat, message, state or version; and the PC's own line is never mine
    for v in [receipt("nope", &m, "read"), receipt(&c, "nope", "read"), receipt(&c, &m, "seen"), receipt(&c, &m, ""), json!({"v": 2, "chat": c, "re": m, "state": "read"})] {
        assert!(!s.apply_receipt(&v), "{v}");
    }
    let before = s.clone();
    assert!(s.apply_reply(&reply(&c, &m, "done", "hi", 1, 1), None).is_some());
    assert_eq!(shows(&s), ("", ""), "answered: the line goes");
    assert!(!s.apply_receipt(&receipt(&c, &m, "read")));
    // survives a restart (the store is the chat file); a file from before receipts still loads
    let path = std::env::temp_dir().join(format!("callme-receipts-{}.json", std::process::id()));
    before.save(&path).unwrap();
    assert_eq!(Store::load(&path).unwrap().get(&c).unwrap().lines[0].seen, "read");
    std::fs::remove_file(&path).unwrap();
    let old: Store = serde_json::from_str(r#"{"chats":[{"id":"c1","lines":[{"id":"m1","me":true,"state":"sent"}]}]}"#).unwrap();
    assert_eq!(status(&old.chats[0], now()), ("Sent · the PC hasn't picked it up yet", ""), "no receipt for long");
}

#[test]
fn a_receipt_proves_a_failed_send_went() {
    let (mut s, c, m) = store();
    s.sent(&c, &m, false);
    assert_eq!(status(s.get(&c).unwrap(), now()), ("Waiting for connection…", ""));
    assert!(s.apply_receipt(&receipt(&c, &m, "delivered")));
    assert_eq!(status(s.get(&c).unwrap(), now()), ("Delivered", ""));
    assert_eq!(s.retry(&c, "", true), None, "nothing to retry");
    // a send that never finished is retried after a while, and fails once AUTO_FOR is over
    let (mut s, c, _) = store();
    s.chats[0].lines[0].since -= 16_000;
    assert_eq!(status(s.get(&c).unwrap(), now()), ("Waiting for connection…", ""));
    s.chats[0].lines[0].first -= AUTO_FOR;
    assert_eq!(status(s.get(&c).unwrap(), now()), ("Failed · Tap to retry", "retry"));
    // only my latest message carries the line
    let m2 = s.add_mine(&c, "again", None).unwrap();
    s.sent(&c, &m2, true);
    assert_eq!(status(s.get(&c).unwrap(), now()), ("Sent", ""));
}

#[test]
fn bare_links() {
    assert_eq!(inline("see https://a.dev/x."), "see <https://a.dev/x>.");
    assert_eq!(inline("https://a.dev, and (http://b.io/p?q=1)"), "<https://a.dev>, and (<http://b.io/p?q=1>)");
    assert_eq!(inline("[docs](https://a.dev) `https://code.dev` x<https://c.dev>"), "[docs](https://a.dev) `https://code.dev` x&lt;https://c.dev>");
    assert_eq!(inline("no https:// here"), "no https:// here");
}

#[test]
fn composed_word_goes_where_the_cursor_is() {
    assert_eq!(composed("hello world", "big ", 6), "hello big world");
    assert_eq!(composed("héllo", "x", 2), "hxéllo"); // inside é: back to its start
    assert_eq!(composed("hi", "", 99), "hi");
}

#[test]
fn thinking_only_once_the_pc_works_and_queued_is_not_an_answer() {
    let (mut s, c, m) = store();
    let chat = |s: &Store| s.get(&c).unwrap().clone();
    s.sent(&c, &m, true);
    assert!(!waiting(&chat(&s)), "sent is not thinking");
    assert_eq!(think(&chat(&s), now()), "");
    assert!(s.apply_reply(&reply(&c, &m, "queued", "", 1, 1), None).is_none());
    assert_eq!((chat(&s).lines.len(), chat(&s).lines[0].state.as_str()), (1, "queued"), "queued: no Done. line");
    assert_eq!(status(&chat(&s), now()), ("Queued", "stop"));
    assert_eq!(think(&chat(&s), now()), "Queued");
    assert!(!waiting(&chat(&s)));
    assert!(s.apply_reply(&reply(&c, &m, "thinking-hard", "", 1, 1), None).is_none(), "unknown state");
    assert_eq!(chat(&s).lines.len(), 1, "an unknown state is never a final line");
    s.apply_reply(&reply(&c, &m, "working", "", 1, 1), None);
    assert!(waiting(&chat(&s)));
    assert_eq!(think(&chat(&s), now()), "Thinking");
    s.chats[0].lines[0].since -= 3 * 60_000;
    assert_eq!(think(&chat(&s), now()), "Thinking · 3 min");
    // a working message never goes back to queued
    s.apply_reply(&reply(&c, &m, "queued", "", 1, 1), None);
    assert_eq!(chat(&s).lines[0].state, "working");
}

#[test]
fn sent_but_not_picked_up_and_receiving_a_reply() {
    let (mut s, c, m) = store();
    s.sent(&c, &m, true);
    s.chats[0].lines[0].since -= 61_000;
    assert_eq!(status(&s.chats[0], now()), ("Sent · the PC hasn't picked it up yet", ""));
    s.apply_receipt(&receipt(&c, &m, "delivered"));
    assert_eq!(status(&s.chats[0], now()), ("Delivered", ""));
    s.apply_reply(&reply(&c, &m, "working", "", 1, 1), None);
    s.apply_reply(&reply(&c, &m, "done", "first half ", 1, 2), None);
    assert_eq!(status(&s.chats[0], now()), ("Delivered", "stop"), "a part just came: no notice yet");
    assert!(s.stalled(now()).is_empty());
    s.chats[0].lines[1].at -= 20_000;
    assert_eq!(status(&s.chats[0], now()), ("Receiving reply…", "stop"));
    assert_eq!(s.stalled(now()), vec![(c.clone(), m.clone())], "the PC is asked again");
    s.apply_reply(&reply(&c, &m, "done", "second", 2, 2), None);
    assert_eq!(status(&s.chats[0], now()), ("", ""));
    assert!(s.stalled(now()).is_empty());
}

#[test]
fn unsent_messages_retry_by_themselves_then_fail() {
    let (mut s, c, m) = store();
    s.sent(&c, &m, false);
    let t = now();
    assert_eq!(status(&s.chats[0], t), ("Waiting for connection…", ""));
    assert!(s.due(t).is_empty(), "not straight away");
    assert_eq!(s.due(t + 15_000), vec![(c.clone(), m.clone())]);
    assert_eq!(s.retry(&c, &m, false), Some(m.clone()));
    assert_eq!(s.chats[0].lines[0].tries, 1);
    s.sent(&c, &m, false);
    assert!(s.due(now() + 15_000).is_empty(), "backs off");
    assert_eq!(s.due(now() + 30_000).len(), 1);
    // past AUTO_FOR from the first send: failed, tap to retry; a tap starts over
    s.chats[0].lines[0].first -= AUTO_FOR;
    assert_eq!(status(&s.chats[0], now()), ("Failed · Tap to retry", "retry"));
    assert!(s.due(now() + 600_000).is_empty());
    s.retry(&c, "", true);
    s.sent(&c, &m, false);
    assert_eq!(status(&s.chats[0], now()), ("Waiting for connection…", ""));
    // an earlier message is retried by its own id
    let m2 = s.add_mine(&c, "second", None).unwrap();
    s.sent(&c, &m2, true);
    assert_eq!(s.retry(&c, &m, true), Some(m.clone()));
    assert_eq!(s.retry(&c, &m2, true), None, "it went");
    // a file from before these fields: first falls back to its time
    let old: Store = serde_json::from_str(r#"{"chats":[{"id":"c1","lines":[{"id":"m1","me":true,"state":"not_sent","at":1,"since":1}]}]}"#).unwrap();
    assert_eq!(status(&old.chats[0], now()), ("Failed · Tap to retry", "retry"));
}

#[test]
fn drafts_and_picks_belong_to_their_chat_and_go_with_the_send() {
    let (mut s, a, _) = store();
    let b = s.new_chat("codex", None);
    let pic = Media { path: "/m/p.jpg".into(), ..Media::picked(b"abc", "image/jpeg").unwrap() };
    s.set_draft(&a, "half a thought");
    assert!(s.set_pick(&a, Some(pic.clone())));
    assert_eq!((s.get(&b).unwrap().draft.as_str(), s.get(&b).unwrap().pick.is_none()), ("", true), "B's box stays empty");
    assert!(s.uses("/m/p.jpg"));
    let path = std::env::temp_dir().join(format!("callme-drafts-{}.json", std::process::id()));
    s.save(&path).unwrap();
    let back = Store::load(&path).unwrap();
    std::fs::remove_file(&path).unwrap();
    assert_eq!(back.get(&a).unwrap().draft, "half a thought", "survives the app being closed");
    assert_eq!(back.get(&a).unwrap().pick, Some(pic.clone()));
    let msg = s.send(&a, "half a thought, done").unwrap();
    let chat = s.get(&a).unwrap();
    assert_eq!((chat.draft.as_str(), chat.pick.is_none()), ("", true));
    let l = chat.lines.iter().find(|l| l.id == msg).unwrap();
    assert_eq!((l.text.as_str(), l.media[0].path.as_str()), ("half a thought, done", "/m/p.jpg"));
    assert!(s.send(&b, "  ").is_none(), "nothing to send: nothing changes");
}

#[test]
fn only_empty_chats_of_mine_go_quietly() {
    let mut s = Store::default();
    let empty = s.new_chat("claude", Some("opus-5.5"));
    let drafted = s.new_chat("claude", None);
    s.set_draft(&drafted, "later");
    s.apply_note(&note("ask-q", "q", "ask", &[], "?", 1, 1), None);
    let ask = "ask-q".to_string();
    s.get_mut(&ask).unwrap().lines.clear();
    assert!(s.drop_empty(&empty));
    assert!(!s.drop_empty(&drafted), "a draft keeps it");
    assert!(!s.drop_empty(&ask), "a thread the PC started stays");
    let (mut s, c, _) = store();
    assert!(!s.drop_empty(&c), "it has a message");
    assert!(s.gone.is_empty());
}

#[test]
fn an_answer_shows_as_sending_until_the_pc_takes_it() {
    let mut s = Store::default();
    s.apply_note(&note("ask-r", "r", "ask", &["Yes"], "Ship it?", 1, 1), None);
    let m = s.add_mine("ask-r", "Yes", None).unwrap();
    let st = |s: &Store| ask_state(s.get("ask-r").unwrap(), now());
    assert_eq!(st(&s), "sending");
    s.sent("ask-r", &m, true);
    assert_eq!(st(&s), "sending", "sent, not yet taken");
    s.apply_receipt(&receipt("ask-r", &m, "delivered"));
    assert_eq!(st(&s), "answered");
    // never acknowledged (a PC without receipts): answered after a minute, not stuck
    let mut s2 = Store::default();
    s2.apply_note(&note("ask-r", "r", "ask", &["Yes"], "Ship it?", 1, 1), None);
    let m2 = s2.add_mine("ask-r", "Yes", None).unwrap();
    s2.sent("ask-r", &m2, true);
    s2.chats[0].lines[1].since -= 61_000;
    assert_eq!(st(&s2), "answered");
    // failed for good: the choices come back
    s2.chats[0].lines[1].state = "not_sent".into();
    assert_eq!(st(&s2), "sending", "still retrying by itself");
    s2.chats[0].lines[1].first -= AUTO_FOR;
    assert_eq!(st(&s2), "waiting");
}

#[test]
fn copy_plain_text_and_code_exactly() {
    let (mut s, c, m) = store();
    let answer = "Run **this**:\n\n```sh\n  cargo  build\t--release\n```\n\n- see [docs](https://a.dev) and `x`\n  - nested";
    s.apply_reply(&reply(&c, &m, "done", answer, 1, 1), None);
    let chat = s.get(&c).unwrap();
    assert_eq!(copy_text(chat, &m, true, 0).unwrap(), "hello");
    assert_eq!(copy_text(chat, &m, false, 1).unwrap(), "  cargo  build\t--release", "code: exact whitespace");
    assert_eq!(copy_text(chat, &m, false, 0).unwrap(), "Run this:\n\n  cargo  build\t--release\n\n• see docs (https://a.dev) and x\n\n  • nested");
    assert!(!copy_text(chat, &m, false, 0).unwrap().contains('\u{200b}'));
    assert!(copy_text(chat, "nope", false, 0).is_none());
    assert!(has_link("see https://a.dev") && has_link("[a](b)") && !has_link("`https://in.code`") && !has_link("plain"));
}

#[test]
fn room_counts_the_signed_payload() {
    assert_eq!(room("hi"), 2798);
    assert_eq!(room(&"x".repeat(2800)), 0);
    assert!(room(&"x".repeat(2801)) < 0);
    assert_eq!(room(&"é".repeat(1400)), 0, "bytes, not characters");
    // quotes grow when escaped: the signed message is the limit
    assert!(room(&"\"".repeat(1800)) < 0);
    let (mut s, c, _) = store();
    let fits = "\"".repeat(1749);
    assert!(room(&fits) >= 0);
    let m = s.add_mine(&c, &fits, None).unwrap();
    assert_eq!(s.get(&c).unwrap().lines.iter().find(|l| l.id == m).unwrap().text, fits, "what room allows is sent whole");
}

#[test]
fn failed_media_can_be_fetched_again() {
    let (mut s, c, m) = store();
    let mut v = reply(&c, &m, "done", "", 1, 1);
    v["att"] = att();
    s.apply_reply(&v, None);
    let url = s.downloads()[0].url.clone();
    s.media_done(&url, None);
    assert!(s.downloads().is_empty());
    assert!(s.refetch(&url));
    assert_eq!(s.downloads().len(), 1);
    assert!(!s.refetch(&url), "only a failed one");
}

#[test]
fn send_again_makes_a_new_message_with_the_same_text() {
    let (mut s, c, m) = store();
    s.apply_reply(&reply(&c, &m, "failed", "That didn't finish.", 1, 1), None);
    let m2 = s.again(&c, &m).unwrap();
    assert_ne!(m2, m);
    let l = s.get(&c).unwrap().lines.last().unwrap();
    assert_eq!((l.text.as_str(), l.state.as_str(), l.me), ("hello", "sending", true));
}

#[test]
fn a_notification_answer_skips_deleted_chats_and_closed_questions() {
    let (mut s, mine, _) = store();
    s.apply_note(&note("ask-r1", "r1", "ask", &["Yes", "No"], "Ship it?", 1, 1), None);
    assert_eq!(s.needing(), 1);
    // deleted, its Undo still showing: gone for the notification, and it no longer needs me
    assert!(s.delete("ask-r1", 1));
    assert_eq!(s.needing(), 0);
    let before = s.clone();
    assert!(s.answer("ask-r1", "Yes", now()).is_none());
    assert_eq!(s, before, "nothing added to a deleted chat");
    s.undelete("ask-r1");
    // the question waits: the choice goes and the chat is read; the home line's question (asks) is over
    let (c, msg) = s.answer("ask-r1", "Yes", now()).unwrap();
    assert!(msg.is_some());
    assert_eq!(ask_state(&c, now()), "waiting", "the chat as it was");
    assert_eq!(ask_state(s.get("ask-r1").unwrap(), now()), "sending");
    assert_eq!(s.needing(), 0);
    // a second tap (answered meanwhile): nothing more is added
    let n = s.get("ask-r1").unwrap().lines.len();
    assert!(s.answer("ask-r1", "No", now()).unwrap().1.is_none());
    assert_eq!(s.get("ask-r1").unwrap().lines.len(), n);
    // a chat of mine takes Reply's words; empty words add nothing
    assert!(s.answer(&mine, "  ", now()).unwrap().1.is_none());
    assert!(s.answer(&mine, "more", now()).unwrap().1.is_some());
}

#[test]
fn a_notification_answer_that_was_not_saved_says_so() {
    let mut s = Store::default();
    s.apply_note(&note("ask-r1", "r1", "ask", &["Yes", "No"], "Ship it?", 1, 1), None);
    let ask = s.get("ask-r1").unwrap().clone();
    let mine = Chat::default();
    let t = now();
    let choices = (vec!["Yes".to_string(), "No".to_string()], true, false);
    // saved: shown as sent; a question has nothing left to tap, a chat of mine keeps Reply
    assert_eq!(after_answer(&ask, true, true, t), (true, None, (vec![], false, false)));
    assert_eq!(after_answer(&mine, true, true, t), (true, None, (vec![], true, false)));
    // not saved (the store couldn't be written): not shown as sent, said so, and the buttons stay to try again
    assert_eq!(after_answer(&ask, true, false, t), (false, Some("Not sent. Try again"), choices));
    assert_eq!(after_answer(&mine, true, false, t), (false, Some("Not sent. Try again"), (vec![], true, false)));
    // answered in the app a moment ago (still on its way), or closed
    s.answer("ask-r1", "Yes", t);
    let sending = s.get("ask-r1").unwrap();
    assert_eq!(after_answer(sending, false, false, t), (false, Some("Already answered"), (vec![], false, false)));
    assert_eq!(after_answer(&ask, false, false, t + ASK_FOR + 2000).1, Some("This question has closed"));
}

#[test]
fn retries_resend_the_uploaded_link_until_ntfy_may_have_dropped_it() {
    let (mut s, c, _) = store();
    let pic = Media { path: "/m/p.jpg".into(), ..Media::picked(b"abc", "image/jpeg").unwrap() };
    let m = s.add_mine(&c, "look", Some(pic)).unwrap();
    s.uploaded(&c, &m, "https://ntfy.sh/file/up.jpg");
    for _ in 0..5 {
        s.sent(&c, &m, false); // the publish failed after the upload worked
        assert_eq!(s.retry(&c, &m, false), Some(m.clone()));
        assert!(s.to_upload(&c, &m).is_none(), "auto-retry never uploads it again");
    }
    s.sent(&c, &m, false);
    s.retry(&c, &m, true);
    assert!(s.to_upload(&c, &m).is_none(), "nor does a tap while the link works");
    // past LINK_KEEP the link may be gone: the next retry uploads it once more
    s.get_mut(&c).unwrap().lines.last_mut().unwrap().media[0].uploaded -= LINK_KEEP + 1;
    s.sent(&c, &m, false);
    s.retry(&c, &m, true);
    assert_eq!(s.to_upload(&c, &m).unwrap().path, "/m/p.jpg");
}

#[test]
fn an_answer_on_its_way_takes_the_text_box_away() {
    let mut s = Store::default();
    s.apply_note(&note("ask-q", "n1", "ask", &["Yes", "No"], "Ship it?", 1, 1), None).unwrap();
    let now = now();
    let state = |s: &Store| ask_state(s.get("ask-q").unwrap(), now);
    assert_eq!(ended(state(&s)), "", "waiting: the box");
    let m = s.add_mine("ask-q", "Yes", None).unwrap();
    assert_eq!(ended(state(&s)), "Sending your answer…", "no second answer can be typed");
    s.sent("ask-q", &m, true);
    s.apply_receipt(&receipt("ask-q", &m, "delivered"));
    assert_eq!(ended(state(&s)), "You've answered this. Start a new chat to say more.");
    // an answer that fails for good: the question waits again and the box comes back
    let mut t = Store::default();
    t.apply_note(&note("ask-q", "n1", "ask", &[], "Ship it?", 1, 1), None).unwrap();
    let m = t.add_mine("ask-q", "Yes", None).unwrap();
    t.sent("ask-q", &m, false);
    t.get_mut("ask-q").unwrap().lines[1].first -= AUTO_FOR;
    assert_eq!(ended(state(&t)), "");
    assert_eq!(ended("expired"), "This question has expired.");
}

#[test]
fn a_new_chat_waiting_on_the_picker_survives_the_app_being_closed() {
    let mut s = Store::default();
    let c = s.new_chat("claude", None);
    s.picking_for(&c);
    let path = std::env::temp_dir().join(format!("callme-picking-{}.json", std::process::id()));
    s.save(&path).unwrap();
    let mut back = Store::load(&path).unwrap();
    std::fs::remove_file(&path).unwrap();
    assert_eq!(back.picking, c, "the picker's chat is kept with the store");
    assert!(!back.drop_empty(&c), "empty, but its pick is still to come");
    let pic = Media { path: "/m/p.jpg".into(), ..Media::picked(b"abc", "image/jpeg").unwrap() };
    assert!(back.set_pick(&c, Some(pic)));
    assert_eq!(back.picking, "", "the pick landed");
    assert!(!back.drop_empty(&c), "it has a pick now");
    // closed without a pick: an empty chat goes as before
    let d = back.new_chat("claude", None);
    back.picking_for(&d);
    back.picked(&c); // another chat's answer leaves it waiting
    assert_eq!(back.picking, d);
    back.picked(&d);
    assert!(back.drop_empty(&d));
}

#[test]
fn picked_files_are_known_by_their_first_bytes_when_the_type_is_missing() {
    assert_eq!(Media::kind("image/png", b""), Some("image/png"), "a type it sends is taken as stated");
    assert_eq!(Media::kind("", b"\xff\xd8\xff\xe0rest"), Some("image/jpeg"));
    assert_eq!(Media::kind("image/jpg", b"\x89PNG\r\n"), Some("image/png"));
    assert_eq!(Media::kind("", b"GIF89a"), Some("image/gif"));
    assert_eq!(Media::kind("", b"RIFF\0\0\0\0WEBPVP8 "), Some("image/webp"));
    assert_eq!(Media::kind("", b"\x1a\x45\xdf\xa3"), Some("video/webm"));
    assert_eq!(Media::kind("", b"\0\0\0\x18ftypmp42"), Some("video/mp4"));
    assert_eq!(Media::kind("application/octet-stream", b"\0\0\0\x14ftypqt  "), Some("video/quicktime"));
    assert_eq!(Media::kind("", b"\0\0\0\x18ftypheic"), Some("image/heif"));
    assert_eq!(Media::kind("image/heic", b""), Some("image/heic"));
    assert!(Media::heif("image/heic") && Media::heif("image/heif") && !Media::heif("image/jpeg"));
    assert_eq!(Media::kind("", b"%PDF-1.7"), None);
    assert_eq!(Media::kind("", b""), None);
    assert!(Media::picked(b"x", "image/heif").is_err(), "HEIF itself is never sent: it is made a JPEG first");
}

#[test]
fn a_deleted_chat_is_not_asked_again_for_a_missing_part() {
    let (mut s, c, m) = store();
    s.apply_reply(&reply(&c, &m, "done", "half", 1, 2), None);
    let t = now() + 20_000;
    assert_eq!(s.stalled(t).len(), 1);
    s.delete(&c, now());
    assert!(s.stalled(t).is_empty(), "its Undo is still showing");
}
