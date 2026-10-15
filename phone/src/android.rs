//! The few Android framework calls the call screen needs, made from Rust through JNI: the call link it was opened
//! with, call audio routing (earpiece / loudspeaker), the screen switching off at the ear, the system bar and
//! keyboard insets, and closing the screen.
use jni::{
    JNIEnv, JavaVM,
    objects::{GlobalRef, JObject, JValue},
};
use crate::jx::service;
use std::{cell::OnceCell, sync::Mutex};

struct Ctx {
    vm: JavaVM,
    activity: GlobalRef,
}
thread_local! {
    /// This thread's screen. Each screen (home, call) runs on its own thread, and both can be open at once in the one
    /// process the call listener keeps alive, so the activity is per thread, not per process.
    static CTX: OnceCell<Ctx> = const { OnceCell::new() };
}
static WAKE: Mutex<Option<GlobalRef>> = Mutex::new(None);
/// The home screen is on screen (between onStart and onStop, Shell.kt): away, it draws and polls nothing.
static HOME_SHOWN: std::sync::atomic::AtomicBool = std::sync::atomic::AtomicBool::new(true);

pub fn home_shown() -> bool {
    HOME_SHOWN.load(std::sync::atomic::Ordering::Relaxed)
}

pub fn set_home_shown(on: bool) {
    HOME_SHOWN.store(on, std::sync::atomic::Ordering::Relaxed);
}

pub fn init(app: &android_activity::AndroidApp) {
    let vm = unsafe { JavaVM::from_raw(app.vm_as_ptr().cast()) }.expect("vm");
    let activity = {
        let env = vm.attach_current_thread().expect("attach");
        let obj = unsafe { JObject::from_raw(app.activity_as_ptr().cast()) };
        env.new_global_ref(obj).expect("activity ref")
    };
    CTX.with(|c| {
        let _ = c.set(Ctx { vm, activity });
    });
}

/// Runs `f` with a JNI env and this thread's activity; errors (and pending Java exceptions) are logged and dropped.
pub fn with<R>(f: impl FnOnce(&mut JNIEnv, &JObject) -> jni::errors::Result<R>) -> Option<R> {
    CTX.with(|c| {
        let ctx = c.get()?;
        let mut env = ctx.vm.attach_current_thread_permanently().ok()?;
        crate::jx::ok(&mut env, |env| f(env, ctx.activity.as_obj()))
    })
}

pub fn log(msg: &str) {
    use std::ffi::CString;
    let tag = CString::new("callme").unwrap();
    let text = CString::new(msg.replace('\0', "")).unwrap();
    unsafe { ndk_sys_log(4, tag.as_ptr(), text.as_ptr()) };
}
#[link(name = "log")]
unsafe extern "C" {
    #[link_name = "__android_log_write"]
    fn ndk_sys_log(prio: i32, tag: *const std::ffi::c_char, text: *const std::ffi::c_char) -> i32;
}

/// The call link and whether the user already tapped Answer on the notification.
pub fn intent() -> (Option<String>, bool) {
    with(|env, act| {
        let intent = env.call_method(act, "getIntent", "()Landroid/content/Intent;", &[])?.l()?;
        Ok((crate::jx::extra(env, &intent, "link")?, crate::jx::extra_bool(env, &intent, "answer")?))
    })
    .unwrap_or((None, false))
}

const EARPIECE: i32 = 1;
const SPEAKER: i32 = 2;

/// Call audio from now on (MODE_IN_COMMUNICATION), on the earpiece or the loudspeaker; the screen switches off
/// at the ear while on the earpiece. Safe to repeat: the system (or the mic opening) sometimes moves the route.
pub fn route(speaker: bool) {
    with(|env, act| {
        let am = service(env, act, "audio")?;
        if env.call_method(&am, "getMode", "()I", &[])?.i()? != 3 {
            env.call_method(&am, "setMode", "(I)V", &[JValue::Int(3)])?;
        }
        let want = if speaker { SPEAKER } else { EARPIECE };
        let cur = env.call_method(&am, "getCommunicationDevice", "()Landroid/media/AudioDeviceInfo;", &[])?.l()?;
        let cur_type = if cur.is_null() { -1 } else { env.call_method(&cur, "getType", "()I", &[])?.i()? };
        if cur_type != want {
            let list = env.call_method(&am, "getAvailableCommunicationDevices", "()Ljava/util/List;", &[])?.l()?;
            let n = env.call_method(&list, "size", "()I", &[])?.i()?;
            for i in 0..n {
                let d = env.call_method(&list, "get", "(I)Ljava/lang/Object;", &[JValue::Int(i)])?.l()?;
                if env.call_method(&d, "getType", "()I", &[])?.i()? == want {
                    env.call_method(&am, "setCommunicationDevice", "(Landroid/media/AudioDeviceInfo;)Z", &[JValue::Object(&d)])?;
                    break;
                }
            }
        }
        // The proximity sensor turns the screen off at the ear (earpiece only).
        let mut wake = WAKE.lock().unwrap();
        if wake.is_none() {
            let pm = service(env, act, "power")?;
            let tag = env.new_string("callme:ear")?;
            let w = env.call_method(&pm, "newWakeLock", "(ILjava/lang/String;)Landroid/os/PowerManager$WakeLock;", &[JValue::Int(32), JValue::Object(&tag)])?.l()?;
            *wake = Some(env.new_global_ref(w)?);
        }
        let w = wake.as_ref().unwrap();
        let held = env.call_method(w, "isHeld", "()Z", &[])?.z()?;
        if speaker && held {
            env.call_method(w, "release", "()V", &[])?;
        } else if !speaker && !held {
            env.call_method(w, "acquire", "()V", &[])?;
        }
        Ok(())
    });
}

/// Back to normal audio, screen on; the mic and earpiece are free again.
pub fn release() {
    with(|env, act| {
        if let Some(w) = WAKE.lock().unwrap().take() {
            if env.call_method(&w, "isHeld", "()Z", &[])?.z()? {
                env.call_method(&w, "release", "()V", &[])?;
            }
        }
        let am = service(env, act, "audio")?;
        env.call_method(&am, "clearCommunicationDevice", "()V", &[])?;
        env.call_method(&am, "setMode", "(I)V", &[JValue::Int(0)])?;
        Ok(())
    });
}

/// The window's insets of 	ypes (WindowInsets.Type bits) as (top, bottom) in dp; None until the window is attached.
fn window_insets(types: i32) -> Option<(f32, f32)> {
    with(|env, act| {
        let win = env.call_method(act, "getWindow", "()Landroid/view/Window;", &[])?.l()?;
        let decor = env.call_method(&win, "getDecorView", "()Landroid/view/View;", &[])?.l()?;
        let ins = env.call_method(&decor, "getRootWindowInsets", "()Landroid/view/WindowInsets;", &[])?.l()?;
        if ins.is_null() {
            return Ok(None);
        }
        let i = env.call_method(&ins, "getInsets", "(I)Landroid/graphics/Insets;", &[JValue::Int(types)])?.l()?;
        let (top, bottom) = (env.get_field(&i, "top", "I")?.i()? as f32, env.get_field(&i, "bottom", "I")?.i()? as f32);
        let d = crate::jx::density(env, act)?;
        Ok(Some((top / d, bottom / d)))
    })
    .flatten()
}

/// Room for the status bar and the gesture bar, in dp.
pub fn insets() -> Option<(f32, f32)> {
    window_insets(1 | 2).map(|(top, bottom)| (top.max(36.0), bottom.max(24.0))) // statusBars() | navigationBars()
}

/// How far the on-screen keyboard reaches up from the bottom edge, in whole dp (0 when hidden).
pub fn keyboard() -> f32 {
    window_insets(8).map_or(0.0, |(_, bottom)| bottom.round()) // Type.ime()
}
/// The call connected: stop ringing, show the ongoing-call notification.
pub fn call_began() {
    with(|env, act| crate::service::call_began(env, act));
}
pub fn finish() {
    with(|env, act| env.call_method(act, "finish", "()V", &[]).map(drop));
}

/// Asks for 60 frames a second (a call doesn't need 120): half the drawing work. Looked up at run time because it
/// lives in libandroid (Android 11+), not where the NDK bindings expect it.
pub fn set_frame_rate(window: *mut std::ffi::c_void, fps: f32) -> bool {
    use std::ffi::{c_char, c_void};
    unsafe extern "C" {
        fn dlopen(name: *const c_char, flags: i32) -> *mut c_void;
        fn dlsym(handle: *mut c_void, symbol: *const c_char) -> *mut c_void;
    }
    unsafe {
        let lib = dlopen(c"libandroid.so".as_ptr(), 1);
        let f = if lib.is_null() { std::ptr::null_mut() } else { dlsym(lib, c"ANativeWindow_setFrameRate".as_ptr()) };
        if f.is_null() {
            return false;
        }
        let f: extern "C" fn(*mut c_void, f32, i8) -> i32 = std::mem::transmute(f);
        f(window, fps, 0) == 0
    }
}

/// The activity's class: which screen this is (the call screen or the home screen).
pub fn activity_name() -> String {
    with(|env, act| {
        let class = env.call_method(act, "getClass", "()Ljava/lang/Class;", &[])?.l()?;
        let name = env.call_method(&class, "getName", "()Ljava/lang/String;", &[])?.l()?;
        Ok(env.get_string(&name.into())?.into())
    })
    .unwrap_or_default()
}
