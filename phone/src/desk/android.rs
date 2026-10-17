use crate::jx::{JNIEnv, JObject, Result};
use std::{marker::PhantomData, path::PathBuf, sync::atomic::{AtomicBool, Ordering}};

pub static SHOWN: AtomicBool = AtomicBool::new(true);
pub static FOCUSED: AtomicBool = AtomicBool::new(true);

pub fn with<R>(f: impl FnOnce(&mut JNIEnv, &JObject) -> Result<R>) -> Option<R> {
    let (mut env, act) = (JNIEnv(PhantomData), JObject(PhantomData));
    crate::jx::ok(&mut env, |env| f(env, &act))
}

pub fn log(msg: &str) {
    eprintln!("{msg}");
}

pub fn home_shown() -> bool {
    SHOWN.load(Ordering::Relaxed)
}

pub fn kb() -> f32 {
    0.0
}

pub fn insets() -> Option<(f32, f32)> {
    Some((12.0, 16.0))
}

pub fn route(_: bool) {}
pub fn release() {}
pub fn call_began() {
    crate::desk_app::stop_ringing();
}

pub fn finish() {
    crate::desk_app::close_call();
}

pub fn leave() {
    crate::desk_app::minimize();
}
pub fn backable(_: bool) {}
pub fn rate(_: f32) {}
pub fn clear_field() {}
pub fn drawn() {}

pub fn keep_home(_: &mut JNIEnv, _: &JObject) -> Result<()> {
    Ok(())
}

pub fn focused(_: &mut JNIEnv) -> bool {
    FOCUSED.load(Ordering::Relaxed) && SHOWN.load(Ordering::Relaxed)
}

pub fn vote_60() -> bool {
    true
}

pub fn density(window: &slint::Window) -> f32 {
    window.scale_factor()
}

pub fn files_dir(_: &mut JNIEnv, _: &JObject) -> Result<PathBuf> {
    Ok(data_dir())
}

pub fn data_dir() -> PathBuf {
    let dir = std::env::var_os("CALLME_DESK_DATA").map(PathBuf::from).unwrap_or_else(|| PathBuf::from(std::env::var_os("LOCALAPPDATA").unwrap_or_default()).join("ClaudeCall"));
    let _ = std::fs::create_dir_all(&dir);
    dir
}
