use slint::winit_030::winit::{self, raw_window_handle::{HasWindowHandle, RawWindowHandle}};
use windows::Win32::{
    Foundation::{ERROR_ALREADY_EXISTS, GetLastError, HWND, POINT},
    Graphics::Gdi::{MONITOR_DEFAULTTONULL, MonitorFromPoint},
    Media::Audio::{PlaySoundW, SND_ASYNC, SND_FILENAME, SND_LOOP, SND_NODEFAULT},
    System::{Registry::{HKEY_CURRENT_USER, REG_SZ, RegSetKeyValueW}, Threading::CreateMutexW},
    UI::Shell::ShellExecuteW,
    UI::WindowsAndMessaging::{SW_SHOWNORMAL, FLASHW_ALL, FLASHW_TIMERNOFG, FLASHWINFO, FindWindowW, FlashWindowEx, SW_RESTORE, SetForegroundWindow, ShowWindow},
};
use windows::core::{HSTRING, PCWSTR, w};

pub const TITLE: &str = "Claude Call";

pub fn already_running() -> bool {
    unsafe {
        let _ = CreateMutexW(None, true, w!("Local\\ClaudeCallDesk"));
        if GetLastError() != ERROR_ALREADY_EXISTS {
            return false;
        }
        if let Ok(h) = FindWindowW(PCWSTR::null(), &HSTRING::from(TITLE)) {
            let _ = ShowWindow(h, SW_RESTORE);
            let _ = SetForegroundWindow(h);
        }
        true
    }
}

pub fn start_at_login() {
    let Ok(exe) = std::env::current_exe() else { return };
    if exe.components().any(|c| c.as_os_str() == "target" || c.as_os_str().to_string_lossy().starts_with("target-")) {
        return;
    }
    let value: Vec<u16> = format!("\"{}\"", exe.display()).encode_utf16().chain([0]).collect();
    unsafe {
        let _ = RegSetKeyValueW(HKEY_CURRENT_USER, w!("Software\\Microsoft\\Windows\\CurrentVersion\\Run"), w!("ClaudeCall"), REG_SZ.0, Some(value.as_ptr().cast()), (value.len() * 2) as u32);
    }
}

pub fn on_screen(x: i32, y: i32) -> bool {
    unsafe { !MonitorFromPoint(POINT { x: x + 40, y: y + 20 }, MONITOR_DEFAULTTONULL).is_invalid() }
}

fn hwnd(w: &winit::window::Window) -> Option<HWND> {
    match w.window_handle().ok()?.as_raw() {
        RawWindowHandle::Win32(h) => Some(HWND(h.hwnd.get() as _)),
        _ => None,
    }
}

pub fn flash(w: &winit::window::Window) {
    if let Some(h) = hwnd(w) {
        let info = FLASHWINFO { cbSize: size_of::<FLASHWINFO>() as u32, hwnd: h, dwFlags: FLASHW_ALL | FLASHW_TIMERNOFG, uCount: 0, dwTimeout: 0 };
        unsafe {
            let _ = FlashWindowEx(&info);
        }
    }
}

pub fn ringtone(on: bool) {
    unsafe {
        if on {
            let _ = PlaySoundW(w!(r"C:\Windows\Media\Ring05.wav"), None, SND_FILENAME | SND_ASYNC | SND_LOOP | SND_NODEFAULT);
        } else {
            let _ = PlaySoundW(PCWSTR::null(), None, SND_ASYNC);
        }
    }
}

pub fn open(target: &str) {
    unsafe {
        ShellExecuteW(None, w!("open"), &HSTRING::from(target), PCWSTR::null(), PCWSTR::null(), SW_SHOWNORMAL);
    }
}
