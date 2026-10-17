fn main() {
    let windows = std::env::var("CARGO_CFG_TARGET_OS").is_ok_and(|os| os == "windows");
    #[cfg(windows)]
    if windows {
        println!("cargo:rerun-if-changed=ui/icon/claude-call.ico");
        winresource::WindowsResource::new().set_icon("ui/icon/claude-call.ico").compile().expect("Windows icon resource");
    }
    let ui = if windows { "ui/desk.slint" } else { "ui/app.slint" };
    slint_build::compile(ui).expect(ui);
}
