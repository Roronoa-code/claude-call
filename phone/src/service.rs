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

static ACTIVE: Mutex<Option<(String, i64)>> = Mutex::new(None);
static SCREEN: Mutex<Option<(GlobalRef, String, bool)>> = Mutex::new(None);
static RUNNING: AtomicU64 = AtomicU64::new(u64::MAX);

const SPECIAL_USE: i32 = 0x4000_0000;
const MICROPHONE: i32 = 0x80;
const READY_ID: i32 = 1;
const CALL_ID: i32 = 3;

pub fn active_link() -> Option<String> {
    ACTIVE.lock().unwrap().as_ref().map(|a| a.0.clone())
}

pub fn start(env: &mut JNIEnv, ctx: &JObject) -> Result<()> {
    let i = jx::intent(env, ctx, "ListenService")?;
    env.call_method(ctx, "startForegroundService", "(Landroid/content/Intent;)Landroid/content/ComponentName;", &[JValue::Object(&i)])?;
    Ok(())
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

fn can_promote(env: &mut JNIEnv) -> bool {
    // Older Androids lack these fields: each miss leaves a JNI exception that must be cleared.
    let mut field = |class: &str, name: &str| {
        let v = env.get_static_field(class, name, "I").and_then(|v| v.i());
        if env.exception_check().unwrap_or(false) {
            let _ = env.exception_clear();
        }
        v
    };
    let full = field("android/os/Build$VERSION", "SDK_INT_FULL");
    let baklava_1 = field("android/os/Build$VERSION_CODES_FULL", "BAKLAVA_1");
    matches!((full, baklava_1), (Ok(f), Ok(b)) if f >= b)
}

fn notification<'a>(env: &mut JNIEnv<'a>, svc: &JObject) -> Result<(JObject<'a>, i32)> {
    let nm = service(env, svc, "notification")?;
    ring::channel(env, &nm, "listening", "Waiting for calls", 1, false)?;
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
    ring::channel(env, &nm, "ongoing_calls", "Ongoing Claude call", 2, false)?;
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
        // Some Samsung builds leave CallStyle out of their Live Update styles.
        let big = env.new_object("android/app/Notification$BigTextStyle", "()V", &[])?;
        let t = s(env, "Tap to return to the call")?;
        env.call_method(&big, "bigText", "(Ljava/lang/CharSequence;)Landroid/app/Notification$BigTextStyle;", &[JValue::Object(&t)])?;
        jx::call(env, &b, "setStyle", style_sig, &[JValue::Object(&big)])?;
        let a = end_action(env, &end)?;
        let actions = env.new_object_array(1, "android/app/Notification$Action", &a)?;
        jx::call(env, &b, "setActions", "([Landroid/app/Notification$Action;)Landroid/app/Notification$Builder;", &[JValue::Object(&actions)])?;
        n = env.call_method(&b, "build", "()Landroid/app/Notification;", &[])?.l()?;
    }
    if sdk >= 30 && jx::granted(env, svc, "android.permission.RECORD_AUDIO")? {
        types |= MICROPHONE;
    }
    Ok((n, types))
}

pub fn foreground(env: &mut JNIEnv, svc: &JObject) -> Result<()> {
    // Android 16 keeps a promoted call card when the same id is updated to a plain notification.
    let (n, types) = notification(env, svc)?;
    let (id, other) = if active_link().is_some() { (CALL_ID, READY_ID) } else { (READY_ID, CALL_ID) };
    env.call_method(svc, "startForeground", "(ILandroid/app/Notification;I)V", &[JValue::Int(id), JValue::Object(&n), JValue::Int(types)])?;
    let nm = service(env, svc, "notification")?;
    env.call_method(&nm, "cancel", "(I)V", &[JValue::Int(other)])?;
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

pub fn call_opened(env: &mut JNIEnv, act: &JObject, intent: &JObject) -> Result<()> {
    let link = jx::extra(env, intent, "link")?.unwrap_or_default();
    if link == ring::TEST_LINK {
        let (t, len) = (s(env, "✓ Test call worked")?, 1);
        let sig = "(Landroid/content/Context;Ljava/lang/CharSequence;I)Landroid/widget/Toast;";
        let toast = env.call_static_method("android/widget/Toast", "makeText", sig, &[JValue::Object(act), JValue::Object(&t), JValue::Int(len)])?.l()?;
        env.call_method(&toast, "show", "()V", &[])?;
        ring::stop(env, act, &link, true)?;
        env.call_method(act, "finish", "()V", &[])?;
        return Ok(());
    }
    {
        let mut sc = SCREEN.lock().unwrap();
        let answered = sc.as_ref().is_some_and(|s| s.1 == link && s.2);
        *sc = Some((env.new_global_ref(act)?, link.clone(), answered));
    }
    if jx::extra_bool(env, intent, "answer")? {
        ring::stop(env, act, &link, true)?;
    }
    Ok(())
}

pub fn screen_touched(env: &mut JNIEnv, act: &JObject) -> Result<()> {
    let link = SCREEN.lock().unwrap().as_ref().map(|s| s.1.clone());
    match link {
        Some(l) => ring::stop(env, act, &l, true),
        None => Ok(()),
    }
}

pub fn call_began(env: &mut JNIEnv, act: &JObject) -> Result<()> {
    let link = {
        let mut sc = SCREEN.lock().unwrap();
        sc.as_mut().map(|s| {
            s.2 = true;
            s.1.clone()
        })
    };
    match link {
        Some(l) => {
            ring::stop(env, act, &l, true)?;
            call_started(env, act, &l)
        }
        None => Ok(()),
    }
}

pub fn call_back(env: &mut JNIEnv, act: &JObject) -> Result<()> {
    if crate::screen::back() {
        return Ok(());
    }
    let answered = SCREEN.lock().unwrap().as_ref().is_some_and(|s| s.2);
    if answered {
        env.call_method(act, "moveTaskToBack", "(Z)Z", &[JValue::Bool(1)])?;
    } else {
        env.call_method(act, "finish", "()V", &[])?;
    }
    Ok(())
}

pub fn call_closed(env: &mut JNIEnv, act: &JObject) -> Result<()> {
    let link = {
        let mut sc = SCREEN.lock().unwrap();
        match sc.as_ref() {
            Some(s) if env.is_same_object(s.0.as_obj(), act)? => sc.take().map(|s| s.1),
            _ => None,
        }
    };
    if let Some(l) = &link {
        ring::stop(env, act, l, true)?;
    }
    if let Some(link) = link.filter(|l| l != ring::TEST_LINK) {
        net::hang_up(&link);
        call_ended(env, act, &link)?;
    }
    Ok(())
}

pub fn declined(env: &mut JNIEnv, ctx: &JObject, intent: &JObject) -> Result<()> {
    let Some(link) = jx::extra(env, intent, "link")? else { return Ok(()) };
    ring::stop(env, ctx, &link, true)?;
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

/// Android resumes the home screen before the call screen is gone.
fn live_link(env: &mut JNIEnv) -> Result<Option<String>> {
    let Some(link) = active_link() else { return Ok(None) };
    let screen = SCREEN.lock().unwrap().as_ref().filter(|s| s.1 == link).map(|s| s.0.clone());
    if let Some(act) = screen {
        if env.call_method(act.as_obj(), "isFinishing", "()Z", &[])?.z()? {
            return Ok(None);
        }
    }
    Ok(Some(link))
}

pub fn home_opened(env: &mut JNIEnv, act: &JObject) -> Result<()> {
    let intent = env.call_method(act, "getIntent", "()Landroid/content/Intent;", &[])?.l()?;
    if let Some(chat) = jx::extra(env, &intent, "chat")? {
        *crate::inbox::OPEN_CHAT.lock().unwrap() = Some(chat);
        let k = s(env, "chat")?;
        env.call_method(&intent, "removeExtra", "(Ljava/lang/String;)V", &[JValue::Object(&k)])?;
    }
    let Some(link) = live_link(env)? else { return Ok(()) };
    let i = ring::call_intent(env, act, &link, true)?;
    jx::start_activity(env, act, &i)?;
    env.call_method(act, "finish", "()V", &[])?;
    Ok(())
}

pub fn home_resumed(env: &mut JNIEnv, act: &JObject) -> Result<()> {
    if live_link(env)?.is_some() {
        return home_opened(env, act);
    }
    start(env, act)?;
    crate::widget::update_all(env, act)?;
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

pub fn permissions_answered(env: &mut JNIEnv, act: &JObject) -> Result<()> {
    settings::answered(env, act)?;
    start(env, act)?;
    settings::set_up_rest(env, act)
}
