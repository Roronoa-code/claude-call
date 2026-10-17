use crate::jx::{JNIEnv, JObject, Result};

pub static RINGS: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(0);

pub fn ring(_: &mut JNIEnv, _: &JObject, link: &str, _: &str) -> Result<()> {
    RINGS.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
    crate::desk_app::ring(link.to_string());
    Ok(())
}

pub fn tick(_: &mut JNIEnv, _: &JObject) -> Result<()> {
    Ok(())
}
