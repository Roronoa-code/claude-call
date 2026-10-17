use crate::jx::{JNIEnv, JObject, Result};

pub fn cancel(_: &mut JNIEnv, _: &JObject, _: &str) -> Result<()> {
    Ok(())
}

pub fn notify(_: &mut JNIEnv, _: &JObject, _: &str, _: &str, _: &str) -> Result<()> {
    Ok(())
}
