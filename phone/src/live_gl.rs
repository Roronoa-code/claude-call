//! Draws the live Clawd and Null (alive.rs) on Slint's own GL context: each shader (art/live3d, shared with the
//! offline renderer) raymarches its toy into a texture that Slint shows as an image (Toys.clawd-live, Toys.null-live).
//! Only a new pose is drawn; between poses each texture keeps its last frame.
use crate::{
    HomeScreen, Toys,
    alive::{CLAWD_SIZE, NULL_SIZE},
    android::log,
    cast::Uniforms,
};
use glow::HasContext;
use slint::{ComponentHandle, GraphicsAPI, RenderingState};
use std::{cell::RefCell, num::NonZeroU32, rc::Rc};

const CLAWD_FRAG: &str = include_str!("../../art/live3d/clawd_cast.frag");
const NULL_FRAG: &str = include_str!("../../art/live3d/null.frag");
const VERT: &str = "#version 300 es
void main() { vec2 p = vec2(float((gl_VertexID << 1) & 2), float(gl_VertexID & 2)); gl_Position = vec4(p * 2.0 - 1.0, 0.0, 1.0); }";

/// The shared frame: whether they are on screen at all, and per toy (Clawd, Null) the uniforms to draw and whether
/// those are not drawn yet.
#[derive(Default)]
pub struct Scene {
    pub show: bool,
    pub frames: [(Uniforms, bool); 2],
}

struct Toy {
    program: glow::Program,
    vao: glow::VertexArray,
    fbo: glow::Framebuffer,
    tex: glow::Texture,
    size: (i32, i32),
    res: Option<glow::UniformLocation>,
    u: Vec<(&'static str, Option<glow::UniformLocation>)>,
}

// Skia draws the rest of the screen on the same GL context and remembers what it last set, so whatever this
// touches is put back exactly as it was.
const CAPS: [u32; 5] = [glow::BLEND, glow::SCISSOR_TEST, glow::DEPTH_TEST, glow::STENCIL_TEST, glow::CULL_FACE];

impl Toy {
    /// `frag` with `define` added after its version line (the vinyl Clawd's material is fixed at compile time).
    unsafe fn new(gl: &glow::Context, frag: &str, define: &str, names: Vec<&'static str>, size: (i32, i32)) -> Result<Self, String> {
        unsafe {
            let (version, rest) = frag.split_once('\n').ok_or("empty shader")?;
            let frag = format!("{}\n{define}\n{rest}", version.trim_end());
            let program = gl.create_program()?;
            for (kind, src) in [(glow::VERTEX_SHADER, VERT), (glow::FRAGMENT_SHADER, frag.as_str())] {
                let s = gl.create_shader(kind)?;
                gl.shader_source(s, src);
                gl.compile_shader(s);
                if !gl.get_shader_compile_status(s) {
                    let e = gl.get_shader_info_log(s);
                    gl.delete_shader(s);
                    gl.delete_program(program);
                    return Err(e);
                }
                gl.attach_shader(program, s);
                gl.delete_shader(s);
            }
            gl.link_program(program);
            if !gl.get_program_link_status(program) {
                let e = gl.get_program_info_log(program);
                gl.delete_program(program);
                return Err(e);
            }
            let res = gl.get_uniform_location(program, "uRes");
            let u = names.into_iter().map(|n| (n, gl.get_uniform_location(program, n))).collect();
            let vao = gl.create_vertex_array()?;
            let old_tex = gl.get_parameter_texture(glow::TEXTURE_BINDING_2D);
            let tex = gl.create_texture()?;
            gl.bind_texture(glow::TEXTURE_2D, Some(tex));
            gl.tex_image_2d(glow::TEXTURE_2D, 0, glow::RGBA8 as i32, size.0, size.1, 0, glow::RGBA, glow::UNSIGNED_BYTE, glow::PixelUnpackData::Slice(None));
            for (k, v) in [(glow::TEXTURE_MIN_FILTER, glow::LINEAR), (glow::TEXTURE_MAG_FILTER, glow::LINEAR), (glow::TEXTURE_WRAP_S, glow::CLAMP_TO_EDGE), (glow::TEXTURE_WRAP_T, glow::CLAMP_TO_EDGE)] {
                gl.tex_parameter_i32(glow::TEXTURE_2D, k, v as i32);
            }
            gl.bind_texture(glow::TEXTURE_2D, old_tex);
            let old_fbo = gl.get_parameter_framebuffer(glow::DRAW_FRAMEBUFFER_BINDING);
            let old_read = gl.get_parameter_framebuffer(glow::READ_FRAMEBUFFER_BINDING);
            let fbo = gl.create_framebuffer()?;
            gl.bind_framebuffer(glow::FRAMEBUFFER, Some(fbo));
            gl.framebuffer_texture_2d(glow::FRAMEBUFFER, glow::COLOR_ATTACHMENT0, glow::TEXTURE_2D, Some(tex), 0);
            let ok = gl.check_framebuffer_status(glow::FRAMEBUFFER) == glow::FRAMEBUFFER_COMPLETE;
            gl.bind_framebuffer(glow::DRAW_FRAMEBUFFER, old_fbo);
            gl.bind_framebuffer(glow::READ_FRAMEBUFFER, old_read);
            let me = Toy { program, vao, fbo, tex, size, res, u };
            if !ok {
                me.delete(gl);
                return Err("framebuffer incomplete".into());
            }
            Ok(me)
        }
    }

    unsafe fn draw(&self, gl: &glow::Context, frame: &Uniforms) {
        unsafe {
            let fbo = gl.get_parameter_framebuffer(glow::DRAW_FRAMEBUFFER_BINDING);
            let read = gl.get_parameter_framebuffer(glow::READ_FRAMEBUFFER_BINDING);
            let program = gl.get_parameter_program(glow::CURRENT_PROGRAM);
            let vao = gl.get_parameter_vertex_array(glow::VERTEX_ARRAY_BINDING);
            let mut viewport = [0; 4];
            gl.get_parameter_i32_slice(glow::VIEWPORT, &mut viewport);
            let caps = CAPS.map(|c| gl.is_enabled(c));
            let mask = gl.get_parameter_bool_array::<4>(glow::COLOR_WRITEMASK);

            gl.bind_framebuffer(glow::FRAMEBUFFER, Some(self.fbo));
            gl.viewport(0, 0, self.size.0, self.size.1);
            CAPS.iter().for_each(|&c| gl.disable(c));
            gl.color_mask(true, true, true, true);
            gl.use_program(Some(self.program));
            gl.uniform_2_f32(self.res.as_ref(), self.size.0 as f32, self.size.1 as f32);
            for (name, v) in frame {
                if let Some((_, loc)) = self.u.iter().find(|(n, _)| n == name) {
                    gl.uniform_4_f32(loc.as_ref(), v[0], v[1], v[2], v[3]);
                }
            }
            gl.bind_vertex_array(Some(self.vao));
            gl.draw_arrays(glow::TRIANGLES, 0, 3);

            gl.bind_vertex_array(vao);
            gl.use_program(program);
            gl.color_mask(mask[0], mask[1], mask[2], mask[3]);
            for (&c, on) in CAPS.iter().zip(caps) {
                if on { gl.enable(c) }
            }
            gl.viewport(viewport[0], viewport[1], viewport[2], viewport[3]);
            gl.bind_framebuffer(glow::DRAW_FRAMEBUFFER, fbo);
            gl.bind_framebuffer(glow::READ_FRAMEBUFFER, read);
        }
    }

    fn image(&self) -> slint::Image {
        let id = NonZeroU32::new(self.tex.0.get()).expect("texture id");
        unsafe { slint::BorrowedOpenGLTextureBuilder::new_gl_2d_rgba_texture(id, (self.size.0 as u32, self.size.1 as u32).into()) }
            .origin(slint::BorrowedOpenGLTextureOrigin::BottomLeft)
            .build()
    }

    fn delete(&self, gl: &glow::Context) {
        unsafe {
            gl.delete_framebuffer(self.fbo);
            gl.delete_texture(self.tex);
            gl.delete_vertex_array(self.vao);
            gl.delete_program(self.program);
        }
    }
}

/// Builds both toys on the GPU and draws each new pose into its texture before a frame of the home screen. If the
/// GPU can't build them, both stay still sprites instead (Toys.live false).
pub fn attach(ui: &HomeScreen, scene: Rc<RefCell<Scene>>, names: [Vec<&'static str>; 2]) {
    let weak = ui.as_weak();
    let mut gl: Option<(glow::Context, [Toy; 2])> = None;
    let set = ui.window().set_rendering_notifier(move |state, api| match state {
        RenderingState::RenderingSetup => {
            let (GraphicsAPI::NativeOpenGL { get_proc_address }, Some(ui)) = (api, weak.upgrade()) else { return };
            let ctx = unsafe { glow::Context::from_loader_function_cstr(|s| get_proc_address(s)) };
            let sf = ui.window().scale_factor();
            let px = |(w, h): (f32, f32)| ((w * sf).round().min(1200.0) as i32, (h * sf).round().min(1000.0) as i32);
            let built = unsafe { Toy::new(&ctx, CLAWD_FRAG, "#define MAT_ID 0", names[0].clone(), px(CLAWD_SIZE)) }.and_then(|c| {
                match unsafe { Toy::new(&ctx, NULL_FRAG, "", names[1].clone(), px(NULL_SIZE)) } {
                    Ok(n) => Ok([c, n]),
                    Err(e) => {
                        c.delete(&ctx);
                        Err(e)
                    }
                }
            });
            match built {
                Ok(toys) => {
                    let g = ui.global::<Toys>();
                    g.set_clawd_live(toys[0].image());
                    g.set_null_live(toys[1].image());
                    gl = Some((ctx, toys));
                }
                Err(e) => {
                    log(&format!("live toys: {e}"));
                    ui.global::<Toys>().set_live(false);
                }
            }
        }
        RenderingState::BeforeRendering => {
            // Slint redraws the whole screen at the display rate while anything animates, so only a new pose is drawn.
            let mut s = scene.borrow_mut();
            let Some((ctx, toys)) = &gl else { return };
            if !s.show {
                return;
            }
            for (toy, (frame, fresh)) in toys.iter().zip(s.frames.iter_mut()) {
                if *fresh {
                    unsafe { toy.draw(ctx, frame) };
                    *fresh = false;
                }
            }
        }
        RenderingState::RenderingTeardown => {
            if let Some((ctx, toys)) = gl.take() {
                toys.iter().for_each(|t| t.delete(&ctx));
            }
        }
        _ => {}
    });
    if let Err(e) = set {
        log(&format!("live toys: no rendering notifier: {e:?}"));
        ui.global::<Toys>().set_live(false);
    }
}
