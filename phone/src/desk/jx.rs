use std::marker::PhantomData;

pub struct JNIEnv<'a>(pub PhantomData<&'a ()>);
pub struct JObject<'a>(pub PhantomData<&'a ()>);
pub type Result<T> = std::result::Result<T, String>;

pub fn ok<R>(env: &mut JNIEnv, f: impl FnOnce(&mut JNIEnv) -> Result<R>) -> Option<R> {
    f(env).map_err(|e| crate::android::log(&format!("jx: {e}"))).ok()
}

pub fn now_ms() -> i64 {
    std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).unwrap().as_millis() as i64
}

pub fn copy(_: &mut JNIEnv, _: &JObject, _: &str) -> Result<()> {
    Ok(())
}

pub fn open_link(_: &mut JNIEnv, _: &JObject, _: &str) -> Result<()> {
    Ok(())
}
