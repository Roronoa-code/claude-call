//! The home screen: whether calls can reach this phone, how agents may reach you, a test ring, the widget and the
//! private channel (settings.rs, widget.rs, ring.rs).
use crate::{
    Chats, Home, HomeScreen, Toys, alive, android, jx, ring, service, settings,
    toys::{Body, Toy},
};
use slint::ComponentHandle;
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
    let mut chats = crate::inbox::wire(&ui);

    // The vinyl Clawd and Null live on the home screen (toys.rs): blinks, glances, Null thinking at its laptop, asleep
    // in the quiet modes, and a hop when tapped. Clawd is drawn live in 3D by the GPU (alive.rs) and watches a finger.
    let start = Instant::now();
    let toys = Rc::new(std::cell::RefCell::new((alive::Live::new(0x9e37_79b9), Toy::new(true, 0x85eb_ca6b))));
    ui.global::<Toys>().on_tapped({
        let toys = toys.clone();
        move |who| {
            let now = start.elapsed().as_secs_f32();
            let mut t = toys.borrow_mut();
            if who == "codex" { t.1.hop(now) } else { t.0.hop(now) }
        }
    });
    ui.global::<Toys>().on_point({
        let toys = toys.clone();
        move |x, y, down| toys.borrow_mut().0.point(x, y, down, start.elapsed().as_secs_f32())
    });
    let scene = Rc::new(std::cell::RefCell::new(alive::Scene::default()));
    alive::attach(&ui, scene.clone());
    let mut drawn = 0.0f32;
    let mut last_refresh = Instant::now();
    let timer = slint::Timer::default();
    // Every frame: Clawd's springs need it; the rest is cheap.
    timer.start(slint::TimerMode::Repeated, Duration::from_millis(16), {
        let ui = ui.as_weak();
        move || {
            let Some(ui) = ui.upgrade() else { return };
            let home = ui.global::<Home>();
            chats.tick(&ui);
            // Permissions granted in Settings, or a mode changed elsewhere, show up within a second.
            if last_refresh.elapsed() > Duration::from_secs(1) {
                last_refresh = Instant::now();
                refresh(&ui);
            }
            if requested_at.get().is_some_and(|t| t.elapsed() > Duration::from_secs(90) || service::active_link().is_some()) {
                requested_at.set(None);
                home.set_requested(false);
            }
            // The channel sheet (and a chat's field) rides above the keyboard. An open chat always follows it: back from
            // another app, Android raises the keyboard again for the field without it taking focus anew.
            let chats = ui.global::<Chats>();
            let in_chat = chats.get_open() && !chats.get_current().is_empty();
            let kb = if editing.get() || in_chat { android::keyboard() } else { 0.0 };
            if home.get_kb() != kb {
                home.set_kb(kb);
            }
            let asleep = matches!(home.get_mode().as_str(), "none" | "wake" | "sleep");
            let now = start.elapsed().as_secs_f32();
            let g = ui.global::<Toys>();
            g.set_asleep(asleep);
            let mut t = toys.borrow_mut();
            if t.0.tick(now, asleep) {
                let Body { pose, squash, lift, lean } = t.0.toy.body;
                (g.set_clawd_pose(pose), g.set_clawd_squash(squash), g.set_clawd_lift(lift), g.set_clawd_lean(lean));
            }
            // Live Clawd: a new frame at 60 fps while he moves, 30 while he only breathes, 15 asleep, none when hidden.
            let show = g.get_live() && !ui.global::<Chats>().get_open() && kb == 0.0;
            let fps = if t.0.moving() { 60.0 } else if asleep { 15.0 } else { 30.0 };
            let mut s = scene.borrow_mut();
            if show && (!s.show || now - drawn >= 1.0 / fps - 0.004) {
                s.pose = t.0.pose();
                s.fresh = true;
                drawn = now;
                ui.window().request_redraw();
            }
            s.show = show;
            if t.1.tick(now, asleep) {
                let Body { pose, squash, lift, lean } = t.1.body;
                (g.set_null_pose(pose), g.set_null_squash(squash), g.set_null_lift(lift), g.set_null_lean(lean));
            }
        }
    });
    let _ = ui.run();
}
