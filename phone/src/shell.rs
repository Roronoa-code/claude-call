//! The Android entry points. Android itself must create the app's activities, service and receivers as Java classes,
//! so those stay as one-line Kotlin shells (android/app/.../Shell.kt) that forward here (fyi.mani.callme.Native).
use crate::{jx, net, service, widget};
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
export!(Java_fyi_mani_callme_Native_widgets, |env, ctx| widget::update_all(env, &ctx));
export!(Java_fyi_mani_callme_Native_stopRinging, |env, act| service::screen_touched(env, &act));

/// ListenService stopped: the listener ends within a second.
#[unsafe(no_mangle)]
pub extern "system" fn Java_fyi_mani_callme_Native_stop(_env: JNIEnv, _class: JClass) {
    net::stop();
}
