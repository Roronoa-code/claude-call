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
fn working_then_done_in_parts() {
    let (mut s, c, m) = store();
    assert!(s.apply_reply(&reply(&c, &m, "working", "", 1, 1), None).is_none());
    assert_eq!(s.get(&c).unwrap().lines[0].state, State::Working);
    for (p, t) in [(2, "world "), (1, "hello "), (2, "world "), (3, "!")] {
        let r = s.apply_reply(&reply(&c, &m, "done", t, p, 3), None);
        assert_eq!(r.is_some(), p == 3, "notifies once, when whole");
    }
    assert!(s.apply_reply(&reply(&c, &m, "done", "!", 3, 3), None).is_none(), "replay");
    let chat = s.get(&c).unwrap();
    assert_eq!((chat.lines[1].text.as_str(), chat.lines[1].state), ("hello world !", State::Done));
    assert_eq!((chat.lines[0].state, chat.unread, chat.lines.len()), (State::Answered, 1, 2));
    assert_eq!(status(chat, now()), ("", ""));

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
    assert_eq!((l.media[0].state, l.media[1].state, l.media[1].path.as_str()), (MediaState::Failed, MediaState::Ready, "/data/v.mp4"));
    assert!(s.downloads().is_empty());
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
fn delete_undo_keeps_late_replies_and_purge_frees_files() {
    let (mut s, c, m) = store();
    let pic = |path: &str| Media { path: path.into(), state: MediaState::Ready, ..Media::default() };
    s.get_mut(&c).unwrap().lines[0].media = vec![pic("/m/shared.jpg"), pic("/m/only.jpg")];
    s.get_mut(&c).unwrap().pick = Some(pic("/m/picked.jpg"));
    let other = s.new_chat("codex", None);
    let m2 = s.add_mine(&other, "hi", Some(pic("/other/spelling/shared.jpg"))).unwrap();
    s.apply_reply(&reply(&c, &m, "working", "", 1, 1), None);

    assert!(s.delete(&c, 1000));
    assert!(!s.delete(&c, 2000), "once");
    assert_eq!(s.undoable().unwrap().id, c);
    assert!(s.apply_reply(&reply(&c, &m, "done", "late", 1, 1), None).is_none());
    assert!(s.undelete(&c));
    assert_eq!(s.get(&c).unwrap().lines[1].text, "late");
    assert_eq!(s.get(&c).unwrap().unread, 1);

    assert!(s.delete(&c, 1000) && s.delete(&other, 4000));
    assert_eq!(s.undoable().unwrap().id, other, "Undo brings back the newest");
    let gone = s.purge(999);
    assert!(gone.is_empty(), "not before its time");
    let gone = s.purge(1000);
    assert_eq!(gone.len(), 1);
    assert_eq!((s.get(&c), s.undoable().map(|c| c.id.clone())), (None, Some(other.clone())));
    assert_eq!(s.unused_files(&gone[0]), vec!["only.jpg".to_string(), "picked.jpg".to_string()], "a file the other chat uses stays");
    assert!(s.apply_reply(&reply(&c, &m, "done", "later", 1, 1), None).is_none());
    assert!(s.get(&c).is_none());
    assert!(s.undelete(&other));
    assert!(s.apply_reply(&reply(&other, &m2, "done", "ok", 1, 1), None).is_some());
}

#[test]
fn signature_matches_shared_vector() {
    let c = Chat { id: "c1".into(), agent: "claude".into(), model: Some("haiku-5.5".into()), ..Chat::default() };
    let l = Line { id: "m1".into(), at: 1700000000000, text: "héllo 👋".into(), ..Line::default() };
    assert_eq!(sign("test-key", &c, &l), "ab529206c0c5da8e9c12efc38dac2b71aacc25df75a33f8d9c1b230be8b2f44e");
    let m = Media { url: "https://ntfy.sh/file/abc123.jpg".into(), name: "pic é.jpg".into(), mime: "image/jpeg".into(), size: 1234, sha256: "ab".repeat(32), ..Media::default() };
    let sig = |m: &Media| sign("test-key", &c, &Line { media: vec![m.clone()], ..l.clone() });
    assert_eq!(sig(&m), "f61fe75b634ec0074a302a1f927b60591ae6d5a1028b6cd8a4203befa10d0cf5");
    assert_ne!(sig(&Media { size: 1, ..m.clone() }), sig(&m));
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
    assert_eq!(v["sig"], sign("k", s.get(&c2).unwrap(), l).as_str());
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
    assert_eq!(unsent(s.get(&c).unwrap().lines.last().unwrap(), now()), Unsent::Waiting);
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
fn bad_notes_are_dropped_but_long_unicode_fits() {
    let mut s = Store::default();
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
        ("choices", json!(["Blue\u{1f}Keep it"])),
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
    assert!(s.apply_note(&note("ask-r1", "r1", "ask", &["Yes"], "!", 2, 2), None).is_none());
    assert!(s.apply_note(&ask, None).is_none());
    assert!(s.get("ask-r1").is_none());
    s.remove("updates").unwrap();
    assert!(s.apply_note(&note("updates", "n1", "note", &[], "Built.", 1, 1), None).is_none());
    assert!(s.apply_note(&note("updates", "n2", "note", &[], "Deployed.", 1, 1), None).is_some());
    assert_eq!(s.get("updates").unwrap().lines.len(), 1);
    s.remove(&mine).unwrap();
    assert_eq!(s.gone, vec!["ask-r1/r1".to_string(), "updates/n1".to_string()]);
}

fn receipt(chat: &str, re: &str, state: &str) -> Value {
    json!({"v": 1, "chat": chat, "re": re, "state": state})
}

#[test]
fn receipt_vectors_match_the_pc() {
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
    assert_eq!(line(&s).seen, Seen::Read);
    for v in [receipt("nope", &m, "read"), receipt(&c, "nope", "read"), receipt(&c, &m, "seen"), receipt(&c, &m, ""), json!({"v": 2, "chat": c, "re": m, "state": "read"})] {
        assert!(!s.apply_receipt(&v), "{v}");
    }
    let before = s.clone();
    assert!(s.apply_reply(&reply(&c, &m, "done", "hi", 1, 1), None).is_some());
    assert_eq!(shows(&s), ("", ""), "answered: the line goes");
    assert!(!s.apply_receipt(&receipt(&c, &m, "read")));
    let path = std::env::temp_dir().join(format!("callme-receipts-{}.json", std::process::id()));
    before.save(&path).unwrap();
    assert_eq!(Store::load(&path).unwrap().get(&c).unwrap().lines[0].seen, Seen::Read);
    std::fs::remove_file(&path).unwrap();
    let old: Store = serde_json::from_str(r#"{"chats":[{"id":"c1","lines":[{"id":"m1","me":true,"state":"sent"}]}]}"#).unwrap();
    assert_eq!(status(&old.chats[0], now()), ("Sent · the PC hasn't picked it up yet", ""), "no receipt for long");
    let line = |state: &str, seen: &str, media: &str| {
        let m = format!(r#"{{"part":1,"url":"","name":"","mime":"","size":0,"sha256":"","path":"","state":"{media}","uploaded":0}}"#);
        format!(r#"{{"id":"m","me":true,"text":"","at":0,"since":0,"state":"{state}","parts":[],"media":[{m}],"choices":[],"seen":"{seen}","first":0,"tries":0}}"#)
    };
    let states = ["", "sending", "sent", "not_sent", "working", "queued", "answered", "partial", "done", "failed", "stopped", "missed"];
    let (seen, media) = (["", "delivered", "read"], ["", "loading", "ready", "failed"]);
    let lines: Vec<String> = states.iter().enumerate().map(|(i, s)| line(s, seen[i % 3], media[i % 4])).collect();
    for kind in ["", "ask", "note"] {
        let file = format!(
            r#"{{"chats":[{{"id":"c1","agent":"claude","model":null,"title":"","updated":0,"unread":0,"lines":[{}],"kind":"{kind}","asked":0,"draft":"","pick":null,"deleted":0}}],"gone":[],"picking":""}}"#,
            lines.join(",")
        );
        let back = serde_json::to_string(&serde_json::from_str::<Store>(&file).unwrap()).unwrap();
        assert_eq!(back, file, "the saved file stays byte for byte the same");
    }
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
    s.chats[0].lines[0].first -= AUTO_FOR;
    assert_eq!(status(&s.chats[0], now()), ("Failed · Tap to retry", "retry"));
    assert!(s.due(now() + 600_000).is_empty());
    s.retry(&c, "", true);
    s.sent(&c, &m, false);
    assert_eq!(status(&s.chats[0], now()), ("Waiting for connection…", ""));
    let m2 = s.add_mine(&c, "second", None).unwrap();
    s.sent(&c, &m2, true);
    assert_eq!(s.retry(&c, &m, true), Some(m.clone()));
    assert_eq!(s.retry(&c, &m2, true), None, "it went");
    let old: Store = serde_json::from_str(r#"{"chats":[{"id":"c1","lines":[{"id":"m1","me":true,"state":"not_sent","at":1,"since":1}]}]}"#).unwrap();
    assert_eq!(status(&old.chats[0], now()), ("Failed · Tap to retry", "retry"));

    let (mut s, c, m) = store();
    s.sent(&c, &m, false);
    let t = now();
    s.get_mut(&c).unwrap().lines[0].since = t + 3_600_000;
    assert_eq!(s.due(t), vec![(c.clone(), m.clone())], "a clock set back an hour does not hold up the retry");
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
    let msg = s.send(&a, "half a thought, done", true).unwrap();
    let chat = s.get(&a).unwrap();
    assert_eq!((chat.draft.as_str(), chat.pick.is_none()), ("", true));
    let l = chat.lines.iter().find(|l| l.id == msg).unwrap();
    assert_eq!((l.text.as_str(), l.media[0].path.as_str()), ("half a thought, done", "/m/p.jpg"));
    assert!(s.send(&b, "  ", true).is_none(), "nothing to send: nothing changes");
    s.set_pick(&b, Some(pic.clone()));
    let m = s.send(&b, "just words", false).unwrap();
    let chat = s.get(&b).unwrap();
    assert!(chat.lines.iter().find(|l| l.id == m).unwrap().media.is_empty());
    assert!(chat.pick.is_none());
    s.set_pick(&b, Some(pic.clone()));
    assert!(s.send(&b, "", false).is_none(), "only the hidden pick: nothing to send");
    assert_eq!(composed("hello world", "big ", 6), "hello big world");
    assert_eq!(composed("héllo", "x", 2), "hxéllo", "a cursor inside é goes back to its start");
    assert_eq!(composed("hi", "", 99), "hi");
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
    assert_eq!(st(&s), Ask::Sending);
    s.sent("ask-r", &m, true);
    assert_eq!(st(&s), Ask::Sending, "sent, not yet taken");
    s.apply_receipt(&receipt("ask-r", &m, "delivered"));
    assert_eq!(st(&s), Ask::Sending, "delivered: the PC has it, but hasn't taken it yet");
    s.chats[0].lines[1].since -= 10 * 60_000;
    assert_eq!(st(&s), Ask::Sending, "no time limit stands in for the PC's word");
    s.apply_reply(&reply("ask-r", &m, "done", "Got it.", 1, 1), None);
    assert_eq!(st(&s), Ask::Answered);
    let mut s3 = Store::default();
    s3.apply_note(&note("ask-r", "r", "ask", &["Yes"], "Ship it?", 1, 1), None);
    let m3 = s3.add_mine("ask-r", "Yes", None).unwrap();
    s3.apply_reply(&reply("ask-r", &m3, "missed", "This question expired (over a day old), so nothing is waiting for the answer now.", 1, 1), None);
    assert_eq!(st(&s3), Ask::Expired);
    assert_eq!(quick_answers(s3.get("ask-r").unwrap(), now()), QuickAnswers::default(), "nothing left to tap");
    let mut s2 = Store::default();
    s2.apply_note(&note("ask-r", "r", "ask", &["Yes"], "Ship it?", 1, 1), None);
    let m2 = s2.add_mine("ask-r", "Yes", None).unwrap();
    s2.sent("ask-r", &m2, true);
    s2.chats[0].lines[1].state = State::NotSent;
    assert_eq!(st(&s2), Ask::Sending, "still retrying by itself");
    s2.chats[0].lines[1].first -= AUTO_FOR;
    assert_eq!(st(&s2), Ask::Waiting);
}


#[test]
fn retries_resend_the_uploaded_link_until_ntfy_may_have_dropped_it() {
    let (mut s, c, _) = store();
    let pic = Media { path: "/m/p.jpg".into(), ..Media::picked(b"abc", "image/jpeg").unwrap() };
    let m = s.add_mine(&c, "look", Some(pic)).unwrap();
    s.uploaded(&c, &m, "https://ntfy.sh/file/up.jpg");
    for _ in 0..5 {
        s.sent(&c, &m, false);
        assert_eq!(s.retry(&c, &m, false), Some(m.clone()));
        assert!(s.to_upload(&c, &m).is_none(), "auto-retry never uploads it again");
    }
    s.sent(&c, &m, false);
    s.retry(&c, &m, true);
    assert!(s.to_upload(&c, &m).is_none(), "nor does a tap while the link works");
    s.get_mut(&c).unwrap().lines.last_mut().unwrap().media[0].uploaded -= LINK_KEEP + 1;
    s.sent(&c, &m, false);
    s.retry(&c, &m, true);
    assert_eq!(s.to_upload(&c, &m).unwrap().path, "/m/p.jpg");
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
    let d = back.new_chat("claude", None);
    back.picking_for(&d);
    back.picked(&c);
    assert_eq!(back.picking, d);
    back.picked(&d);
    assert!(back.drop_empty(&d));
}


#[test]
fn a_notification_reply_too_long_to_send_waits_whole_in_the_chat() {
    let (mut s, mine, _) = store();
    let long = format!("{} 👋 \"quoted\"", "word ".repeat(700));
    assert!(room(&long) < 0);
    let n = s.get(&mine).unwrap().lines.len();
    let Answered { msg, too_long, .. } = s.answer(&mine, &long, now()).unwrap();
    assert_eq!((msg, too_long), (None, true), "nothing goes, nothing is cut");
    assert_eq!(s.get(&mine).unwrap().lines.len(), n);
    assert_eq!(s.get(&mine).unwrap().draft, long.trim(), "the whole text waits in its box");
    s.set_draft(&mine, "already typed");
    s.answer(&mine, &long, now());
    assert_eq!(s.get(&mine).unwrap().draft, format!("already typed\n\n{}", long.trim()));
    s.apply_note(&note("ask-l", "l1", "ask", &[], "?", 1, 1), None);
    let m = s.add_mine("ask-l", "Yes", None).unwrap();
    s.apply_reply(&reply("ask-l", &m, "done", "Got it.", 1, 1), None);
    assert!(!s.answer("ask-l", &long, now()).unwrap().too_long);
    assert_eq!(s.get("ask-l").unwrap().draft, "");
}

