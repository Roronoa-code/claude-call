//! The app's settings (SharedPreferences "callme"): the private channel and the contact mode, which is shared with
//! the PC agents over ntfy; and what setup still needs the user's OK.
use crate::jx::{self, granted, s, service, PKG};
use jni::{
    JNIEnv,
    errors::Result,
    objects::{JObject, JValue},
};

pub const MODES: [&str; 4] = ["call", "text", "both", "none"];

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

pub fn set_topic(env: &mut JNIEnv, ctx: &JObject, topic: &str) -> Result<()> {
    put(env, ctx, &[("topic", topic.trim())], &[])
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

/// Walks through whatever is still missing, one system screen at a time.
pub fn set_up(env: &mut JNIEnv, act: &JObject) -> Result<()> {
    let m = missing(env, act)?;
    if m.contains("Notifications") || m.contains("Microphone") {
        let perms = env.new_object_array(2, "java/lang/String", JObject::null())?;
        for (i, p) in ["android.permission.POST_NOTIFICATIONS", "android.permission.RECORD_AUDIO"].iter().enumerate() {
            let p = s(env, p)?;
            env.set_object_array_element(&perms, i as i32, p)?;
        }
        env.call_method(act, "requestPermissions", "([Ljava/lang/String;I)V", &[JValue::Object(&perms), JValue::Int(1)])?;
        return Ok(());
    }
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

fn widgets<'a>(env: &mut JNIEnv<'a>, ctx: &JObject) -> Result<JObject<'a>> {
    env.call_static_method("android/appwidget/AppWidgetManager", "getInstance", "(Landroid/content/Context;)Landroid/appwidget/AppWidgetManager;", &[JValue::Object(ctx)])?.l()
}

pub fn can_pin(env: &mut JNIEnv, ctx: &JObject) -> Result<bool> {
    let w = widgets(env, ctx)?;
    env.call_method(&w, "isRequestPinAppWidgetSupported", "()Z", &[])?.z()
}

/// Asks the launcher to place the widget (2x2, or 4x2 when `wide`).
pub fn pin(env: &mut JNIEnv, ctx: &JObject, wide: bool) -> Result<()> {
    let (w, cn) = (widgets(env, ctx)?, crate::widget::component(env, ctx, if wide { "ModeWidgetWide" } else { "ModeWidget" })?);
    let none = JObject::null();
    let sig = "(Landroid/content/ComponentName;Landroid/os/Bundle;Landroid/app/PendingIntent;)Z";
    env.call_method(&w, "requestPinAppWidget", sig, &[JValue::Object(&cn), JValue::Object(&none), JValue::Object(&none)])?;
    Ok(())
}
