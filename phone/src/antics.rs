//! Clawd's personality: short one-off "antics" laid over its base pose. Every antic is a curve that starts and
//! ends at zero, so it always eases back into the base pose; cutting one short fades it out instead.

pub fn sm(t: f32) -> f32 {
    if t <= 0.0 { 0.0 } else if t >= 1.0 { 1.0 } else { t * t * (3.0 - 2.0 * t) }
}
pub fn lerp(a: f32, b: f32, t: f32) -> f32 {
    a + (b - a) * t
}
fn seg(k: f32, a: f32, b: f32) -> f32 {
    sm((k - a) / (b - a))
}

#[derive(Clone, Copy, PartialEq, Debug)]
pub enum Antic {
    Cartwheel,
    Giggle,
    Look,
    Wiggle,
    Peek,
    Shuffle,
    Surprise,
    Farewell,
}

#[derive(Default)]
pub struct Delta {
    pub hop: f32,
    pub squash: f32,
    pub dx: f32,
    pub eye_dx: f32,
    pub eye_dy: f32,
    pub lean: f32,
    pub pop: f32,
    pub turn: f32,
    pub flip: f32,
    pub arms: i32,
    pub arm_wave: i32,
    pub legs: Option<[bool; 4]>,
    pub happy: bool,
}

/// A hop with real squash and stretch: crouch, stretch up, round at the top, stretch down, squash, settle.
fn hop(k: f32, h: f32, s: f32, a: f32, l: f32) -> (f32, f32) {
    if k < a {
        return (0.0, s * sm(k / a));
    }
    if k < l {
        let t = (k - a) / (l - a);
        let air = 4.0 * t * (1.0 - t);
        let blend = sm((t / 0.18).min(1.0));
        return (air * h, lerp(s, -0.9 * s * (1.0 - air), blend));
    }
    let u = (k - l) / (1.0 - l);
    (0.0, if u < 0.35 { lerp(-0.9 * s, s, sm(u / 0.35)) } else { s * (1.0 - sm((u - 0.35) / 0.65)) })
}

impl Antic {
    pub fn dur(self) -> f32 {
        match self {
            Antic::Cartwheel => 1.35,
            Antic::Giggle => 0.95,
            Antic::Look => 1.7,
            Antic::Wiggle => 1.1,
            Antic::Peek => 1.8,
            Antic::Shuffle => 1.7,
            Antic::Surprise => 0.85,
            Antic::Farewell => 1.45,
        }
    }
    pub fn burst(self) -> bool {
        self == Antic::Farewell
    }
    pub fn big(self) -> bool {
        self == Antic::Cartwheel // plays at wild size in the mixed personality
    }
    pub fn f(self, k: f32) -> Delta {
        let mut d = Delta::default();
        match self {
            Antic::Cartwheel => {
                (d.hop, d.squash) = hop(k, 6.0, 2.0, 0.14, 0.8);
                d.turn = 2.0 * seg(k, 0.18, 0.78);
                d.flip = seg(k, 0.3, 0.62);
                d.arms = if k > 0.14 && k < 0.84 { 2 } else { 0 };
            }
            Antic::Giggle => {
                let n = (k * 3.0) % 1.0;
                (d.hop, d.squash) = hop(n, 1.6, 1.0, 0.16, 0.76);
                d.happy = true;
                d.dx = (k * std::f32::consts::PI * 6.0).sin().round();
            }
            Antic::Look => {
                d.eye_dx = if k < 0.45 {
                    -1.4 * seg(k, 0.0, 0.12)
                } else if k < 0.85 {
                    lerp(-1.4, 1.4, seg(k, 0.45, 0.55))
                } else {
                    1.4 * (1.0 - seg(k, 0.85, 1.0))
                };
            }
            Antic::Wiggle => {
                if k > 0.08 && k < 0.92 {
                    d.legs = Some(if (k * 14.0).floor() as i32 % 2 == 1 { [true, false, true, false] } else { [false, true, false, true] });
                }
            }
            Antic::Peek => {
                let w = seg(k, 0.0, 0.2) * (1.0 - seg(k, 0.8, 1.0));
                (d.eye_dx, d.eye_dy, d.lean, d.hop) = (1.3 * w, 1.3 * w, w, -0.6 * w);
            }
            Antic::Shuffle => {
                let w = seg(k, 0.0, 0.1) * (1.0 - seg(k, 0.9, 1.0));
                let b = (k * 8.0).floor() as i32;
                d.dx = w * if b % 4 < 2 { 1.0 } else { -1.0 } * 1.4;
                d.hop = w * if b % 2 == 1 { 0.8 } else { 0.0 };
                if w > 0.3 {
                    d.legs = Some(if b % 2 == 1 { [true, false, false, true] } else { [false, true, true, false] });
                    d.arms = b % 2;
                }
            }
            Antic::Surprise => {
                (d.hop, d.squash) = hop(k, 2.2, 1.5, 0.08, 0.6);
                d.pop = if k < 0.8 { 1.0 } else { 0.0 };
            }
            Antic::Farewell => {
                if k > 0.38 && k < 0.7 {
                    (d.hop, d.squash) = hop((k - 0.38) / 0.32, 3.0, 1.5, 0.16, 0.76);
                }
                d.arm_wave = if k < 0.38 || k > 0.7 { if (k * 16.0).floor() as i32 % 2 == 1 { 2 } else { 1 } } else { 2 };
                d.happy = true;
            }
        }
        d
    }
}

pub const IDLE: [Antic; 4] = [Antic::Look, Antic::Wiggle, Antic::Peek, Antic::Shuffle];
