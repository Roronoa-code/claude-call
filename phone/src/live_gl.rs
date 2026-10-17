use crate::{
    android::log,
    cast::{Uniforms, Who},
};
use glow::HasContext;
use slint::{ComponentHandle, GraphicsAPI, RenderingState};
use std::{cell::RefCell, num::NonZeroU32, rc::Rc};

const CLAWD_FRAG: &str = include_str!("../../art/live3d/clawd_cast.frag");
const NULL_FRAG: &str = include_str!("../../art/live3d/null.frag");
const VERT: &str = "
void main() { vec2 p = vec2(float((gl_VertexID << 1) & 2), float(gl_VertexID & 2)); gl_Position = vec4(p * 2.0 - 1.0, 0.0, 1.0); }";

pub struct Slot {
    pub who: Who,
    pub res: (i32, i32),
    pub view: (i32, i32),
    pub frame: Uniforms,
    pub fresh: bool,
}

#[derive(Default)]
pub struct Scene {
    pub slots: Vec<Slot>,
    pub failed: bool,
    pub warm: Vec<Who>,
}

impl Scene {
    pub fn new(n: usize) -> Self {
        let slot = || Slot { who: Who::Null, res: (1, 1), view: (1, 1), frame: vec![], fresh: false };
        Scene { slots: (0..n).map(|_| slot()).collect(), failed: false, warm: vec![] }
    }
}

pub fn texture_px(size: (f32, f32), sf: f32) -> (i32, i32) {
    ((size.0 * sf).ceil().min(2048.0) as i32, (size.1 * sf).ceil().min(2048.0) as i32)
}

struct Program {
    who: Who,
    program: glow::Program,
    u: Vec<(&'static str, Option<glow::UniformLocation>)>,
}

impl Program {
    fn loc(&mut self, gl: &glow::Context, name: &'static str) -> Option<glow::UniformLocation> {
        if let Some((_, l)) = self.u.iter().find(|(n, _)| *n == name) {
            return l.clone();
        }
        let l = unsafe { gl.get_uniform_location(self.program, name) };
        self.u.push((name, l.clone()));
        l
    }
}

struct Target {
    fbo: glow::Framebuffer,
    tex: glow::Texture,
    size: (i32, i32),
}

struct Gl {
    ctx: glow::Context,
    vao: glow::VertexArray,
    targets: Vec<Target>,
    programs: Vec<Program>,
}

// Skia shares this GL context and remembers what it last set, so whatever this touches is put back.
const CAPS: [u32; 5] = [glow::BLEND, glow::SCISSOR_TEST, glow::DEPTH_TEST, glow::STENCIL_TEST, glow::CULL_FACE];

unsafe fn compile(gl: &glow::Context, who: Who) -> Result<glow::Program, String> {
    let (frag, define) = match who {
        Who::Null => (NULL_FRAG, String::new()),
        Who::Clawd(m) => (CLAWD_FRAG, format!("#define MAT_ID {}", m as i32)),
    };
    let (version, rest) = frag.split_once('\n').ok_or("empty shader")?;
    let version = if gl.version().is_embedded { version.trim_end() } else { "#version 330 core" };
    let frag = format!("{version}\n{define}\n{rest}");
    let vert = format!("{version}{VERT}");
    unsafe {
        let program = gl.create_program()?;
        for (kind, src) in [(glow::VERTEX_SHADER, vert.as_str()), (glow::FRAGMENT_SHADER, frag.as_str())] {
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
        Ok(program)
    }
}

unsafe fn target(gl: &glow::Context, size: (i32, i32)) -> Result<Target, String> {
    unsafe {
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
        if ok {
            // a new texture's contents are undefined
            let mut clear = [0.0f32; 4];
            gl.get_parameter_f32_slice(glow::COLOR_CLEAR_VALUE, &mut clear);
            let scissor = gl.is_enabled(glow::SCISSOR_TEST);
            gl.disable(glow::SCISSOR_TEST);
            gl.clear_color(0.0, 0.0, 0.0, 0.0);
            gl.clear(glow::COLOR_BUFFER_BIT);
            gl.clear_color(clear[0], clear[1], clear[2], clear[3]);
            if scissor {
                gl.enable(glow::SCISSOR_TEST);
            }
        }
        gl.bind_framebuffer(glow::DRAW_FRAMEBUFFER, old_fbo);
        gl.bind_framebuffer(glow::READ_FRAMEBUFFER, old_read);
        let t = Target { fbo, tex, size };
        if !ok {
            t.delete(gl);
            return Err("framebuffer incomplete".into());
        }
        Ok(t)
    }
}

impl Target {
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
        }
    }
}

impl Gl {
    fn build(ctx: glow::Context, sizes: &[(i32, i32)]) -> Result<Gl, String> {
        let vao = unsafe { ctx.create_vertex_array() };
        let mut gl = Gl { vao: vao?, ctx, targets: vec![], programs: vec![] };
        for &s in sizes {
            match unsafe { target(&gl.ctx, s) } {
                Ok(t) => gl.targets.push(t),
                Err(e) => {
                    gl.delete();
                    return Err(e);
                }
            }
        }
        Ok(gl)
    }

    fn draw(&mut self, slots: &mut [Slot]) -> Result<(), String> {
        let gl = &self.ctx;
        unsafe {
            let fbo = gl.get_parameter_framebuffer(glow::DRAW_FRAMEBUFFER_BINDING);
            let read = gl.get_parameter_framebuffer(glow::READ_FRAMEBUFFER_BINDING);
            let program = gl.get_parameter_program(glow::CURRENT_PROGRAM);
            let vao = gl.get_parameter_vertex_array(glow::VERTEX_ARRAY_BINDING);
            let mut viewport = [0; 4];
            gl.get_parameter_i32_slice(glow::VIEWPORT, &mut viewport);
            let caps = CAPS.map(|c| gl.is_enabled(c));
            let mask = gl.get_parameter_bool_array::<4>(glow::COLOR_WRITEMASK);
            CAPS.iter().for_each(|&c| gl.disable(c));
            gl.color_mask(true, true, true, true);
            gl.bind_vertex_array(Some(self.vao));

            let mut result = Ok(());
            for (slot, t) in slots.iter_mut().zip(&self.targets).filter(|(s, _)| s.fresh) {
                slot.fresh = false;
                let i = match self.programs.iter().position(|p| p.who == slot.who) {
                    Some(i) => i,
                    None => match compile(gl, slot.who) {
                        Ok(program) => {
                            self.programs.push(Program { who: slot.who, program, u: vec![] });
                            self.programs.len() - 1
                        }
                        Err(e) => {
                            result = Err(format!("{:?}: {e}", slot.who));
                            break;
                        }
                    },
                };
                let p = &mut self.programs[i];
                gl.bind_framebuffer(glow::FRAMEBUFFER, Some(t.fbo));
                gl.viewport(0, 0, slot.view.0.min(t.size.0), slot.view.1.min(t.size.1));
                gl.use_program(Some(p.program));
                gl.uniform_2_f32(p.loc(gl, "uRes").as_ref(), slot.res.0 as f32, slot.res.1 as f32);
                for name in ["uStage", "uKey", "uKeyCol", "uShadow"] {
                    gl.uniform_4_f32(p.loc(gl, name).as_ref(), 0.0, 0.0, 0.0, 0.0);
                }
                for (name, v) in &slot.frame {
                    gl.uniform_4_f32(p.loc(gl, name).as_ref(), v[0], v[1], v[2], v[3]);
                }
                gl.draw_arrays(glow::TRIANGLES, 0, 3);
            }

            gl.bind_vertex_array(vao);
            gl.use_program(program);
            gl.color_mask(mask[0], mask[1], mask[2], mask[3]);
            for (&c, on) in CAPS.iter().zip(caps) {
                if on {
                    gl.enable(c)
                }
            }
            gl.viewport(viewport[0], viewport[1], viewport[2], viewport[3]);
            gl.bind_framebuffer(glow::DRAW_FRAMEBUFFER, fbo);
            gl.bind_framebuffer(glow::READ_FRAMEBUFFER, read);
            result
        }
    }

    fn delete(&self) {
        let gl = &self.ctx;
        self.targets.iter().for_each(|t| t.delete(gl));
        unsafe {
            self.programs.iter().for_each(|p| gl.delete_program(p.program));
            gl.delete_vertex_array(self.vao);
        }
    }
}

pub fn attach<C: ComponentHandle + 'static>(ui: &C, scene: Rc<RefCell<Scene>>, sizes: Vec<(f32, f32)>, publish: impl Fn(Vec<slint::Image>) + 'static) {
    let mut gl: Option<Gl> = None;
    let weak = ui.as_weak();
    let fail = {
        let scene = scene.clone();
        move |e: String| {
            log(&format!("live cast: {e}"));
            scene.borrow_mut().failed = true;
        }
    };
    let set = ui.window().set_rendering_notifier({
        let fail = fail.clone();
        move |state, api| match state {
            RenderingState::RenderingSetup => {
                let GraphicsAPI::NativeOpenGL { get_proc_address } = api else { return fail("no OpenGL".into()) };
                let Some(sf) = weak.upgrade().map(|u| u.window().scale_factor()) else { return };
                let ctx = unsafe { glow::Context::from_loader_function_cstr(|s| get_proc_address(s)) };
                let px: Vec<(i32, i32)> = sizes.iter().map(|&s| texture_px(s, sf)).collect();
                match Gl::build(ctx, &px) {
                    Ok(g) => {
                        publish(g.targets.iter().map(Target::image).collect());
                        gl = Some(g);
                    }
                    Err(e) => fail(e),
                }
            }
            RenderingState::BeforeRendering => {
                let Some(g) = &mut gl else { return };
                let mut s = scene.borrow_mut();
                if let Some(who) = s.warm.pop().filter(|w| !g.programs.iter().any(|p| p.who == *w)) {
                    match unsafe { compile(&g.ctx, who) } {
                        Ok(program) => g.programs.push(Program { who, program, u: vec![] }),
                        Err(e) => log(&format!("live cast: {who:?}: {e}")),
                    }
                }
                if s.failed || !s.slots.iter().any(|s| s.fresh) {
                    return;
                }
                if let Err(e) = g.draw(&mut s.slots) {
                    drop(s);
                    fail(e);
                }
            }
            // Android keeps its starting window up until a frame is drawn into a new surface.
            RenderingState::AfterRendering => {
                crate::android::drawn();
            }
            RenderingState::RenderingTeardown => {
                if let Some(g) = gl.take() {
                    g.delete();
                }
            }
            _ => {}
        }
    });
    if let Err(e) = set {
        fail(format!("no rendering notifier: {e:?}"));
    }
}
