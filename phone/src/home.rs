//! The home screen: whether calls can reach this phone, how agents may reach you, a test ring, the widget and the
//! private channel (settings.rs, widget.rs, ring.rs).
use crate::{
    Home, HomeScreen, Px, android, jx,
    mascot::{Mascot, Phase},
    null::Null,
    ring, service, settings,
};
use slint::{ComponentHandle, ModelRc, VecModel};
use std::{
    cell::Cell,
    rc::Rc,
    time::{Duration, Instant},
};

fn refresh(ui: &HomeScreen) {
    let home = ui.global::<Home>();
    let read = android::with(|env, act| {
        Ok((settings::missing(env, act)?, settings::mode(env, act)?, settings::topic(env, act)?, settings::can_pin(env, act)?))
    });
    if let Some((missing, mode, topic, can_pin)) = read {
        home.set_ready(missing.is_empty());
        home.set_missing(format!("Still needed: {missing}").into());
        home.set_mode(mode.into());
        home.set_topic(topic.into());
        home.set_can_pin(can_pin);
    }
    if let Some((top, bottom)) = android::insets() {
        home.set_sat(top);
        home.set_sab(bottom);
    }
}

pub fn run(_app: android_activity::AndroidApp) {
    let ui = HomeScreen::new().expect("home screen");
    let home = ui.global::<Home>();
    home.on_set_mode({
        let ui = ui.as_weak();
        move |m| {
            if let Some(ui) = ui.upgrade() {
                ui.global::<Home>().set_mode(m.clone()); // at once; the phone and PC follow
            }
            android::with(|env, act| settings::choose(env, act, &m));
        }
    });
    home.on_set_up(|| {
        android::with(settings::set_up);
    });
    home.on_test_ring(|| {
        android::with(|env, act| {
            let app = jx::app(env, act)?;
            ring::ring(env, &app, ring::TEST_LINK, "Test ring")
        });
    });
    // The Call button: "Calling you…" until the call rings (or a minute and a half passes).
    let requested_at = Rc::new(Cell::new(None::<Instant>));
    home.on_call({
        let (ui, requested_at) = (ui.as_weak(), requested_at.clone());
        move |agent| {
            if let Some(topic) = android::with(|env, act| settings::topic(env, act)) {
                crate::net::request_call(topic, agent.to_string());
                requested_at.set(Some(Instant::now()));
                if let Some(ui) = ui.upgrade() {
                    ui.global::<Home>().set_requested(true);
                }
            }
        }
    });
    home.on_pin(|wide| {
        android::with(|env, act| settings::pin(env, act, wide));
    });
    home.on_save_topic(|t| {
        android::with(|env, act| {
            settings::set_topic(env, act, &t)?;
            service::restart(env, act)
        });
    });
    let editing = Rc::new(Cell::new(false));
    home.on_editing({
        let editing = editing.clone();
        move |on| editing.set(on)
    });
    refresh(&ui);

    // Clawd idles on the home screen (asleep while it is quiet) and Null idles beside it (at its laptop while it is
    // quiet: they keep working); only what changed is drawn.
    let start = Instant::now();
    let mut mascot = Mascot::new(0.0);
    let mut null = Null::default();
    let mut last: Vec<Px> = vec![];
    let mut last_refresh = Instant::now();
    let timer = slint::Timer::default();
    timer.start(slint::TimerMode::Repeated, Duration::from_millis(33), {
        let ui = ui.as_weak();
        move || {
            let Some(ui) = ui.upgrade() else { return };
            let home = ui.global::<Home>();
            // Permissions granted in Settings, or a mode changed elsewhere, show up within a second.
            if last_refresh.elapsed() > Duration::from_secs(1) {
                last_refresh = Instant::now();
                refresh(&ui);
            }
            if requested_at.get().is_some_and(|t| t.elapsed() > Duration::from_secs(90) || service::active_link().is_some()) {
                requested_at.set(None);
                home.set_requested(false);
            }
            // The channel sheet rides above the keyboard.
            let kb = if editing.get() { android::keyboard() } else { 0.0 };
            if home.get_kb() != kb {
                home.set_kb(kb);
            }
            let phase = if matches!(home.get_mode().as_str(), "none" | "wake" | "sleep") { Phase::Hold } else { Phase::Listening };
            let now = start.elapsed().as_secs_f32();
            let n = null.frame(now, if phase == Phase::Hold { Phase::Hold } else { Phase::Hearing });
            if home.get_null_frame() != n {
                home.set_null_frame(n);
            }
            let cells = mascot.frame(now, phase, false, 0.0, 0.0);
            let px: Vec<Px> = cells
                .iter()
                .map(|c| Px {
                    x: c.x,
                    y: c.y,
                    w: c.w,
                    h: c.h,
                    c: slint::Color::from_rgb_u8((c.c >> 16) as u8, (c.c >> 8) as u8, c.c as u8),
                    o: c.o,
                })
                .collect();
            if px != last {
                home.set_clawd(ModelRc::new(VecModel::from(px.clone())));
                last = px;
            }
        }
    });
    let _ = ui.run();
}
