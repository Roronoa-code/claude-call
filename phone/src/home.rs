//! The home screen: whether calls can reach this phone, how agents may reach you, a test ring, the widget and the
//! private channel. The Android parts (permissions, settings, widgets, ringing) are MainActivity.kt's methods.
use crate::{
    Home, HomeScreen, Px, android,
    mascot::{Mascot, Phase},
};
use slint::{ComponentHandle, ModelRc, VecModel};
use std::{
    cell::{Cell, RefCell},
    rc::Rc,
    time::{Duration, Instant},
};

fn refresh(ui: &HomeScreen) -> f32 {
    let home = ui.global::<Home>();
    let missing = android::call_str("missing");
    home.set_ready(missing.is_empty());
    home.set_missing(format!("Still needed: {missing}").into());
    home.set_mode(android::call_str("mode").into());
    home.set_topic(android::call_str("topic").into());
    home.set_can_pin(android::call_bool("canPinWidgets"));
    match android::insets() {
        Some((top, bottom, density)) => {
            home.set_sat((top / density).max(36.0));
            home.set_sab((bottom / density).max(24.0));
            density
        }
        None => 0.0,
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
            android::call_with_str("setMode", &m);
        }
    });
    home.on_set_up(|| android::call("setUp"));
    home.on_test_ring(|| android::call("testRing"));
    home.on_pin(|wide| android::call_with_bool("pinWidget", wide));
    home.on_save_topic(|t| android::call_with_str("saveTopic", &t));
    let editing = Rc::new(Cell::new(false));
    home.on_editing({
        let editing = editing.clone();
        move |on| editing.set(on)
    });
    let mut density = refresh(&ui);

    // Clawd idles on the home screen (asleep while it is quiet); only changed pixels are drawn.
    let start = Instant::now();
    let mascot = Rc::new(RefCell::new(Mascot::new(0.0)));
    let mut last: Vec<Px> = vec![];
    let mut last_refresh = Instant::now();
    let timer = slint::Timer::default();
    timer.start(slint::TimerMode::Repeated, Duration::from_millis(33), {
        let (ui, mascot) = (ui.as_weak(), mascot.clone());
        move || {
            let Some(ui) = ui.upgrade() else { return };
            let home = ui.global::<Home>();
            // Permissions granted in Settings, or a mode changed elsewhere, show up within a second.
            if last_refresh.elapsed() > Duration::from_secs(1) {
                last_refresh = Instant::now();
                let d = refresh(&ui);
                if d > 0.0 {
                    density = d;
                }
            }
            // The channel sheet rides above the keyboard.
            let kb = if editing.get() && density > 0.0 { (android::keyboard() / density).round() } else { 0.0 };
            if home.get_kb() != kb {
                home.set_kb(kb);
            }
            let phase = if home.get_mode() == "none" { Phase::Hold } else { Phase::Listening };
            let cells = mascot.borrow_mut().frame(start.elapsed().as_secs_f32(), phase, false, 0.0, 0.0);
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
