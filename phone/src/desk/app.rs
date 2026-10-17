use crate::{HomeScreen, android, screen};
use slint::winit_030::{EventResult, WinitWindowAccessor, winit::event::{ElementState, MouseButton, WindowEvent}, winit::keyboard::{Key, NamedKey}};
use slint::{BackendSelector, CloseRequestResponse, ComponentHandle, LogicalSize, PhysicalPosition};
use std::{cell::RefCell, rc::Rc, sync::atomic::Ordering};

thread_local! {
    static HOME: RefCell<Option<slint::Weak<HomeScreen>>> = const { RefCell::new(None) };
    static CALL: RefCell<Option<screen::Open>> = const { RefCell::new(None) };
}

const SIZE: LogicalSize = LogicalSize { width: 412.0, height: 892.0 };

pub fn desktop_main() {
    let select = |gl: fn(BackendSelector) -> BackendSelector| gl(BackendSelector::new().backend_name("winit".into()).renderer_name("skia-opengl".into())).select();
    if select(|b| b.require_opengl_es_with_version(3, 0)).is_err() {
        select(|b| b.require_opengl_with_version(3, 3)).expect("no OpenGL 3.3 or ES 3.0");
    }
    if crate::desk_os::already_running() {
        return;
    }
    let home = crate::home::open();
    let w = home.ui.window();
    place(w, "home");
    HOME.with(|h| *h.borrow_mut() = Some(home.ui.as_weak()));
    w.on_close_requested(|| {
        minimize();
        CloseRequestResponse::KeepWindowShown
    });
    w.on_winit_window_event(|w, e| {
        match e {
            WindowEvent::Occluded(o) => android::SHOWN.store(!o, Ordering::Relaxed),
            WindowEvent::Focused(f) => {
                android::FOCUSED.store(*f, Ordering::Relaxed);
                if *f {
                    android::SHOWN.store(true, Ordering::Relaxed);
                }
            }
            WindowEvent::Moved(_) | WindowEvent::Resized(_) => remember(w, "home"),
            WindowEvent::KeyboardInput { event, .. } if event.state == ElementState::Pressed && event.logical_key == Key::Named(NamedKey::Escape) => back(),
            WindowEvent::MouseInput { state: ElementState::Pressed, button: MouseButton::Back, .. } => back(),
            _ => {}
        }
        EventResult::Propagate
    });
    crate::desk_os::start_at_login();
    crate::service::start();
    let _ = home.ui.run();
}

fn back() {
    crate::home::back_gesture(2, 1.0);
}

pub fn minimize() {
    HOME.with(|h| {
        if let Some(ui) = h.borrow().as_ref().and_then(|w| w.upgrade()) {
            ui.window().set_minimized(true);
            android::SHOWN.store(false, Ordering::Relaxed);
        }
    });
}

pub fn raise_home() {
    HOME.with(|h| {
        if let Some(ui) = h.borrow().as_ref().and_then(|w| w.upgrade()) {
            ui.window().set_minimized(false);
            let _ = ui.show();
            ui.window().with_winit_window(|w| w.focus_window());
            android::SHOWN.store(true, Ordering::Relaxed);
        }
    });
}

fn place(w: &slint::Window, key: &str) {
    w.set_size(SIZE);
    if let Some(p) = crate::settings::get(key).and_then(|v| Some((v["x"].as_i64()? as i32, v["y"].as_i64()? as i32))) {
        if crate::desk_os::on_screen(p.0, p.1) {
            w.set_position(PhysicalPosition::new(p.0, p.1));
        }
    }
}

fn remember(w: &slint::Window, key: &str) {
    if w.is_minimized() {
        return;
    }
    let p = w.position();
    crate::settings::put(&[(key, serde_json::json!({ "x": p.x, "y": p.y }))]);
}

pub fn ring(link: String) {
    let _ = slint::invoke_from_event_loop(move || {
        if CALL.with(|c| c.borrow().is_some()) {
            return;
        }
        let call = screen::open(&local(&link), false);
        let w = call.ui.window();
        place(w, "call");
        w.on_close_requested(|| {
            hang_up();
            CloseRequestResponse::KeepWindowShown
        });
        w.on_winit_window_event(|w, e| {
            if matches!(e, WindowEvent::Moved(_)) {
                remember(w, "call");
            }
            EventResult::Propagate
        });
        let _ = call.ui.show();
        w.with_winit_window(|w| {
            w.focus_window();
            crate::desk_os::flash(w);
        });
        CALL.with(|c| *c.borrow_mut() = Some(call));
        crate::desk_os::ringtone(true);
        crate::service::watch(link);
    });
}

fn hang_up() {
    CALL.with(|c| {
        if let Some(call) = c.borrow().as_ref() {
            call.engine.hang_up();
        }
    });
    close_call();
}

pub fn close_call() {
    crate::desk_os::ringtone(false);
    if let Some(call) = CALL.with(|c| c.borrow_mut().take()) {
        let _ = call.ui.hide();
    }
}

pub fn stop_ringing() {
    crate::desk_os::ringtone(false);
}

pub fn ringing_link() -> Option<String> {
    crate::service::active_link()
}

fn local(link: &str) -> String {
    let near = std::net::TcpStream::connect_timeout(&([127, 0, 0, 1], 47809).into(), std::time::Duration::from_millis(200)).is_ok();
    match link.split_once("/?") {
        Some((_, q)) if near => format!("http://127.0.0.1:47809/?{q}"),
        _ => link.to_string(),
    }
}

pub fn with_home(f: impl FnOnce(&HomeScreen)) {
    HOME.with(|h| {
        if let Some(ui) = h.borrow().as_ref().and_then(|w| w.upgrade()) {
            f(&ui);
        }
    });
}

#[allow(dead_code)]
fn _keep(_: Rc<()>) {}
