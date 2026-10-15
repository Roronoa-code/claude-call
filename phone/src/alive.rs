//! The live Clawd on the home screen: a real 3D toy drawn by the GPU every frame, not swapped sprites. One fragment
//! shader raymarches him from soft shapes (a rounded body, stubby arms, four legs, glossy eyes) with vinyl lighting,
//! into a texture Slint shows as an image (Toys.clawd-live). The life comes from here: toys.rs still decides when he
//! blinks, glances and hops, and springs turn those steps into breathing, squash, jiggle, floppy arms and eyes that
//! follow your finger.
use crate::toys::{C_BLINK, C_HAPPY, C_LOOK_LEFT, C_LOOK_RIGHT, Toy};
use std::f32::consts::TAU;

/// The image's size in logical pixels; toys.slint draws it at exactly this size.
pub const W: f32 = 300.0;
pub const H: f32 = 250.0;

/// What the shader draws this frame (its four uniforms).
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct Pose {
    pub body: [f32; 4], // height scale (squash), lift, lean, turn
    pub arms: [f32; 4], // left and right arm angles
    pub eyes: [f32; 4], // look x, look y, open, sleepy lids
    pub mood: [f32; 4], // happy
}

/// A damped spring: `hz` is how fast it moves, `zeta` below 1 overshoots and wobbles before settling.
#[derive(Clone, Copy, Default)]
struct Spring {
    x: f32,
    v: f32,
}
impl Spring {
    fn at(x: f32) -> Self {
        Spring { x, v: 0.0 }
    }
    fn step(&mut self, to: f32, hz: f32, zeta: f32, dt: f32) {
        let w = TAU * hz;
        self.v += (w * w * (to - self.x) - 2.0 * zeta * w * self.v) * dt;
        self.x += self.v * dt;
    }
}

const GRAVITY: f32 = 31.0; // units/s²: a 16 px hop is in the air for about a quarter of a second
const UNITS_PER_PX: f32 = 0.0163; // toys.rs lifts in pixels
const ARM_REST: f32 = -0.5; // out and a little down
const ARM_UP: f32 = 0.55;

pub struct Live {
    pub toy: Toy,
    t: f32,
    squash: Spring,
    lean: Spring,
    turn: Spring,
    look: (Spring, Spring),
    open: Spring,
    lid: Spring,
    happy: Spring,
    arms: (Spring, Spring),
    y: f32,
    vy: f32,
    v0: f32, // the speed he took off at
    toy_lift: f32,
    breath: f32,
    asleep_for: f32,
    finger: Option<(f32, f32)>,
    finger_up: Option<f32>, // when the finger left: he keeps looking there a moment
    moving: bool,
    gaze: (f32, f32), // where the eyes last darted to
    next_saccade: f32,
    blink_until: f32, // a blink of his own: on a big change of gaze
    rng: u64,
}

impl Live {
    pub fn new(seed: u64) -> Self {
        let rest = Spring::at(ARM_REST);
        Live {
            toy: Toy::new(false, seed),
            t: 0.0,
            squash: Spring::at(1.0),
            lean: Spring::default(),
            turn: Spring::default(),
            look: (Spring::default(), Spring::default()),
            open: Spring::at(1.0),
            lid: Spring::default(),
            happy: Spring::default(),
            arms: (rest, rest),
            y: 0.0,
            vy: 0.0,
            v0: 1.0,
            toy_lift: 0.0,
            breath: 0.0,
            asleep_for: 0.0,
            finger: None,
            finger_up: None,
            moving: false,
            gaze: (0.0, 0.0),
            next_saccade: 1.0,
            blink_until: 0.0,
            rng: seed.rotate_left(17) | 1,
        }
    }

    fn rand(&mut self) -> f32 {
        self.rng ^= self.rng << 13;
        self.rng ^= self.rng >> 7;
        self.rng ^= self.rng << 17;
        (self.rng % 10_000) as f32 / 10_000.0
    }

    pub fn hop(&mut self, now: f32) {
        self.asleep_for = 0.0; // a tap wakes him for a moment
        self.toy.hop(now);
    }

    /// The finger, in logical pixels from the middle of his image; `down` false when it lifts.
    pub fn point(&mut self, x: f32, y: f32, down: bool, now: f32) {
        self.finger = Some((x, y));
        self.finger_up = if down { None } else { Some(now) };
    }

    /// True while anything is still moving (beyond breathing): then he is drawn at the full frame rate.
    pub fn moving(&self) -> bool {
        self.moving
    }

    /// Advances to `now` (seconds). True when the sprite fallback's body changed.
    pub fn tick(&mut self, now: f32, asleep: bool) -> bool {
        let changed = self.toy.tick(now, asleep);
        let dt = (now - self.t).clamp(0.0, 0.05);
        self.t = now;
        let b = self.toy.body;
        self.asleep_for = if asleep { self.asleep_for + dt } else { 0.0 };
        if self.finger_up.is_some_and(|t| now - t > 1.4) {
            (self.finger, self.finger_up) = (None, None);
        }

        let happy = b.pose == C_HAPPY;
        let (mut lx, mut ly) = match b.pose {
            C_LOOK_LEFT => (-0.75, -0.1),
            C_LOOK_RIGHT => (0.75, -0.1),
            _ => (0.0, 0.0),
        };
        if let Some((x, y)) = self.finger {
            (lx, ly) = ((x / 140.0).clamp(-1.0, 1.0), (-y / 170.0).clamp(-1.0, 1.0));
        }
        // The eyes never rest quite still: little darts to somewhere nearby every second or two (smaller while
        // following a finger), and a blink of their own when the gaze jumps far.
        if now >= self.next_saccade {
            let r = if self.finger.is_some() { 0.06 } else { 0.22 };
            self.gaze = ((self.rand() - 0.5) * 2.0 * r, (self.rand() - 0.5) * 1.4 * r);
            self.next_saccade = now + 0.5 + self.rand().powi(2) * 2.4;
        }
        if !asleep && (lx - self.look.0.x).abs() > 0.7 && now > self.blink_until + 1.0 {
            self.blink_until = now + 0.12;
        }
        (lx, ly) = ((lx + self.gaze.0).clamp(-1.0, 1.0), (ly + self.gaze.1).clamp(-1.0, 1.0));
        let open = if b.pose == C_BLINK || now < self.blink_until { 0.0 } else { 1.0 };
        // Asleep: lids droop half-shut, then close a few seconds later. Awake again for a happy hop.
        let lid = match (asleep && !happy, self.asleep_for > 5.0) {
            (false, _) => 0.0,
            (true, false) => 0.55,
            (true, true) => 1.0,
        };
        // A hop: toys.rs lifts him; here he is thrown up and falls back, and the landing squashes him and swings the arms.
        if b.lift > self.toy_lift + 1.0 && self.y <= 0.0 {
            self.vy = (2.0 * GRAVITY * b.lift * UNITS_PER_PX).sqrt();
            self.v0 = self.vy;
            self.arms.0.v -= 3.0;
            self.arms.1.v -= 3.0;
            self.lean.v += (self.rand() - 0.5) * 0.6; // never quite the same hop twice
        }
        self.toy_lift = b.lift;
        let wobble = 0.012 * (now * 0.9).sin() + 0.008 * (now * 0.37 + 1.0).sin(); // never quite still
        let arm_to = if happy { ARM_UP } else { ARM_REST };
        let (land_wobble, airborne) = ((self.rand() - 0.5) * 0.5, self.y > 0.0);

        let steps = (dt * 240.0).ceil().max(1.0);
        let h = dt / steps;
        for _ in 0..steps as usize {
            if self.y > 0.0 || self.vy > 0.0 {
                self.vy -= GRAVITY * h;
                self.y += self.vy * h;
                if self.y <= 0.0 && airborne {
                    self.squash.v -= self.vy.abs() * 0.3;
                    self.lean.v += land_wobble; // he lands a little off balance and wobbles upright
                    self.arms.0.v -= 4.0;
                    self.arms.1.v -= 4.0;
                    (self.y, self.vy) = (0.0, 0.0);
                }
            }
            // In the air he stretches with his speed (most at take-off and just before landing), round at the top.
            let squash_to = if self.y > 0.0 { 1.0 + 0.08 * self.vy.abs() / self.v0 } else { b.squash };
            self.squash.step(squash_to, 4.2, 0.5, h);
            self.lean.step(b.lean.to_radians() + lx * 0.03 + wobble, 2.0, 0.45, h);
            self.turn.step(lx * 0.22 + 0.04 * (now * 0.21).sin(), 1.4, 0.75, h);
            // Eyes dart (fast, nearly no overshoot) while the body turns after them slowly: overlapping action.
            self.look.0.step(lx, 9.0, 0.9, h);
            self.look.1.step(ly, 9.0, 0.9, h);
            self.open.step(open, 16.0, 1.0, h);
            self.lid.step(lid, 1.1, 1.0, h);
            self.happy.step(if happy { 1.0 } else { 0.0 }, 4.0, 0.85, h);
            // The arms trail the body: they lift as he squashes and swing against a lean, a beat late.
            let carry = (1.0 - self.squash.x) * 1.6;
            self.arms.0.step(arm_to + carry + self.lean.x * 2.5, 2.3, 0.4, h);
            self.arms.1.step(arm_to + carry - self.lean.x * 2.5, 2.5, 0.4, h);
        }
        let period = if asleep { 5.2 } else { 3.2 };
        self.breath = (self.breath + dt * TAU / period) % TAU;
        let springs = [&self.squash, &self.lean, &self.turn, &self.look.0, &self.look.1, &self.open, &self.lid, &self.happy, &self.arms.0, &self.arms.1];
        self.moving = self.y > 0.0 || self.finger.is_some() || springs.iter().any(|s| s.v.abs() > 0.02);
        changed
    }

    pub fn pose(&self) -> Pose {
        let amount = 0.016 + 0.012 * self.lid.x.clamp(0.0, 1.0); // deeper breaths as he dozes
        let breath = self.breath.sin();
        let lag = (self.breath - 0.6).sin() * 0.05; // the arms rise a beat after the body
        Pose {
            body: [self.squash.x.clamp(0.86, 1.08) * (1.0 + amount * breath), self.y, self.lean.x, self.turn.x],
            arms: [self.arms.0.x + lag, self.arms.1.x + lag, 0.0, 0.0],
            eyes: [self.look.0.x, self.look.1.x, self.open.x.clamp(0.0, 1.0), self.lid.x.clamp(0.0, 1.0)],
            mood: [self.happy.x.clamp(0.0, 1.0), 0.0, 0.0, 0.0],
        }
    }
}

/// The fragment shader. Units: the body is 2 wide; y is up from the floor; he faces +z.
pub const FRAG: &str = r#"#version 300 es
precision highp float;
uniform vec2 uRes;
uniform vec4 uBody; // height scale, lift, lean, turn
uniform vec4 uArms; // left, right arm angle
uniform vec4 uEyes; // look x, look y, open, lids
uniform vec4 uMood; // happy
out vec4 outColor;

const vec3 BODY = vec3(1.06, 0.82, 0.80); // half size
const float BODY_Y = 1.22;               // body centre above the floor
const vec2 EYE = vec2(0.37, 1.37);       // the right eye's centre on the face
const float FL = 5.0;                    // lens: a long one, like a product shot

mat2 rot(float a) { float c = cos(a), s = sin(a); return mat2(c, s, -s, c); }
float smin(float a, float b, float k) { float h = max(k - abs(a - b), 0.0) / k; return min(a, b) - h * h * k * 0.25; }
float sdRoundBox(vec3 p, vec3 b, float r) { vec3 q = abs(p) - b + r; return length(max(q, 0.0)) + min(max(q.x, max(q.y, q.z)), 0.0) - r; }
float sdEllipsoid(vec3 p, vec3 r) { float k0 = length(p / r), k1 = length(p / (r * r)); return k0 * (k0 - 1.0) / k1; }
// A leg: a short post, a little wider at the foot, with a flat rounded sole.
float sdLeg(vec3 p) {
    float ra = 0.27 - 0.04 * clamp(p.y / 0.45, 0.0, 1.0);
    vec2 d = vec2(length(p.xz) - ra + 0.07, abs(p.y - 0.35) - 0.35 + 0.07);
    return min(max(d.x, d.y), 0.0) + length(max(d, 0.0)) - 0.07;
}

// World to his own space: hop, lean on the feet, turn, then squash about the feet keeping his volume.
vec3 local(vec3 p) {
    p.y -= uBody.y;
    p.xy = rot(uBody.z) * p.xy;
    p.xz = rot(uBody.w) * p.xz;
    float sx = inversesqrt(uBody.x);
    return p / vec3(sx, uBody.x, sx);
}
vec3 worldDir(vec3 v) { v.xz = rot(-uBody.w) * v.xz; v.xy = rot(-uBody.z) * v.xy; return v; }

float arm(vec3 q, float side, float ang) {
    q.x *= side;
    q -= vec3(0.9, 1.12, 0.16);
    q.xz = rot(0.5) * q.xz;
    q.xy = rot(-ang) * q.xy;
    return sdEllipsoid(q - vec3(0.38, 0.0, 0.0), vec3(0.40, 0.275, 0.29));
}

float map(vec3 p) {
    vec3 q = local(p);
    vec3 c = q - vec3(0.0, BODY_Y, 0.0);
    c.xz *= 1.0 + 0.035 * clamp(c.y / BODY.y, -1.0, 1.0); // a little narrower at the top, like a pillow
    vec3 u = clamp(c / BODY, -1.0, 1.0);
    vec3 puff = (1.0 - u * u) * (1.0 - u * u); // flat at the edges, so no crease
    // a marshmallow: every face swells a little in the middle, the top most of all
    float d = sdRoundBox(c, BODY, 0.52) - 0.08 * (puff.y * puff.z + puff.x * puff.y + 0.6 * puff.x * puff.z);
    float legs = sdLeg(vec3(abs(q.x) - 0.62, q.y, abs(q.z) - 0.42));
    d = smin(d, legs, 0.13);
    d = smin(d, min(arm(q, -1.0, uArms.x), arm(q, 1.0, uArms.y)), 0.05);
    return 0.9 * d * min(uBody.x, inversesqrt(uBody.x));
}

vec3 normal(vec3 p) {
    const vec2 k = vec2(1.0, -1.0);
    const float e = 0.0015;
    return normalize(k.xyy * map(p + k.xyy * e) + k.yyx * map(p + k.yyx * e) + k.yxy * map(p + k.yxy * e) + k.xxx * map(p + k.xxx * e));
}

float occlusion(vec3 p, vec3 n) {
    float o = 0.0, s = 1.0;
    for (int i = 0; i < 5; i++) {
        float h = 0.02 + 0.08 * float(i);
        o += (h - map(p + n * h)) * s;
        s *= 0.75;
    }
    return clamp(1.0 - 1.5 * o, 0.0, 1.0);
}

// The eyes, as shapes on the face (2D distance, negative inside): tall ovals that squeeze into a closed arc to
// blink, sleepy lids that come down from the top and close into a smile-shaped line, and ^ ^ when happy.
float sdEllipse(vec2 p, vec2 r) { float k0 = length(p / r), k1 = length(p / (r * r)); return k0 * (k0 - 1.0) / k1; }
float sdArc(vec2 p, float ap, float ra, float rb) {
    vec2 sc = vec2(sin(ap), cos(ap));
    p.x = abs(p.x);
    return ((sc.y * p.x > sc.x * p.y) ? length(p - sc * ra) : abs(length(p) - ra)) - rb;
}
float sdSeg(vec2 p, vec2 a, vec2 b, float r) { vec2 pa = p - a, ba = b - a; float h = clamp(dot(pa, ba) / dot(ba, ba), 0.0, 1.0); return length(pa - ba * h) - r; }
float eye(vec2 p) {
    float happy = uMood.x, open = uEyes.z * clamp(1.0 - 2.0 * happy, 0.0, 1.0); // to ^ ^ by way of a blink
    float f = mix(0.07, 1.0, open);
    // squeezes to a sliver, widening as it goes (so a half-shut eye is a wide oval, never a dot)
    float e = sdEllipse(p - vec2(0.0, 0.05 * (1.0 - open)), vec2(0.14 * clamp(pow(f, -0.75), 1.0, 1.5), 0.29 * f));
    e = max(e, p.y - 0.29 * (1.0 - 2.0 * uEyes.w));
    e = mix(sdArc(p + vec2(0.0, 0.12), 1.0, 0.17, 0.032), e, smoothstep(0.02, 0.18, open));
    e = mix(e, sdArc(vec2(p.x, -p.y) + vec2(0.0, 0.02), 1.0, 0.17, 0.03), smoothstep(0.75, 1.0, uEyes.w));
    float chev = min(sdSeg(p, vec2(-0.14, -0.07), vec2(0.0, 0.08), 0.05), sdSeg(p, vec2(0.14, -0.07), vec2(0.0, 0.08), 0.05));
    return mix(e, chev, smoothstep(0.5, 1.0, happy));
}
float eyes(vec2 f) {
    vec2 look = uEyes.xy * vec2(0.17, 0.09);
    vec2 c = vec2(f.x > look.x ? EYE.x : -EYE.x, EYE.y) + look;
    return eye(f - c);
}

// The studio he is lit in, as seen in his gloss: a dark room, a big softbox up on the left, a dim card on the right.
vec3 softbox(vec3 r) {
    return vec3(1.0, 0.96, 0.9) * 5.0 * smoothstep(0.86, 0.94, dot(r, normalize(vec3(-0.35, 0.93, 0.15))))
         + vec3(0.55, 0.5, 0.7) * 1.2 * smoothstep(0.86, 0.95, dot(r, normalize(vec3(0.75, 0.35, 0.55))));
}
vec3 env(vec3 r) { return mix(vec3(0.01, 0.01, 0.015), vec3(0.3, 0.26, 0.24), smoothstep(0.0, 0.9, r.y)) + softbox(r); }

// Fine orange-peel grain, like moulded vinyl.
float hash(vec3 p) { p = fract(p * 0.3183099 + 0.1); p *= 17.0; return fract(p.x * p.y * p.z * (p.x + p.y + p.z)); }
float noise(vec3 x) {
    vec3 i = floor(x), f = fract(x);
    f = f * f * (3.0 - 2.0 * f);
    return mix(mix(mix(hash(i), hash(i + vec3(1, 0, 0)), f.x), mix(hash(i + vec3(0, 1, 0)), hash(i + vec3(1, 1, 0)), f.x), f.y),
               mix(mix(hash(i + vec3(0, 0, 1)), hash(i + vec3(1, 0, 1)), f.x), mix(hash(i + vec3(0, 1, 1)), hash(i + vec3(1, 1, 1)), f.x), f.y), f.z);
}

vec3 light(vec3 n, vec3 v, vec3 alb, float gloss, float occ) {
    vec3 l1 = normalize(vec3(-0.55, 0.80, 0.55)); // key: top left, in front
    vec3 l2 = normalize(vec3(0.80, 0.30, 0.50));  // fill
    vec3 l3 = normalize(vec3(0.45, 0.20, -0.85)); // rim from behind, the stage's violet
    vec3 l4 = normalize(vec3(0.35, 0.90, 0.25));  // a small light above: the second sharp highlight
    float d1 = dot(n, l1);
    float wrap = max((d1 + 0.4) / 1.4, 0.0);
    vec3 col = alb * wrap * 1.22;
    col += alb * vec3(1.0, 0.3, 0.06) * 0.30 * smoothstep(0.0, 0.35, wrap) * (1.0 - smoothstep(0.35, 0.9, wrap)); // warmth under the skin
    col += alb * max(dot(n, l2), 0.0) * 0.22;
    col += alb * (0.10 + 0.08 * n.y);
    col *= occ;
    float ndv = max(dot(n, v), 0.0);
    float fr = 0.04 + 0.96 * pow(1.0 - ndv, 5.0);
    vec3 h1 = normalize(l1 + v), h4 = normalize(l4 + v);
    float s = pow(max(dot(n, h4), 0.0), 120.0) * 0.4 + pow(max(dot(n, h1), 0.0), 18.0) * 0.3;
    col += vec3(1.0, 0.96, 0.92) * s * gloss * mix(0.6, 1.0, occ);
    col += vec3(0.55, 0.42, 1.0) * fr * 0.7 * smoothstep(0.0, 0.7, dot(n, l3)) * occ;
    vec3 r = reflect(-v, n);
    col += ((0.03 + fr) * env(r) + 0.02 * softbox(r)) * gloss * occ;
    return col;
}

vec3 shade(vec3 p, vec3 rd, float foot) {
    vec3 n = normal(p), v = -rd;
    vec3 q = local(p);
    vec3 g = q * 55.0;
    float g0 = noise(g);
    vec3 grain = vec3(noise(g + vec3(0.3, 0, 0)), noise(g + vec3(0, 0.3, 0)), noise(g + vec3(0, 0, 0.3))) - g0;
    n = normalize(n + worldDir(grain) * 0.08 * (1.0 - smoothstep(0.008, 0.016, foot))); // fades out where too small to see
    float occ = occlusion(p, n);
    vec3 orange = vec3(0.95, 0.21, 0.010);
    orange *= mix(0.62, 1.0, smoothstep(0.0, 1.5, q.y)) * mix(0.6, 1.0, smoothstep(-0.7, 0.1, n.y)); // deeper low down and underneath
    vec3 col = light(n, v, orange, 0.55, occ);
    if (q.z > 0.3) {
        float e = eyes(q.xy);
        vec2 g = vec2(eyes(q.xy + vec2(0.002, 0.0)) - e, eyes(q.xy + vec2(0.0, 0.002)) - e) / 0.002;
        vec3 out2 = normalize(worldDir(vec3(g, 0.0)) + 1e-5);
        float aa = foot / max(dot(n, v), 0.35);
        // a thin groove round each eye, then the glossy black eye domed like a bead
        float groove = smoothstep(0.02 + aa, 0.0, e);
        vec3 ng = normalize(n - out2 * 0.7 * groove);
        col = mix(col, light(ng, v, orange * 0.2, 0.2, occ * 0.4), groove * 0.9);
        float s = clamp(-e / 0.12, 0.0, 1.0);
        float slope = min((1.0 - s) / sqrt(max(1.0 - (1.0 - s) * (1.0 - s), 1e-4)), 3.0) * 0.9;
        vec3 ne = normalize(n + out2 * slope);
        vec3 black = light(ne, v, vec3(0.004), 0.35, 1.0) * mix(0.5, 1.0, s);
        vec3 r = reflect(-v, ne);
        black += softbox(r) * 0.2 * smoothstep(0.1, 0.5, s); // deep gloss: the softbox as a crisp glint
        black += vec3(0.6, 0.14, 0.02) * 0.10 * smoothstep(0.0, -0.6, r.y); // and his own orange, low in the eye
        col = mix(col, black, smoothstep(aa, -aa, e));
    }
    return col;
}

void main() {
    vec2 sp = (2.0 * gl_FragCoord.xy - uRes) / uRes.y;
    vec3 ta = vec3(0.0, 1.12, 0.0);
    vec3 ro = ta + 9.1 * vec3(sin(-0.17) * cos(0.18), sin(0.18), cos(-0.17) * cos(0.18));
    vec3 ww = normalize(ta - ro), uu = normalize(cross(ww, vec3(0.0, 1.0, 0.0))), vv = cross(uu, ww);
    vec3 rd = normalize(sp.x * uu + sp.y * vv + FL * ww);
    float pix = 2.0 / (uRes.y * FL); // a pixel's width per unit of distance

    vec3 col = vec3(0.0);
    float cover = 0.0, tHit = 1e9;
    vec3 bc = vec3(0.0, 1.0 + uBody.y, 0.0);
    vec3 oc = ro - bc;
    float b = dot(oc, rd), disc = b * b - dot(oc, oc) + 2.2 * 2.2;
    if (disc > 0.0) {
        float t = max(-b - sqrt(disc), 0.0), tEnd = -b + sqrt(disc);
        float best = 1e9, tBest = t;
        for (int i = 0; i < 96; i++) {
            float d = map(ro + rd * t);
            float px = d / (pix * t);
            if (px < best) { best = px; tBest = t; }
            if (d < 0.25 * pix * t) { best = 0.0; tBest = t; break; }
            t += d;
            if (t > tEnd) break;
        }
        // Anti-aliased edges: a ray that just misses still covers part of its pixel.
        cover = clamp(1.0 - best, 0.0, 1.0);
        if (cover > 0.0) {
            tHit = tBest;
            col = shade(ro + rd * tBest, rd, pix * tBest);
        }
    }
    // The soft contact shadow on an invisible floor; it fades as he leaves the ground.
    float shadow = 0.0;
    if (rd.y < 0.0) {
        float tf = -ro.y / rd.y;
        vec3 f = ro + rd * tf;
        if ((tf < tHit || cover < 1.0) && length(f.xz) < 2.6) {
            float o = 0.0, s = 1.0;
            for (int i = 0; i < 5; i++) {
                float h = 0.05 + 0.17 * float(i);
                o += max(h - map(f + vec3(0.0, h, 0.0)), 0.0) * s;
                s *= 0.7;
            }
            shadow = clamp(o * 1.1, 0.0, 0.88) * smoothstep(2.3, 1.3, length(f.xz)); // gone well before the image's edge
        }
    }
    float a = cover + shadow * (1.0 - cover);
    col = col * cover / max(a, 1e-4);
    outColor = vec4(pow(clamp(col, 0.0, 1.0), vec3(1.0 / 2.2)), a);
}
"#;

const VERT: &str = "#version 300 es
void main() { vec2 p = vec2(float((gl_VertexID << 1) & 2), float(gl_VertexID & 2)); gl_Position = vec4(p * 2.0 - 1.0, 0.0, 1.0); }";

#[cfg(target_os = "android")]
pub use gl::attach;

#[cfg(target_os = "android")]
mod gl {
    use super::Pose;
    use crate::{HomeScreen, Toys, android::log};
    use glow::HasContext;
    use slint::{ComponentHandle, GraphicsAPI, RenderingState};
    use std::{cell::RefCell, num::NonZeroU32, rc::Rc};

    /// The shared frame: the pose to draw, whether he is on screen at all, and whether that pose is not drawn yet.
    #[derive(Default)]
    pub struct Scene {
        pub pose: Pose,
        pub show: bool,
        pub fresh: bool,
    }

    struct Gl {
        gl: glow::Context,
        program: glow::Program,
        vao: glow::VertexArray,
        fbo: glow::Framebuffer,
        tex: glow::Texture,
        size: (i32, i32),
        u: [Option<glow::UniformLocation>; 5],
    }

    // Skia draws the rest of the screen on the same GL context and remembers what it last set, so whatever this
    // touches is put back exactly as it was.
    const CAPS: [u32; 5] = [glow::BLEND, glow::SCISSOR_TEST, glow::DEPTH_TEST, glow::STENCIL_TEST, glow::CULL_FACE];

    impl Gl {
        unsafe fn new(gl: glow::Context, size: (i32, i32)) -> Result<Self, String> {
            unsafe {
                let program = gl.create_program()?;
                for (kind, src) in [(glow::VERTEX_SHADER, super::VERT), (glow::FRAGMENT_SHADER, super::FRAG)] {
                    let s = gl.create_shader(kind)?;
                    gl.shader_source(s, src);
                    gl.compile_shader(s);
                    if !gl.get_shader_compile_status(s) {
                        return Err(gl.get_shader_info_log(s));
                    }
                    gl.attach_shader(program, s);
                    gl.delete_shader(s);
                }
                gl.link_program(program);
                if !gl.get_program_link_status(program) {
                    return Err(gl.get_program_info_log(program));
                }
                let u = ["uRes", "uBody", "uArms", "uEyes", "uMood"].map(|n| gl.get_uniform_location(program, n));
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
                let me = Gl { gl, program, vao, fbo, tex, size, u };
                if !ok {
                    me.delete();
                    return Err("framebuffer incomplete".into());
                }
                Ok(me)
            }
        }

        unsafe fn draw(&self, p: &Pose) {
            unsafe {
                let gl = &self.gl;
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
                gl.uniform_2_f32(self.u[0].as_ref(), self.size.0 as f32, self.size.1 as f32);
                for (u, v) in self.u[1..].iter().zip([p.body, p.arms, p.eyes, p.mood]) {
                    gl.uniform_4_f32(u.as_ref(), v[0], v[1], v[2], v[3]);
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

        fn delete(&self) {
            unsafe {
                self.gl.delete_framebuffer(self.fbo);
                self.gl.delete_texture(self.tex);
                self.gl.delete_vertex_array(self.vao);
                self.gl.delete_program(self.program);
            }
        }
    }

    /// Draws Clawd into his texture before each frame of the home screen. If the GPU can't build him he falls back
    /// to the sprites.
    pub fn attach(ui: &HomeScreen, scene: Rc<RefCell<Scene>>) {
        let weak = ui.as_weak();
        let mut gl: Option<Gl> = None;
        let set = ui.window().set_rendering_notifier(move |state, api| match state {
            RenderingState::RenderingSetup => {
                let (GraphicsAPI::NativeOpenGL { get_proc_address }, Some(ui)) = (api, weak.upgrade()) else { return };
                let ctx = unsafe { glow::Context::from_loader_function_cstr(|s| get_proc_address(s)) };
                let sf = ui.window().scale_factor();
                let size = ((super::W * sf).round().min(1200.0) as i32, (super::H * sf).round().min(1000.0) as i32);
                match unsafe { Gl::new(ctx, size) } {
                    Ok(g) => {
                        let id = NonZeroU32::new(g.tex.0.get()).expect("texture id");
                        let image = unsafe { slint::BorrowedOpenGLTextureBuilder::new_gl_2d_rgba_texture(id, (size.0 as u32, size.1 as u32).into()) }
                            .origin(slint::BorrowedOpenGLTextureOrigin::BottomLeft)
                            .build();
                        ui.global::<Toys>().set_clawd_live(image);
                        gl = Some(g);
                    }
                    Err(e) => {
                        log(&format!("live Clawd: {e}"));
                        ui.global::<Toys>().set_live(false);
                    }
                }
            }
            RenderingState::BeforeRendering => {
                // Slint redraws the whole screen at the display rate while anything animates (Null's breathing),
                // so only a new pose redraws him; otherwise the texture keeps his last frame.
                let mut s = scene.borrow_mut();
                if let (Some(g), true, true) = (&gl, s.show, s.fresh) {
                    unsafe { g.draw(&s.pose) };
                    s.fresh = false;
                }
            }
            RenderingState::RenderingTeardown => {
                if let Some(g) = gl.take() {
                    g.delete();
                }
            }
            _ => {}
        });
        if let Err(e) = set {
            log(&format!("live Clawd: no rendering notifier: {e:?}"));
            ui.global::<Toys>().set_live(false);
        }
    }
}
#[cfg(target_os = "android")]
pub use gl::Scene;

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_tap_throws_him_up_lands_squashed_and_settles() {
        let mut l = Live::new(7);
        let mut now = 0.0;
        l.tick(now, false);
        l.hop(0.5);
        let (mut top, mut low) = (0.0f32, 1.0f32);
        while now < 3.0 {
            now += 1.0 / 60.0;
            l.tick(now, false);
            top = top.max(l.y);
            low = low.min(l.squash.x);
        }
        assert!(top > 0.15 && top < 0.3, "hop height {top}");
        assert!(low < 0.95, "landing squash {low}");
        assert!(!l.moving() || l.toy.body.pose != C_HAPPY);
        assert!((l.squash.x - 1.0).abs() < 0.01 && l.y == 0.0);
    }

    #[test]
    fn eyes_follow_the_finger_then_come_back() {
        let mut l = Live::new(3);
        l.tick(0.0, false);
        l.point(140.0, 0.0, true, 0.0);
        let mut now = 0.0;
        while now < 1.0 {
            now += 1.0 / 60.0;
            l.tick(now, false);
        }
        assert!(l.pose().eyes[0] > 0.9);
        l.point(140.0, 0.0, false, now);
        while now < 4.0 {
            now += 1.0 / 60.0;
            l.tick(now, false);
        }
        assert!(l.finger.is_none());
    }
}
