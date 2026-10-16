//! Small JNI helpers for the Android parts written in Rust. App classes are named by string (setClassName), never
//! looked up with FindClass: threads started from Rust only see the system's classes.
use jni::{
    JNIEnv,
    errors::Result,
    objects::{JObject, JString, JValue},
};

pub const PKG: &str = "fyi.mani.callme";
pub const FLAGS: i32 = 0x0400_0000 | 0x0800_0000; // PendingIntent.FLAG_IMMUTABLE | FLAG_UPDATE_CURRENT

pub fn s<'a>(env: &mut JNIEnv<'a>, text: &str) -> Result<JObject<'a>> {
    Ok(env.new_string(text)?.into())
}

/// A Java String (or null) as a Rust string.
pub fn string(env: &mut JNIEnv, o: JObject) -> Result<Option<String>> {
    if o.is_null() {
        return Ok(None);
    }
    Ok(Some(env.get_string(&JString::from(o))?.into()))
}

/// Runs `f`; a failure (and its pending Java exception) is logged and dropped.
pub fn ok<R>(env: &mut JNIEnv, f: impl FnOnce(&mut JNIEnv) -> Result<R>) -> Option<R> {
    match f(env) {
        Ok(r) => Some(r),
        Err(e) => {
            if env.exception_check().unwrap_or(false) {
                let _ = env.exception_describe();
                let _ = env.exception_clear();
            }
            crate::android::log(&format!("jni: {e}"));
            None
        }
    }
}

/// A method call whose result isn't needed (builder setters, RemoteViews calls).
pub fn call(env: &mut JNIEnv, o: &JObject, method: &str, sig: &str, args: &[JValue]) -> Result<()> {
    env.call_method(o, method, sig, args)?;
    Ok(())
}

pub fn sdk(env: &mut JNIEnv) -> Result<i32> {
    env.get_static_field("android/os/Build$VERSION", "SDK_INT", "I")?.i()
}

pub fn service<'a>(env: &mut JNIEnv<'a>, ctx: &JObject, name: &str) -> Result<JObject<'a>> {
    let n = s(env, name)?;
    env.call_method(ctx, "getSystemService", "(Ljava/lang/String;)Ljava/lang/Object;", &[JValue::Object(&n)])?.l()
}

pub fn app<'a>(env: &mut JNIEnv<'a>, ctx: &JObject) -> Result<JObject<'a>> {
    env.call_method(ctx, "getApplicationContext", "()Landroid/content/Context;", &[])?.l()
}

/// An Intent for one of the app's own classes, e.g. "CallActivity".
pub fn intent<'a>(env: &mut JNIEnv<'a>, ctx: &JObject, class: &str) -> Result<JObject<'a>> {
    let i = env.new_object("android/content/Intent", "()V", &[])?;
    let c = s(env, &format!("{PKG}.{class}"))?;
    env.call_method(&i, "setClassName", "(Landroid/content/Context;Ljava/lang/String;)Landroid/content/Intent;", &[JValue::Object(ctx), JValue::Object(&c)])?;
    Ok(i)
}

pub fn put(env: &mut JNIEnv, i: &JObject, key: &str, value: JValue) -> Result<()> {
    let k = s(env, key)?;
    let sig = match value {
        JValue::Bool(_) => "(Ljava/lang/String;Z)Landroid/content/Intent;",
        _ => "(Ljava/lang/String;Ljava/lang/String;)Landroid/content/Intent;",
    };
    env.call_method(i, "putExtra", sig, &[JValue::Object(&k), value])?;
    Ok(())
}

pub fn extra(env: &mut JNIEnv, i: &JObject, key: &str) -> Result<Option<String>> {
    let k = s(env, key)?;
    let v = env.call_method(i, "getStringExtra", "(Ljava/lang/String;)Ljava/lang/String;", &[JValue::Object(&k)])?.l()?;
    string(env, v)
}

pub fn extra_bool(env: &mut JNIEnv, i: &JObject, key: &str) -> Result<bool> {
    let k = s(env, key)?;
    env.call_method(i, "getBooleanExtra", "(Ljava/lang/String;Z)Z", &[JValue::Object(&k), JValue::Bool(0)])?.z()
}

/// PendingIntent.getActivity / getBroadcast.
pub fn pending<'a>(env: &mut JNIEnv<'a>, ctx: &JObject, kind: &str, code: i32, i: &JObject) -> Result<JObject<'a>> {
    let sig = "(Landroid/content/Context;ILandroid/content/Intent;I)Landroid/app/PendingIntent;";
    env.call_static_method("android/app/PendingIntent", kind, sig, &[JValue::Object(ctx), JValue::Int(code), JValue::Object(i), JValue::Int(FLAGS)])?.l()
}

/// Intent(action, Uri.parse(uri)).
pub fn view_intent<'a>(env: &mut JNIEnv<'a>, action: &str, uri: &str) -> Result<JObject<'a>> {
    let (a, u) = (s(env, action)?, s(env, uri)?);
    let uri = env.call_static_method("android/net/Uri", "parse", "(Ljava/lang/String;)Landroid/net/Uri;", &[JValue::Object(&u)])?.l()?;
    env.new_object("android/content/Intent", "(Ljava/lang/String;Landroid/net/Uri;)V", &[JValue::Object(&a), JValue::Object(&uri)])
}

pub fn start_activity(env: &mut JNIEnv, ctx: &JObject, i: &JObject) -> Result<()> {
    env.call_method(ctx, "startActivity", "(Landroid/content/Intent;)V", &[JValue::Object(i)])?;
    Ok(())
}

pub fn granted(env: &mut JNIEnv, ctx: &JObject, permission: &str) -> Result<bool> {
    let p = s(env, permission)?;
    Ok(env.call_method(ctx, "checkSelfPermission", "(Ljava/lang/String;)I", &[JValue::Object(&p)])?.i()? == 0)
}

/// A resource id by name, e.g. ("id", "tile_call").
pub fn res(env: &mut JNIEnv, ctx: &JObject, kind: &str, name: &str) -> Result<i32> {
    let r = env.call_method(ctx, "getResources", "()Landroid/content/res/Resources;", &[])?.l()?;
    let (n, k, p) = (s(env, name)?, s(env, kind)?, s(env, PKG)?);
    let sig = "(Ljava/lang/String;Ljava/lang/String;Ljava/lang/String;)I";
    env.call_method(&r, "getIdentifier", sig, &[JValue::Object(&n), JValue::Object(&k), JValue::Object(&p)])?.i()
}

pub fn density(env: &mut JNIEnv, ctx: &JObject) -> Result<f32> {
    let r = env.call_method(ctx, "getResources", "()Landroid/content/res/Resources;", &[])?.l()?;
    let dm = env.call_method(&r, "getDisplayMetrics", "()Landroid/util/DisplayMetrics;", &[])?.l()?;
    env.get_field(&dm, "density", "F")?.f()
}

pub fn now_ms() -> i64 {
    std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).unwrap().as_millis() as i64
}

/// Puts `text` on the clipboard (plain text).
pub fn copy(env: &mut JNIEnv, ctx: &JObject, text: &str) -> Result<()> {
    let cm = service(env, ctx, "clipboard")?;
    let (label, t) = (s(env, "Claude Call")?, s(env, text)?);
    let sig = "(Ljava/lang/CharSequence;Ljava/lang/CharSequence;)Landroid/content/ClipData;";
    let clip = env.call_static_method("android/content/ClipData", "newPlainText", sig, &[JValue::Object(&label), JValue::Object(&t)])?.l()?;
    env.call_method(&cm, "setPrimaryClip", "(Landroid/content/ClipData;)V", &[JValue::Object(&clip)])?;
    Ok(())
}
