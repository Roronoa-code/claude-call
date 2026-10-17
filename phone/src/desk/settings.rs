use crate::jx::{JNIEnv, JObject, Result};

pub const MODES: [&str; 6] = ["call", "text", "both", "none", "wake", "sleep"];

pub fn topic(_: &mut JNIEnv, _: &JObject) -> Result<String> {
    Ok(String::new())
}

pub fn chat_key(_: &mut JNIEnv, _: &JObject) -> Result<Option<String>> {
    Ok(None)
}

pub fn cursor(_: &mut JNIEnv, _: &JObject) -> Result<i64> {
    Ok(0)
}

pub fn set_cursor(_: &mut JNIEnv, _: &JObject, _: i64) -> Result<()> {
    Ok(())
}

pub fn mode(_: &mut JNIEnv, _: &JObject) -> Result<String> {
    Ok("both".into())
}

pub fn choose(_: &mut JNIEnv, _: &JObject, _: &str) -> Result<()> {
    Ok(())
}

pub fn received(_: &mut JNIEnv, _: &JObject, _: &str, _: i64) -> Result<()> {
    Ok(())
}

pub fn hinted(_: &mut JNIEnv, _: &JObject) -> Result<bool> {
    Ok(true)
}

pub fn set_hinted(_: &mut JNIEnv, _: &JObject) -> Result<()> {
    Ok(())
}

pub fn missing(_: &mut JNIEnv, _: &JObject) -> Result<String> {
    Ok(String::new())
}

pub fn setup_line(_: &mut JNIEnv, _: &JObject) -> Result<String> {
    Ok(String::new())
}

pub fn set_up(_: &mut JNIEnv, _: &JObject) -> Result<()> {
    Ok(())
}
