//! The home screen: whether calls can reach this phone, how agents may reach you (dragged on Null's face, alive.rs),
//! the chats, and setup while it is unfinished (settings.rs).
use crate::{Chats, Home, HomeScreen, Toys, alive, android, cast, faces, live_gl, ring, service, settings, stage};
use slint::{ComponentHandle, Model};
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
        Ok((settings::missing(env, act)?, settings::setup_line(env, act)?, settings::mode(env, act)?))
    });
    if let Some((missing, line, mode)) = read {
        let missing = if fake == 2 { "Notifications, Microphone".to_string() } else { missing };
        if fake == 2 {
            ui.global::<Chats>().set_unread(true); // and a chat with something new
        }
        home.set_ready(missing.is_empty());
        home.set_setup_text(line.into());
        if fake == 0 || first {
            home.set_mode(mode.into());
        }
    }
    // An agent's question waiting (the chat list's rows, inbox.rs): the line under the mode names it. Its words stay
    // when it goes, so they fade out rather than vanish.
    let rows = ui.global::<Chats>().get_rows();
    let ask = rows.iter().find(|r| r.asks);
    home.set_ask_chat(ask.as_ref().map(|r| r.id.clone()).unwrap_or_default());
    if let Some(r) = ask {
        home.set_ask_line(format!("{} has a question", short(&r.name, 26)).into());
    }
    if let Some((top, bottom)) = android::insets() {
        home.set_sat(top);
        home.set_sab(bottom);
    }
}

/// `s` in at most `n` characters, ending in "…" when cut.
fn short(s: &str, n: usize) -> String {
    if s.chars().count() <= n {
        return s.to_string();
    }
    format!("{}…", s.chars().take(n - 1).collect::<String>().trim_end())
}

/// The system back swipe's steps not yet played, in order: (phase, progress).
static BACK: std::sync::Mutex<Vec<(i32, f32)>> = std::sync::Mutex::new(Vec::new());

/// The system back swipe, from Android's main thread (Shell.kt, shell.rs): phase 0 starts, 1 moves (progress 0..1),
/// 2 is Back, 3 cancels. Played by the home screen's frame timer (home.slint back-gesture), not posted to the event
/// loop: that proxy is the newest screen's, which after a call is the call's.
pub fn back_gesture(phase: i32, progress: f32) {
    BACK.lock().unwrap().push((phase, progress));
}

/// The most device px per logical px the pair is drawn at: past it (a very dense screen) the GPU's work grows with
/// the square for detail the eye can't see; the picture is scaled up to its place instead.
const PAIR_SF: f32 = 2.75;

pub fn run(app: android_activity::AndroidApp) {
    let ui = HomeScreen::new().expect("home screen");
    let home = ui.global::<Home>();
    // PREVIEW (test emulator): `debug.callme.fake` set keeps mode changes and the Call button on this screen only
    // (nothing stored, published or asked of the PC), and plays the one-time hint on every start.
    let fake = stage::prop(c"debug.callme.fake", 2);
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
    // The Call button: "Calling you…" until the call rings; a minute and a half with no call: "No answer from the PC"
    // for a moment, then the button as it was.
    let requested_at = Rc::new(Cell::new(None::<Instant>));
    let rings_at = Rc::new(Cell::new(0u64)); // ring::RINGS when the call was asked for
    let no_answer = Rc::new(Cell::new(None::<Instant>));
    home.on_call({
        let (ui, requested_at, rings_at, no_answer) = (ui.as_weak(), requested_at.clone(), rings_at.clone(), no_answer.clone());
        move |agent| {
            rings_at.set(ring::RINGS.load(std::sync::atomic::Ordering::Relaxed));
            no_answer.set(None);
            if let Some(ui) = ui.upgrade() {
                ui.global::<Home>().set_no_answer(false);
            }
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
    // Back with nothing open: to the background, warm, as the launcher leaves it (finish() made the next open cold).
    home.on_leave(|| {
        android::with(|env, act| env.call_method(act, "moveTaskToBack", "(Z)Z", &[jni::objects::JValue::Bool(1)]).map(drop));
    });
    // Something Back can close is open: the system back swipe comes here as it moves (Shell.kt backable).
    home.on_backable(|on| {
        android::with(|env, act| env.call_method(act, "backable", "(Z)V", &[jni::objects::JValue::Bool(on as u8)]).map(drop));
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
    // A drag on Null rolls his face through the modes, a light tick at each; letting go keeps the one he shows.
    let hint = Rc::new(Cell::new((fake > 0 || android::with(settings::hinted) == Some(false)).then_some(0.0f32)));
    ui.global::<Toys>().on_roll({
        let (pair, ui, hint) = (pair.clone(), ui.as_weak(), hint.clone());
        move |dy, phase| {
            let Some(ui) = ui.upgrade() else { return };
            let home = ui.global::<Home>();
            if phase == 0 {
                let m = pair.borrow_mut().drag(dy) as i32;
                if m != home.get_shown() {
                    if home.get_shown() >= 0 {
                        android::with(ring::tick);
                    }
                    home.set_shown(m);
                }
                return;
            }
            let m = pair.borrow_mut().release(phase == 1);
            home.set_shown(-1);
            home.invoke_set_mode(settings::MODES[m].into());
            if phase == 1 && hint.get().is_some() {
                hint.set(Some(-1.0)); // learnt: the hint goes
            }
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
    // The pair's pictures are sized for at most PAIR_SF px per logical px (`r` of the screen's own).
    let screen_sf = app.config().density().map_or(1.0, |dpi| dpi as f32 / 160.0);
    let r = (PAIR_SF / screen_sf).min(1.0);
    let home_size = |i: usize, extra: f32| {
        let (w, h) = [alive::CLAWD_SIZE, alive::NULL_SIZE][i];
        (w * st.k, (h + extra) * st.k)
    };
    let sizes = [home_size(0, below[0]), home_size(1, below[1]), small, big];
    let tex = [(sizes[0].0 * r, sizes[0].1 * r), (sizes[1].0 * r, sizes[1].1 * r), small, big];
    live_gl::attach(&ui, scene.clone(), tex.to_vec(), {
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
    let mut warmed = false; // the chats' characters' shaders built ahead (live_gl.rs Scene::warm)
    let mut voted = (0.0f32, Instant::now()); // the frame rate last asked of the display (0: no preference)
    let touched = Rc::new(Cell::new(Instant::now()));
    ui.global::<Toys>().on_point({
        let (pair, ui, touched) = (pair.clone(), ui.as_weak(), touched.clone());
        move |x, y, down| {
            touched.set(Instant::now());
            let Some(ui) = ui.upgrade() else { return };
            let dx = ui.global::<Toys>().get_null_dx();
            pair.borrow_mut().point(x, y, dx, down, start.elapsed().as_secs_f32())
        }
    });
    let timer = Rc::new(slint::Timer::default());
    // Every frame: the toys' springs need it; the rest is cheap.
    timer.start(slint::TimerMode::Repeated, Duration::from_millis(16), {
        let ui = ui.as_weak();
        let tick = Rc::downgrade(&timer);
        move || {
            let Some(ui) = ui.upgrade() else { return };
            // Away (another app in front, the screen off): nothing is drawn or polled; it only looks, ten times a
            // second, for the app to come back.
            let away = !android::home_shown();
            if let Some(t) = tick.upgrade() {
                let every = Duration::from_millis(if away { 100 } else { 16 });
                if t.interval() != every {
                    t.set_interval(every);
                }
            }
            if away {
                shown = false; // all drawn afresh on the way back
                BACK.lock().unwrap().clear();
                return;
            }
            let home = ui.global::<Home>();
            // the keyboard and the back swipe, as Android's main thread last said (android::kb, back_gesture)
            if home.get_kb() != android::kb() {
                home.set_kb(android::kb());
            }
            for (phase, progress) in std::mem::take(&mut *BACK.lock().unwrap()) {
                ui.invoke_back_gesture(phase, progress);
            }
            chats.tick(&ui);
            // Permissions granted in Settings, or a mode changed elsewhere, show up within a second.
            if last_refresh.elapsed() > Duration::from_secs(1) {
                last_refresh = Instant::now();
                refresh(&ui, fake, false);
            }
            let wait = Duration::from_secs(if fake > 0 { 6 } else { 90 });
            if let Some(t) = requested_at.get() {
                let rang = ring::RINGS.load(std::sync::atomic::Ordering::Relaxed) != rings_at.get() || service::active_link().is_some();
                if rang || t.elapsed() > wait {
                    requested_at.set(None);
                    home.set_requested(false);
                    if !rang {
                        no_answer.set(Some(Instant::now()));
                        home.set_no_answer(true);
                    }
                }
            }
            if no_answer.get().is_some_and(|t| t.elapsed() > Duration::from_secs(4)) {
                no_answer.set(None);
                home.set_no_answer(false);
            }
            let kb = home.get_kb(); // the keyboard, followed by home.slint itself
            let now = start.elapsed().as_secs_f32();
            let mut p = pair.borrow_mut();
            // The one-time hint: a chevron by Null's feet and two small nudges of his face, then never again.
            if let Some(from) = hint.get() {
                if from == 0.0 {
                    if now > 1.5 {
                        hint.set(Some(now));
                        home.set_hint(true);
                    }
                } else if from > 0.0 && now - from < 8.0 {
                    p.nudge(now - from - 0.6);
                    p.nudge(now - from - 2.6);
                } else {
                    hint.set(None);
                    home.set_hint(false);
                    if fake == 0 {
                        android::with(settings::set_hinted);
                    }
                }
            }
            let mode = settings::MODES.iter().position(|m| *m == home.get_mode().as_str()).unwrap_or(2);
            p.tick(now, mode);
            let mut s = scene.borrow_mut();
            if s.failed && ui.global::<Toys>().get_live() {
                ui.global::<Toys>().set_live(false); // still pictures instead
                ui.global::<Chats>().set_live(false);
            }
            // Each toy: a new frame at 60 fps while it moves, 20 while it only breathes, 10 asleep, none when hidden.
            let show = !s.failed && !ui.global::<Chats>().get_open() && kb == 0.0;
            let sf = ui.window().scale_factor() * r;
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
                any |= faces.borrow_mut().tick(&ui, &mut s.slots[2..], now, ui.window().scale_factor());
            }
            // The chats list at rest: the chats' characters' shaders are built now, unseen, one a frame.
            let chats_open = ui.global::<Chats>().get_open();
            if !warmed && chats_open && ui.global::<Chats>().get_current().is_empty() && !ui.window().has_active_animations() {
                warmed = true;
                s.warm = [cast::Mat::Cartoon, cast::Mat::Plush, cast::Mat::Chrome].map(cast::Who::Clawd).to_vec();
            }
            any |= !s.failed && !s.warm.is_empty() && chats_open;
            if any {
                ui.window().request_redraw();
            }
            // The home stage at rest (nothing moving, animating or touched for a moment): the display is told the
            // screen changes only at the pair's breathing rate, so it can run slower; anything else lifts that.
            let rest = show
                && hint.get().is_none()
                && !p.clawd.moving() && !p.null.moving()
                && touched.get().elapsed() > Duration::from_secs(2)
                && !ui.window().has_active_animations();
            let want = if !rest { 0.0 } else if p.clawd.state() == cast::State::Sleep && p.null.state() == cast::State::Sleep { 10.0 } else { 20.0 };
            if want != voted.0 || (want > 0.0 && voted.1.elapsed() > Duration::from_secs(1)) {
                voted = (want, Instant::now());
                android::with(|env, act| env.call_method(act, "rate", "(F)V", &[want.into()]).map(drop));
            }
        }
    });
    let _ = ui.run();
}
