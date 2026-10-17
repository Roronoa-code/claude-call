fn main() {
    let ui = if std::env::var("CARGO_CFG_TARGET_OS").is_ok_and(|os| os == "windows") { "ui/desk.slint" } else { "ui/app.slint" };
    slint_build::compile(ui).expect(ui);
}
