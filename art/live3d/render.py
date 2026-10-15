# Offline renderer for the cast shaders (moderngl, desktop GL 3.3): the same GLSL ES source the phone compiles,
# with only the version line swapped. Used by the sheet, side-by-side and video scripts in this folder.
import sys
sys.path.insert(0, 'B:/Tools/pylibs')
import os, json, moderngl, numpy as np
from PIL import Image

HERE = os.path.dirname(os.path.abspath(__file__))
OUT = r'D:\Outputs\CallMe\design\live3d\cast'
VERT = '#version 330\nvoid main() { vec2 p = vec2(float((gl_VertexID << 1) & 2), float(gl_VertexID & 2)); gl_Position = vec4(p * 2.0 - 1.0, 0.0, 1.0); }'
# Each shader's image size in logical pixels (what Slint would draw), as alive.rs's W and H.
SIZE = {'clawd_cast': (300, 250), 'null': (150, 230)}

_ctx = None
_progs = {}


def program(name):
    global _ctx
    if _ctx is None:
        _ctx = moderngl.create_standalone_context(require=330)
    if name not in _progs:
        src = open(os.path.join(HERE, name + '.frag'), encoding='utf8').read()
        src = src.replace('#version 300 es', '#version 330', 1).replace('precision highp float;', '', 1)
        _progs[name] = _ctx.program(vertex_shader=VERT, fragment_shader=src)
    return _progs[name]


_fbos = {}


def render(name, uniforms, scale=2.75):
    """RGBA float image (premultiplied colour is NOT used: straight colour plus alpha), top row first."""
    prog = program(name)
    w, h = (round(v * scale) for v in SIZE[name])
    key = (w, h)
    if key not in _fbos:
        _fbos[key] = (_ctx.simple_framebuffer((w, h), components=4), _ctx.vertex_array(prog, []))
    fbo, _ = _fbos[key]
    vao = _ctx.vertex_array(prog, [])
    prog['uRes'].value = (w, h)
    for k, v in uniforms.items():
        if k in prog:
            prog[k].value = tuple(float(x) for x in v)
    fbo.use()
    fbo.clear(0, 0, 0, 0)
    vao.render(moderngl.TRIANGLES, vertices=3)
    vao.release()
    return np.frombuffer(fbo.read(components=4), np.uint8).reshape(h, w, 4)[::-1].astype(np.float32) / 255


def stage(w, h, scale):
    """The home screen's stage behind the toys: near-black with a warm glow and a faint violet ring."""
    y, x = np.mgrid[0:h, 0:w].astype(np.float32)
    r = np.hypot(x - w / 2, y - h * 0.6) / (220 * scale)
    bg = np.array([0x11, 0x12, 0x14], np.float32) / 255
    warm = np.clip(1 - r / 0.4, 0, 1)[..., None] * (0x2e / 255) * (np.array([0xd9, 0x77, 0x57]) / 255 - bg)
    cool = np.clip(1 - np.abs(r - 0.4) / 0.3, 0, 1)[..., None] * (0x14 / 255) * (np.array([0x8a, 0x5c, 0xff]) / 255 - bg)
    return bg + warm + cool


def over(rgba, bg=None, scale=2.75):
    h, w = rgba.shape[:2]
    if bg is None:
        bg = stage(w, h, scale)
    a = rgba[..., 3:]
    return Image.fromarray((np.clip(rgba[..., :3] * a + bg * (1 - a), 0, 1) * 255 + 0.5).astype(np.uint8))


if __name__ == '__main__':
    # quick look: render.py <shader> <out.png> '<json uniforms>' [scale]
    u = json.loads(sys.argv[3])
    s = float(sys.argv[4]) if len(sys.argv) > 4 else 2.75
    over(render(sys.argv[1], u, s), scale=s).save(sys.argv[2])
