use super::*;

fn message() -> Value {
    json!({"v":1,"chat":"c1","msg":"m1","agent":"claude","model":"haiku-5.5","ts":1700000000000i64,
        "text":"héllo 👋","sig":"ab529206c0c5da8e9c12efc38dac2b71aacc25df75a33f8d9c1b230be8b2f44e"})
}

#[test]
fn outgoing_signatures_match_existing_clients_and_reject_tampering() {
    let mut v = message();
    assert!(mine_signed("test-key", &v));
    for (key, value) in [("text", json!("changed")), ("agent", json!("codex")), ("ts", json!(1)), ("sig", json!("bad"))] {
        let mut bad = v.clone();
        bad[key] = value;
        assert!(!mine_signed("test-key", &bad), "{key}");
    }
    assert!(!mine_signed("wrong-key", &v));
    v["att"] = json!({"url":"https://ntfy.sh/file/abc123.jpg","name":"pic é.jpg","type":"image/jpeg","size":1234,"sha256":"ab".repeat(32)});
    v["sig"] = json!("f61fe75b634ec0074a302a1f927b60591ae6d5a1028b6cd8a4203befa10d0cf5");
    assert!(mine_signed("test-key", &v));
    v["att"]["size"] = json!(1);
    assert!(!mine_signed("test-key", &v));
    v.as_object_mut().unwrap().remove("att");
    assert!(!mine_signed("test-key", &v));
}

#[test]
fn mirrored_messages_accept_replies_without_resending_or_resetting_drafts() {
    let mut s = Store::default();
    let v = message();
    assert!(s.apply_mine(&v).is_some());
    let c = s.get("c1").unwrap();
    assert_eq!((c.agent.as_str(), c.title.as_str(), c.unread), ("claude", "héllo 👋", 0));
    assert!(c.lines[0].me && c.lines[0].mirrored);
    assert_eq!(c.lines[0].state, State::Sent);
    assert!(s.due(now()).is_empty() && s.unanswered(now()).is_empty());
    s.set_draft("c1", "unfinished local draft");
    let pick = Media::picked(b"local", "image/png").unwrap();
    s.set_pick("c1", Some(pick.clone()));
    let reply = json!({"v":1,"chat":"c1","re":"m1","state":"done","text":"hello back","part":1,"parts":1});
    s.reply_at(&reply, Some("c1"), 1700000000500);
    assert_eq!(s.get("c1").unwrap().lines[1].at, 1700000000500);
    let before = s.clone();
    assert!(s.apply_mine(&v).is_none());
    assert_eq!(s, before, "echo must preserve reply state, draft and pending photo");
    let mut second = v.clone();
    second["msg"] = json!("m2");
    second["ts"] = json!(1700000001000i64);
    second["text"] = json!("second question");
    s.apply_mine(&second).unwrap();
    let c = s.get("c1").unwrap();
    assert_eq!(c.lines.iter().map(|l| l.me).collect::<Vec<_>>(), [true, false, true], "cached delivery order is preserved");
    assert_eq!((&c.draft, &c.pick), (&"unfinished local draft".to_string(), &Some(pick)));
    let restored: Store = serde_json::from_slice(&serde_json::to_vec(&s).unwrap()).unwrap();
    assert!(restored.get("c1").unwrap().lines[0].mirrored);
    assert!(restored.due(now()).is_empty() && restored.unanswered(now()).is_empty());
    s.get_mut("c1").unwrap().lines[1].state = State::Partial;
    s.get_mut("c1").unwrap().lines[1].at = now() - 20_000;
    assert!(s.stalled(now()).is_empty(), "a mirrored reply must not re-submit the original request");
}

#[test]
fn replay_does_not_recreate_deleted_chats_or_change_local_outbox() {
    let mut s = Store::default();
    let v = message();
    s.apply_mine(&v).unwrap();
    s.delete("c1", 1);
    s.purge(2);
    assert!(s.apply_mine(&v).is_none() && s.get("c1").is_none());
    let id = s.new_chat("codex", None);
    let msg = s.add_mine(&id, "local", None).unwrap();
    let echo: Value = serde_json::from_str(&s.body(&id, &msg, "key").unwrap()).unwrap();
    let before = s.clone();
    assert!(s.apply_mine(&echo).is_none());
    assert_eq!(s, before);
    assert!(!s.get(&id).unwrap().lines[0].mirrored);
}

#[test]
fn mirrored_files_use_existing_validation_and_download_flow() {
    let mut v = message();
    v["att"] = json!({"url":"https://ntfy.sh/file/p.png","name":"p.png","type":"image/png","size":3,"sha256":"ab".repeat(32)});
    let mut s = Store::default();
    s.apply_mine(&v).unwrap();
    assert_eq!(s.downloads().len(), 1);
    s.media_done("https://ntfy.sh/file/p.png", Some("local.png"));
    s.apply_mine(&v);
    assert!(s.downloads().is_empty());
    for (key, value) in [("url", json!("https://evil.invalid/p.png")), ("size", json!(MEDIA_MAX + 1)), ("sha256", json!("bad"))] {
        let mut bad = v.clone();
        bad["att"][key] = value;
        assert!(Store::default().apply_mine(&bad).is_none());
    }
    for (key, value) in [("chat", json!("ask-orphan")), ("msg", json!("../bad")), ("model", json!("bad\nmodel")), ("agent", json!("unknown")),
        ("v", json!(2)), ("ts", json!(-1)), ("text", json!("x".repeat(MAX_TEXT + 1)))] {
        let mut bad = message();
        bad[key] = value;
        assert!(Store::default().apply_mine(&bad).is_none(), "{key}");
    }
}

#[test]
fn cached_notes_keep_their_original_dates() {
    let mut s = Store::default();
    let note = json!({"v":1,"chat":"updates","note":"n1","title":"Update","kind":"note","choices":[],"ts":1700000000000i64,
        "text":"earlier update","part":1,"parts":1});
    s.apply_note(&note, None);
    let c = s.get("updates").unwrap();
    assert_eq!(c.updated, 1700000000000);
    assert_eq!(c.lines[0].at, 1700000000000);
}
