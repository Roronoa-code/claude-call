use crate::jx::{JNIEnv, JObject, Result};
use std::{marker::PhantomData, path::PathBuf};

pub fn with<R>(f: impl FnOnce(&mut JNIEnv, &JObject) -> Result<R>) -> Option<R> {
    let (mut env, act) = (JNIEnv(PhantomData), JObject(PhantomData));
    crate::jx::ok(&mut env, |env| f(env, &act))
}

pub fn log(msg: &str) {
    eprintln!("{msg}");
}

pub fn home_shown() -> bool {
    true
}

pub fn kb() -> f32 {
    0.0
}

pub fn insets() -> Option<(f32, f32)> {
    Some((12.0, 16.0))
}

pub fn route(_: bool) {}
pub fn release() {}
pub fn call_began() {}
pub fn finish() {}
pub fn leave() {}
pub fn backable(_: bool) {}
pub fn rate(_: f32) {}
pub fn clear_field() {}
pub fn drawn() {}

pub fn keep_home(_: &mut JNIEnv, _: &JObject) -> Result<()> {
    Ok(())
}

pub fn focused(_: &mut JNIEnv) -> bool {
    true
}

pub fn vote_60() -> bool {
    true
}

pub fn density(window: &slint::Window) -> f32 {
    window.scale_factor()
}

pub fn files_dir(_: &mut JNIEnv, _: &JObject) -> Result<PathBuf> {
    let dir = PathBuf::from(r"B:\Projects\CallMe\desk-data");
    std::fs::create_dir_all(&dir).map_err(|e| e.to_string())?;
    Ok(dir)
}
