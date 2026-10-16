use jni::{
    JNIEnv,
    errors::Result,
    objects::{GlobalRef, JObject, JValue},
};
use crate::jx::{intent, pending, put, s, service};
use std::{collections::HashSet, sync::Mutex, time::Duration};

const CHANNEL: &str = "calls";
pub const RING_ID: i32 = 2;
const RING_FOR: Duration = Duration::from_secs(60);
pub const TEST_LINK: &str = "test:ring";

#[derive(Default)]
struct State {
    current: Option<String>,
    handled: HashSet<String>,
    ringtone: Option<GlobalRef>,
    vibrator: Option<GlobalRef>,
}
static STATE: Mutex<Option<State>> = Mutex::new(None);
pub static RINGS: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(0);

pub fn call_intent<'a>(env: &mut JNIEnv<'a>, ctx: &JObject, link: &str, answer: bool) -> Result<JObject<'a>> {
    let i = intent(env, ctx, "CallActivity")?;
    env.call_method(&i, "addFlags", "(I)Landroid/content/Intent;", &[JValue::Int(crate::jx::FLAG_ACTIVITY_NEW_TASK)])?;
    let l = s(env, link)?;
    put(env, &i, "link", JValue::Object(&l))?;
    put(env, &i, "answer", JValue::Bool(answer as u8))?;
    Ok(i)
}

pub fn call_screen<'a>(env: &mut JNIEnv<'a>, ctx: &JObject, link: &str, answer: bool, code: i32) -> Result<JObject<'a>> {
    let i = call_intent(env, ctx, link, answer)?;
    pending(env, ctx, "getActivity", code, &i)
}

pub fn hang_up<'a>(env: &mut JNIEnv<'a>, ctx: &JObject, link: &str, code: i32) -> Result<JObject<'a>> {
    let i = intent(env, ctx, "DeclineReceiver")?;
    let l = s(env, link)?;
    put(env, &i, "link", JValue::Object(&l))?;
    pending(env, ctx, "getBroadcast", code, &i)
}

pub fn channel(env: &mut JNIEnv, nm: &JObject, id: &str, name: &str, importance: i32, silent: bool) -> Result<()> {
    let (i, n) = (s(env, id)?, s(env, name)?);
    let ch = env.new_object("android/app/NotificationChannel", "(Ljava/lang/String;Ljava/lang/CharSequence;I)V", &[JValue::Object(&i), JValue::Object(&n), JValue::Int(importance)])?;
    if silent {
        let none = JObject::null();
        env.call_method(&ch, "setSound", "(Landroid/net/Uri;Landroid/media/AudioAttributes;)V", &[JValue::Object(&none), JValue::Object(&none)])?;
    }
    env.call_method(nm, "createNotificationChannel", "(Landroid/app/NotificationChannel;)V", &[JValue::Object(&ch)])?;
    Ok(())
}

pub fn set_text(env: &mut JNIEnv, b: &JObject, method: &str, text: &str) -> Result<()> {
    let t = s(env, text)?;
    env.call_method(b, method, "(Ljava/lang/CharSequence;)Landroid/app/Notification$Builder;", &[JValue::Object(&t)])?;
    Ok(())
}

pub fn builder<'a>(env: &mut JNIEnv<'a>, ctx: &JObject, channel: &str) -> Result<JObject<'a>> {
    let ch = s(env, channel)?;
    let b = env.new_object("android/app/Notification$Builder", "(Landroid/content/Context;Ljava/lang/String;)V", &[JValue::Object(ctx), JValue::Object(&ch)])?;
    let icon = env.get_static_field("android/R$drawable", "sym_action_call", "I")?.i()?;
    env.call_method(&b, "setSmallIcon", "(I)Landroid/app/Notification$Builder;", &[JValue::Int(icon)])?;
    env.call_method(&b, "setOngoing", "(Z)Landroid/app/Notification$Builder;", &[JValue::Bool(1)])?;
    Ok(b)
}

pub fn person<'a>(env: &mut JNIEnv<'a>, name: &str) -> Result<JObject<'a>> {
    let p = env.new_object("android/app/Person$Builder", "()V", &[])?;
    let n = s(env, name)?;
    env.call_method(&p, "setName", "(Ljava/lang/CharSequence;)Landroid/app/Person$Builder;", &[JValue::Object(&n)])?;
    env.call_method(&p, "setImportant", "(Z)Landroid/app/Person$Builder;", &[JValue::Bool(1)])?;
    env.call_method(&p, "build", "()Landroid/app/Person;", &[])?.l()
}

pub fn ring(env: &mut JNIEnv, ctx: &JObject, link: &str, reason: &str) -> Result<()> {
    {
        let mut st = STATE.lock().unwrap();
        let st = st.get_or_insert_with(State::default);
        if st.handled.contains(link) || st.current.as_deref() == Some(link) {
            return Ok(());
        }
    }
    stop_sound(env)?;
    STATE.lock().unwrap().as_mut().unwrap().current = Some(link.to_string());
    RINGS.fetch_add(1, std::sync::atomic::Ordering::Relaxed);

    let nm = service(env, ctx, "notification")?;
    channel(env, &nm, CHANNEL, "Calls from Claude", crate::jx::IMPORTANCE_HIGH, true)?; // a channel's sound plays once, so the ringtone is played here to loop
    let full = call_screen(env, ctx, link, false, 1)?;
    let answer = call_screen(env, ctx, link, true, 2)?;
    let decline = hang_up(env, ctx, link, 3)?;
    let b = builder(env, ctx, CHANNEL)?;
    set_text(env, &b, "setContentTitle", "Claude is calling")?;
    set_text(env, &b, "setContentText", reason)?;
    let cat = s(env, "call")?;
    env.call_method(&b, "setCategory", "(Ljava/lang/String;)Landroid/app/Notification$Builder;", &[JValue::Object(&cat)])?;
    env.call_method(&b, "setFullScreenIntent", "(Landroid/app/PendingIntent;Z)Landroid/app/Notification$Builder;", &[JValue::Object(&full), JValue::Bool(1)])?;
    env.call_method(&b, "setContentIntent", "(Landroid/app/PendingIntent;)Landroid/app/Notification$Builder;", &[JValue::Object(&answer)])?;
    env.call_method(&b, "setTimeoutAfter", "(J)Landroid/app/Notification$Builder;", &[JValue::Long(RING_FOR.as_millis() as i64)])?;
    let who = person(env, "Claude")?;
    let sig = "(Landroid/app/Person;Landroid/app/PendingIntent;Landroid/app/PendingIntent;)Landroid/app/Notification$CallStyle;";
    let style = env
        .call_static_method("android/app/Notification$CallStyle", "forIncomingCall", sig, &[JValue::Object(&who), JValue::Object(&decline), JValue::Object(&answer)])?
        .l()?;
    env.call_method(&b, "setStyle", "(Landroid/app/Notification$Style;)Landroid/app/Notification$Builder;", &[JValue::Object(&style)])?;
    let n = env.call_method(&b, "build", "()Landroid/app/Notification;", &[])?.l()?;
    env.call_method(&nm, "notify", "(ILandroid/app/Notification;)V", &[JValue::Int(RING_ID), JValue::Object(&n)])?;

    // alarm usage wakes through Do Not Disturb
    let alarm = crate::settings::mode(env, ctx)? == "wake";
    let uri = env.call_static_method("android/media/RingtoneManager", "getDefaultUri", "(I)Landroid/net/Uri;", &[JValue::Int(if alarm { crate::jx::RINGTONE_TYPE_ALARM } else { crate::jx::RINGTONE_TYPE_RINGTONE })])?.l()?;
    let sig = "(Landroid/content/Context;Landroid/net/Uri;)Landroid/media/Ringtone;";
    let tone = env.call_static_method("android/media/RingtoneManager", "getRingtone", sig, &[JValue::Object(ctx), JValue::Object(&uri)])?.l()?;
    if !tone.is_null() {
        let ab = env.new_object("android/media/AudioAttributes$Builder", "()V", &[])?;
        env.call_method(&ab, "setUsage", "(I)Landroid/media/AudioAttributes$Builder;", &[JValue::Int(if alarm { crate::jx::USAGE_ALARM } else { crate::jx::USAGE_NOTIFICATION_RINGTONE })])?;
        env.call_method(&ab, "setContentType", "(I)Landroid/media/AudioAttributes$Builder;", &[JValue::Int(crate::jx::CONTENT_TYPE_SONIFICATION)])?;
        let attrs = env.call_method(&ab, "build", "()Landroid/media/AudioAttributes;", &[])?.l()?;
        env.call_method(&tone, "setAudioAttributes", "(Landroid/media/AudioAttributes;)V", &[JValue::Object(&attrs)])?;
        env.call_method(&tone, "setLooping", "(Z)V", &[JValue::Bool(1)])?;
        env.call_method(&tone, "play", "()V", &[])?;
    }
    let vm = service(env, ctx, "vibrator_manager")?;
    let vib = env.call_method(&vm, "getDefaultVibrator", "()Landroid/os/Vibrator;", &[])?.l()?;
    let timings = env.new_long_array(3)?;
    env.set_long_array_region(&timings, 0, &[0, 900, 700])?;
    let effect = env.call_static_method("android/os/VibrationEffect", "createWaveform", "([JI)Landroid/os/VibrationEffect;", &[JValue::Object(&timings), JValue::Int(0)])?.l()?;
    env.call_method(&vib, "vibrate", "(Landroid/os/VibrationEffect;)V", &[JValue::Object(&effect)])?;
    {
        let mut st = STATE.lock().unwrap();
        let st = st.as_mut().unwrap();
        st.ringtone = if tone.is_null() { None } else { Some(env.new_global_ref(&tone)?) };
        st.vibrator = Some(env.new_global_ref(&vib)?);
    }

    let (vm, ctx, link) = (env.get_java_vm()?, env.new_global_ref(ctx)?, link.to_string());
    std::thread::spawn(move || {
        std::thread::sleep(RING_FOR);
        let still = STATE.lock().unwrap().as_ref().is_some_and(|s| s.current.as_deref() == Some(link.as_str()));
        if still {
            if let Ok(mut env) = vm.attach_current_thread() {
                let _ = stop(&mut env, ctx.as_obj(), &link, false);
            }
        }
    });
    Ok(())
}

pub fn tick(env: &mut JNIEnv, ctx: &JObject) -> Result<()> {
    let vib = service(env, ctx, "vibrator")?;
    let effect = env.call_static_method("android/os/VibrationEffect", "createPredefined", "(I)Landroid/os/VibrationEffect;", &[JValue::Int(crate::jx::EFFECT_TICK)])?.l()?;
    env.call_method(&vib, "vibrate", "(Landroid/os/VibrationEffect;)V", &[JValue::Object(&effect)])?;
    Ok(())
}

pub fn stop(env: &mut JNIEnv, ctx: &JObject, link: &str, handled: bool) -> Result<()> {
    {
        let mut st = STATE.lock().unwrap();
        let st = st.get_or_insert_with(State::default);
        if handled {
            st.handled.insert(link.to_string());
        }
        if st.current.as_deref() != Some(link) {
            return Ok(());
        }
        st.current = None;
    }
    stop_sound(env)?;
    let nm = service(env, ctx, "notification")?;
    env.call_method(&nm, "cancel", "(I)V", &[JValue::Int(RING_ID)])?;
    Ok(())
}

fn stop_sound(env: &mut JNIEnv) -> Result<()> {
    let (tone, vib) = {
        let mut st = STATE.lock().unwrap();
        let st = st.get_or_insert_with(State::default);
        (st.ringtone.take(), st.vibrator.take())
    };
    if let Some(t) = tone {
        env.call_method(&t, "stop", "()V", &[])?;
    }
    if let Some(v) = vib {
        env.call_method(&v, "cancel", "()V", &[])?;
    }
    Ok(())
}
