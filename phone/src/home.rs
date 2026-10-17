use crate::{Chats, Home, HomeScreen, Toys, alive, android, cast, faces, live_gl, ring, service, settings, stage};
use slint::{ComponentHandle, Model};
use std::{
    cell::Cell,
    rc::Rc,
    time::{Duration, Instant},
};

fn refresh(ui: &HomeScreen) {
    let home = ui.global::<Home>();
    let read = android::with(|env, act| {
        Ok((settings::missing(env, act)?, settings::setup_line(env, act)?, settings::mode(env, act)?))
    });
    if let Some((missing, line, mode)) = read {
        home.set_ready(missing.is_empty());
        home.set_setup_text(line.into());
        home.set_mode(mode.into());
    }
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

fn short(s: &str, n: usize) -> String {
    if s.chars().count() <= n {
        return s.to_string();
    }
    format!("{}…", s.chars().take(n - 1).collect::<String>().trim_end())
}

static BACK: std::sync::Mutex<Vec<(i32, f32)>> = std::sync::Mutex::new(Vec::new());

pub fn back_gesture(phase: i32, progress: f32) {
    BACK.lock().unwrap().push((phase, progress));
}

fn follow(ui: &HomeScreen, swiping: &Cell<bool>) -> bool {
    let home = ui.global::<Home>();
    let mut moved = swiping.get();
    if home.get_kb() != android::kb() {
        home.set_kb(android::kb());
        moved = true;
    }
    for (phase, progress) in std::mem::take(&mut *BACK.lock().unwrap()) {
        swiping.set(phase <= 1);
        ui.invoke_back_gesture(phase, progress);
        moved = true;
    }
    moved
}

fn follow_fast(fast: &Rc<slint::Timer>, ui: &HomeScreen, swiping: &Rc<Cell<bool>>) {
    if fast.running() {
        return;
    }
    let (ui, sw, me) = (ui.as_weak(), swiping.clone(), Rc::downgrade(fast));
    let mut still = Instant::now();
    fast.start(slint::TimerMode::Repeated, Duration::from_millis(4), move || {
        let on = android::home_shown() && ui.upgrade().is_some_and(|ui| follow(&ui, &sw));
        if on {
            still = Instant::now();
        } else if still.elapsed() > Duration::from_millis(150) || !android::home_shown() {
            if let Some(t) = me.upgrade() {
                t.stop();
            }
        }
    });
}

const MAX_PAIR_PX_PER_DP: f32 = 2.75;
const LIGHT_EASE_RATE: f32 = 12.0;

pub struct Open {
    pub ui: HomeScreen,
    _timer: Rc<slint::Timer>,
}

#[cfg(target_os = "android")]
pub fn run() {
    let _ = open().ui.run();
}

pub fn open() -> Open {
    let ui = HomeScreen::new().expect("home screen");
    let home = ui.global::<Home>();
    home.on_set_mode({
        let ui = ui.as_weak();
        move |m| {
            if let Some(ui) = ui.upgrade() {
                ui.global::<Home>().set_mode(m.clone());
            }
            android::with(|env, act| settings::choose(env, act, &m));
        }
    });
    home.on_set_up(|| {
        android::with(settings::set_up);
    });
    let requested_at = Rc::new(Cell::new(None::<Instant>));
    let rings_at = Rc::new(Cell::new(0u64));
    let no_answer = Rc::new(Cell::new(None::<Instant>));
    home.on_call({
        let (ui, requested_at, rings_at, no_answer) = (ui.as_weak(), requested_at.clone(), rings_at.clone(), no_answer.clone());
        move |agent| {
            rings_at.set(ring::RINGS.load(std::sync::atomic::Ordering::Relaxed));
            no_answer.set(None);
            if let Some(ui) = ui.upgrade() {
                ui.global::<Home>().set_no_answer(false);
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
    // finish() makes the next open cold; the launcher leaves the task warm in the background.
    home.on_leave(|| {
        android::leave();
    });
    home.on_backable(|on| {
        android::backable(on);
    });
    refresh(&ui);
    let mut chats = crate::inbox::wire(&ui);

    let start = Instant::now();
    let pair = Rc::new(std::cell::RefCell::new(alive::Pair::new(0x9e37_79b9)));
    ui.global::<Toys>().on_tapped({
        let pair = pair.clone();
        move |who| pair.borrow_mut().hop(who == "codex", start.elapsed().as_secs_f32())
    });
    let hint = Rc::new(Cell::new((android::with(settings::hinted) == Some(false)).then_some(0.0f32)));
    ui.global::<Toys>().on_roll({
        let (pair, ui, hint) = (pair.clone(), ui.as_weak(), hint.clone());
        move |dy, phase| {
            let Some(ui) = ui.upgrade() else { return };
            let home = ui.global::<Home>();
            let now = start.elapsed().as_secs_f32();
            if phase == 0 {
                let m = pair.borrow_mut().drag(dy, now) as i32;
                if m != home.get_shown() {
                    if home.get_shown() >= 0 {
                        android::with(ring::tick);
                    }
                    home.set_shown(m);
                }
                return;
            }
            pair.borrow_mut().release(phase == 1, now);
            home.set_shown(-1);
            if phase == 1 && hint.get().is_some() {
                hint.set(Some(-1.0));
            }
        }
    });
    let faces = Rc::new(std::cell::RefCell::new(faces::Faces::new()));
    ui.global::<Chats>().on_poke({
        let faces = faces.clone();
        move || faces.borrow_mut().poke(start.elapsed().as_secs_f32())
    });
    let scene = Rc::new(std::cell::RefCell::new(live_gl::Scene::new(4)));
    let [small, big] = faces::sizes();
    let st = &stage::SPOT;
    let who = [cast::Who::Clawd(cast::Mat::Vinyl), cast::Who::Null];
    let below = who.map(|w| st.below(w));
    {
        let t = ui.global::<Toys>();
        t.set_k(st.k);
        t.set_clawd_below(below[0]);
        t.set_null_below(below[1]);
    }
    let screen_sf = android::density(ui.window());
    let r = (MAX_PAIR_PX_PER_DP / screen_sf).min(1.0);
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
    let mut lit = [1.0f32; 2];
    let mut last_now = 0.0f32;
    let mut shown = false;
    let swiping = Rc::new(Cell::new(false));
    let fast = Rc::new(slint::Timer::default());
    let mut last_refresh = Instant::now();
    let mut warmed = false;
    let mut voted = (0.0f32, Instant::now());
    ui.global::<Toys>().on_step({
        let pair = pair.clone();
        move |by| {
            pair.borrow_mut().step(by);
            android::with(ring::tick);
        }
    });
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
    timer.start(slint::TimerMode::Repeated, Duration::from_millis(16), {
        let ui = ui.as_weak();
        let tick = Rc::downgrade(&timer);
        move || {
            let Some(ui) = ui.upgrade() else { return };
            let away = !android::home_shown();
            if let Some(t) = tick.upgrade() {
                let every = Duration::from_millis(if away { 100 } else { 16 });
                if t.interval() != every {
                    t.set_interval(every);
                }
            }
            if away {
                shown = false;
                BACK.lock().unwrap().clear();
                if swiping.replace(false) {
                    ui.invoke_back_gesture(3, 0.0);
                }
                return;
            }
            let home = ui.global::<Home>();
            if follow(&ui, &swiping) {
                follow_fast(&fast, &ui, &swiping);
            }
            chats.tick(&ui);
            if last_refresh.elapsed() > Duration::from_secs(1) {
                last_refresh = Instant::now();
                refresh(&ui);
            }
            let wait = Duration::from_secs(90);
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
            let kb = home.get_kb();
            let now = start.elapsed().as_secs_f32();
            let mut p = pair.borrow_mut();
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
                    android::with(settings::set_hinted);
                }
            }
            let mode = settings::MODES.iter().position(|m| *m == home.get_mode().as_str()).unwrap_or(2);
            if let Some(m) = p.tick(now, mode).filter(|m| *m != mode) {
                home.invoke_set_mode(settings::MODES[m].into());
            }
            let (at, spread) = p.drum();
            if home.get_drum() != at || home.get_spread() != spread {
                home.set_drum(at);
                home.set_spread(spread);
            }
            let mut s = scene.borrow_mut();
            if s.failed && ui.global::<Toys>().get_live() {
                ui.global::<Toys>().set_live(false);
                ui.global::<Chats>().set_live(false);
            }
            let show = !s.failed && !ui.global::<Chats>().get_open() && kb == 0.0;
            let sf = ui.window().scale_factor() * r;
            let mut any = false;
            let codex = home.get_caller() == "codex";
            let ease = 1.0 - (-(now - last_now) * LIGHT_EASE_RATE).exp();
            last_now = now;
            for (i, c) in [&p.clawd, &p.null].into_iter().enumerate() {
                let want = if (i == 1) == codex { 1.0 } else { 0.5 };
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
            let chats_open = ui.global::<Chats>().get_open();
            if !warmed && chats_open && ui.global::<Chats>().get_current().is_empty() && !ui.window().has_active_animations() {
                warmed = true;
                s.warm = [cast::Mat::Cartoon, cast::Mat::Plush, cast::Mat::Chrome].map(cast::Who::Clawd).to_vec();
            }
            any |= !s.failed && !s.warm.is_empty() && chats_open;
            if any {
                ui.window().request_redraw();
            }
            let rest = show
                && hint.get().is_none()
                && !p.clawd.moving() && !p.null.moving() && !p.turning()
                && touched.get().elapsed() > Duration::from_secs(2)
                && !ui.window().has_active_animations();
            let want = if !rest { 0.0 } else if p.clawd.state() == cast::State::Sleep && p.null.state() == cast::State::Sleep { 10.0 } else { 20.0 };
            if want != voted.0 || (want > 0.0 && voted.1.elapsed() > Duration::from_secs(1)) {
                voted = (want, Instant::now());
                android::rate(want);
            }
        }
    });
    Open { ui, _timer: timer }
}
