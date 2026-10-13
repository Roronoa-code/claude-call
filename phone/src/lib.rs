//! The Claude Call phone app, in Rust: the home screen, the call screen and call audio (drawn with Slint), and
//! the always-on call listener. Kotlin keeps only the Android entry points and calls (android/app/...).
#[cfg(target_os = "android")]
mod android;
#[cfg(target_os = "android")]
mod audio;
#[cfg(target_os = "android")]
mod engine;
#[cfg(target_os = "android")]
mod net;
mod antics;
mod mascot;
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
