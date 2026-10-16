use super::*;

pub(super) fn delete(ui: &HomeScreen, chat: &str) {
    let c = ui.global::<Chats>();
    if update(|s| s.delete(chat, chats::now())) != Some(true) {
        c.set_leaving("".into());
        show(ui);
        return;
    }
    android::with(|env, act| chat_alert::cancel(env, act, chat));
    if c.get_current() == chat {
        c.set_current("".into());
    }
    changed();
    show(ui);
    undo_timer();
}

pub(super) fn undo_timer() {
    UNDO_TIMER.with(|t| {
        t.start(slint::TimerMode::Repeated, Duration::from_millis(250), || {
            forget(chats::now() - UNDO_FOR.as_millis() as i64);
            if read(|s| s.undoable().is_none()).unwrap_or(true) {
                UNDO_TIMER.with(slint::Timer::stop);
            }
        })
    });
}

pub(super) fn undo(ui: &HomeScreen) {
    let c = ui.global::<Chats>();
    let Some(Some(id)) = read(|s| s.undoable().map(|c| c.id.clone())) else { return };
    if update(|s| s.undelete(&id)) != Some(true) {
        return;
    }
    c.set_leaving("".into());
    c.set_returning(id.into());
    changed();
    show(ui);
    fetch_media();
}

pub(super) fn forget(before: i64) {
    let Some((n, files)) = update(|s| {
        let gone = s.purge(before);
        (gone.len(), gone.iter().flat_map(|c| s.unused_files(c)).collect::<Vec<_>>())
    }) else {
        return;
    };
    if n == 0 {
        return;
    }
    changed();
    let Some(dir) = media_dir() else { return };
    for name in files {
        let _ = std::fs::remove_file(dir.join(name));
    }
}
