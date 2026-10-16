# Home-stage previews (branch preview/home-stage): four worlds for Clawd and Null to stand in. Renders each stage's
# backdrop for the phone (phone/ui/stage/<name>.png: logical px BW wide, ABOVE over and BELOW under the pair's feet
# line, centred on the screen) and an offline composite of the pair in it, lit, shadowed and reflected as the app's
# shaders draw them (uStage, uKey, uKeyCol, uShadow; the numbers come from STAGES, as phone/src/stage.rs has them).
# The backdrop is ray traced with Clawd's camera turned square on (same height, tilt and distance), so its floor is
# the floor his feet, shadow and reflection are on. Usage: python stages.py [name ...] [--scale 2.625] [--asset]
import os, sys, json, math
from render import VERT, HERE, np, Image, moderngl
import render as R

TL = r'D:\Outputs\CallMe\design\live3d\cast\timelines'
ASSETS = os.path.join(HERE, '..', '..', 'phone', 'ui', 'stage')
PREV = r'D:\Outputs\CallMe\design\step2-stage\offline'
BW, ABOVE, BELOW = 500, 640, 760  # the backdrop's logical size round the anchor (screen centre, feet line)
ASSET_SCALE = 1.5                 # backdrop px per logical px: smooth light, so 1.5 is plenty on a 3x screen

# Per stage: character scale k, logical px of floor under each picture, and the shader's stage uniforms.
STAGES = {
    # k: the pair's scale; floor: logical px under the feet line their reflections and shadows may reach (the desk's
    # front edge cuts them there); the rest are the shaders' uniforms (uStage.y and uShadow.z are filled in per picture)
    'spot': dict(k=1.08, floor=110, uStage=(0.32, 0, 0.08, 3.0), uKey=(0.0, 1.0, 0.15, 0.2), uKeyCol=(-0.05, -0.12, -0.2, -0.45), uShadow=(0.65, 3.0, 30, 0.35)),
}
# Each character's camera is turned round the floor's vertical from the backdrop's square-on one (clawd_cast.frag,
# null.frag): the key light is turned the same way, so it falls on them from where it falls on the backdrop.
AZ = {'clawd': -0.17, 'null': 0.24}


def turn(v, a):
    x, y, z = v[:3]
    return (x * math.cos(a) + z * math.sin(a), y, -x * math.sin(a) + z * math.cos(a)) + tuple(v[3:])

# --- the backdrop camera: Clawd's (clawd_cast.frag main) without its turn -------------------------------------------
TA = np.array([0.0, 1.27, 0.0])
EL, DIST, FL = 0.18, 9.5, 5.0
RO = TA + DIST * np.array([0.0, math.sin(EL), math.cos(EL)])
WW = (TA - RO) / np.linalg.norm(TA - RO)
UU = np.cross(WW, [0.0, 1.0, 0.0]); UU /= np.linalg.norm(UU)
VV = np.cross(UU, WW)
FEET_C = (-50.0, -96.25)  # Clawd's picture centre from the anchor at k = 1 (alive.rs framing in home.slint's row)


def rays(X, Y, k):
    """Unit view rays for logical points X (right) and Y (down) from the anchor. Clawd's picture is 250 k px tall and
    the shader's screen runs -1..1 over it; turned square on, the picture's centre sits on the screen's middle."""
    sx = (X - FEET_C[0] * k - 50.0 * k) / (125.0 * k)  # turned square on, the pair's middle is straight ahead
    sy = (FEET_C[1] * k - Y) / (125.0 * k)
    d = sx[..., None] * UU + sy[..., None] * VV + FL * WW
    return d / np.linalg.norm(d, axis=-1, keepdims=True)


def floor_hit(rd, y=0.0):
    t = np.where(rd[..., 1] < -1e-6, (y - RO[1]) / np.minimum(rd[..., 1], -1e-6), np.inf)
    return t, RO + rd * np.where(np.isfinite(t), t, 0)[..., None]


def anchor_world(k):
    """Where the anchor (the pair's middle, on the feet line) is on the floor."""
    t, p = floor_hit(rays(np.array([0.0]), np.array([0.0]), k))
    return p[0]


# --- small helpers -------------------------------------------------------------------------------------------------
def ss(a, b, x):
    t = np.clip((x - a) / (b - a), 0, 1)
    return t * t * (3 - 2 * t)


def nrm(v):
    return v / np.maximum(np.linalg.norm(v, axis=-1, keepdims=True), 1e-9)


def dot(a, b):
    return np.sum(a * b, axis=-1)


def hash2(ix, iy, seed=0):
    h = (ix * 374761393 + iy * 668265263 + seed * 2147483647) & 0xFFFFFFFF
    h = ((h ^ (h >> 13)) * 1274126177) & 0xFFFFFFFF
    return ((h ^ (h >> 16)) & 0xFFFF) / 65535.0


def vnoise(x, y, seed=0):
    ix, iy = np.floor(x).astype(np.int64), np.floor(y).astype(np.int64)
    fx, fy = x - ix, y - iy
    fx, fy = fx * fx * (3 - 2 * fx), fy * fy * (3 - 2 * fy)
    a, b = hash2(ix, iy, seed), hash2(ix + 1, iy, seed)
    c, d = hash2(ix, iy + 1, seed), hash2(ix + 1, iy + 1, seed)
    return (a * (1 - fx) + b * fx) * (1 - fy) + (c * (1 - fx) + d * fx) * fy


def fbm(x, y, n=4, seed=0):
    s, a, w = 0.0, 0.5, 0.0
    for i in range(n):
        s = s + a * vnoise(x, y, seed + i)
        w += a
        x, y, a = x * 2.03 + 1.7, y * 2.03 - 0.9, a * 0.5
    return s / w


# --- the four worlds (linear light) --------------------------------------------------------------------------------
def spot(X, Y, k, rng):
    """A black void, one cone of warm light straight down on a glossy black floor: a pool, the beam in faint haze,
    and the beam's reflection in the floor."""
    rd = rays(X, Y, k)
    P0 = anchor_world(k) + np.array([0.0, 0.0, -0.35])
    L = P0 + np.array([0.0, 9.0, 0.9])
    axis = nrm(P0 - L)
    half, soft = math.radians(13.0), math.radians(2.5)
    warm = np.array([1.0, 0.86, 0.7])

    def cone(p):
        d = nrm(p - L)
        return ss(math.cos(half + soft), math.cos(half - soft), dot(d, axis)) / np.maximum(dot(p - L, p - L), 16.0) * 81.0

    def haze(o, d, tmax, n=56):
        # in-scattered light along the ray, thinner higher up so the beam fades into the dark above the pair
        acc = 0.0
        t0, t1 = 2.0, np.minimum(tmax, 22.0)
        jit = rng.random(o.shape[:-1] if o.ndim > 1 else rd.shape[:-1])
        for i in range(n):
            t = t0 + (t1 - t0) * (i + jit) / n
            p = o + d * t[..., None]
            dens = np.exp(-np.maximum(p[..., 1], 0) / 7.0) * (p[..., 1] > 0)
            acc = acc + cone(p) * dens * (t1 - t0) / n
        return acc

    tf, F = floor_hit(rd)
    floor = np.isfinite(tf)
    col = np.zeros(rd.shape)
    beam = haze(np.broadcast_to(RO, rd.shape), rd, np.where(floor, tf, 22.0))
    col += warm * 0.026 * beam[..., None]
    # the pool: a dark glossy floor, its edge soft; a little light scattered round it
    irr = cone(F) * ss(0, 1, nrm(L - F)[..., 1])
    alb = 0.05 * (0.92 + 0.08 * fbm(F[..., 0] * 3, F[..., 2] * 3, 3, 7))
    pool = warm * (alb * irr)[..., None] * 1.7
    glow = warm * 0.0025 * np.exp(-np.hypot(F[..., 0] - P0[0], (F[..., 2] - P0[2]) * 1.0) / 2.6)[..., None]
    # the floor mirrors the beam: the same haze along the mirrored ray, softened by the floor's roughness
    r = rd * np.array([1, -1, 1])
    refl = np.zeros(tf.shape)
    for j in range(4):
        rj = nrm(r + rng.normal(0, 0.035, r.shape))
        refl = refl + haze(F, rj, np.full(tf.shape, 22.0), 28) / 4
    fres = 0.04 + 0.96 * (1 - np.clip(-rd[..., 1], 0, 1)) ** 5
    flo = pool + glow + warm * (0.013 * refl * np.minimum(fres * 4.0, 0.6))[..., None]
    col = np.where(floor[..., None], col + flo, col)
    return col


WORLDS = {'spot': spot}


def backdrop(name, scale, ssaa=2):
    k = STAGES[name]['k']
    w, h = round(BW * scale), round((ABOVE + BELOW) * scale)
    rng = np.random.default_rng(5)
    acc = np.zeros((h, w, 3))
    for sy in range(ssaa):
        for sx in range(ssaa):
            jy, jx = (sy + 0.5) / ssaa, (sx + 0.5) / ssaa
            Y, X = np.mgrid[0:h, 0:w].astype(np.float64)
            X = (X + jx) / scale - BW / 2
            Y = (Y + jy) / scale - ABOVE
            acc += WORLDS[name](X, Y, k, rng)
    return acc / (ssaa * ssaa)


def to_srgb8(lin, seed=1):
    s = np.where(lin <= 0.0031308, lin * 12.92, 1.055 * np.power(np.maximum(lin, 0), 1 / 2.4) - 0.055)
    # triangular dither: no banding in the dark gradients
    rng = np.random.default_rng(seed)
    n = (rng.random(s.shape) - rng.random(s.shape)) / 255.0
    return (np.clip(s + n, 0, 1) * 255 + 0.5).astype(np.uint8)


# --- the pair, as the app draws them -------------------------------------------------------------------------------
def program(name, define):
    src = open(os.path.join(HERE, name + '.frag'), encoding='utf8').read()
    version, rest = src.split('\n', 1)
    src = '#version 330\n' + define + '\n' + rest.replace('precision highp float;', '', 1)
    R.render('null', {}, 0.1)  # make the context
    return R._ctx.program(vertex_shader=VERT, fragment_shader=src)


def draw(prog, res, view, u):
    ctx = R._ctx
    fbo = ctx.simple_framebuffer(view, components=4)
    vao = ctx.vertex_array(prog, [])
    prog['uRes'].value = res
    for k, v in u.items():
        if k in prog:
            prog[k].value = tuple(float(x) for x in v)
    fbo.use()
    fbo.clear(0, 0, 0, 0)
    vao.render(moderngl.TRIANGLES, vertices=3)
    a = np.frombuffer(fbo.read(components=4), np.uint8).reshape(view[1], view[0], 4)[::-1].astype(np.float32) / 255
    vao.release()
    fbo.release()
    return a


# (picture w, h, feet as a fraction of h, its left edge from the anchor and its top above the feet line at k = 1)
PICS = {'clawd': (300, 250, -200, 221.25), 'null': (150, 230, 42, 194.6)}


def composite(name, uc, un, scale, pick='claude', bg=None):
    st = STAGES[name]
    k = st['k']
    img = (backdrop(name, scale, 1) if bg is None else bg).copy()
    out = to_srgb8(img).astype(np.float32) / 255
    for who, u, prog in (('clawd', uc, PC), ('null', un, PN)):
        pw, ph, left, top = PICS[who]
        res = (math.ceil(pw * k * scale), math.ceil(ph * k * scale))
        below = max(st['floor'] - (ph - PICS[who][3]), 0)  # from the picture's bottom, which is under the feet
        full = (res[0], math.ceil((ph + below) * k * scale))
        uu = dict(u)
        uu['uStage'] = (st['uStage'][0], full[1] - res[1], st['uStage'][2], st['uStage'][3])
        uu['uShadow'] = (st['uShadow'][0], st['uShadow'][1], st['uShadow'][2] * k * scale, st['uShadow'][3])
        uu['uKey'] = turn(st['uKey'], AZ[who])
        uu['uKeyCol'] = st['uKeyCol']
        a = draw(prog, res, full, uu)
        if (who == 'clawd') != (pick == 'claude'):
            a[..., 3] *= 0.55
        x = round((BW / 2 + left * k) * scale)
        y = round((ABOVE - top * k) * scale)
        h, w = a.shape[:2]
        reg = out[y:y + h, x:x + w]
        al = a[..., 3:]
        out[y:y + h, x:x + w] = a[..., :3] * al + reg * (1 - al)
    return Image.fromarray((np.clip(out, 0, 1) * 255 + 0.5).astype(np.uint8))


if __name__ == '__main__':
    args = sys.argv[1:]
    scale = float(args[args.index('--scale') + 1]) if '--scale' in args else 1.3125
    names = [a for a in args if a in STAGES] or list(STAGES)
    if '--asset' in args:
        os.makedirs(ASSETS, exist_ok=True)
        for n in names:
            im = Image.fromarray(to_srgb8(backdrop(n, ASSET_SCALE)))
            im.save(os.path.join(ASSETS, n + '.png'), optimize=True)
            print('asset', n, os.path.getsize(os.path.join(ASSETS, n + '.png')) // 1024, 'KB')
        sys.exit()
    os.makedirs(PREV, exist_ok=True)
    PC, PN = program('clawd_cast', '#define MAT_ID 0'), program('null', '')
    clawd, null = (json.load(open(os.path.join(TL, f'{n}.json'))) for n in ('clawd', 'null'))
    top = lambda d: max(d['story'], key=lambda f: f['u']['uBody'][1])['u']
    for n in names:
        bg = backdrop(n, scale, 1)
        composite(n, clawd['poses']['idle'], null['poses']['idle'], scale, bg=bg).save(os.path.join(PREV, f'{n}-idle.png'))
        composite(n, top(clawd), null['poses']['idle'], scale, bg=bg).save(os.path.join(PREV, f'{n}-hop.png'))
        print('saved', n)
