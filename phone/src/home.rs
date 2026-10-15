//! The home screen: whether calls can reach this phone, how agents may reach you, a test ring, the widget and the
//! private channel (settings.rs, widget.rs, ring.rs).
use crate::{Chats, Home, HomeScreen, Toys, alive, android, cast, faces, jx, live_gl, ring, service, settings, stage};
use slint::ComponentHandle;
use std::{
    cell::Cell,
    rc::Rc,
    time::{Duration, Instant},
};

/// `fake` (preview only): 0, or keep the mode chosen on this screen (1) and also pretend setup isn't finished and a
/// chat is unread (2).
fn refresh(ui: &HomeScreen, fake: usize, first: bool) {
    let home = ui.global::<Home>();
    let read = android::with(|env, act| {
        Ok((settings::missing(env, act)?, settings::mode(env, act)?, settings::topic(env, act)?, settings::can_pin(env, act)?))
    });
    if let Some((missing, mode, topic, can_pin)) = read {
        let missing = if fake == 2 { "Notifications, Microphone".to_string() } else { missing };
        if fake == 2 {
            ui.global::<Chats>().set_unread(true); // and a chat with something new
        }
        home.set_ready(missing.is_empty());
        home.set_missing(format!("Still needed: {missing}").into());
        if fake == 0 || first {
            home.set_mode(mode.into());
        }
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
    // PREVIEW (branch preview/home-controls-2): which home controls show (controls.slint), and `debug.callme.fake` set
    // keeps mode taps and the Call button on this screen only (nothing stored, published or asked of the PC).
    let fake = stage::prop(c"debug.callme.fake", 2);
    ui.global::<Toys>().set_controls(stage::prop(c"debug.callme.controls", 4) as i32);
    home.on_set_mode({
        let ui = ui.as_weak();
        move |m| {
            if let Some(ui) = ui.upgrade() {
                ui.global::<Home>().set_mode(m.clone()); // at once; the phone and PC follow
            }
            if fake == 0 {
                android::with(|env, act| settings::choose(env, act, &m));
            }
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
            if fake > 0 {
                requested_at.set(Some(Instant::now()));
                if let Some(ui) = ui.upgrade() {
                    ui.global::<Home>().set_requested(true);
                }
                return;
            }
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
    refresh(&ui, fake, true);
    let mut chats = crate::inbox::wire(&ui);

    // The vinyl Clawd and Null live on the home screen in 3D, drawn by the GPU (alive.rs, live_gl.rs): blinks,
    // glances, Null thinking at its laptop, asleep in the quiet modes, a hop when tapped, and eyes on your finger.
    let start = Instant::now();
    let pair = Rc::new(std::cell::RefCell::new(alive::Pair::new(0x9e37_79b9)));
    ui.global::<Toys>().on_tapped({
        let pair = pair.clone();
        move |who| pair.borrow_mut().hop(who == "codex", start.elapsed().as_secs_f32())
    });
    ui.global::<Toys>().on_point({
        let (pair, ui) = (pair.clone(), ui.as_weak());
        move |x, y, down| {
            let Some(ui) = ui.upgrade() else { return };
            let dx = ui.global::<Toys>().get_null_dx();
            pair.borrow_mut().point(x, y, dx, down, start.elapsed().as_secs_f32())
        }
    });
    // The chat thread's character too (faces.rs): slots 0 and 1 are Clawd and Null, 2 and 3 the chat's, small and big.
    let faces = Rc::new(std::cell::RefCell::new(faces::Faces::new()));
    ui.global::<Chats>().on_poke({
        let faces = faces.clone();
        move || faces.borrow_mut().poke(start.elapsed().as_secs_f32())
    });
    let scene = Rc::new(std::cell::RefCell::new(live_gl::Scene::new(4)));
    let [small, big] = faces::sizes();
    // PREVIEW: the stage they stand in (stage.rs); its floor is drawn under each picture, so the textures are taller.
    let v = stage::pick();
    let st = &stage::STAGES[v];
    let who = [cast::Who::Clawd(cast::Mat::Vinyl), cast::Who::Null];
    let below = who.map(|w| st.below(w));
    {
        let t = ui.global::<Toys>();
        t.set_stage(v as i32);
        t.set_k(st.k);
        t.set_clawd_below(below[0]);
        t.set_null_below(below[1]);
    }
    let home_size = |i: usize, extra: f32| {
        let (w, h) = [alive::CLAWD_SIZE, alive::NULL_SIZE][i];
        (w * st.k, (h + extra) * st.k)
    };
    let sizes = [home_size(0, below[0]), home_size(1, below[1]), small, big];
    live_gl::attach(&ui, scene.clone(), sizes.to_vec(), {
        let ui = ui.as_weak();
        move |images| {
            let Some(ui) = ui.upgrade() else { return };
            let t = ui.global::<Toys>();
            t.set_clawd_live(images[0].clone());
            t.set_null_live(images[1].clone());
            let c = ui.global::<Chats>();
            c.set_face_live(images[2].clone());
            c.set_big_live(images[3].clone());
        }
    });
    let mut drawn = [0.0f32; 2];
    let mut lit = [1.0f32; 2]; // PREVIEW: each one's light on a stage, easing to 1 or DIM as the Call button's pick changes
    let mut last_now = 0.0f32;
    let mut shown = false;
    let mut last_refresh = Instant::now();
    let timer = slint::Timer::default();
    // Every frame: the toys' springs need it; the rest is cheap.
    timer.start(slint::TimerMode::Repeated, Duration::from_millis(16), {
        let ui = ui.as_weak();
        move || {
            let Some(ui) = ui.upgrade() else { return };
            let home = ui.global::<Home>();
            chats.tick(&ui);
            // Permissions granted in Settings, or a mode changed elsewhere, show up within a second.
            if last_refresh.elapsed() > Duration::from_secs(1) {
                last_refresh = Instant::now();
                refresh(&ui, fake, false);
            }
            let wait = Duration::from_secs(if fake > 0 { 6 } else { 90 });
            if requested_at.get().is_some_and(|t| t.elapsed() > wait || service::active_link().is_some()) {
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
            let mut p = pair.borrow_mut();
            p.tick(now, asleep);
            let mut s = scene.borrow_mut();
            if s.failed && ui.global::<Toys>().get_live() {
                ui.global::<Toys>().set_live(false); // still pictures instead
                ui.global::<Chats>().set_live(false);
            }
            // Each toy: a new frame at 60 fps while it moves, 30 while it only breathes, 15 asleep, none when hidden.
            let show = !s.failed && !ui.global::<Chats>().get_open() && kb == 0.0;
            let sf = ui.window().scale_factor();
            let mut any = false;
            let codex = home.get_caller() == "codex";
            let ease = 1.0 - (-(now - last_now) * 12.0).exp(); // about 250 ms, as the opacity it replaces
            last_now = now;
            for (i, c) in [&p.clawd, &p.null].into_iter().enumerate() {
                let want = if v == 0 || (i == 1) == codex { 1.0 } else { 0.5 };
                let dimming = (lit[i] - want).abs() > 0.002;
                lit[i] = if dimming { lit[i] + (want - lit[i]) * ease } else { want };
                if show && (!shown || dimming || alive::due(c, drawn[i], now)) {
                    let (res, view) = (live_gl::texture_px(home_size(i, 0.0), sf), live_gl::texture_px(sizes[i], sf));
                    let mut frame = c.uniforms();
                    frame.extend(st.uniforms(c.who, res.1, view.1, sf, lit[i]));
                    s.slots[i] = live_gl::Slot { who: c.who, res, view, frame, fresh: true };
                    drawn[i] = now;
                    any = true;
                }
            }
            shown = show;
            if !s.failed {
                any |= faces.borrow_mut().tick(&ui, &mut s.slots[2..], now, sf);
            }
            if any {
                ui.window().request_redraw();
            }
        }
    });
    let _ = ui.run();
}
