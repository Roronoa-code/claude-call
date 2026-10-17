use crate::{
    HomeScreen,
    chats::Media,
    jx::{JNIEnv, JObject, Result},
};

pub fn open(_: &mut JNIEnv, _: &JObject, _: &str) -> Result<()> {
    Ok(())
}

pub fn failed(_: &str) -> bool {
    false
}

pub fn clear(_: &str) {}

pub fn show(_: &HomeScreen, _: &str, _: Option<&Media>) {}

pub fn picture(_: &str, _: bool, _: i32) -> Option<(slint::Image, f32)> {
    None
}

pub fn play(_: &mut JNIEnv, _: &JObject, _: &str) -> Result<()> {
    Ok(())
}

pub fn thumb(_: &Media) -> Option<(slint::Image, f32)> {
    None
}
