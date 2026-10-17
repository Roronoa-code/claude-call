use crate::jx::{JNIEnv, JObject, Result};
use windows::{
    Data::Xml::Dom::XmlDocument,
    Foundation::TypedEventHandler,
    UI::Notifications::{ToastNotification, ToastNotificationManager},
    Win32::{System::Registry::{HKEY_CURRENT_USER, REG_SZ, RegSetKeyValueW}, UI::Shell::SetCurrentProcessExplicitAppUserModelID},
    core::{HSTRING, IInspectable, w},
};

const APP: &str = "ClaudeCall.Desktop";

// An unpackaged app can only toast under an ID registered in the user's classes.
pub fn register() {
    let name: Vec<u16> = crate::desk_os::TITLE.encode_utf16().chain([0]).collect();
    let icon = crate::android::data_dir().join("claude-call.png");
    let icon_saved = std::fs::write(&icon, include_bytes!("../../ui/icon/claude-call.png")).is_ok();
    let icon: Vec<u16> = icon.to_string_lossy().encode_utf16().chain([0]).collect();
    unsafe {
        let _ = RegSetKeyValueW(HKEY_CURRENT_USER, w!(r"Software\Classes\AppUserModelId\ClaudeCall.Desktop"), w!("DisplayName"), REG_SZ.0, Some(name.as_ptr().cast()), (name.len() * 2) as u32);
        if icon_saved {
            let _ = RegSetKeyValueW(HKEY_CURRENT_USER, w!(r"Software\Classes\AppUserModelId\ClaudeCall.Desktop"), w!("IconUri"), REG_SZ.0, Some(icon.as_ptr().cast()), (icon.len() * 2) as u32);
        }
        let _ = SetCurrentProcessExplicitAppUserModelID(&HSTRING::from(APP));
    }
}

fn tag(chat: &str) -> HSTRING {
    HSTRING::from(chat.chars().filter(|c| c.is_ascii_alphanumeric()).take(60).collect::<String>())
}

fn escape(s: &str) -> String {
    s.replace('&', "&amp;").replace('<', "&lt;").replace('>', "&gt;").replace('"', "&quot;")
}

pub fn cancel(_: &mut JNIEnv, _: &JObject, chat: &str) -> Result<()> {
    let _ = ToastNotificationManager::History().and_then(|h| h.RemoveGroupedTagWithId(&tag(chat), &HSTRING::from("chats"), &HSTRING::from(APP)));
    Ok(())
}

pub fn notify(_: &mut JNIEnv, _: &JObject, chat: &str, name: &str, text: &str) -> Result<()> {
    let show = || -> windows::core::Result<()> {
        let xml = XmlDocument::new()?;
        xml.LoadXml(&HSTRING::from(format!(
            "<toast><visual><binding template=\"ToastGeneric\"><text>{}</text><text>{}</text></binding></visual></toast>",
            escape(name),
            escape(text)
        )))?;
        let toast = ToastNotification::CreateToastNotification(&xml)?;
        toast.SetTag(&tag(chat))?;
        toast.SetGroup(&HSTRING::from("chats"))?;
        let chat = chat.to_string();
        toast.Activated(&TypedEventHandler::<ToastNotification, IInspectable>::new(move |_, _| {
            *crate::inbox::OPEN_CHAT.lock().unwrap() = Some(chat.clone());
            let _ = slint::invoke_from_event_loop(crate::desk_app::raise_home);
            Ok(())
        }))?;
        ToastNotificationManager::CreateToastNotifierWithId(&HSTRING::from(APP))?.Show(&toast)
    };
    show().map_err(|e| e.to_string())
}
