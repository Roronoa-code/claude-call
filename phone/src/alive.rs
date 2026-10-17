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

const TIMER_SLACK_S: f32 = 0.004;

pub fn due(c: &Cast, last: f32, now: f32) -> bool {
    let fps = if c.moving() { 60.0 } else if c.state() == State::Sleep { 10.0 } else { 20.0 };
    ((now + TIMER_SLACK_S) * fps).floor() > ((last + TIMER_SLACK_S) * fps).floor()
}

const WAKE_FOR: f32 = 8.0;
const DOZE_AFTER: f32 = 120.0;
const NOTICE_EVERY: (f32, f32) = (45.0, 90.0);
const BEAT_EVERY: (f32, f32) = (150.0, 300.0);

#[derive(Clone, Copy)]
enum Beat {
    Glance(bool, f32, f32, f32),
    Beam(bool, f32),
    Hop(bool),
    Sway(bool, f32),
    Stretch(f32),
    Wipe(f32),
}

pub const STEP: f32 = 48.0;
const LAST: f32 = 5.0;
const LATE: f32 = 0.22;
const DWELL: f32 = 0.3;
const FLING_S: f32 = 0.12;
const FLING_FRESH_S: f32 = 0.1;
const MIN_MOVE_DT: f32 = 0.008;
const DRUM_K: f32 = 9.0;
const DRUM_ZETA: f32 = 0.55;
const SETTLE_GAP: f32 = 0.015;
const SETTLE_SPEED: f32 = 0.04;
const SPREAD_IN_K: f32 = 22.0;
const SPREAD_OUT_K: f32 = 5.5;
const REST_GAP: f32 = 0.002;
const REST_SPEED: f32 = 0.02;

fn nearest(p: f32) -> usize {
    p.round().clamp(0.0, LAST) as usize
}

pub fn snap(place: f32, speed: f32) -> usize {
    nearest(place + speed * FLING_S)
}

struct Acted {
    clawd: usize,
    null: usize,
}

#[derive(Clone, Copy)]
struct Dial {
    from: f32,
    at: f32,
    speed: f32,
    t: f32,
}

#[derive(Clone, Copy)]
struct Roll {
    place: f32,
    speed: f32,
}

impl Roll {
    fn still(place: f32) -> Self {
        Roll { place, speed: 0.0 }
    }

    fn pull(&mut self, to: f32, k: f32, zeta: f32, dt: f32) {
        self.speed += (k * k * (to - self.place) - 2.0 * zeta * k * self.speed) * dt;
        self.place += self.speed * dt;
        if (self.place - to).abs() < REST_GAP && self.speed.abs() < REST_SPEED {
            *self = Roll::still(to);
        }
    }
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
    acted: Acted,
    copy: usize,
    talk_until: f32,
    dial: Option<Dial>,
    roll: Roll,
    target: usize,
    pending: bool,
    spread: Roll,
    last: f32,
    active_at: f32,
    dozing: bool,
    hovering: bool,
    script: Vec<(f32, Beat)>,
    next_notice: f32,
    next_beat: f32,
    beats: usize,
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
            acted: Acted { clawd: usize::MAX, null: usize::MAX },
            copy: 5,
            talk_until: 0.0,
            dial: None,
            roll: Roll::still(0.0),
            target: 0,
            pending: false,
            spread: Roll::still(0.0),
            last: 0.0,
            active_at: 0.0,
            dozing: false,
            hovering: false,
            script: Vec::new(),
            next_notice: 30.0,
            next_beat: 90.0,
            beats: 0,
        }
    }

    fn rand(&mut self) -> f32 {
        self.rng ^= self.rng << 13;
        self.rng ^= self.rng >> 7;
        self.rng ^= self.rng << 17;
        (self.rng % 10_000) as f32 / 10_000.0
    }

    pub fn active(&mut self, now: f32) {
        self.active_at = now;
        if self.dozing {
            self.dozing = false;
            self.script.clear();
            for c in [&mut self.clawd, &mut self.null] {
                c.set_state(State::Idle);
                c.hop(now);
            }
        }
    }

    pub fn dozing(&self) -> bool {
        self.dozing
    }

    pub fn hover_call(&mut self, on: bool, now: f32) {
        self.active(now);
        if on == self.hovering {
            return;
        }
        self.hovering = on;
        if on {
            self.script.clear();
            self.clawd.glance(0.45, -0.95, f32::MAX);
            self.null.glance(-0.4, -0.95, f32::MAX);
            self.clawd.sway(0.35);
            self.null.perk();
        } else {
            self.clawd.unglance();
            self.null.unglance();
        }
    }

    pub fn call_pressed(&mut self, now: f32) {
        self.active(now);
        self.clawd.hop(now);
        self.script.push((now + 0.12, Beat::Hop(true)));
    }

    fn cast(&mut self, null: bool) -> &mut Cast {
        if null { &mut self.null } else { &mut self.clawd }
    }

    fn play(&mut self, now: f32) {
        let mut due = Vec::new();
        self.script.retain(|&(at, b)| if at <= now { due.push(b); false } else { true });
        for b in due {
            match b {
                Beat::Glance(n, x, y, for_s) => self.cast(n).glance(x, y, now + for_s),
                Beat::Beam(n, for_s) => self.cast(n).beam(now + for_s),
                Beat::Hop(n) => self.cast(n).hop(now),
                Beat::Sway(n, v) => self.cast(n).sway(v),
                Beat::Stretch(for_s) => self.clawd.stretch(now, for_s),
                Beat::Wipe(for_s) => self.null.wipe(now, for_s),
            }
        }
    }

    fn idle_life(&mut self, now: f32) {
        let calm = self.clawd.state() == State::Idle && self.null.state() == State::Idle;
        if self.dozing || self.hovering || self.dial.is_some() || !self.script.is_empty() || !calm {
            return;
        }
        if now >= self.next_beat {
            self.next_beat = now + BEAT_EVERY.0 + self.rand() * (BEAT_EVERY.1 - BEAT_EVERY.0);
            self.next_notice = self.next_notice.max(now + 20.0);
            self.beats += 1;
            self.script = match self.beats % 3 {
                1 => vec![(now, Beat::Stretch(1.2))],
                2 => vec![(now, Beat::Wipe(0.7)), (now + 0.75, Beat::Beam(true, 1.0))],
                _ => (0..6).map(|i| (now + i as f32 * 0.09, Beat::Sway(false, if i % 2 == 0 { -2.4 } else { 2.4 }))).collect(),
            };
        } else if now >= self.next_notice {
            self.next_notice = now + NOTICE_EVERY.0 + self.rand() * (NOTICE_EVERY.1 - NOTICE_EVERY.0);
            self.script = vec![
                (now, Beat::Glance(false, 0.95, -0.05, 2.6)),
                (now + 0.55, Beat::Glance(true, -0.95, 0.0, 2.0)),
                (now + 0.95, Beat::Beam(true, 1.4)),
                (now + 1.25, Beat::Hop(false)),
            ];
        }
    }

    pub fn hop(&mut self, codex: bool, now: f32) {
        self.active(now);
        if codex {
            self.awake_until.1 = now + WAKE_FOR;
            self.null.hop(now);
        } else {
            self.awake_until.0 = now + WAKE_FOR;
            self.clawd.hop(now);
        }
    }

    pub fn point(&mut self, x: f32, y: f32, null_dx: f32, down: bool, now: f32) {
        self.active(now);
        self.clawd.point(x, y, down, now);
        self.null.point(x - null_dx, y, down, now);
    }

    pub fn drag(&mut self, dy: f32, now: f32) -> usize {
        self.active(now);
        let from = self.dial.map_or(self.roll.place, |d| d.from);
        let mut p = from + dy / STEP;
        let give = |x: f32| 0.3 * (1.0 - (-x / 0.3).exp());
        if p < 0.0 {
            p = -give(-p);
        } else if p > LAST {
            p = LAST + give(p - LAST);
        }
        let speed = self.dial.map_or(0.0, |d| (p - d.at) / (now - d.t).max(MIN_MOVE_DT));
        self.dial = Some(Dial { from, at: p, speed, t: now });
        self.roll = Roll::still(p);
        nearest(p)
    }

    pub fn release(&mut self, keep: bool, now: f32) {
        let Some(d) = self.dial.take() else { return };
        let speed = if now - d.t < FLING_FRESH_S { d.speed } else { 0.0 };
        self.roll.speed = speed;
        if keep {
            (self.target, self.pending) = (snap(d.at, speed), true);
        }
    }

    pub fn step(&mut self, by: i32) {
        if self.dial.is_none() {
            let from = if self.pending { self.target } else { nearest(self.roll.place) };
            (self.target, self.pending) = ((from as i32 + by).clamp(0, LAST as i32) as usize, true);
        }
    }

    pub fn drum(&self) -> (f32, f32) {
        (self.roll.place, self.spread.place)
    }

    pub fn turning(&self) -> bool {
        self.spread.place > 0.0
    }

    pub fn nudge(&mut self, t: f32) {
        if self.dial.is_none() && (0.0..1.4).contains(&t) && self.shown <= 5 {
            let k = (t / 1.4 * std::f32::consts::TAU).sin() * (1.0 - t / 1.4);
            self.roll = Roll { place: self.shown as f32 - 0.32 * k, speed: 0.0 };
        }
    }

    pub fn tick(&mut self, now: f32, mode: usize) -> Option<usize> {
        let dt = (now - self.last).clamp(0.0, 0.05);
        self.last = now;
        let mode = mode.min(5);
        if self.shown > 5 {
            (self.shown, self.shown_at, self.acted, self.copy) = (mode, now - DWELL, Acted { clawd: mode, null: mode }, mode);
            (self.roll, self.target) = (Roll::still(mode as f32), mode);
        }
        if self.dial.is_none() {
            if !self.pending {
                self.target = mode;
            }
            self.roll.pull(self.target as f32, DRUM_K, DRUM_ZETA, dt);
        }
        let moving = self.dial.is_some() || self.roll.speed.abs() > SETTLE_SPEED || (self.target as f32 - self.roll.place).abs() > SETTLE_GAP;
        let chosen = (self.pending && !moving).then(|| {
            self.pending = false;
            self.target
        });
        let k = if moving { SPREAD_IN_K } else { SPREAD_OUT_K };
        self.spread.pull(if moving { 1.0 } else { 0.0 }, k, 1.0, dt);
        let m = nearest(self.roll.place);
        if m != self.shown {
            (self.shown, self.shown_at) = (m, now);
        }
        self.null.roll = (self.roll.place - self.shown as f32).clamp(-0.5, 0.5);
        let settled = now - self.shown_at >= DWELL;
        if !self.dozing && self.shown < 5 && self.dial.is_none() && !self.hovering && now - self.active_at > DOZE_AFTER {
            self.dozing = true;
            self.script.clear();
        }
        self.idle_life(now);
        self.play(now);
        if now - self.shown_at >= LATE {
            self.copy = self.shown;
        }
        self.act_null(now, settled);
        self.act_clawd(now, self.copy, settled);
        self.clawd.tick(now);
        self.null.tick(now);
        chosen
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
        let (state, hold) = if self.dozing { (State::Sleep, Hold { dim: 0.85, ..Hold::default() }) } else { (state, hold) };
        let n = &mut self.null;
        n.set_hold(hold);
        n.attend(m == 0 || m == 4);
        if n.state() != State::Happy && n.state() != state {
            n.set_state(state);
        }
        if settled && self.acted.null != m {
            self.acted.null = m;
            if m == 2 {
                n.set_state(State::Happy);
            }
        }
    }

    fn act_clawd(&mut self, now: f32, m: usize, settled: bool) {
        let fresh = settled && self.acted.clawd != m && m == self.shown;
        if fresh {
            self.acted.clawd = m;
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
        let (state, hold) = if self.dozing { (State::Sleep, Hold::default()) } else { (state, hold) };
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

#[cfg(test)]
#[path = "alive_tests.rs"]
mod tests;
