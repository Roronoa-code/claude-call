//! Clawd, Claude Code's pixel creature, animated from the call's real state (the "mixed" personality: playful day
//! to day, with wild's big moments). The base pose eases toward each state's target, antics play over it and
//! always ease back, so nothing snaps. Output: whole-pixel rects for the screen.

use crate::antics::{Antic, Delta, IDLE};

const ORANGE: u32 = 0xD97757;
const INK: u32 = 0x141416;
const WARM: [u32; 5] = [0xD97757, 0xF2C7AE, 0xFFB38A, 0xEADFD2, 0xE99A73];

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Phase {
    Incoming,
    Connecting,
    Talking,
    Listening,
    Hearing,
    Thinking,
    Hold,
    Ended,
}

/// A rect in grid units (Clawd's grid: 36 x 10 cells, each drawn u px wide and 2u tall).
#[derive(Clone, Copy, PartialEq, Debug)]
pub struct Cell {
    pub x: f32,
    pub y: f32,
    pub w: f32,
    pub h: f32,
    pub c: u32,
    pub o: f32,
}

#[derive(Default, Clone, Copy)]
struct Pose {
    hop: f32,
    squash: f32,
    lean: f32,
    eye_dx: f32,
    eye_dy: f32,
    eye_open: f32,
    arm_l: f32,
    arm_r: f32,
    reach: f32,
    tilt: f32,
    fade: f32,
    breath: f32,
}

#[derive(Default)]
struct Extra {
    legs: Option<[bool; 4]>,
    thought: bool,
    sleepy: bool,
}

/// Base pose for each call state. `t`: seconds; `te`: seconds in this state; lv_c / lv_m: Claude's and your level.
fn targets(phase: Phase, muted: bool, t: f32, te: f32, lv_c: f32, lv_m: f32) -> (Pose, Extra) {
    use std::f32::consts::PI;
    let mut p = Pose { eye_open: 1.0, fade: 1.0, breath: 0.5 + 0.5 * (t * PI * 2.0 / 3.4).sin(), ..Default::default() };
    let mut e = Extra::default();
    let blink = t % 3.9 < 0.13;
    match phase {
        Phase::Incoming => {
            // a squashy hop on every ring
            let k = (te % 1.5) / 0.8;
            if k < 1.0 {
                let (c, l) = (0.16, 0.76);
                if k < c {
                    p.squash = 1.5 * (k / c);
                } else if k < l {
                    let u = (k - c) / (l - c);
                    let air = 4.0 * u * (1.0 - u);
                    p.hop = air * 2.6;
                    p.squash = -1.2 * (1.0 - air);
                } else {
                    p.squash = 1.5 * (PI * (k - l) / (1.0 - l)).sin();
                }
            }
            p.arm_r = if k < 0.8 { 1.0 } else { 0.0 };
        }
        Phase::Talking => {
            // a dance in time with Claude's voice: stomps
            let beat = (t * 5.0).floor() as i32 % 2 == 1;
            p.hop = lv_c * 3.0 * if beat { 1.0 } else { 0.35 };
            p.squash = if beat { (2.4 * lv_c).clamp(0.0, 2.0) } else { -lv_c * 1.2 };
            if lv_c > 0.18 {
                e.legs = Some(if beat { [true, false, true, false] } else { [false, true, false, true] });
            }
            p.arm_l = if lv_c > 0.3 && beat { 2.0 } else if lv_c > 0.2 { 1.0 } else { 0.0 };
            p.arm_r = if lv_c > 0.3 && !beat { 2.0 } else if lv_c > 0.2 { 1.0 } else { 0.0 };
            p.eye_open = (1.0 - 2.0 * (lv_c - 0.22).max(0.0)).max(0.3);
        }
        Phase::Listening => {
            // attentive: leans toward you, eyes wide and down
            if !muted {
                p.eye_dy = 1.0;
                p.eye_open = 1.4;
                p.reach = 1.0;
                p.hop = -0.6;
            }
        }
        Phase::Hearing => {
            // leans in and nods; its eyes follow your voice
            let nod = if (t * PI * 2.0 * 1.6).sin() > 0.0 { 1.0 } else { 0.0 };
            p.eye_dy = 1.0 - (lv_m * 2.5).min(1.0) * 2.0;
            p.eye_open = 1.4;
            p.reach = 1.0;
            p.lean = 1.0;
            p.squash = nod * (lv_m * 3.0).min(1.0);
            p.hop = -0.6;
            p.tilt = 6.0 * nod * (lv_m * 3.0).min(1.0);
        }
        Phase::Thinking => {
            // eyes up to the side, a slow tilt, thought dots, a tapping foot
            p.eye_dx = 1.0;
            p.eye_dy = -1.0;
            p.lean = 0.55 + 0.5 * (t * 0.9).sin();
            p.tilt = -7.0 + 3.0 * (t * 0.9).sin();
            e.thought = true;
            e.legs = Some([false, false, false, (t * 4.0).floor() as i32 % 2 == 1]);
        }
        Phase::Hold => {
            // sleepy: drooping eyes, slow breathing, z's
            let peek = t % 5.0 < 0.5;
            p.eye_open = if peek { 0.9 } else { 0.2 };
            p.eye_dy = 0.5;
            e.sleepy = true;
            p.breath = 0.5 + 0.5 * (t * PI * 2.0 / 4.0).sin();
            p.squash = p.breath;
        }
        Phase::Ended => {
            p.eye_open = 0.2;
            // gone before the app closes the screen at 1.2 s
            p.fade = if te < 0.75 { 1.0 } else { (1.0 - (te - 0.75) / 0.4).max(0.0) };
        }
        Phase::Connecting => {}
    }
    if blink && p.eye_open > 0.5 && phase != Phase::Ended {
        p.eye_open = 0.1;
    }
    (p, e)
}


struct Running {
    antic: Antic,
    start: f32,
    cut_at: Option<f32>,
    scale: f32,
}

struct Confetti {
    born: f32,
    x: f32,
    y: f32,
    vx: f32,
    vy: f32,
    c: u32,
    big: bool,
}

pub struct Mascot {
    p: Pose,
    phase: Phase,
    entered: f32,
    prev: f32,
    lv_c: f32,
    lv_m: f32,
    antics: Vec<Running>,
    confetti: Vec<Confetti>,
    next_idle: f32,
    last_idle: Option<Antic>,
    seed: u32,
}

impl Mascot {
    pub fn new(now: f32) -> Self {
        let (p, _) = targets(Phase::Incoming, false, 0.0, 0.0, 0.0, 0.0);
        Mascot { p, phase: Phase::Incoming, entered: now, prev: now, lv_c: 0.0, lv_m: 0.0, antics: vec![], confetti: vec![], next_idle: now + 2.5, last_idle: None, seed: 0x9e37_79b9 }
    }

    fn rand(&mut self) -> f32 {
        self.seed ^= self.seed << 13;
        self.seed ^= self.seed >> 17;
        self.seed ^= self.seed << 5;
        (self.seed % 10_000) as f32 / 10_000.0
    }

    pub fn start(&mut self, a: Antic, now: f32) {
        for r in &mut self.antics {
            r.cut_at.get_or_insert(now);
        }
        self.antics.push(Running { antic: a, start: now, cut_at: None, scale: if a.big() { 1.5 } else { 1.0 } });
        if a.burst() {
            self.confetti = (0..24)
                .map(|i| {
                    // A fan up and to the right, kept low so it never reaches text above or the caption below.
                    let a = -std::f32::consts::PI * (0.06 + 0.36 * (i as f32 / 23.0)) + (self.rand() - 0.5) * 0.2;
                    let v = 5.0 + self.rand() * 4.0;
                    Confetti { born: now, x: 28.0, y: -1.0, vx: a.cos() * v * 1.3, vy: a.sin() * v, c: WARM[i % 5], big: i % 3 == 0 }
                })
                .collect();
        }
    }

    /// Tapped: three quick giggling bounces.
    pub fn poke(&mut self, now: f32) {
        self.start(Antic::Giggle, now);
    }

    /// Advances to `now` (seconds) and returns Clawd's pixels in grid units, or None while it stands still
    /// (reduced redraws). `busy` = something is animating (the caller keeps ticking).
    pub fn frame(&mut self, now: f32, phase: Phase, muted: bool, lv_claude: f32, lv_me: f32) -> Vec<Cell> {
        if phase != self.phase {
            // state changes: cut what is playing, celebrate what deserves it
            let was = self.phase;
            self.phase = phase;
            self.entered = now;
            for r in &mut self.antics {
                r.cut_at.get_or_insert(now);
            }
            if was == Phase::Incoming && phase != Phase::Ended {
                self.start(Antic::Cartwheel, now);
            } else if phase == Phase::Hearing && was == Phase::Talking {
                self.start(Antic::Surprise, now);
            } else if phase == Phase::Ended {
                self.start(Antic::Farewell, now);
            }
            self.next_idle = now + 2.5;
        }
        let playing = self.antics.iter().any(|a| now - a.start < a.antic.dur());
        if phase == Phase::Listening && !playing && now > self.next_idle {
            let pool: Vec<Antic> = IDLE.iter().copied().filter(|a| Some(*a) != self.last_idle).collect();
            let pick = pool[(self.rand() * pool.len() as f32) as usize % pool.len()];
            self.last_idle = Some(pick);
            self.start(pick, now);
            self.next_idle = now + 4.0 + self.rand() * 2.0;
        }
        self.antics.retain(|a| now - a.start < a.antic.dur() && !a.cut_at.is_some_and(|c| now - c > 0.2));
        let dt = (now - self.prev).clamp(0.0, 0.05);
        self.prev = now;
        self.lv_c += (lv_claude - self.lv_c) * (1.0 - (-dt * 18.0).exp());
        self.lv_m += (lv_me - self.lv_m) * (1.0 - (-dt * 18.0).exp());
        let (tg, extra) = targets(phase, muted, now, now - self.entered, self.lv_c, self.lv_m);
        let ease = |cur: f32, target: f32, rate: f32| cur + (target - cur) * (1.0 - (-dt * rate).exp());
        let p = &mut self.p;
        p.hop = ease(p.hop, tg.hop, 24.0);
        p.squash = ease(p.squash, tg.squash, 28.0);
        p.arm_l = ease(p.arm_l, tg.arm_l, 30.0);
        p.arm_r = ease(p.arm_r, tg.arm_r, 30.0);
        p.eye_open = ease(p.eye_open, tg.eye_open, 30.0);
        p.eye_dx = ease(p.eye_dx, tg.eye_dx, 8.0);
        p.eye_dy = ease(p.eye_dy, tg.eye_dy, 12.0);
        p.lean = ease(p.lean, tg.lean, 6.0);
        p.reach = ease(p.reach, tg.reach, 7.0);
        p.tilt = ease(p.tilt, tg.tilt, 6.0);
        p.fade = ease(p.fade, tg.fade, 12.0);
        p.breath = ease(p.breath, tg.breath, 8.0);
        let d = self.deltas(now);
        self.confetti.retain(|c| now - c.born < 1.7);
        let mut out = self.particles(&extra, now);
        clawd(&self.p, &d, &extra, &mut out);
        out
    }

    /// Lift of the whole box in grid units (positive = up).
    pub fn lift(&self, now: f32) -> f32 {
        (self.p.hop + self.deltas(now).hop).max(-2.0)
    }

    pub fn fade(&self) -> f32 {
        self.p.fade
    }

    fn deltas(&self, now: f32) -> Delta {
        let mut d = Delta::default();
        let mut lead = 0.0;
        for a in &self.antics {
            let k = (now - a.start) / a.antic.dur();
            if k >= 1.0 {
                continue;
            }
            let w = ((now - a.start) / 0.08).min(1.0) * a.cut_at.map_or(1.0, |c| (1.0 - (now - c) / 0.2).max(0.0));
            let f = a.antic.f(k.max(0.0));
            d.hop += f.hop * w * a.scale;
            d.squash += f.squash * w * a.scale;
            d.dx += f.dx * w;
            d.eye_dx += f.eye_dx * w;
            d.eye_dy += f.eye_dy * w;
            d.lean += f.lean * w;
            if w > lead {
                // turns, flips, arms and legs come from the strongest antic
                lead = w;
                d.turn = if w > 0.5 { f.turn } else { 0.0 };
                d.flip = f.flip;
                d.arms = f.arms;
                d.arm_wave = f.arm_wave;
                d.legs = f.legs;
                d.happy = f.happy && w > 0.5;
                d.pop = if f.pop > 0.0 { w } else { 0.0 };
            }
        }
        d
    }

    fn particles(&self, e: &Extra, now: f32) -> Vec<Cell> {
        let mut out = vec![];
        let t = now;
        if e.thought {
            // thought dots pop up to the upper right, one at a time
            let c = (t % 1.8) / 1.8;
            for (i, (x, y, w, h)) in [(33.0, -1.0, 2.0, 1.0), (36.0, -3.0, 2.0, 1.0), (39.0, -6.0, 4.0, 2.0)].into_iter().enumerate() {
                if c > 0.12 + i as f32 * 0.22 {
                    out.push(Cell { x, y, w, h, c: 0xF2C7AE, o: 1.0 });
                }
            }
        }
        if e.sleepy {
            // small z's just off the head, taking turns: the second only appears once the first has gone
            for i in 0..2 {
                let k = ((t + i as f32 * 1.3) % 2.6) / 2.6;
                if k >= 0.46 {
                    continue;
                }
                let (x, y) = ((31.0 + k * 8.0).round(), (-1.0 - k * 6.0).round());
                let o = (if k < 0.06 { k / 0.06 } else { 1.0 }) * (1.0 - ((k - 0.32) / 0.14).max(0.0));
                if o <= 0.0 {
                    continue;
                }
                // a 5 x 5 Z in half rows: full bars and a one-pixel diagonal from top right to bottom left
                for (j, row) in ["#####", "...#.", "..#..", ".#...", "#####"].iter().enumerate() {
                    for (i2, ch) in row.chars().enumerate() {
                        if ch == '#' {
                            out.push(Cell { x: x + i2 as f32, y: y + j as f32 * 0.5, w: 1.0, h: 0.5, c: 0xF2C7AE, o });
                        }
                    }
                }
            }
        }
        for p in &self.confetti {
            let age = now - p.born;
            out.push(Cell {
                x: (p.x + p.vx * age).round(),
                y: (p.y + p.vy * age + 0.5 * 9.0 * age * age).round(),
                w: if p.big { 2.0 } else { 1.0 },
                h: 1.0,
                c: p.c,
                o: 1.0 - ((age - 1.2) / 0.5).max(0.0),
            });
        }
        out
    }
}

/// Clawd's body. Every rect sits on whole cells; turns and flips are drawn as pixel-art frames, never rotations.
fn clawd(p: &Pose, d: &Delta, _e: &Extra, out: &mut Vec<Cell>) {
    use std::f32::consts::PI;
    // Upside down at the top of a flip: plain Clawd mirrored row for row (legs up, eyes low).
    let flipped = d.flip > 0.3 && d.flip < 0.7;
    let s = if flipped { 0.0 } else { (p.squash + d.squash).round().clamp(-2.0, 2.0) };
    let turn = d.turn % 1.0;
    let f0 = (2.0 * PI * turn).cos().abs();
    let side = (2.0 * PI * turn).sin();
    // Turning: full, three-quarter and side-on frames (no bare edge-on sliver).
    let f = if f0 > 0.85 { 1.0 } else if f0 > 0.5 { 0.66 } else { 0.36 };
    let dx = d.dx.round();
    let lean = if f < 1.0 { 0.0 } else { (p.lean + d.lean).clamp(0.0, 1.0).round() };
    let half = ((12.0 + s) * f).round().max(2.0);
    let x0 = 18.0 - half + dx;
    let wd = 2.0 * half;
    let mut rects: Vec<Cell> = vec![];
    let mut r = |x: f32, y: f32, w: f32, h: f32, c: u32| rects.push(Cell { x, y, w, h, c, o: 1.0 });
    r(x0 + lean, s, wd, 4.0 - s, ORANGE); // head rows (a squash lowers the top, a stretch raises it)
    r(x0, 4.0, wd, 4.0, ORANGE); // body rows
    // Arms stay joined to the body: raised arms sit level with the top of the head, wherever that is.
    let lvl = |a: f32| if a >= 2.0 { s - 1.0 } else if a >= 1.0 { s.max(2.0) } else { 4.0 };
    let a_l = if flipped { 0.0 } else { p.arm_l.round().max(d.arms as f32) };
    let a_r = if flipped { 0.0 } else { p.arm_r.round().max(d.arms as f32).max(d.arm_wave as f32) };
    if f > 0.5 {
        r(x0 - 4.0, lvl(a_l), 4.0, 2.0, ORANGE);
        r(x0 + wd, lvl(a_r), 4.0, 2.0, ORANGE);
    }
    let legs = d.legs.or(_e.legs).unwrap_or([false; 4]);
    let tuck = p.hop + d.hop > 1.4;
    if f > 0.5 {
        for (i, o) in [-10.0f32, -6.0, 4.0, 8.0].into_iter().enumerate() {
            r(18.0 + dx + (o * f).round(), 8.0, 2.0, if tuck || legs[i] { 1.0 } else { 2.0 }, ORANGE);
        }
    } else {
        for (i, x) in [16.0 + dx, 18.0 + dx].into_iter().enumerate() {
            r(x, 8.0, 2.0, if tuck || legs[i] { 1.0 } else { 2.0 }, ORANGE);
        }
    }
    // Eyes: happy (closed), popped (wide), or open as far as eye_open says, looking around.
    let ew = if d.pop > 0.5 { 3.0 } else { 2.0 };
    let h = if d.happy { 1.0 } else if d.pop > 0.5 { 3.0 } else if p.eye_open < 0.35 { 1.0 } else if p.eye_open > 1.25 { 3.0 } else { 2.0 };
    let look = (p.eye_dx + d.eye_dx).round().clamp(-1.0, 1.0) + lean;
    let ey = (p.eye_dy + d.eye_dy).round().clamp(-1.0, 1.0);
    // Eyes always keep a row of head above them, so they never notch the top edge.
    let base = if d.happy { 2.0 } else if h == 1.0 { 3.0 } else if h == 3.0 { 1.0 } else { 2.0 };
    let top = (s + 1.0).max(base + if d.happy { 0.0 } else { ey });
    let eyes: Vec<f32> = if f > 0.5 {
        vec![18.0 + dx - (7.0 * f).round() - 1.0, 18.0 + dx + (7.0 * f).round() - 1.0]
    } else if f > 0.2 {
        vec![18.0 + dx + if side > 0.0 { 2.0 } else { -4.0 }]
    } else {
        vec![]
    };
    for x in eyes {
        r(x + look - (ew - 2.0), top, ew, h, INK);
    }
    // Particles first (already in `out`), so they pass behind the body rather than over its face.
    for mut q in rects {
        if flipped {
            q.y = 10.0 - q.y - q.h;
        }
        out.push(q);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn stands_on_whole_cells_in_every_state() {
        let mut m = Mascot::new(0.0);
        let phases = [Phase::Incoming, Phase::Listening, Phase::Talking, Phase::Hearing, Phase::Thinking, Phase::Hold, Phase::Ended];
        for (i, ph) in phases.iter().enumerate() {
            for f in 0..90 {
                let now = i as f32 * 3.0 + f as f32 / 30.0;
                let cells = m.frame(now, *ph, false, 0.5, 0.4);
                assert!(!cells.is_empty() || *ph == Phase::Ended);
                for c in cells.iter().filter(|c| c.c == ORANGE || c.c == INK) {
                    assert_eq!(c.x, c.x.round(), "{ph:?} x");
                    assert!(c.w > 0.0 && c.h > 0.0, "{ph:?} size");
                }
            }
        }
    }
}
