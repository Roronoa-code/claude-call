//! The few Android framework calls the call screen needs, made from Rust through JNI: the call link it was opened
//! with, call audio routing (earpiece / loudspeaker), the screen switching off at the ear, the system bar insets,
//! and closing the screen.
use jni::{
    JNIEnv, JavaVM,
    objects::{GlobalRef, JObject, JValue},
};
use std::sync::{Mutex, OnceLock};

struct Ctx {
    vm: JavaVM,
    activity: GlobalRef,
}
static CTX: OnceLock<Ctx> = OnceLock::new();
static WAKE: Mutex<Option<GlobalRef>> = Mutex::new(None);

pub fn init(app: &android_activity::AndroidApp) {
    let vm = unsafe { JavaVM::from_raw(app.vm_as_ptr().cast()) }.expect("vm");
    let activity = {
        let env = vm.attach_current_thread().expect("attach");
        let obj = unsafe { JObject::from_raw(app.activity_as_ptr().cast()) };
        env.new_global_ref(obj).expect("activity ref")
    };
    let _ = CTX.set(Ctx { vm, activity });
}

/// Runs `f` with a JNI env on this thread; errors (and pending Java exceptions) are logged and dropped.
fn with<R>(f: impl FnOnce(&mut JNIEnv, &JObject) -> jni::errors::Result<R>) -> Option<R> {
    let ctx = CTX.get()?;
    let mut env = ctx.vm.attach_current_thread_permanently().ok()?;
    match f(&mut env, ctx.activity.as_obj()) {
        Ok(r) => Some(r),
        Err(e) => {
            if env.exception_check().unwrap_or(false) {
                let _ = env.exception_describe();
                let _ = env.exception_clear();
            }
            log(&format!("jni: {e}"));
            None
        }
    }
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
        let key = env.new_string("link")?;
        let link = env.call_method(&intent, "getStringExtra", "(Ljava/lang/String;)Ljava/lang/String;", &[JValue::Object(&key)])?.l()?;
        let link = if link.is_null() { None } else { Some(env.get_string(&link.into())?.into()) };
        let key = env.new_string("answer")?;
        let answer = env.call_method(&intent, "getBooleanExtra", "(Ljava/lang/String;Z)Z", &[JValue::Object(&key), JValue::Bool(0)])?.z()?;
        Ok((link, answer))
    })
    .unwrap_or((None, false))
}

fn service<'a>(env: &mut JNIEnv<'a>, act: &JObject, name: &str) -> jni::errors::Result<JObject<'a>> {
    let n = env.new_string(name)?;
    env.call_method(act, "getSystemService", "(Ljava/lang/String;)Ljava/lang/Object;", &[JValue::Object(&n)])?.l()
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

/// Status bar and gesture bar heights, in px (None until the window is attached).
pub fn insets() -> Option<(f32, f32, f32)> {
    with(|env, act| {
        let win = env.call_method(act, "getWindow", "()Landroid/view/Window;", &[])?.l()?;
        let decor = env.call_method(&win, "getDecorView", "()Landroid/view/View;", &[])?.l()?;
        let ins = env.call_method(&decor, "getRootWindowInsets", "()Landroid/view/WindowInsets;", &[])?.l()?;
        if ins.is_null() {
            return Ok(None);
        }
        let types = 1 | 2; // WindowInsets.Type.statusBars() | navigationBars()
        let i = env.call_method(&ins, "getInsets", "(I)Landroid/graphics/Insets;", &[JValue::Int(types)])?.l()?;
        let top = env.get_field(&i, "top", "I")?.i()? as f32;
        let bottom = env.get_field(&i, "bottom", "I")?.i()? as f32;
        let res = env.call_method(act, "getResources", "()Landroid/content/res/Resources;", &[])?.l()?;
        let dm = env.call_method(&res, "getDisplayMetrics", "()Landroid/util/DisplayMetrics;", &[])?.l()?;
        let density = env.get_field(&dm, "density", "F")?.f()?;
        Ok(Some((top, bottom, density)))
    })
    .flatten()
}

/// The app's own call hooks (CallActivity.kt): the ongoing-call notification, and closing the screen.
pub fn call_began() {
    with(|env, act| env.call_method(act, "callBegan", "()V", &[]).map(drop));
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

/// How far the on-screen keyboard reaches up from the bottom edge, in px (0 when hidden).
pub fn keyboard() -> f32 {
    with(|env, act| {
        let win = env.call_method(act, "getWindow", "()Landroid/view/Window;", &[])?.l()?;
        let decor = env.call_method(&win, "getDecorView", "()Landroid/view/View;", &[])?.l()?;
        let ins = env.call_method(&decor, "getRootWindowInsets", "()Landroid/view/WindowInsets;", &[])?.l()?;
        if ins.is_null() {
            return Ok(0.0);
        }
        let i = env.call_method(&ins, "getInsets", "(I)Landroid/graphics/Insets;", &[JValue::Int(8)])?.l()?; // Type.ime()
        Ok(env.get_field(&i, "bottom", "I")?.i()? as f32)
    })
    .unwrap_or(0.0)
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

// The home screen's Android calls (MainActivity.kt).
pub fn call(method: &str) {
    with(|env, act| env.call_method(act, method, "()V", &[]).map(drop));
}
pub fn call_str(method: &str) -> String {
    with(|env, act| {
        let s = env.call_method(act, method, "()Ljava/lang/String;", &[])?.l()?;
        Ok(if s.is_null() { String::new() } else { env.get_string(&s.into())?.into() })
    })
    .unwrap_or_default()
}
pub fn call_with_str(method: &str, arg: &str) {
    with(|env, act| {
        let a = env.new_string(arg)?;
        env.call_method(act, method, "(Ljava/lang/String;)V", &[JValue::Object(&a)]).map(drop)
    });
}
pub fn call_bool(method: &str) -> bool {
    with(|env, act| env.call_method(act, method, "()Z", &[])?.z()).unwrap_or(false)
}
pub fn call_with_bool(method: &str, arg: bool) {
    with(|env, act| env.call_method(act, method, "(Z)V", &[JValue::Bool(arg as u8)]).map(drop));
}
