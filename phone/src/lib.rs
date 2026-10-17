#[cfg(any(target_os = "android", windows))]
#[cfg_attr(windows, path = "desk/android.rs")]
mod android;
#[cfg(any(target_os = "android", windows))]
#[cfg_attr(windows, path = "desk/attach.rs")]
mod attach;
#[cfg(any(target_os = "android", windows))]
#[cfg_attr(windows, path = "desk/audio.rs")]
mod audio;
#[cfg(any(target_os = "android", windows))]
mod caller;
#[cfg(any(target_os = "android", windows))]
#[cfg_attr(windows, path = "desk/chat_alert.rs")]
mod chat_alert;
#[cfg(any(target_os = "android", windows))]
mod chat_view;
#[cfg(any(target_os = "android", windows))]
mod engine;
#[cfg(any(target_os = "android", windows))]
mod faces;
#[cfg(any(target_os = "android", windows))]
mod inbox;
#[cfg(any(target_os = "android", windows))]
#[cfg_attr(windows, path = "desk/jx.rs")]
mod jx;
#[cfg(any(target_os = "android", windows))]
mod live_gl;
#[cfg(any(target_os = "android", windows))]
mod net;
#[cfg(any(target_os = "android", windows))]
#[cfg_attr(windows, path = "desk/ring.rs")]
mod ring;
#[cfg(any(target_os = "android", windows))]
#[cfg_attr(windows, path = "desk/service.rs")]
mod service;
#[cfg(any(target_os = "android", windows))]
#[cfg_attr(windows, path = "desk/settings.rs")]
mod settings;
#[cfg(target_os = "android")]
mod shell;
#[cfg(any(target_os = "android", windows))]
#[cfg_attr(windows, path = "desk/widget.rs")]
mod widget;
mod alive;
mod cast;
mod chats;
mod stage;
mod view;

slint::include_modules!();

#[cfg(any(target_os = "android", windows))]
mod home;
#[cfg(any(target_os = "android", windows))]
mod screen;

#[cfg(target_os = "android")]
#[unsafe(no_mangle)]
fn android_main(app: android_activity::AndroidApp) {
    android::init(&app);
    slint::android::init(app.clone()).expect("slint android");
    if android::activity_name().ends_with(".MainActivity") {
        home::run();
    } else {
        screen::run(app);
    }
}

#[cfg(windows)]
#[path = "desk/app.rs"]
mod desk_app;
#[cfg(windows)]
pub use desk_app::desktop_main;
