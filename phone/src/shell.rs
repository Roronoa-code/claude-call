use crate::{attach, chat_alert, jx, net, service, widget};
use jni::{
    JNIEnv,
    objects::{JClass, JObject},
};

macro_rules! export {
    ($name:ident, |$env:ident, $($arg:ident),*| $body:expr) => {
        #[unsafe(no_mangle)]
        pub extern "system" fn $name(mut env: JNIEnv, _class: JClass, $($arg: JObject),*) {
            jx::ok(&mut env, |$env| $body);
        }
    };
}

export!(Java_fyi_mani_callme_Native_foreground, |env, svc| service::foreground(env, &svc));
export!(Java_fyi_mani_callme_Native_start, |env, ctx| service::start(env, &ctx));
export!(Java_fyi_mani_callme_Native_callOpened, |env, act, intent| service::call_opened(env, &act, &intent));
export!(Java_fyi_mani_callme_Native_callBack, |env, act| service::call_back(env, &act));
export!(Java_fyi_mani_callme_Native_callClosed, |env, act| service::call_closed(env, &act));
export!(Java_fyi_mani_callme_Native_homeOpened, |env, act| service::home_opened(env, &act));
export!(Java_fyi_mani_callme_Native_homeResumed, |env, act| service::home_resumed(env, &act));
export!(Java_fyi_mani_callme_Native_permissionsAnswered, |env, act| service::permissions_answered(env, &act));
export!(Java_fyi_mani_callme_Native_declined, |env, ctx, intent| service::declined(env, &ctx, &intent));
export!(Java_fyi_mani_callme_Native_modeTapped, |env, ctx, intent| widget::tapped(env, &ctx, &intent));
export!(Java_fyi_mani_callme_Native_chatAction, |env, ctx, intent| chat_alert::acted(env, &ctx, &intent));
export!(Java_fyi_mani_callme_Native_widgets, |env, ctx| widget::update_all(env, &ctx));
export!(Java_fyi_mani_callme_Native_stopRinging, |env, act| service::screen_touched(env, &act));
export!(Java_fyi_mani_callme_Native_picked, |env, act, data| attach::picked(env, &act, &data));

#[unsafe(no_mangle)]
pub extern "system" fn Java_fyi_mani_callme_Native_homeShown(_env: JNIEnv, _class: JClass, on: jni::sys::jboolean) {
    crate::android::set_home_shown(on != 0);
}

#[unsafe(no_mangle)]
pub extern "system" fn Java_fyi_mani_callme_Native_stop(_env: JNIEnv, _class: JClass) {
    net::stop();
}

#[unsafe(no_mangle)]
pub extern "system" fn Java_fyi_mani_callme_Native_back(_env: JNIEnv, _class: JClass, phase: jni::sys::jint, progress: jni::sys::jfloat) {
    crate::home::back_gesture(phase, progress);
}

#[unsafe(no_mangle)]
pub extern "system" fn Java_fyi_mani_callme_Native_keyboard(_env: JNIEnv, _class: JClass, dp: jni::sys::jfloat) {
    crate::android::set_kb(dp);
}
