//! The always-on part (ListenService): its notification (waiting for calls, or the ongoing call), the call listener
//! thread, and the call screen's life: answered, Back, Decline, closed.
use crate::{
    jx::{self, s, service},
    net, ring, settings,
};
use jni::{
    JNIEnv,
    errors::Result,
    objects::{GlobalRef, JObject, JValue},
};
use std::sync::{
    Mutex,
    atomic::{AtomicU64, Ordering},
};

/// The answered call: its link and when it started (ms).
static ACTIVE: Mutex<Option<(String, i64)>> = Mutex::new(None);
/// The open call screen, its link, and whether it was answered.
static SCREEN: Mutex<Option<(GlobalRef, String, bool)>> = Mutex::new(None);
/// Which listener generation is running (net::GEN says which one should be).
static RUNNING: AtomicU64 = AtomicU64::new(u64::MAX);

const SPECIAL_USE: i32 = 0x4000_0000; // ServiceInfo.FOREGROUND_SERVICE_TYPE_SPECIAL_USE
const MICROPHONE: i32 = 0x80;

pub fn active_link() -> Option<String> {
    ACTIVE.lock().unwrap().as_ref().map(|a| a.0.clone())
}

/// Starts (or refreshes) ListenService.
pub fn start(env: &mut JNIEnv, ctx: &JObject) -> Result<()> {
    let i = jx::intent(env, ctx, "ListenService")?;
    env.call_method(ctx, "startForegroundService", "(Landroid/content/Intent;)Landroid/content/ComponentName;", &[JValue::Object(&i)])?;
    Ok(())
}

/// A new channel: the listener reconnects on it.
pub fn restart(env: &mut JNIEnv, ctx: &JObject) -> Result<()> {
    let i = jx::intent(env, ctx, "ListenService")?;
    env.call_method(ctx, "stopService", "(Landroid/content/Intent;)Z", &[JValue::Object(&i)])?;
    start(env, ctx)
}

fn end_action<'a>(env: &mut JNIEnv<'a>, end: &JObject) -> Result<JObject<'a>> {
    let (none, t) = (JObject::null(), s(env, "End call")?);
    let ab = env.new_object(
        "android/app/Notification$Action$Builder",
        "(Landroid/graphics/drawable/Icon;Ljava/lang/CharSequence;Landroid/app/PendingIntent;)V",
        &[JValue::Object(&none), JValue::Object(&t), JValue::Object(end)],
    )?;
    env.call_method(&ab, "build", "()Landroid/app/Notification$Action;", &[])?.l()
}

/// Android 16 QPR1+ shows an ongoing call as a Live Update; earlier versions don't have the call.
fn can_promote(env: &mut JNIEnv) -> bool {
    let full = env.get_static_field("android/os/Build$VERSION", "SDK_INT_FULL", "I").and_then(|v| v.i());
    let baklava_1 = env.get_static_field("android/os/Build$VERSION_CODES_FULL", "BAKLAVA_1", "I").and_then(|v| v.i());
    if env.exception_check().unwrap_or(false) {
        let _ = env.exception_clear();
    }
    matches!((full, baklava_1), (Ok(f), Ok(b)) if f >= b)
}

fn notification<'a>(env: &mut JNIEnv<'a>, svc: &JObject) -> Result<(JObject<'a>, i32)> {
    let nm = service(env, svc, "notification")?;
    ring::channel(env, &nm, "listening", "Waiting for calls", 1, false)?; // IMPORTANCE_MIN
    let sdk = jx::sdk(env)?;
    let mut types = if sdk >= 34 { SPECIAL_USE } else { 0 };
    let active = ACTIVE.lock().unwrap().clone();
    let Some((link, at)) = active else {
        let b = ring::builder(env, svc, "listening")?;
        ring::set_text(env, &b, "setContentTitle", "Ready for Claude's calls")?;
        let home = jx::intent(env, svc, "MainActivity")?;
        let open = jx::pending(env, svc, "getActivity", 0, &home)?;
        jx::call(env, &b, "setContentIntent", "(Landroid/app/PendingIntent;)Landroid/app/Notification$Builder;", &[JValue::Object(&open)])?;
        return Ok((env.call_method(&b, "build", "()Landroid/app/Notification;", &[])?.l()?, types));
    };
    ring::channel(env, &nm, "ongoing_calls", "Ongoing Claude call", 2, false)?; // IMPORTANCE_LOW
    let open = ring::call_screen(env, svc, &link, true, 4)?;
    let end = ring::hang_up(env, svc, &link, 5)?;
    let b = ring::builder(env, svc, "ongoing_calls")?;
    ring::set_text(env, &b, "setContentTitle", "On call with Claude")?;
    ring::set_text(env, &b, "setContentText", "Tap to return to the call")?;
    jx::call(env, &b, "setContentIntent", "(Landroid/app/PendingIntent;)Landroid/app/Notification$Builder;", &[JValue::Object(&open)])?;
    let cat = s(env, "call")?;
    jx::call(env, &b, "setCategory", "(Ljava/lang/String;)Landroid/app/Notification$Builder;", &[JValue::Object(&cat)])?;
    jx::call(env, &b, "setOnlyAlertOnce", "(Z)Landroid/app/Notification$Builder;", &[JValue::Bool(1)])?;
    jx::call(env, &b, "setWhen", "(J)Landroid/app/Notification$Builder;", &[JValue::Long(at)])?;
    jx::call(env, &b, "setShowWhen", "(Z)Landroid/app/Notification$Builder;", &[JValue::Bool(1)])?;
    jx::call(env, &b, "setUsesChronometer", "(Z)Landroid/app/Notification$Builder;", &[JValue::Bool(1)])?;
    let style_sig = "(Landroid/app/Notification$Style;)Landroid/app/Notification$Builder;";
    if sdk >= 31 {
        let who = ring::person(env, "Claude")?;
        let sig = "(Landroid/app/Person;Landroid/app/PendingIntent;)Landroid/app/Notification$CallStyle;";
        let style = env.call_static_method("android/app/Notification$CallStyle", "forOngoingCall", sig, &[JValue::Object(&who), JValue::Object(&end)])?.l()?;
        jx::call(env, &b, "setStyle", style_sig, &[JValue::Object(&style)])?;
    } else {
        let a = end_action(env, &end)?;
        jx::call(env, &b, "addAction", "(Landroid/app/Notification$Action;)Landroid/app/Notification$Builder;", &[JValue::Object(&a)])?;
    }
    let promote = can_promote(env);
    if promote {
        jx::call(env, &b, "setRequestPromotedOngoing", "(Z)Landroid/app/Notification$Builder;", &[JValue::Bool(1)])?;
    }
    let mut n = env.call_method(&b, "build", "()Landroid/app/Notification;", &[])?.l()?;
    if promote && !env.call_method(&n, "hasPromotableCharacteristics", "()Z", &[])?.z()? {
        // Some Samsung builds leave CallStyle out of their Live Update styles: plain text and an End button instead.
        let big = env.new_object("android/app/Notification$BigTextStyle", "()V", &[])?;
        let t = s(env, "Tap to return to the call")?;
        env.call_method(&big, "bigText", "(Ljava/lang/CharSequence;)Landroid/app/Notification$BigTextStyle;", &[JValue::Object(&t)])?;
        jx::call(env, &b, "setStyle", style_sig, &[JValue::Object(&big)])?;
        let a = end_action(env, &end)?;
        let actions = env.new_object_array(1, "android/app/Notification$Action", &a)?;
        jx::call(env, &b, "setActions", "([Landroid/app/Notification$Action;)Landroid/app/Notification$Builder;", &[JValue::Object(&actions)])?;
        n = env.call_method(&b, "build", "()Landroid/app/Notification;", &[])?.l()?;
    }
    // Only an answered call asks for the microphone; setup and boot stay listener-only.
    if sdk >= 30 && jx::granted(env, svc, "android.permission.RECORD_AUDIO")? {
        types |= MICROPHONE;
    }
    Ok((n, types))
}

/// ListenService.onStartCommand: the notification, and the listener if it isn't running.
pub fn foreground(env: &mut JNIEnv, svc: &JObject) -> Result<()> {
    let (n, types) = notification(env, svc)?;
    env.call_method(svc, "startForeground", "(ILandroid/app/Notification;I)V", &[JValue::Int(1), JValue::Object(&n), JValue::Int(types)])?;
    let generation = net::GEN.load(Ordering::Relaxed);
    if RUNNING.swap(generation, Ordering::Relaxed) != generation {
        let (vm, ctx) = (env.get_java_vm()?, jx::app(env, svc)?);
        let ctx = env.new_global_ref(ctx)?;
        let topic = settings::topic(env, svc)?;
        std::thread::Builder::new().name("ntfy".into()).spawn(move || {
            if let Ok(mut env) = vm.attach_current_thread() {
                net::listen(&mut env, ctx.as_obj(), topic, generation);
            }
        }).ok();
    }
    Ok(())
}

pub fn call_started(env: &mut JNIEnv, ctx: &JObject, link: &str) -> Result<()> {
    {
        let mut a = ACTIVE.lock().unwrap();
        if a.as_ref().is_none_or(|a| a.0 != link) {
            *a = Some((link.to_string(), jx::now_ms()));
        }
    }
    start(env, ctx)
}

pub fn call_ended(env: &mut JNIEnv, ctx: &JObject, link: &str) -> Result<()> {
    {
        let mut a = ACTIVE.lock().unwrap();
        if a.as_ref().is_none_or(|a| a.0 != link) {
            return Ok(());
        }
        *a = None;
    }
    start(env, ctx)
}

/// The call screen opened (or was brought back): the test ring says it worked; Answer stops the ringing.
pub fn call_opened(env: &mut JNIEnv, act: &JObject, intent: &JObject) -> Result<()> {
    let link = jx::extra(env, intent, "link")?.unwrap_or_default();
    if link == ring::TEST_LINK {
        let (t, len) = (s(env, "✓ Test call worked")?, 1); // Toast.LENGTH_LONG
        let sig = "(Landroid/content/Context;Ljava/lang/CharSequence;I)Landroid/widget/Toast;";
        let toast = env.call_static_method("android/widget/Toast", "makeText", sig, &[JValue::Object(act), JValue::Object(&t), JValue::Int(len)])?.l()?;
        env.call_method(&toast, "show", "()V", &[])?;
        ring::stop(env, act, true)?;
        env.call_method(act, "finish", "()V", &[])?;
        return Ok(());
    }
    {
        let mut sc = SCREEN.lock().unwrap();
        let answered = sc.as_ref().is_some_and(|s| s.1 == link && s.2);
        *sc = Some((env.new_global_ref(act)?, link, answered));
    }
    if jx::extra_bool(env, intent, "answer")? {
        ring::stop(env, act, true)?;
    }
    Ok(())
}

/// The call connected (from the call screen's engine): quiet, and the ongoing-call notification.
pub fn call_began(env: &mut JNIEnv, act: &JObject) -> Result<()> {
    ring::stop(env, act, true)?;
    let link = {
        let mut sc = SCREEN.lock().unwrap();
        sc.as_mut().map(|s| {
            s.2 = true;
            s.1.clone()
        })
    };
    match link {
        Some(l) => call_started(env, act, &l),
        None => Ok(()),
    }
}

/// Back: a live call keeps going (the notification brings it back); before answering, Back closes it.
pub fn call_back(env: &mut JNIEnv, act: &JObject) -> Result<()> {
    let answered = SCREEN.lock().unwrap().as_ref().is_some_and(|s| s.2);
    if answered {
        env.call_method(act, "moveTaskToBack", "(Z)Z", &[JValue::Bool(1)])?;
    } else {
        env.call_method(act, "finish", "()V", &[])?;
    }
    Ok(())
}

/// The call screen closed: that hangs up.
pub fn call_closed(env: &mut JNIEnv, act: &JObject) -> Result<()> {
    ring::stop(env, act, true)?;
    let link = {
        let mut sc = SCREEN.lock().unwrap();
        match sc.as_ref() {
            Some(s) if env.is_same_object(s.0.as_obj(), act)? => sc.take().map(|s| s.1),
            _ => None,
        }
    };
    if let Some(link) = link.filter(|l| l != ring::TEST_LINK) {
        net::hang_up(&link);
        call_ended(env, act, &link)?;
    }
    Ok(())
}

/// Decline / End call from a notification: close the call screen if it's open (that hangs up), else hang up here.
pub fn declined(env: &mut JNIEnv, ctx: &JObject, intent: &JObject) -> Result<()> {
    ring::stop(env, ctx, true)?;
    let Some(link) = jx::extra(env, intent, "link")? else { return Ok(()) };
    let open = SCREEN.lock().unwrap().as_ref().filter(|s| s.1 == link).map(|s| s.0.clone());
    if let Some(act) = open {
        if !env.call_method(act.as_obj(), "isFinishing", "()Z", &[])?.z()? {
            env.call_method(act.as_obj(), "finish", "()V", &[])?;
            return Ok(());
        }
    }
    call_ended(env, ctx, &link)?;
    net::hang_up(&link);
    Ok(())
}

/// The home screen opened: during a call the app icon brings the call back instead.
pub fn home_opened(env: &mut JNIEnv, act: &JObject) -> Result<()> {
    let Some(link) = active_link() else { return Ok(()) };
    let i = ring::call_intent(env, act, &link, true)?;
    jx::start_activity(env, act, &i)?;
    env.call_method(act, "finish", "()V", &[])?;
    Ok(())
}

/// The home screen came to the front (the app icon brings an open one back without a new intent): during a call that
/// means the call; otherwise listen if setup is done, and pick up a mode changed elsewhere.
pub fn home_resumed(env: &mut JNIEnv, act: &JObject) -> Result<()> {
    if active_link().is_some() {
        return home_opened(env, act);
    }
    if settings::missing(env, act)?.is_empty() {
        start(env, act)?;
    }
    let (vm, ctx, topic) = (env.get_java_vm()?, jx::app(env, act)?, settings::topic(env, act)?);
    let ctx = env.new_global_ref(ctx)?;
    std::thread::spawn(move || {
        let Some((m, at)) = net::latest_mode(&topic) else { return };
        if let Ok(mut env) = vm.attach_current_thread() {
            jx::ok(&mut env, |env| settings::received(env, ctx.as_obj(), &m, at));
        }
    });
    Ok(())
}

/// After the permission dialog: keep going through setup, or start listening.
pub fn permissions_answered(env: &mut JNIEnv, act: &JObject) -> Result<()> {
    if settings::missing(env, act)?.is_empty() { start(env, act) } else { settings::set_up(env, act) }
}
