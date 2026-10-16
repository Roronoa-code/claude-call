//! Deleting a chat with Undo (inbox.rs): hidden for UNDO_FOR, then gone for good with its files.
use super::*;

/// Deletes a chat for UNDO_FOR (its notification goes): hidden, with "Chat deleted · Undo"; replies that come meanwhile
/// are kept with it. Each deleted chat keeps its own Undo time; Undo brings back the newest.
pub(super) fn delete(ui: &HomeScreen, chat: &str) {
    let c = ui.global::<Chats>();
    if update(|s| s.delete(chat, chats::now())) != Some(true) {
        c.set_leaving("".into()); // not deleted (the store can't be saved): the row comes back
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

/// Lets each deleted chat go for good once its Undo is over.
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

/// Undo: the newest deleted chat back, with anything that came for it meanwhile; its row opens back in.
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
    fetch_media(); // anything that was still on its way
}

/// Undo is over for the chats deleted at or before `before`: they go for good, with their photos and videos unless
/// another chat (or a pick) uses them.
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
    changed(); // the Undo bar goes (or moves on to the one deleted before)
    let Some(dir) = media_dir() else { return };
    for name in files {
        let _ = std::fs::remove_file(dir.join(name));
    }
}
