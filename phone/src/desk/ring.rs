use crate::jx::{JNIEnv, JObject, Result};

pub static RINGS: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(0);

pub fn ring(_: &mut JNIEnv, _: &JObject, _: &str, _: &str) -> Result<()> {
    Ok(())
}

pub fn tick(_: &mut JNIEnv, _: &JObject) -> Result<()> {
    Ok(())
}
