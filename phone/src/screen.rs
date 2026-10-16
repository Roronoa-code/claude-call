//! The call screen: draws the call (captions, the thread, controls) from the engine's view, ~30 times a second, and
//! whoever is calling, alive in 3D (caller.rs, live_gl.rs), every frame it moves.
use crate::{
    Call, Cap, CallScreen, Msg as UiMsg, alive, android,
    caller::Caller,
    engine::Engine,
    live_gl, stage,
    view::{self, Msg, Phase, View, Voice, clock},
};
use slint::{ComponentHandle, Model, ModelRc, SharedString, VecModel};
use std::{cell::RefCell, rc::Rc, sync::Arc, time::Duration};

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
    // what the caller's character follows (caller.rs), from the last frame
    phase: Phase,
    agent: String,
    perched: bool,
    me_level: f32,
    moving_until: f64,
    level: f32,
    finished: bool,
    rate_set: bool,
    last_route: f64,
    look: usize, // PREVIEW: debug.callme.call
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
        said: text.into(),
        ..Default::default()
    };
    let spoken = |m: &Msg, tone: &str| {
        let (text, lit) = view::lit(m, Some(part(m)));
        let (said, fresh, rest) = view::split_lit(&text, &lit);
        Cap {
            key: hash(&format!("c{}-{}", m.id, part(m))),
            label: v.agent.clone().into(),
            text: text.into(),
            lit: lit.into(),
            size: size(m),
            tone: tone.into(),
            orb: false,
            said: said.into(),
            fresh: fresh.into(),
            rest: rest.into(),
        }
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

fn present(ui: &CallScreen, e: &Arc<Engine>, f: &mut Frame, app: &android_activity::AndroidApp) {
    let now = e.now();
    if !f.rate_set {
        // Calls don't need 120 Hz: half the drawing work, a cooler phone.
        if let Some(w) = app.native_window() {
            android::set_frame_rate(w.ptr().as_ptr().cast(), 60.0);
            f.rate_set = true;
        }
    }
    // PREVIEW: the directions wait (hidden) for the real bar sizes, so nothing jumps as they arrive
    if !ui.global::<Call>().get_placed() {
        if let Some((top, bottom)) = android::insets() {
            ui.global::<Call>().set_sat(top);
            ui.global::<Call>().set_sab(bottom);
            ui.global::<Call>().set_placed(true);
        }
    }
    if now - f.last_route > 1000.0 {
        f.last_route = now;
        e.keep_route();
        if let Some((top, bottom)) = android::insets() {
            ui.global::<Call>().set_sat(top);
            ui.global::<Call>().set_sab(bottom);
        }
    }
    let claude_level = e.claude_level();
    let mut s = e.lock();
    e.line_progress(&mut s);
    // a typed reply that didn't go: back in the field, before anything typed since
    let unsent = std::mem::take(&mut s.view.unsent);
    if !unsent.is_empty() {
        let now_typed = ui.global::<Call>().get_draft();
        let back = if now_typed.is_empty() { unsent } else { format!("{unsent} {now_typed}") };
        ui.global::<Call>().set_draft(back.into());
        ui.global::<Call>().set_typing(true);
    }
    let ended = s.ended;
    let v = &s.view;
    // PREVIEW (captures only): `adb shell setprop debug.callme.hear "<words>"` shows those words on your turn as if you
    // were saying them (the test emulator's mic is silent). Only the screen sees them; nothing reaches the call.
    let hear = if v.phase == Phase::Listening { stage::prop_text(c"debug.callme.hear") } else { String::new() };
    let faked = !hear.is_empty();
    let vphase = if faked { Phase::Hearing } else { v.phase };
    let partial = if faked { hear } else { v.partial.clone() };
    let thread = matches!(vphase, Phase::Hearing | Phase::Thinking);
    let pending = vphase == Phase::Thinking && !v.transcribing;
    let last_claude = v.msgs.iter().rev().find(|m| !m.me).cloned();
    let last_me = v.msgs.iter().rev().find(|m| m.me).cloned();
    if thread || f.held.claude.is_none() {
        f.held = Held {
            me_text: if pending { last_me.map(|m| m.text) } else { Some(partial).filter(|p| !p.is_empty()) },
            live: vphase == Phase::Hearing,
            pending,
            claude: last_claude.clone(),
        };
    }
    let bubble = if thread { last_claude } else { f.held.claude.clone() };
    // The big caption is frozen while the thread shows, so it can fly back from where it was.
    if !thread || f.cap.is_none() {
        f.cap = Some(caption(v, now));
    }
    let me_level = if faked {
        (0.3 + 0.25 * (now * 0.011).sin() * (now * 0.0037).sin().abs()) as f32
    } else if v.voice == Voice::Me && !v.muted {
        v.mic_level
    } else {
        0.0
    };
    let level = if v.voice == Voice::Claude { claude_level } else { me_level };
    f.level += (level - f.level) * 0.35;
    let status = if !v.notice.is_empty() {
        v.notice.clone()
    } else if faked {
        "Hearing you…".to_string()
    } else if matches!(v.phase, Phase::Connecting | Phase::Talking | Phase::Listening | Phase::Hearing) || (v.phase == Phase::Thinking && !pending) {
        v.status_text().to_string()
    } else {
        String::new()
    };
    // Round 2 shows only what is out of the ordinary (a dropped line, a blocked or muted mic) as a notice.
    let notice = if !v.notice.is_empty() {
        v.notice.clone()
    } else if v.muted && v.phase == Phase::Listening {
        "Mic muted, type a reply".to_string()
    } else {
        String::new()
    };
    // The usage meter rides on the call clock: "On call · 2:31 · $0.04".
    let used = if v.usage.is_empty() { String::new() } else { format!(" · {}", v.usage) };
    let time = match v.phase {
        Phase::Incoming => "Calling you".to_string(),
        Phase::Connecting => "Connecting…".to_string(),
        Phase::Ended => format!("Call ended · {}{used}", clock(v.ended_at - v.call_start)),
        _ => format!("On call · {}{used}", clock(now - v.call_start)),
    };
    let phase = vphase;
    let agent = v.agent.clone();
    let muted = v.muted;
    let msgs: Vec<UiMsg> = v
        .msgs
        .iter()
        .map(|m| {
            if m.me {
                UiMsg { me: true, text: m.text.clone().into(), lit: m.text.clone().into(), said: m.text.clone().into(), ..Default::default() }
            } else {
                let (text, lit) = view::lit(m, None);
                let shown = if m.cut { lit.clone() } else { text };
                let (said, fresh, rest) = view::split_lit(&shown, &lit);
                UiMsg { me: false, text: shown.into(), lit: lit.into(), said: said.into(), fresh: fresh.into(), rest: rest.into() }
            }
        })
        .collect();
    let (speaker, quiet, can_finish, ended_at) = (v.speaker, v.quiet, v.can_finish || faked, v.ended_at);
    drop(s);

    // ---- The caller's character: big over the caption, small on the bubble while the thread shows ----
    f.perched = thread && bubble.is_some();
    (f.phase, f.me_level) = (phase, me_level);
    if f.agent != agent {
        f.agent = agent.clone();
    }

    // ---- Everything else: set only what changed (an unchanged screen is never redrawn) ----
    let phase_name = format!("{phase:?}").to_lowercase();
    if ui.global::<Call>().get_phase() != phase_name.as_str() {
        ui.global::<Call>().set_phase(phase_name.into());
    }
    let key = time.split(" · ").next().unwrap_or_default().to_string();
    set_if(ui.global::<Call>().get_time_key(), key, |t| ui.global::<Call>().set_time_key(t));
    set_if(ui.global::<Call>().get_time_text(), time, |t| ui.global::<Call>().set_time_text(t));
    set_if(ui.global::<Call>().get_status(), status, |t| ui.global::<Call>().set_status(t));
    set_if(ui.global::<Call>().get_notice(), notice, |t| ui.global::<Call>().set_notice(t));
    set_if(ui.global::<Call>().get_agent(), agent, |t| ui.global::<Call>().set_agent(t));
    let cap = f.cap.clone().unwrap();
    if ui.global::<Call>().get_cap() != cap {
        ui.global::<Call>().set_cap(cap);
    }
    let (btext, blit) = bubble.as_ref().map(|m| view::lit(m, None)).unwrap_or_default();
    set_if(ui.global::<Call>().get_bubble_text(), btext, |t| ui.global::<Call>().set_bubble_text(t));
    set_if(ui.global::<Call>().get_bubble_lit(), blit, |t| ui.global::<Call>().set_bubble_lit(t));
    if ui.global::<Call>().get_has_bubble() != bubble.is_some() {
        ui.global::<Call>().set_has_bubble(bubble.is_some());
    }
    set_if(ui.global::<Call>().get_me_text(), f.held.me_text.clone().unwrap_or_default(), |t| ui.global::<Call>().set_me_text(t));
    let show_me = thread || f.held.live || f.held.me_text.is_some();
    if ui.global::<Call>().get_show_me() != show_me {
        ui.global::<Call>().set_show_me(show_me);
    }
    if ui.global::<Call>().get_me_live() != f.held.live {
        ui.global::<Call>().set_me_live(f.held.live);
    }
    if ui.global::<Call>().get_show_pending() != f.held.pending {
        ui.global::<Call>().set_show_pending(f.held.pending);
    }
    if ui.global::<Call>().get_muted() != muted {
        ui.global::<Call>().set_muted(muted);
    }
    if ui.global::<Call>().get_speaker() != speaker {
        ui.global::<Call>().set_speaker(speaker);
    }
    if ui.global::<Call>().get_quiet() != quiet {
        ui.global::<Call>().set_quiet(quiet);
    }
    if ui.global::<Call>().get_can_finish() != can_finish {
        ui.global::<Call>().set_can_finish(can_finish);
    }
    let old = ui.global::<Call>().get_msgs();
    if old.row_count() != msgs.len() || old.iter().zip(&msgs).any(|(a, b)| a != *b) {
        ui.global::<Call>().set_msgs(ModelRc::new(VecModel::from(msgs)));
    }
    let closed = ended && now - ended_at > CLOSE_MS;
    if ui.global::<Call>().get_closed() != closed {
        ui.global::<Call>().set_closed(closed);
    }
    if ended && now - ended_at > FINISH_MS && !f.finished {
        f.finished = true;
        android::release();
        android::finish();
    }

    // Levels and the clock only advance while something moves (the glow at rest is still).
    let moving = (f.look > 0 && phase != Phase::Ended) || f.level > 0.01 || matches!(phase, Phase::Incoming | Phase::Thinking | Phase::Hold) || (thread && f.held.me_text.is_none());
    if moving {
        f.moving_until = now + 600.0;
    }
    if now < f.moving_until {
        ui.global::<Call>().set_t((now / 1000.0) as f32);
    }
    if f.level > 0.01 {
        ui.global::<Call>().set_glow_t((now / 1000.0) as f32);
    }
    let lv = (f.level * 100.0).round() / 100.0;
    if ui.global::<Call>().get_level() != lv {
        ui.global::<Call>().set_level(lv);
    }
    let ml = (me_level * 50.0).round() / 50.0;
    if ui.global::<Call>().get_me_level() != ml {
        ui.global::<Call>().set_me_level(ml);
    }
}

fn set_if(cur: SharedString, new: String, set: impl FnOnce(SharedString)) {
    if cur.as_str() != new {
        set(new.into());
    }
}

/// The call screen has something open over it that Back closes first (call.slint overlays).
pub static OVERLAYS: std::sync::atomic::AtomicBool = std::sync::atomic::AtomicBool::new(false);
/// Back asked for what is open over the call to close; the call's frame timer does it (not the event loop proxy,
/// which is the newest screen's: the home screen's, once it was opened during the call).
static CLOSE: std::sync::atomic::AtomicBool = std::sync::atomic::AtomicBool::new(false);

/// Back on the call screen (from Android's main thread): closes what is open over the call; false when nothing was.
pub fn back() -> bool {
    if !OVERLAYS.swap(false, std::sync::atomic::Ordering::Relaxed) {
        return false;
    }
    CLOSE.store(true, std::sync::atomic::Ordering::Relaxed);
    true
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
    ui.global::<Call>().on_answer({
        let e = e.clone();
        move || e.answer()
    });
    ui.global::<Call>().on_done_talking({
        let e = e.clone();
        move || e.done_talking()
    });
    // Ends here; closing the screen tells the PC (service::call_closed).
    ui.global::<Call>().on_hang_up({
        let e = e.clone();
        move || e.hang_up()
    });
    ui.global::<Call>().on_toggle_mute({
        let e = e.clone();
        move || e.toggle_mute()
    });
    ui.global::<Call>().on_toggle_speaker({
        let e = e.clone();
        move || e.toggle_speaker()
    });
    ui.global::<Call>().on_call_back({
        let e = e.clone();
        move || e.call_back(5)
    });
    ui.global::<Call>().on_toggle_quiet({
        let e = e.clone();
        move || e.toggle_quiet()
    });
    // what is on screen: the typed text with the keyboard's word still being put together, where the cursor is
    ui.global::<Call>().on_send({
        let e = e.clone();
        move |draft, word, at| e.send_typed(&crate::chats::composed(&draft, &word, at.max(0) as usize))
    });
    ui.global::<Call>().on_clear_field(|| {
        android::with(|env, act| env.call_method(act, "clearField", "()V", &[]).map(drop));
    });
    // Back closes the Transcript, the reply field or More before it leaves the call (service.rs call_back).
    ui.on_overlays(|on| OVERLAYS.store(on, std::sync::atomic::Ordering::Relaxed));
    let now = e.now();
    // Which call screen shows: debug.callme.call 4 (or unset): Camera, Mani's pick (look 8); 0: Comic, as shipped before
    // (look 3); 1-3: round 2's Subtitles, Script and Your light (looks 5-7).
    let pick = stage::prop_or(c"debug.callme.call", 4, 4);
    let look = if pick == 0 { 3 } else { 4 + pick };
    ui.global::<Call>().set_look(look as i32);
    let frame = Rc::new(std::cell::RefCell::new(Frame {
        held: Held::default(),
        cap: None,
        phase: Phase::Incoming,
        agent: String::new(),
        perched: false,
        me_level: 0.0,
        moving_until: 0.0,
        level: 0.0,
        finished: false,
        rate_set: false,
        last_route: -1e9,
        look,
    }));
    let start = std::time::Instant::now();
    let caller = Rc::new(RefCell::new(Caller::new(0x51ed_270b ^ now.to_bits(), look)));
    ui.global::<Call>().on_poke({
        let caller = caller.clone();
        move || caller.borrow_mut().poke(start.elapsed().as_secs_f32())
    });
    let scene = Rc::new(RefCell::new(live_gl::Scene::new(1)));
    live_gl::attach(&ui, scene.clone(), vec![Caller::size(look)], {
        let ui = ui.as_weak();
        move |images| {
            if let Some(ui) = ui.upgrade() {
                ui.global::<Call>().set_face_live(images[0].clone());
            }
        }
    });
    // The character, every frame: its springs and the voice's level need it; it is drawn only when due.
    let face_timer = slint::Timer::default();
    face_timer.start(slint::TimerMode::Repeated, Duration::from_millis(16), {
        let (ui, e, f) = (ui.as_weak(), e.clone(), frame.clone());
        let mut drawn = -1.0f32;
        let mut last = None;
        let mut mood = crate::caller::Mood::new(look >= 5);
        move || {
            let Some(ui) = ui.upgrade() else { return };
            // the keyboard and Back, as Android's main thread last said (android::kb, back)
            if ui.global::<Call>().get_kb() != android::kb() {
                ui.global::<Call>().set_kb(android::kb());
            }
            if CLOSE.swap(false, std::sync::atomic::Ordering::Relaxed) {
                ui.invoke_close_overlays();
            }
            let now = start.elapsed().as_secs_f32();
            let (phase, agent, perched, me) = {
                let f = f.borrow();
                (f.phase, f.agent.clone(), f.perched, f.me_level)
            };
            let voice = e.claude_level();
            let mut c = caller.borrow_mut();
            c.tick(now, &agent, phase, perched, voice, me);
            let mut s = scene.borrow_mut();
            if s.failed && ui.global::<Call>().get_face_gpu() {
                ui.global::<Call>().set_face_gpu(false); // a still picture instead
            }
            let p = c.place(now);
            let who = c.cast.who;
            let fade = (c.fade(now) * 100.0).round() / 100.0;
            if ui.global::<Call>().get_face() != who.face() {
                ui.global::<Call>().set_face(who.face().into());
            }
            if ui.global::<Call>().get_face_fade() != fade {
                ui.global::<Call>().set_face_fade(fade);
            }
            let moved = last != Some((p, who));
            if fade > 0.0 && !s.failed && (moved || drawn < 0.0 || alive::due(&c.cast, drawn, now)) {
                let sf = ui.window().scale_factor();
                let (res, mut view) = alive::view(who, alive::solo(who, p.k), sf);
                let mut frame = c.cast.uniforms();
                if look > 0 {
                    // PREVIEW: on the record (round 2: in the spotlight), lit by its lights, its shadow (and in the
                    // spotlight its reflection) on the floor drawn under the picture
                    let st = &stage::STAGES[if look >= 5 { 1 } else { 4 }];
                    let below = (st.below(who) * alive::solo(who, p.k) * sf).round() as i32;
                    view.1 += below;
                    frame.extend(st.uniforms(who, res.1, res.1 + below, sf, 1.0));
                    if look == 2 || look >= 5 {
                        mood.ease(phase, voice, now);
                        mood.apply(&mut frame);
                    }
                }
                s.slots[0] = live_gl::Slot { who, res, view, frame, fresh: true };
                drawn = now;
                last = Some((p, who));
                ui.global::<Call>().set_face_x(p.x);
                ui.global::<Call>().set_face_y(p.y);
                ui.global::<Call>().set_face_k(p.k);
                ui.global::<Call>().set_face_w(view.0);
                ui.global::<Call>().set_face_h(view.1);
                ui.global::<Call>().set_face_th(live_gl::texture_px(Caller::size(look), sf).1);
                ui.window().request_redraw();
            } else if fade == 0.0 {
                drawn = -1.0;
            }
        }
    });
    // 30 frames a second is plenty for captions; properties that didn't change cost nothing.
    let timer = slint::Timer::default();
    timer.start(slint::TimerMode::Repeated, Duration::from_millis(33), {
        let (ui, e, f, app) = (ui.as_weak(), e.clone(), frame.clone(), app.clone());
        move || {
            if let Some(ui) = ui.upgrade() {
                present(&ui, &e, &mut f.borrow_mut(), &app);
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
