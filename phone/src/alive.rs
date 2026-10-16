use crate::cast::{Cast, Hold, Mat, State, Who};

pub const CLAWD_SIZE: (f32, f32) = (300.0, 250.0);
pub const NULL_SIZE: (f32, f32) = (150.0, 230.0);

pub struct Frame {
    pub w: f32,
    pub h: f32,
}

pub fn frame(who: Who) -> Frame {
    match who {
        Who::Null => Frame { w: 150.0, h: 230.0 },
        Who::Clawd(_) => Frame { w: 300.0, h: 250.0 },
    }
}

pub const HEAD: f32 = 40.0;

pub const NULL_SOLO: f32 = 1.2;

pub fn solo(who: Who, k: f32) -> f32 {
    if who == Who::Null { k * NULL_SOLO } else { k }
}

pub fn solo_size(k: f32) -> (f32, f32) {
    let (c, n) = (frame(Who::Clawd(Mat::Vinyl)), frame(Who::Null));
    (c.w.max(n.w * NULL_SOLO) * k, (c.h + HEAD).max((n.h + HEAD) * NULL_SOLO) * k)
}

pub fn view(who: Who, k: f32, sf: f32) -> ((i32, i32), (i32, i32)) {
    let f = frame(who);
    let px = |v: f32| (v * k * sf).round().max(1.0) as i32;
    ((px(f.w), px(f.h)), (px(f.w), px(f.h + HEAD)))
}

pub fn due(c: &Cast, last: f32, now: f32) -> bool {
    let fps = if c.moving() { 60.0 } else if c.state() == State::Sleep { 10.0 } else { 20.0 };
    ((now + 0.004) * fps).floor() > ((last + 0.004) * fps).floor()
}

const WAKE_FOR: f32 = 8.0;

pub const STEP: f32 = 48.0;
const LAST: f32 = 5.0;
const LATE: f32 = 0.22;
const DWELL: f32 = 0.3;

fn nearest(p: f32) -> usize {
    p.round().clamp(0.0, LAST) as usize
}

pub struct Pair {
    pub clawd: Cast,
    pub null: Cast,
    awake_until: (f32, f32),
    moment: f32,
    moment_end: f32,
    rng: u64,
    shown: usize,
    shown_at: f32,
    acted: (usize, usize),
    copy: usize,
    talk_until: f32,
    dial: Option<(usize, f32)>,
    roll: (f32, f32),
    last: f32,
}

impl Pair {
    pub fn new(seed: u64) -> Self {
        Pair {
            clawd: Cast::new(Who::Clawd(Mat::Vinyl), seed),
            null: Cast::new(Who::Null, seed.rotate_left(29) ^ 0x85eb_ca6b),
            awake_until: (0.0, 0.0),
            moment: 14.0,
            moment_end: 0.0,
            rng: seed.rotate_left(17) | 1,
            shown: usize::MAX,
            shown_at: 0.0,
            acted: (usize::MAX, usize::MAX),
            copy: 5,
            talk_until: 0.0,
            dial: None,
            roll: (0.0, 0.0),
            last: 0.0,
        }
    }

    fn rand(&mut self) -> f32 {
        self.rng ^= self.rng << 13;
        self.rng ^= self.rng >> 7;
        self.rng ^= self.rng << 17;
        (self.rng % 10_000) as f32 / 10_000.0
    }

    pub fn hop(&mut self, codex: bool, now: f32) {
        if codex {
            self.awake_until.1 = now + WAKE_FOR;
            self.null.hop(now);
        } else {
            self.awake_until.0 = now + WAKE_FOR;
            self.clawd.hop(now);
        }
    }

    pub fn point(&mut self, x: f32, y: f32, null_dx: f32, down: bool, now: f32) {
        self.clawd.point(x, y, down, now);
        self.null.point(x - null_dx, y, down, now);
    }

    pub fn drag(&mut self, dy: f32) -> usize {
        let from = self.dial.map_or(self.shown.min(5), |d| d.0);
        let mut p = from as f32 + dy / STEP;
        let give = |x: f32| 0.3 * (1.0 - (-x / 0.3).exp());
        if p < 0.0 {
            p = -give(-p);
        } else if p > LAST {
            p = LAST + give(p - LAST);
        }
        self.dial = Some((from, p));
        self.roll = (p, 0.0);
        nearest(p)
    }

    pub fn release(&mut self, keep: bool) -> usize {
        match self.dial.take() {
            Some((_, p)) if keep => nearest(p),
            Some((from, _)) => from,
            None => self.shown,
        }
    }

    pub fn nudge(&mut self, t: f32) {
        if self.dial.is_none() && (0.0..1.4).contains(&t) && self.shown <= 5 {
            let k = (t / 1.4 * std::f32::consts::TAU).sin() * (1.0 - t / 1.4);
            self.roll = (self.shown as f32 - 0.32 * k, 0.0);
        }
    }

    pub fn tick(&mut self, now: f32, mode: usize) {
        let dt = (now - self.last).clamp(0.0, 0.05);
        self.last = now;
        let m = self.dial.map_or(mode.min(5), |d| nearest(d.1));
        if self.shown > 5 {
            (self.shown, self.shown_at, self.acted, self.copy, self.roll) = (m, now - DWELL, (m, m), m, (m as f32, 0.0));
        }
        if m != self.shown {
            if self.dial.is_none() {
                self.roll = (m as f32, 0.0);
            }
            (self.shown, self.shown_at) = (m, now);
        }
        if self.dial.is_none() {
            let (to, w) = (self.shown as f32, std::f32::consts::TAU * 2.2);
            self.roll.1 += (w * w * (to - self.roll.0) - 2.0 * 0.62 * w * self.roll.1) * dt;
            self.roll.0 += self.roll.1 * dt;
            if (self.roll.0 - to).abs() < 0.002 && self.roll.1.abs() < 0.02 {
                self.roll = (to, 0.0);
            }
        }
        self.null.roll = (self.roll.0 - self.shown as f32).clamp(-0.5, 0.5);
        let settled = now - self.shown_at >= DWELL;
        if now - self.shown_at >= LATE {
            self.copy = self.shown;
        }
        self.act_null(now, settled);
        self.act_clawd(now, self.copy, settled);
        self.clawd.tick(now);
        self.null.tick(now);
    }

    fn act_null(&mut self, now: f32, settled: bool) {
        let m = self.shown;
        let awake = now < self.awake_until.1;
        let (state, hold) = match m {
            0 => (State::Talk, Hold::default()),
            1 => (State::Think, Hold::default()),
            2 => (State::Idle, Hold { glad: true, ..Hold::default() }),
            3 => (State::Idle, Hold { work: Some((0.0, -0.85)), drowsy: 0.2, dim: 0.45, glad: false }),
            4 => (State::Idle, Hold { drowsy: 0.45, dim: 0.6, ..Hold::default() }),
            _ if awake => (State::Idle, Hold::default()),
            _ => (State::Sleep, Hold { dim: 0.85, ..Hold::default() }),
        };
        let n = &mut self.null;
        n.set_hold(hold);
        n.attend(m == 0 || m == 4);
        if n.state() != State::Happy && n.state() != state {
            n.set_state(state);
        }
        if settled && self.acted.1 != m {
            self.acted.1 = m;
            if m == 2 {
                n.set_state(State::Happy);
            }
        }
    }

    fn act_clawd(&mut self, now: f32, m: usize, settled: bool) {
        let fresh = settled && self.acted.0 != m && m == self.shown;
        if fresh {
            self.acted.0 = m;
            if m == 0 {
                self.talk_until = now + 2.2;
            }
        }
        let awake = now < self.awake_until.0;
        let (state, hold) = match m {
            0 if now < self.talk_until || self.dial.is_some() => (State::Talk, Hold::default()),
            0 => (State::Idle, Hold::default()),
            1 => (State::Idle, Hold { work: Some((0.45, -0.55)), ..Hold::default() }),
            2 => (State::Idle, Hold { glad: true, ..Hold::default() }),
            3 => (State::Idle, Hold { work: Some((-0.45, -0.65)), ..Hold::default() }),
            _ if awake => (State::Idle, Hold::default()),
            _ => (State::Sleep, Hold::default()),
        };
        let c = &mut self.clawd;
        c.set_hold(hold);
        c.attend(m == 0);
        let thinking = c.state() == State::Think && state == State::Idle && now < self.moment_end;
        if c.state() != State::Happy && c.state() != state && !thinking {
            c.set_state(state);
        }
        if fresh && m == 2 {
            c.set_state(State::Happy);
        }
        if (m == 0 || m == 2) && now >= self.moment && c.state() == State::Idle {
            c.set_state(State::Think);
            self.moment_end = now + 1.6 + self.rand() * 1.8;
            self.moment = self.moment_end + 12.0 + self.rand() * 16.0;
        }
    }
}
