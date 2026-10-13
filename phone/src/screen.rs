//! The call screen: draws the call (Clawd, captions, the thread, controls) from the engine's view, ~30 times a second.
use crate::{
    Cap, CallScreen, Msg as UiMsg, Px, android,
    engine::Engine,
    mascot::{Mascot, Phase},
    null::Null,
    view::{self, Msg, View, Voice, clock},
};
use slint::{ComponentHandle, Model, ModelRc, SharedString, VecModel};
use std::{rc::Rc, sync::Arc, time::Duration};

const CLOSE_MS: f64 = 1100.0; // the screen fades out this long after the call ends...
const FINISH_MS: f64 = 1200.0; // ...and closes here

/// What the thread keeps showing while it hides, so it can fade out instead of vanishing.
#[derive(Default, Clone)]
struct Held {
    me_text: Option<String>,
    live: bool,
    pending: bool,
    claude: Option<Msg>,
}

/// The screen's state between frames.
struct Frame {
    held: Held,
    cap: Option<Cap>,
    mascot: Mascot,
    null: Null, // Codex's pet, for calls from Codex
    perch: (f64, f64, f64), // where the mascot's box bottom-left is easing to: x, bottom, start time
    from: (f64, f64),
    u: f64,
    u_from: (f64, f64), // pixel size it is stepping from, and when it started
    thread_was: bool,
    last_pixels: Vec<Px>,
    moving_until: f64,
    level: f32,
    finished: bool,
    rate_set: bool,
    last_route: f64,
}

fn hash(s: &str) -> i32 {
    s.bytes().fold(0x811c9dc5u32, |h, b| (h ^ b as u32).wrapping_mul(0x0100_0193)) as i32
}

/// The big caption: Claude talking, listening, on hold and the end of the call.
fn caption(v: &View, now: f64) -> Cap {
    let last_claude = v.msgs.iter().rev().find(|m| !m.me);
    let part = |m: &Msg| m.spoken.0.min(m.parts.len().saturating_sub(1));
    // Long sentences get a smaller caption so they always fit above the controls.
    let size = |m: &Msg| {
        let n = m.parts.get(part(m)).map_or(0, |p| view::words(p).len());
        if n > 30 { 2 } else if n > 18 { 1 } else { 0 }
    };
    let plain = |key: &str, label: String, text: &str, tone: &str, orb: bool| Cap {
        key: hash(key),
        label: label.into(),
        text: text.into(),
        lit: text.into(),
        size: 0,
        tone: tone.into(),
        orb,
    };
    let spoken = |m: &Msg, tone: &str| {
        let (text, lit) = view::lit(m, Some(part(m)));
        Cap { key: hash(&format!("c{}-{}", m.id, part(m))), label: v.agent.clone().into(), text: text.into(), lit: lit.into(), size: size(m), tone: tone.into(), orb: false }
    };
    match v.phase {
        Phase::Incoming => plain("inc", "Calling now".into(), &format!("{} would like to talk.", v.agent), "", false),
        Phase::Connecting => plain("con", "Connecting".into(), "One moment…", "dim", false),
        Phase::Talking => match v.msgs.iter().find(|m| m.live).or(last_claude) {
            // Until its first word is heard the label stays invisible (same size, so nothing shifts).
            Some(m) => spoken(m, if m.spoken == (0, 0) { "pre" } else { "" }),
            None => plain("turn", "Your turn".into(), &format!("Go ahead, {} is listening.", v.agent), "dim", false),
        },
        Phase::Hold => plain("hold", format!("On hold · {}", clock(now - v.hold_start)), &format!("{}.", v.hold_label), "hold", true),
        Phase::Ended => plain("end", clock(v.ended_at - v.call_start), "Call ended.", "dim", false),
        _ => match last_claude {
            Some(m) => spoken(m, "rest"),
            None => plain("turn", "Your turn".into(), &format!("Go ahead, {} is listening.", v.agent), "dim", false),
        },
    }
}

fn ease_out(t: f64) -> f64 {
    let t = t.clamp(0.0, 1.0);
    1.0 - (1.0 - t).powi(3)
}

fn present(ui: &CallScreen, e: &Arc<Engine>, f: &mut Frame, app: &android_activity::AndroidApp) {
    let now = e.now();
    if !f.rate_set {
        // Calls don't need 120 Hz: half the drawing work, a cooler phone.
        if let Some(w) = app.native_window() {
            android::set_frame_rate(w.ptr().as_ptr().cast(), 60.0);
            f.rate_set = true;
        }
    }
    if now - f.last_route > 1000.0 {
        f.last_route = now;
        e.keep_route();
        if let Some((top, bottom)) = android::insets() {
            ui.set_sat(top);
            ui.set_sab(bottom);
        }
    }
    let claude_level = e.claude_level();
    let mut s = e.lock();
    e.line_progress(&mut s);
    let ended = s.ended;
    let v = &s.view;
    let thread = matches!(v.phase, Phase::Hearing | Phase::Thinking);
    let pending = v.phase == Phase::Thinking && !v.transcribing;
    let last_claude = v.msgs.iter().rev().find(|m| !m.me).cloned();
    let last_me = v.msgs.iter().rev().find(|m| m.me).cloned();
    if thread || f.held.claude.is_none() {
        f.held = Held {
            me_text: if pending { last_me.map(|m| m.text) } else { Some(v.partial.clone()).filter(|p| !p.is_empty()) },
            live: v.phase == Phase::Hearing,
            pending,
            claude: last_claude.clone(),
        };
    }
    let bubble = if thread { last_claude } else { f.held.claude.clone() };
    // The big caption is frozen while the thread shows, so it can fly back from where it was.
    if !thread || f.cap.is_none() {
        f.cap = Some(caption(v, now));
    }
    let me_level = if v.voice == Voice::Me && !v.muted { v.mic_level } else { 0.0 };
    let level = if v.voice == Voice::Claude { claude_level } else { me_level };
    f.level += (level - f.level) * 0.35;
    let status = if !v.notice.is_empty() {
        v.notice.clone()
    } else if matches!(v.phase, Phase::Connecting | Phase::Talking | Phase::Listening | Phase::Hearing) || (v.phase == Phase::Thinking && !pending) {
        v.status_text().to_string()
    } else {
        String::new()
    };
    let time = match v.phase {
        Phase::Incoming => "Calling you".to_string(),
        Phase::Connecting => "Connecting…".to_string(),
        Phase::Ended => format!("Call ended · {}", clock(v.ended_at - v.call_start)),
        _ => format!("On call · {}", clock(now - v.call_start)),
    };
    let phase = v.phase;
    let agent = v.agent.clone();
    let muted = v.muted;
    let msgs: Vec<UiMsg> = v
        .msgs
        .iter()
        .map(|m| {
            if m.me {
                UiMsg { me: true, text: m.text.clone().into(), lit: m.text.clone().into() }
            } else {
                let (text, lit) = view::lit(m, None);
                let shown = if m.cut { lit.clone() } else { text };
                UiMsg { me: false, text: shown.into(), lit: lit.into() }
            }
        })
        .collect();
    let (speaker, can_finish, ended_at) = (v.speaker, v.can_finish, v.ended_at);
    drop(s);

    // ---- Clawd: whole pixels, perched small on Claude's bubble while the thread shows ----
    let perched = thread && bubble.is_some();
    if perched != f.thread_was {
        f.thread_was = perched;
        f.from = mascot_box(f, now);
        f.perch = if perched { (30.0, 60.0, now) } else { (24.0, 108.0, now) };
        f.u_from = (f.u, now);
    }
    let (bx, bottom) = mascot_box(f, now);
    // Pixel art only ever shows at whole pixels per cell: 4, then 3, then 2 (or back).
    let target_u = if perched { 2.0 } else { 4.0 };
    let steps = ((now - f.u_from.1) / 100.0).floor().min(2.0);
    f.u = if f.u_from.0 == target_u { target_u } else { f.u_from.0 + (target_u - f.u_from.0).signum() * steps };
    let u = f.u;
    let cells = f.mascot.frame((now / 1000.0) as f32, phase, muted, if phase == Phase::Talking { claude_level } else { 0.0 }, me_level);
    let fit = (22.0 / (9.0 * u)).min(1.0);
    let shift = -2.0 * ((f.mascot.lift((now / 1000.0) as f32) as f64) * u * fit / 2.0).round();
    let box_h = 20.0 * u;
    let fade = f.mascot.fade();
    let codex = agent == "Codex";
    if codex {
        let n = f.null.frame((now / 1000.0) as f32, phase);
        if ui.get_null_frame() != n {
            ui.set_null_frame(n);
        }
        ui.set_null_fade(fade);
    }
    let pixels: Vec<Px> = if codex { &[][..] } else { &cells[..] }
        .iter()
        .map(|c| Px {
            x: (c.x as f64 * u) as f32,
            y: (c.y as f64 * 2.0 * u + shift) as f32,
            w: (c.w as f64 * u) as f32,
            h: (c.h as f64 * 2.0 * u) as f32,
            c: slint::Color::from_rgb_u8((c.c >> 16) as u8, (c.c >> 8) as u8, c.c as u8),
            o: c.o * fade,
        })
        .collect();
    if pixels != f.last_pixels {
        ui.set_pixels(ModelRc::new(VecModel::from(pixels.clone())));
        f.last_pixels = pixels;
    }
    ui.set_perch_x(bx.round() as f32);
    ui.set_mascot_h(box_h as f32);
    ui.set_perch_y((bottom - box_h).round() as f32);

    // ---- Everything else: set only what changed (an unchanged screen is never redrawn) ----
    let phase_name = format!("{phase:?}").to_lowercase();
    if ui.get_phase() != phase_name.as_str() {
        ui.set_phase(phase_name.into());
    }
    let key = time.split(" · ").next().unwrap_or_default().to_string();
    set_if(ui.get_time_key(), key, |t| ui.set_time_key(t));
    set_if(ui.get_time_text(), time, |t| ui.set_time_text(t));
    set_if(ui.get_status(), status, |t| ui.set_status(t));
    set_if(ui.get_agent(), agent, |t| ui.set_agent(t));
    let cap = f.cap.clone().unwrap();
    if ui.get_cap() != cap {
        ui.set_cap(cap);
    }
    let (btext, blit) = bubble.as_ref().map(|m| view::lit(m, None)).unwrap_or_default();
    set_if(ui.get_bubble_text(), btext, |t| ui.set_bubble_text(t));
    set_if(ui.get_bubble_lit(), blit, |t| ui.set_bubble_lit(t));
    if ui.get_has_bubble() != bubble.is_some() {
        ui.set_has_bubble(bubble.is_some());
    }
    set_if(ui.get_me_text(), f.held.me_text.clone().unwrap_or_default(), |t| ui.set_me_text(t));
    let show_me = thread || f.held.live || f.held.me_text.is_some();
    if ui.get_show_me() != show_me {
        ui.set_show_me(show_me);
    }
    if ui.get_me_live() != f.held.live {
        ui.set_me_live(f.held.live);
    }
    if ui.get_show_pending() != f.held.pending {
        ui.set_show_pending(f.held.pending);
    }
    if ui.get_muted() != muted {
        ui.set_muted(muted);
    }
    if ui.get_speaker() != speaker {
        ui.set_speaker(speaker);
    }
    if ui.get_can_finish() != can_finish {
        ui.set_can_finish(can_finish);
    }
    let old = ui.get_msgs();
    if old.row_count() != msgs.len() || old.iter().zip(&msgs).any(|(a, b)| a != *b) {
        ui.set_msgs(ModelRc::new(VecModel::from(msgs)));
    }
    let closed = ended && now - ended_at > CLOSE_MS;
    if ui.get_closed() != closed {
        ui.set_closed(closed);
    }
    if ended && now - ended_at > FINISH_MS && !f.finished {
        f.finished = true;
        android::release();
        android::finish();
    }

    // Levels and the clock only advance while something moves (the glow at rest is still).
    let moving = f.level > 0.01 || matches!(phase, Phase::Incoming | Phase::Thinking | Phase::Hold) || (thread && f.held.me_text.is_none());
    if moving {
        f.moving_until = now + 600.0;
    }
    if now < f.moving_until {
        ui.set_t((now / 1000.0) as f32);
    }
    if f.level > 0.01 {
        ui.set_glow_t((now / 1000.0) as f32);
    }
    let lv = (f.level * 100.0).round() / 100.0;
    if ui.get_level() != lv {
        ui.set_level(lv);
    }
    let ml = (me_level * 50.0).round() / 50.0;
    if ui.get_me_level() != ml {
        ui.set_me_level(ml);
    }
}

fn mascot_box(f: &Frame, now: f64) -> (f64, f64) {
    let k = ease_out((now - f.perch.2) / 500.0);
    (f.from.0 + (f.perch.0 - f.from.0) * k, f.from.1 + (f.perch.1 - f.from.1) * k)
}

fn set_if(cur: SharedString, new: String, set: impl FnOnce(SharedString)) {
    if cur.as_str() != new {
        set(new.into());
    }
}

pub fn run(app: android_activity::AndroidApp) {
    app.set_window_flags(android_activity::WindowManagerFlags::KEEP_SCREEN_ON, android_activity::WindowManagerFlags::empty());
    let (link, answer) = android::intent();
    let Some(link) = link else {
        android::finish();
        return;
    };
    let ui = CallScreen::new().expect("call screen");
    let e = Engine::new(&link);
    e.peek();
    ui.on_answer({
        let e = e.clone();
        move || e.answer()
    });
    ui.on_done_talking({
        let e = e.clone();
        move || e.done_talking()
    });
    // Ends here; closing the screen tells the PC (service::call_closed).
    ui.on_hang_up({
        let e = e.clone();
        move || e.hang_up()
    });
    ui.on_toggle_mute({
        let e = e.clone();
        move || e.toggle_mute()
    });
    ui.on_toggle_speaker({
        let e = e.clone();
        move || e.toggle_speaker()
    });
    ui.on_send({
        let e = e.clone();
        move |t| e.send_typed(&t)
    });
    let now = e.now();
    let frame = Rc::new(std::cell::RefCell::new(Frame {
        held: Held::default(),
        cap: None,
        mascot: Mascot::new((now / 1000.0) as f32),
        null: Null::default(),
        perch: (24.0, 108.0, -1e9),
        from: (24.0, 108.0),
        u: 4.0,
        u_from: (4.0, -1e9),
        thread_was: false,
        last_pixels: vec![],
        moving_until: 0.0,
        level: 0.0,
        finished: false,
        rate_set: false,
        last_route: -1e9,
    }));
    ui.on_poke({
        let (f, e) = (frame.clone(), e.clone());
        move || {
            let now = (e.now() / 1000.0) as f32;
            let mut f = f.borrow_mut();
            f.mascot.poke(now);
            f.null.poke(now);
        }
    });
    // 30 frames a second is plenty for pixel art and captions; properties that didn't change cost nothing.
    let timer = slint::Timer::default();
    timer.start(slint::TimerMode::Repeated, Duration::from_millis(33), {
        let (ui, e, f, app) = (ui.as_weak(), e.clone(), frame.clone(), app.clone());
        move || {
            if let Some(ui) = ui.upgrade() {
                present(&ui, &e, &mut f.borrow_mut(), &app);
            }
        }
    });
    // The keyboard, every frame while typing: the reply field rides up with it as it slides in.
    let keys = slint::Timer::default();
    keys.start(slint::TimerMode::Repeated, Duration::from_millis(16), {
        let ui = ui.as_weak();
        let mut had = false;
        move || {
            let Some(ui) = ui.upgrade() else { return };
            if !ui.get_typing() && ui.get_kb() == 0.0 {
                had = false;
                return;
            }
            let kb = android::keyboard();
            if ui.get_kb() != kb {
                ui.set_kb(kb);
            }
            // The keyboard was put away (Back, or swiped down): the reply field closes with it.
            if kb > 0.0 {
                had = true;
            } else if had && ui.get_typing() {
                had = false;
                ui.set_typing(false);
            }
        }
    });
    if answer {
        e.answer(); // opened from "Answer" on the ringing notification
    }
    let _ = ui.run();
    e.hang_up();
    android::release();
}
