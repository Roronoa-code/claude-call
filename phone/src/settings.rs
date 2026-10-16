//! The app's settings (SharedPreferences "callme"): the private channel and the contact mode, which is shared with
//! the PC agents over ntfy; and what setup still needs the user's OK.
use crate::jx::{self, granted, s, service, PKG};
use jni::{
    JNIEnv,
    errors::Result,
    objects::{JObject, JValue},
};

/// The contact modes; "wake" and "sleep" are the two Sleep modes (ring until picked up / do not disturb).
pub const MODES: [&str; 6] = ["call", "text", "both", "none", "wake", "sleep"];

fn prefs<'a>(env: &mut JNIEnv<'a>, ctx: &JObject) -> Result<JObject<'a>> {
    let n = s(env, "callme")?;
    env.call_method(ctx, "getSharedPreferences", "(Ljava/lang/String;I)Landroid/content/SharedPreferences;", &[JValue::Object(&n), JValue::Int(0)])?.l()
}

fn get(env: &mut JNIEnv, ctx: &JObject, key: &str) -> Result<Option<String>> {
    let (p, k, none) = (prefs(env, ctx)?, s(env, key)?, JObject::null());
    let v = env.call_method(&p, "getString", "(Ljava/lang/String;Ljava/lang/String;)Ljava/lang/String;", &[JValue::Object(&k), JValue::Object(&none)])?.l()?;
    jx::string(env, v)
}

/// Writes string and long values in one edit.
fn put(env: &mut JNIEnv, ctx: &JObject, text: &[(&str, &str)], long: &[(&str, i64)]) -> Result<()> {
    let p = prefs(env, ctx)?;
    let e = env.call_method(&p, "edit", "()Landroid/content/SharedPreferences$Editor;", &[])?.l()?;
    for (key, v) in text {
        let (k, v) = (s(env, key)?, s(env, v)?);
        env.call_method(&e, "putString", "(Ljava/lang/String;Ljava/lang/String;)Landroid/content/SharedPreferences$Editor;", &[JValue::Object(&k), JValue::Object(&v)])?;
    }
    for (key, v) in long {
        let k = s(env, key)?;
        env.call_method(&e, "putLong", "(Ljava/lang/String;J)Landroid/content/SharedPreferences$Editor;", &[JValue::Object(&k), JValue::Long(*v)])?;
    }
    env.call_method(&e, "apply", "()V", &[])?;
    Ok(())
}

/// The private ntfy channel; the default comes from res/values/topic.xml (kept out of git: it is effectively a password).
pub fn topic(env: &mut JNIEnv, ctx: &JObject) -> Result<String> {
    if let Some(t) = get(env, ctx, "topic")? {
        return Ok(t);
    }
    let id = jx::res(env, ctx, "string", "ntfy_topic")?;
    let t = env.call_method(ctx, "getString", "(I)Ljava/lang/String;", &[JValue::Int(id)])?.l()?;
    Ok(jx::string(env, t)?.unwrap_or_default())
}

/// The key chat messages are signed with, from res/values/topic.xml; None until the PC has set it up.
pub fn chat_key(env: &mut JNIEnv, ctx: &JObject) -> Result<Option<String>> {
    let id = jx::res(env, ctx, "string", "chat_key")?;
    if id == 0 {
        return Ok(None);
    }
    let k = env.call_method(ctx, "getString", "(I)Ljava/lang/String;", &[JValue::Int(id)])?.l()?;
    Ok(jx::string(env, k)?.map(|k| k.trim().to_string()).filter(|k| !k.is_empty()))
}

/// The time (seconds) of the last ntfy message seen, so the listener catches up from there after being off.
pub fn cursor(env: &mut JNIEnv, ctx: &JObject) -> Result<i64> {
    let (p, k) = (prefs(env, ctx)?, s(env, "ntfyCursor")?);
    env.call_method(&p, "getLong", "(Ljava/lang/String;J)J", &[JValue::Object(&k), JValue::Long(0)])?.j()
}

pub fn set_cursor(env: &mut JNIEnv, ctx: &JObject, secs: i64) -> Result<()> {
    if secs > cursor(env, ctx)? {
        put(env, ctx, &[], &[("ntfyCursor", secs)])?;
    }
    Ok(())
}

pub fn mode(env: &mut JNIEnv, ctx: &JObject) -> Result<String> {
    Ok(get(env, ctx, "mode")?.unwrap_or_else(|| "both".into()))
}

pub fn mode_changed_at(env: &mut JNIEnv, ctx: &JObject) -> Result<i64> {
    let (p, k) = (prefs(env, ctx)?, s(env, "modeChangedAt")?);
    env.call_method(&p, "getLong", "(Ljava/lang/String;J)J", &[JValue::Object(&k), JValue::Long(0)])?.j()
}

/// Chosen on this phone (home screen or widget): store it, tell the PC agents, redraw the widgets.
pub fn choose(env: &mut JNIEnv, ctx: &JObject, m: &str) -> Result<()> {
    if !MODES.contains(&m) || mode(env, ctx)? == m {
        return Ok(());
    }
    put(env, ctx, &[("mode", m)], &[("modeChangedAt", jx::now_ms())])?;
    crate::net::publish_mode(topic(env, ctx)?, m.to_string());
    crate::widget::update_all(env, ctx)
}

/// Set elsewhere (Claude, Discord), arrived over ntfy: stored unless a newer change was made here.
pub fn received(env: &mut JNIEnv, ctx: &JObject, m: &str, at: i64) -> Result<()> {
    if !MODES.contains(&m) || at < mode_changed_at(env, ctx)? - 2_000 {
        return Ok(());
    }
    put(env, ctx, &[("mode", m)], &[("modeChangedAt", at)])?;
    crate::widget::update_all(env, ctx)
}

/// Whether the home screen's one-time hint (drag Null to change the mode) has played.
pub fn hinted(env: &mut JNIEnv, ctx: &JObject) -> Result<bool> {
    Ok(get(env, ctx, "faceHint")?.is_some())
}

pub fn set_hinted(env: &mut JNIEnv, ctx: &JObject) -> Result<()> {
    put(env, ctx, &[("faceHint", "1")], &[])
}

/// What still needs the user's OK, comma-separated ("" = all set).
pub fn missing(env: &mut JNIEnv, ctx: &JObject) -> Result<String> {
    let mut m = vec![];
    if !granted(env, ctx, "android.permission.POST_NOTIFICATIONS")? {
        m.push("Notifications");
    }
    if !granted(env, ctx, "android.permission.RECORD_AUDIO")? {
        m.push("Microphone");
    }
    let nm = service(env, ctx, "notification")?;
    if jx::sdk(env)? >= 34 && !env.call_method(&nm, "canUseFullScreenIntent", "()Z", &[])?.z()? {
        m.push("Full-screen calls");
    }
    let (pm, p) = (service(env, ctx, "power")?, s(env, PKG)?);
    if !env.call_method(&pm, "isIgnoringBatteryOptimizations", "(Ljava/lang/String;)Z", &[JValue::Object(&p)])?.z()? {
        m.push("Battery: unrestricted");
    }
    Ok(m.join(", "))
}

/// The permissions setup asks for in Android's own dialog, by the names `missing` gives them.
const ASKED: [(&str, &str); 2] = [("Notifications", "android.permission.POST_NOTIFICATIONS"), ("Microphone", "android.permission.RECORD_AUDIO")];

/// Those of them still missing that Android will no longer ask for (refused twice, or "Don't ask again"): only its
/// Settings page for the app can give them now. Before the first ask Android answers the same, so it counts only once
/// setup has asked.
fn blocked(env: &mut JNIEnv, act: &JObject, m: &str) -> Result<Vec<&'static str>> {
    let mut out = vec![];
    if get(env, act, "permsAsked")?.is_none() {
        return Ok(out);
    }
    for (name, p) in ASKED {
        let p = s(env, p)?;
        if m.contains(name) && !env.call_method(act, "shouldShowRequestPermissionRationale", "(Ljava/lang/String;)Z", &[JValue::Object(&p)])?.z()? {
            out.push(name);
        }
    }
    Ok(out)
}

/// The home screen's setup line: what a tap on it does.
pub fn setup_line(env: &mut JNIEnv, act: &JObject) -> Result<String> {
    let m = missing(env, act)?;
    Ok(match blocked(env, act, &m)?.as_slice() {
        [] => "Finish setting up",
        ["Notifications"] => "Allow notifications in Settings",
        ["Microphone"] => "Allow the microphone in Settings",
        _ => "Allow notifications and the microphone in Settings",
    }
    .into())
}

/// Walks through whatever is still missing, one screen at a time: Android's permission dialog (or, once it won't ask
/// again, the app's page in Settings), then the system screens.
pub fn set_up(env: &mut JNIEnv, act: &JObject) -> Result<()> {
    let m = missing(env, act)?;
    if !blocked(env, act, &m)?.is_empty() {
        let i = jx::view_intent(env, "android.settings.APPLICATION_DETAILS_SETTINGS", &format!("package:{PKG}"))?;
        return jx::start_activity(env, act, &i);
    }
    let ask: Vec<&str> = ASKED.iter().filter(|(name, _)| m.contains(name)).map(|(_, p)| *p).collect();
    if !ask.is_empty() {
        put(env, act, &[("permsAsked", "1")], &[])?;
        let perms = env.new_object_array(ask.len() as i32, "java/lang/String", JObject::null())?;
        for (i, p) in ask.iter().enumerate() {
            let p = s(env, p)?;
            env.set_object_array_element(&perms, i as i32, p)?;
        }
        env.call_method(act, "requestPermissions", "([Ljava/lang/String;I)V", &[JValue::Object(&perms), JValue::Int(1)])?;
        return Ok(());
    }
    set_up_rest(env, act)
}

/// After the permission dialog, whatever was answered: on to the system screens still needed. Never the dialog
/// again from here: a refusal brought it straight back, round and round.
pub fn set_up_rest(env: &mut JNIEnv, act: &JObject) -> Result<()> {
    let m = missing(env, act)?;
    let action = if m.contains("Full-screen calls") {
        "android.settings.MANAGE_APP_USE_FULL_SCREEN_INTENT"
    } else if m.contains("Battery") {
        "android.settings.REQUEST_IGNORE_BATTERY_OPTIMIZATIONS"
    } else {
        return Ok(());
    };
    let i = jx::view_intent(env, action, &format!("package:{PKG}"))?;
    jx::start_activity(env, act, &i)
}
