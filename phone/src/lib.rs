#[cfg(target_os = "android")]
mod android;
#[cfg(target_os = "android")]
mod attach;
#[cfg(target_os = "android")]
mod audio;
#[cfg(target_os = "android")]
mod caller;
#[cfg(target_os = "android")]
mod chat_alert;
#[cfg(target_os = "android")]
mod chat_view;
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
mod ringwatch;
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
mod stage;
mod view;

slint::include_modules!();

#[cfg(target_os = "android")]
mod home;
#[cfg(target_os = "android")]
mod screen;

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
