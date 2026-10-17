#![cfg_attr(windows, windows_subsystem = "windows")]

#[cfg(windows)]
fn main() {
    callme_phone::desktop_main();
}

#[cfg(not(windows))]
fn main() {}
