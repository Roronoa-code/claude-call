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
    'desk': dict(k=1.0, floor=60, uStage=(0.32, 0, 0.08, 2.2), uKey=(-0.4, 0.7, 0.42, 0.3), uKeyCol=(0.1, -0.05, -0.2, -0.3), uShadow=(0.5, 4.0, 3, 0.2)),
    'moon': dict(k=0.9, floor=110, uStage=(0.30, 0, 0.08, 2.6), uKey=(0.22, 0.72, 0.66, -0.2), uKeyCol=(-0.3, -0.12, 0.1, -0.35), uShadow=(0.6, 6.0, 30, 0.15)),
    'record': dict(k=0.84, floor=70, uStage=(0, 0, 0, 0), uKey=(-0.3, 1.0, 0.45, 0.6), uKeyCol=(0.04, -0.02, -0.08, -0.3), uShadow=(0.45, 4.0, 20, 0.1)),
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


def desk(X, Y, k, rng):
    """A dark walnut desk at night, seen at the toys' eye level: a warm lamp off to the upper left pools on the desk
    and fades up the wall, a screen's cool violet glows on the wall to the right, the desk's rounded front edge catches
    the lamp, and its front, in shadow, holds the controls."""
    rd = rays(X, Y, k)
    P0 = anchor_world(k)
    # the front edge (where the pair's reflections are cut: `floor` px under the feet), the wall, the edge's roundness
    zf, zw, rb = floor_hit(rays(np.array([0.0]), np.array([float(STAGES['desk']['floor'])]), k))[1][0][2], P0[2] - 2.15, 0.05
    lamp = np.array([P0[0] - 3.8, 4.6, P0[2] + 0.9])
    lampc = np.array([1.0, 0.66, 0.38]) * 7.0
    screen = np.array([P0[0] + 4.2, 1.9, zw])
    screenc = np.array([0.40, 0.34, 1.0])
    # wall
    tw = (zw - RO[2]) / rd[..., 2]
    W = RO + rd * tw[..., None]
    dl = lamp - W
    d2 = dot(dl, dl)
    # the lamp's shade: its light falls downwards, so the wall darkens above it
    shade_ = ss(0.15, 0.32, dl[..., 1] / np.sqrt(d2))
    wall_irr = lampc * (np.maximum(dl[..., 2], 0) / np.sqrt(d2) / d2 * shade_)[..., None]
    sd = np.hypot((W[..., 0] - screen[0]) / 1.7, (W[..., 1] - screen[1]) / 1.2)
    wall_scr = screenc * (0.010 * np.exp(-sd * sd) + 0.003 * np.exp(-sd))[..., None]
    plaster = 0.09 * (0.94 + 0.06 * fbm(W[..., 0] * 1.4, W[..., 1] * 1.4, 4, 3))
    wallc = plaster[..., None] * wall_irr + wall_scr + 0.0012
    wallc *= (0.45 + 0.55 * ss(0.0, 0.45, W[..., 1]))[..., None]  # the corner where wall meets desk
    # desk top
    tt, T = floor_hit(rd)
    on_top = np.isfinite(tt) & (T[..., 2] <= zf - rb) & (tt < tw)
    # the rounded edge: a cylinder along x at (y=-rb, z=zf-rb)
    oy, oz = RO[1] + rb, RO[2] - (zf - rb)
    a = rd[..., 1] ** 2 + rd[..., 2] ** 2
    b = 2 * (oy * rd[..., 1] + oz * rd[..., 2])
    c = oy * oy + oz * oz - rb * rb
    disc = b * b - 4 * a * c
    te = (-b - np.sqrt(np.maximum(disc, 0))) / (2 * a)
    E = RO + rd * te[..., None]
    on_edge = (disc > 0) & (E[..., 1] >= -rb) & (E[..., 2] >= zf - rb) & ~on_top
    # front face
    tfr = (zf - RO[2]) / rd[..., 2]
    Fr = RO + rd * tfr[..., None]
    on_front = ~on_top & ~on_edge & (Fr[..., 1] < 0)

    def wood(along, across, seed):
        g = fbm(along * 0.35, across * 9.0, 5, seed)
        rings = 0.5 + 0.5 * np.sin(across * 28.0 + g * 9.0)
        base = np.array([0.060, 0.032, 0.018])
        return base * (0.78 + 0.22 * rings[..., None] + 0.25 * (g[..., None] - 0.5))

    def lit(p, n, alb, gloss, rough):
        dl = lamp - p
        d2 = dot(dl, dl)
        ldir = dl / np.sqrt(d2)[..., None]
        ndl = np.maximum(dot(n, ldir), 0)
        v = -rd
        h = nrm(ldir + v)
        spec = np.maximum(dot(n, h), 0) ** rough * (rough + 2) / 8
        fres = 0.04 + 0.96 * (1 - np.maximum(dot(n, v), 0)) ** 5
        out = alb * (lampc * (ndl / d2)[..., None]) + lampc * (spec * ndl * gloss * fres * 2.5 / d2)[..., None]
        # the screen's violet sheen on the desk, strongest towards the back
        sd = np.hypot((p[..., 0] - screen[0]) / 2.4, (p[..., 2] - screen[2]) / 1.6)
        out += screenc * (0.006 * np.exp(-sd * sd) * np.maximum(n[..., 1], 0))[..., None]
        return out + alb * 0.02

    up = np.zeros(rd.shape); up[..., 1] = 1
    topc = lit(T, up, wood(T[..., 0], T[..., 2], 11), 1.0, 40)
    ne = nrm(np.stack([np.zeros(te.shape), E[..., 1] + rb, E[..., 2] - (zf - rb)], -1))
    edgec = lit(E, ne, wood(E[..., 0], E[..., 2], 11) * 1.1, 1.2, 50)
    # the front: in the desk's own shadow, only a little light from the room; darker further down
    frontc = wood(Fr[..., 0], Fr[..., 1] * 0.6, 23) * 0.22 * (0.2 + 0.8 * np.exp(Fr[..., 1] / 0.3))[..., None]
    frontc += lampc * 0.0009 * np.exp(Fr[..., 1] / 0.12)[..., None]  # the edge's underside, a hair of lamp
    col = np.where(on_front[..., None], frontc, wallc)
    col = np.where(on_top[..., None], topc, col)
    col = np.where(on_edge[..., None], edgec, col)
    return col


def moon(X, Y, k, rng):
    """A room at night: moonlight through a tall window high behind the viewer, off to the right, lays its panes over
    the polished dark floor where the pair stand and folds up the wall behind them."""
    rd = rays(X, Y, k)
    P0 = anchor_world(k)
    zw = P0[2] - 3.0
    Ld = nrm(np.array(STAGES['moon']['uKey'][:3]))
    # the window, as seen along the light: parallel light, so a point's place in it is its position across the light
    a1 = nrm(np.cross(Ld, [0.0, 1.0, 0.0]))
    a2 = np.cross(a1, Ld)
    O = P0 + np.array([0.6, 0.0, -1.9])  # where the window's middle falls
    half = (2.8, 2.9)                     # its half width and half height, across the light
    pen = 0.10                            # the soft edge: the moon and the bright sky round it are a broad source
    moonc = np.array([0.50, 0.62, 1.0]) * 1.15
    amb = np.array([0.0030, 0.0040, 0.0085])

    def lit_by_window(p):
        u, v = dot(p - O, a1), dot(p - O, a2)
        box = ss(-pen, pen, half[0] - np.abs(u)) * ss(-pen, pen, half[1] - np.abs(v))
        # two columns and three rows of panes: mullions down the middle and at a third and two thirds of the height
        mu = np.abs(u)
        mv = np.minimum(np.abs(v - half[1] / 3), np.abs(v + half[1] / 3))
        mull = (1 - ss(0.035, 0.035 + 2 * pen, mu)) + (1 - ss(0.035, 0.035 + 2 * pen, mv))
        return box * np.clip(1 - mull, 0, 1)

    tw = (zw - RO[2]) / rd[..., 2]
    W = RO + rd * tw[..., None]
    tf, F = floor_hit(rd)
    on_floor = np.isfinite(tf) & (tf < tw)
    wl = lit_by_window(W)
    walb = 0.075 * (0.93 + 0.07 * fbm(W[..., 0] * 1.2, W[..., 1] * 1.2, 4, 5))
    wallc = walb[..., None] * (moonc * (wl * Ld[2])[..., None] + amb * 6)
    # a faint cool bounce from the lit floor at the foot of the wall, and the corner itself a little darker
    wallc *= (0.55 + 0.45 * ss(0.0, 0.5, W[..., 1]))[..., None]
    fl = lit_by_window(F)
    mott = fbm(F[..., 0] * 0.8, F[..., 2] * 0.8, 5, 9)
    falb = 0.06 * (0.85 + 0.3 * mott)
    v = -rd
    h = nrm(Ld + v)
    spec = np.maximum(h[..., 1], 0) ** 40 * 0.6
    floorc = falb[..., None] * (moonc * (fl * Ld[1])[..., None] + amb * 6) + moonc * (fl * spec)[..., None] * 0.05
    floorc *= (0.6 + 0.4 * ss(0.0, 0.6, F[..., 2] - zw))[..., None]
    # the lit wall mirrored softly in the polished floor
    r = rd * np.array([1, -1, 1])
    tr = (zw - F[..., 2]) / r[..., 2]
    Rw = F + r * tr[..., None]
    rl = lit_by_window(Rw) * (Rw[..., 1] > 0)
    fres = 0.04 + 0.96 * (1 - np.clip(-rd[..., 1], 0, 1)) ** 5
    floorc += moonc * (rl * Ld[2] * 0.075 * np.minimum(fres * 3, 0.5) * np.exp(-Rw[..., 1] / 2.0))[..., None]
    return np.where(on_floor[..., None], floorc, wallc)


def record(X, Y, k, rng):
    """A black vinyl record floating in the dark as their stage: its grooves catch the lights as long arcs, an orange
    label at the middle, and a thin bright rim."""
    rd = rays(X, Y, k)
    P0 = anchor_world(k)
    C = P0 + np.array([0.0, 0.0, -0.45])
    Rr, th = 2.75, 0.08
    L1, L1c = nrm(np.array([-0.45, 0.85, 0.55])), np.array([1.0, 0.86, 0.72]) * 1.1   # warm key, upper left
    L2, L2c = nrm(np.array([0.7, 0.45, -0.6])), np.array([0.55, 0.5, 1.0]) * 0.9     # violet rim, behind right
    col = np.zeros(rd.shape) + np.array([0.0030, 0.0028, 0.0036])
    col += np.array([0.012, 0.0065, 0.004]) * np.exp(-((X / 240.0) ** 2 + ((Y + 40) / 140.0) ** 2))[..., None]
    col += np.array([0.006, 0.005, 0.013]) * np.exp(-((X / 280.0) ** 2 + ((Y + 190) / 240.0) ** 2))[..., None]
    tt, T = floor_hit(rd)
    q = T - C
    rr = np.hypot(q[..., 0], q[..., 2])
    on_top = np.isfinite(tt) & (rr < Rr)
    o = RO - C
    a = rd[..., 0] ** 2 + rd[..., 2] ** 2
    b = 2 * (o[0] * rd[..., 0] + o[2] * rd[..., 2])
    c = o[0] ** 2 + o[2] ** 2 - Rr * Rr
    disc = b * b - 4 * a * c
    ts = (-b - np.sqrt(np.maximum(disc, 0))) / (2 * a)
    S = RO + rd * ts[..., None]
    on_side = (disc > 0) & (S[..., 1] < 0) & (S[..., 1] > -th) & ~on_top
    v = -rd
    u = rr / Rr
    tang = nrm(np.stack([-q[..., 2], np.zeros(rr.shape), q[..., 0]], -1))

    def aniso(L, n_exp):
        h = nrm(L + v)
        th_ = dot(tang, h)
        return np.sqrt(np.clip(1 - th_ * th_, 0, 1)) ** n_exp

    tracks = np.ones(rr.shape)
    for g in (0.55, 0.66, 0.76, 0.86):  # the gaps between songs: smoother, darker rings
        tracks *= 1 - 0.55 * np.exp(-((u - g) / 0.006) ** 2)
    grooved = ss(0.40, 0.43, u) * (1 - ss(0.965, 0.985, u))
    fine = 0.85 + 0.15 * np.sin(rr * 900.0)
    gloss = grooved * tracks * fine
    top = np.zeros(rd.shape) + np.array([0.005, 0.005, 0.006])
    top += L1c * (aniso(L1, 140) * 0.20 * gloss)[..., None] + L2c * (aniso(L2, 110) * 0.16 * gloss)[..., None]
    smooth = (1 - grooved) * (u > 0.345)
    hs = nrm(L1 + v)
    top += L1c * (smooth * np.maximum(hs[..., 1], 0) ** 80 * 0.08)[..., None]
    lab = u < 0.345
    paper = np.array([0.62, 0.17, 0.06]) * (0.9 + 0.1 * fbm(q[..., 0] * 9, q[..., 2] * 9, 3, 4))[..., None]
    paper *= (1 - 0.35 * np.exp(-((u - 0.30) / 0.004) ** 2))[..., None]
    labc = paper * (0.06 + 0.32 * L1[1])
    labc = np.where((u < 0.025)[..., None], np.array([0.002, 0.002, 0.002]), labc)
    top = np.where(lab[..., None], labc, top)
    sidec = np.array([0.004, 0.004, 0.005]) + L1c * 0.05 * ss(-th, 0, S[..., 1])[..., None]
    lip = np.exp(-((rr - Rr) / 0.012) ** 2)
    top += np.array([0.30, 0.27, 0.25]) * (lip * 0.6)[..., None]
    col = np.where(on_side[..., None], sidec, col)
    col = np.where(on_top[..., None], top, col)
    return col


WORLDS = {'spot': spot, 'desk': desk, 'moon': moon, 'record': record}


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
