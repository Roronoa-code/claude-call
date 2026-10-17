use jni::{
    JNIEnv, JavaVM,
    objects::{GlobalRef, JObject, JValue},
};
use crate::jx::service;
use std::{cell::OnceCell, sync::Mutex};

struct Ctx {
    vm: JavaVM,
    activity: GlobalRef,
    app: android_activity::AndroidApp,
}
thread_local! {
    static CTX: OnceCell<Ctx> = const { OnceCell::new() };
}
static WAKE: Mutex<Option<GlobalRef>> = Mutex::new(None);
static HOME_SHOWN: std::sync::atomic::AtomicBool = std::sync::atomic::AtomicBool::new(true);

pub fn home_shown() -> bool {
    HOME_SHOWN.load(std::sync::atomic::Ordering::Relaxed)
}

pub fn set_home_shown(on: bool) {
    HOME_SHOWN.store(on, std::sync::atomic::Ordering::Relaxed);
}

/// Slint's event loop proxy is one per process (the newest screen's), so each screen's frame timer reads this instead.
static KB: std::sync::atomic::AtomicU32 = std::sync::atomic::AtomicU32::new(0);

pub fn kb() -> f32 {
    f32::from_bits(KB.load(std::sync::atomic::Ordering::Relaxed))
}

pub fn set_kb(dp: f32) {
    KB.store(dp.to_bits(), std::sync::atomic::Ordering::Relaxed);
}

pub fn init(app: &android_activity::AndroidApp) {
    let vm = unsafe { JavaVM::from_raw(app.vm_as_ptr().cast()) }.expect("vm");
    let activity = {
        let env = vm.attach_current_thread().expect("attach");
        let obj = unsafe { JObject::from_raw(app.activity_as_ptr().cast()) };
        env.new_global_ref(obj).expect("activity ref")
    };
    CTX.with(|c| {
        let _ = c.set(Ctx { vm, activity, app: app.clone() });
    });
}

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

pub fn intent() -> (Option<String>, bool) {
    with(|env, act| {
        let intent = env.call_method(act, "getIntent", "()Landroid/content/Intent;", &[])?.l()?;
        Ok((crate::jx::extra(env, &intent, "link")?, crate::jx::extra_bool(env, &intent, "answer")?))
    })
    .unwrap_or((None, false))
}

const EARPIECE: i32 = 1;
const SPEAKER: i32 = 2;

/// Safe to repeat: the system (or the mic opening) sometimes moves the audio route.
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

pub fn insets() -> Option<(f32, f32)> {
    window_insets(crate::jx::INSETS_STATUS_BARS | crate::jx::INSETS_NAVIGATION_BARS).map(|(top, bottom)| (top.max(36.0), bottom.max(24.0)))
}

pub fn call_began() {
    with(|env, act| crate::service::call_began(env, act));
}
pub fn finish() {
    with(|env, act| env.call_method(act, "finish", "()V", &[]).map(drop));
}

pub fn drawn() {
    with(|env, act| {
        if env.get_field(act, "waiting", "Z")?.z()? {
            env.call_method(act, "drawn", "()V", &[])?;
        }
        Ok(())
    });
}

pub fn leave() {
    with(|env, act| env.call_method(act, "moveTaskToBack", "(Z)Z", &[JValue::Bool(1)]).map(drop));
}

pub fn backable(on: bool) {
    with(|env, act| env.call_method(act, "backable", "(Z)V", &[JValue::Bool(on as u8)]).map(drop));
}

pub fn rate(fps: f32) {
    with(|env, act| env.call_method(act, "rate", "(F)V", &[fps.into()]).map(drop));
}

pub fn clear_field() {
    with(|env, act| env.call_method(act, "clearField", "()V", &[]).map(drop));
}

static HOME: Mutex<Option<GlobalRef>> = Mutex::new(None);

pub fn keep_home(env: &mut JNIEnv, act: &JObject) -> jni::errors::Result<()> {
    *HOME.lock().unwrap() = Some(env.new_global_ref(act)?);
    Ok(())
}

pub fn focused(env: &mut JNIEnv) -> bool {
    let home = HOME.lock().unwrap().clone();
    home.is_some_and(|h| env.call_method(&h, "hasWindowFocus", "()Z", &[]).and_then(|v| v.z()).unwrap_or(false))
}

pub fn files_dir(env: &mut JNIEnv, ctx: &JObject) -> jni::errors::Result<std::path::PathBuf> {
    let dir = env.call_method(ctx, "getFilesDir", "()Ljava/io/File;", &[])?.l()?;
    let dir = env.call_method(&dir, "getAbsolutePath", "()Ljava/lang/String;", &[])?.l()?;
    Ok(std::path::PathBuf::from(crate::jx::string(env, dir)?.unwrap_or_default()))
}

pub fn density(_: &slint::Window) -> f32 {
    CTX.with(|c| c.get().and_then(|c| c.app.config().density())).map_or(1.0, |dpi| dpi as f32 / 160.0)
}

/// True once the native window exists and has been asked for 60 fps.
pub fn vote_60() -> bool {
    CTX.with(|c| c.get().and_then(|c| c.app.native_window())).is_some_and(|w| {
        set_frame_rate(w.ptr().as_ptr().cast(), 60.0);
        true
    })
}

/// `ANativeWindow_setFrameRate` (libandroid, Android 11+) is missing from the ndk bindings, so it is looked up at run time.
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

pub fn activity_name() -> String {
    with(|env, act| {
        let class = env.call_method(act, "getClass", "()Ljava/lang/Class;", &[])?.l()?;
        let name = env.call_method(&class, "getName", "()Ljava/lang/String;", &[])?.l()?;
        Ok(env.get_string(&name.into())?.into())
    })
    .unwrap_or_default()
}
