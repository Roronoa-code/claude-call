//! The Claude Call phone app, in Rust: the home screen, the call screen and call audio (drawn with Slint), and
//! the always-on call listener, ringing, settings and the widget. Kotlin keeps only the classes Android itself
//! must create, as one-line shells that forward here (android/app/.../Shell.kt, shell.rs).
#[cfg(target_os = "android")]
mod android;
#[cfg(target_os = "android")]
mod attach;
#[cfg(target_os = "android")]
mod audio;
#[cfg(target_os = "android")]
mod caller;
#[cfg(target_os = "android")]
mod engine;
#[cfg(target_os = "android")]
mod faces;
#[cfg(target_os = "android")]
mod inbox;
#[cfg(target_os = "android")]
mod jx;
#[cfg(target_os = "android")]
mod live_gl;
#[cfg(target_os = "android")]
mod net;
#[cfg(target_os = "android")]
mod ring;
#[cfg(target_os = "android")]
mod service;
#[cfg(target_os = "android")]
mod settings;
#[cfg(target_os = "android")]
mod shell;
#[cfg(target_os = "android")]
mod widget;
mod alive;
mod cast;
mod chats;
mod view;

slint::include_modules!();

#[cfg(target_os = "android")]
mod home;
#[cfg(target_os = "android")]
mod screen;

/// One library for both screens: the activity that opened it says which.
#[cfg(target_os = "android")]
#[unsafe(no_mangle)]
fn android_main(app: android_activity::AndroidApp) {
    android::init(&app);
    slint::android::init(app.clone()).expect("slint android");
    if android::activity_name().ends_with(".MainActivity") {
        home::run(app);
    } else {
        screen::run(app);
    }
}
