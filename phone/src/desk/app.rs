use slint::{BackendSelector, ComponentHandle};

pub fn desktop_main() {
    let select = |gl: fn(BackendSelector) -> BackendSelector| gl(BackendSelector::new().backend_name("winit".into()).renderer_name("skia-opengl".into())).select();
    if select(|b| b.require_opengl_es_with_version(3, 0)).is_err() {
        select(|b| b.require_opengl_with_version(3, 3)).expect("no OpenGL 3.3 or ES 3.0");
    }
    let _ = crate::home::open().ui.run();
}
