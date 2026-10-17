use std::f32::consts::TAU;

#[derive(Clone, Copy, Debug, PartialEq)]
pub enum Mat {
    Vinyl,
    Cartoon,
    Plush,
    Chrome,
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub enum Who {
    Null,
    Clawd(Mat),
}

impl Who {
    pub fn named(name: &str) -> Who {
        let n = name.to_lowercase();
        if n.contains("codex") || n.contains("null") {
            Who::Null
        } else if n.contains("haiku") {
            Who::Clawd(Mat::Cartoon)
        } else if n.contains("sonnet") {
            Who::Clawd(Mat::Plush)
        } else if n.contains("opus") {
            Who::Clawd(Mat::Chrome)
        } else {
            Who::Clawd(Mat::Vinyl)
        }
    }

    pub fn face(self) -> &'static str {
        match self {
            Who::Null => "null",
            Who::Clawd(Mat::Vinyl) => "clawd",
            Who::Clawd(Mat::Cartoon) => "haiku",
            Who::Clawd(Mat::Plush) => "sonnet",
            Who::Clawd(Mat::Chrome) => "opus",
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub enum State {
    Idle,
    Think,
    Talk,
    Happy,
    Sleep,
}

#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct Hold {
    pub glad: bool,
    pub work: Option<(f32, f32)>,
    pub drowsy: f32,
    pub dim: f32,
}

pub type Uniforms = Vec<(&'static str, [f32; 4])>;

#[derive(Clone, Copy, Default, Debug)]
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

#[derive(Clone, Copy)]
struct Syllable {
    level: f32,
    until: f32,
}

#[derive(Clone, Copy)]
struct ArmKick {
    left: f32,
    right: f32,
    until: f32,
}

#[derive(Clone, Copy, Default)]
struct Head {
    roll: Spring,
    nod: Spring,
    turn: Spring,
}

#[derive(Clone, Copy, Default)]
struct Antenna {
    side: Spring,
    forward: Spring,
    stretch: Spring,
}

#[derive(Clone, Copy)]
struct HeadVel {
    x: f32,
    y: f32,
}

#[derive(Clone, Copy)]
struct Tune {
    hz: f32,
    zeta: f32,
}

#[derive(Clone, Copy)]
struct Feel {
    squash: Tune,
    land: f32,
    hop: f32,
    lean: Tune,
    arms: Tune,
    arm_rest: f32,
    breath: f32,
}
fn feel(who: Who) -> Feel {
    match who {
        Who::Clawd(Mat::Vinyl) => Feel { squash: Tune { hz: 5.0, zeta: 0.55 }, land: 0.3, hop: 0.22, lean: Tune { hz: 2.0, zeta: 0.45 }, arms: Tune { hz: 3.0, zeta: 0.35 }, arm_rest: -0.6, breath: 0.016 },
        Who::Clawd(Mat::Cartoon) => Feel { squash: Tune { hz: 6.5, zeta: 0.38 }, land: 0.42, hop: 0.28, lean: Tune { hz: 2.6, zeta: 0.4 }, arms: Tune { hz: 4.0, zeta: 0.3 }, arm_rest: -0.85, breath: 0.02 },
        Who::Clawd(Mat::Plush) => Feel { squash: Tune { hz: 2.6, zeta: 0.2 }, land: 0.75, hop: 0.16, lean: Tune { hz: 1.3, zeta: 0.42 }, arms: Tune { hz: 1.8, zeta: 0.2 }, arm_rest: -0.8, breath: 0.024 },
        Who::Clawd(Mat::Chrome) => Feel { squash: Tune { hz: 7.5, zeta: 0.7 }, land: 0.16, hop: 0.15, lean: Tune { hz: 1.8, zeta: 0.6 }, arms: Tune { hz: 3.5, zeta: 0.5 }, arm_rest: -0.55, breath: 0.01 },
        Who::Null => Feel { squash: Tune { hz: 5.5, zeta: 0.5 }, land: 0.25, hop: 0.16, lean: Tune { hz: 2.2, zeta: 0.5 }, arms: Tune { hz: 6.0, zeta: 0.5 }, arm_rest: 0.0, breath: 0.012 },
    }
}

const GRAVITY: f32 = 54.0;

pub struct Cast {
    pub who: Who,
    f: Feel,
    state: State,
    t: f32,
    rng: u64,
    squash: Spring,
    lean: Spring,
    turn: Spring,
    y: f32,
    vy: f32,
    launch_at: Option<f32>,
    breath: f32,
    lean_to: f32,
    next_shift: f32,
    look: (Spring, Spring),
    gaze: (f32, f32),
    jitter: (f32, f32),
    next_saccade: f32,
    next_jitter: f32,
    open: Spring,
    blink_until: f32,
    next_blink: f32,
    lid: Spring,
    asleep_for: f32,
    finger: Option<(f32, f32)>,
    finger_up: Option<f32>,
    glance: Option<(f32, f32, f32)>,
    stretch_until: f32,
    wipe_until: f32,
    happy: Spring,
    happy_until: f32,
    think: Spring,
    talk: Spring,
    voice: Spring,
    syllable: Syllable,
    syllables_left: i32,
    level: Option<f32>,
    attend: bool,
    nod: Spring,
    nod_until: f32,
    arms: (Spring, Spring),
    arm_to: (Spring, Spring),
    arm_kick: ArmKick,
    next_fidget: f32,
    head: Head,
    laptop: Spring,
    taps: (Spring, Spring),
    next_tap: f32,
    tap_side: bool,
    tap_until: f32,
    ant: Antenna,
    droop: Spring,
    head_v: HeadVel,
    bright: Spring,
    pub hold: Hold,
    pub roll: f32,
}

impl Cast {
    pub fn new(who: Who, seed: u64) -> Self {
        let f = feel(who);
        Cast {
            who,
            f,
            state: State::Idle,
            t: 0.0,
            rng: seed | 1,
            squash: Spring::at(1.0),
            lean: Spring::default(),
            turn: Spring::default(),
            y: 0.0,
            vy: 0.0,
            launch_at: None,
            breath: 0.0,
            lean_to: 0.0,
            next_shift: 2.0,
            look: (Spring::default(), Spring::default()),
            gaze: (0.0, 0.0),
            jitter: (0.0, 0.0),
            next_saccade: 0.8,
            next_jitter: 0.3,
            open: Spring::at(1.0),
            blink_until: 0.0,
            next_blink: 1.4,
            lid: Spring::default(),
            asleep_for: 0.0,
            finger: None,
            finger_up: None,
            glance: None,
            stretch_until: 0.0,
            wipe_until: 0.0,
            happy: Spring::default(),
            happy_until: 0.0,
            think: Spring::default(),
            talk: Spring::default(),
            voice: Spring::default(),
            syllable: Syllable { level: 0.0, until: 0.0 },
            syllables_left: 0,
            level: None,
            attend: false,
            nod: Spring::default(),
            nod_until: 0.0,
            arms: (Spring::at(f.arm_rest), Spring::at(f.arm_rest)),
            arm_to: (Spring::at(f.arm_rest), Spring::at(f.arm_rest)),
            arm_kick: ArmKick { left: 0.0, right: 0.0, until: 0.0 },
            next_fidget: 4.0,
            head: Head::default(),
            laptop: Spring::default(),
            taps: (Spring::default(), Spring::default()),
            next_tap: 0.0,
            tap_side: false,
            tap_until: 0.0,
            ant: Antenna::default(),
            droop: Spring::default(),
            head_v: HeadVel { x: 0.0, y: 0.0 },
            bright: Spring::at(1.0),
            hold: Hold::default(),
            roll: 0.0,
        }
    }

    fn rand(&mut self) -> f32 {
        self.rng ^= self.rng << 13;
        self.rng ^= self.rng >> 7;
        self.rng ^= self.rng << 17;
        (self.rng % 1_000_000) as f32 / 1_000_000.0
    }
    fn range(&mut self, a: f32, b: f32) -> f32 {
        a + (b - a) * self.rand()
    }
    fn wait(&mut self, mean: f32, lo: f32, hi: f32) -> f32 {
        (-mean * (1.0 - self.rand()).max(1e-4).ln()).clamp(lo, hi)
    }

    pub fn state(&self) -> State {
        self.state
    }
    pub fn set_state(&mut self, s: State) {
        if s == self.state {
            return;
        }
        if s == State::Happy {
            self.happy_until = self.t + 1.6;
            self.hop(self.t);
        }
        if s == State::Talk {
            self.syllables_left = 0;
        }
        self.asleep_for = 0.0;
        self.state = s;
        self.next_saccade = self.t + 0.05;
    }

    pub fn hop(&mut self, now: f32) {
        if self.y > 0.0 || self.launch_at.is_some() {
            return;
        }
        self.launch_at = Some(now + 0.08);
        self.happy_until = self.happy_until.max(now + 1.0);
        if self.state == State::Sleep {
            self.state = State::Idle;
        }
    }

    pub fn set_hold(&mut self, h: Hold) {
        if h != self.hold {
            self.hold = h;
            self.next_saccade = self.t + 0.05;
        }
    }

    pub fn set_voice(&mut self, level: Option<f32>) {
        self.level = level;
    }

    pub fn attend(&mut self, on: bool) {
        if on != self.attend {
            self.attend = on;
            self.next_saccade = self.t + 0.05;
        }
    }

    pub fn nod(&mut self, now: f32) {
        self.nod_until = now + 0.22;
    }

    pub fn point(&mut self, x: f32, y: f32, down: bool, now: f32) {
        self.finger = Some((x, y));
        self.finger_up = if down { None } else { Some(now) };
    }

    pub fn glance(&mut self, gx: f32, gy: f32, until: f32) {
        self.glance = Some((gx, gy, until));
    }

    pub fn unglance(&mut self) {
        self.glance = None;
    }

    pub fn beam(&mut self, until: f32) {
        self.happy_until = self.happy_until.max(until);
    }

    pub fn sway(&mut self, v: f32) {
        self.lean.v += v;
    }

    pub fn perk(&mut self) {
        self.ant.stretch.v += 2.4;
        self.ant.forward.v -= 1.6;
        self.arms.0.v += 1.2;
        self.arms.1.v += 1.2;
    }

    pub fn stretch(&mut self, now: f32, for_s: f32) {
        self.stretch_until = now + for_s;
        self.happy_until = self.happy_until.max(now + for_s);
    }

    pub fn wipe(&mut self, now: f32, for_s: f32) {
        self.wipe_until = now + for_s;
        self.blink_until = now + for_s * 0.6;
        self.head.roll.v += 1.5;
    }

    fn schedule(&mut self, now: f32) {
        let st = self.state;
        let null = self.who == Who::Null;
        if now >= self.next_saccade {
            let (gx, gy, hold) = match st {
                State::Think if null => {
                    if self.rand() < 0.6 { (self.range(-0.25, 0.25), -0.75, self.range(0.8, 2.0)) } else { (self.range(-0.8, 0.8), 0.6, self.range(0.5, 1.4)) }
                }
                State::Think => {
                    let side = if self.gaze.0 > 0.0 { 1.0 } else { -1.0 };
                    let side = if self.rand() < 0.25 { -side } else { side };
                    (side * self.range(0.45, 0.75), self.range(0.55, 0.85), self.range(0.9, 2.2))
                }
                State::Talk => {
                    if self.rand() < 0.7 { (self.range(-0.15, 0.15), self.range(-0.1, 0.1), self.range(0.6, 1.8)) } else { (self.range(-0.7, 0.7), self.range(-0.3, 0.4), self.range(0.3, 0.8)) }
                }
                State::Happy => (self.range(-0.2, 0.2), self.range(0.0, 0.2), self.range(0.4, 1.0)),
                State::Sleep => (0.0, -0.2, 3.0),
                State::Idle if self.hold.work.is_some() => {
                    let (x, y) = self.hold.work.unwrap_or_default();
                    if self.rand() < 0.92 { (x + self.range(-0.08, 0.08), y + self.range(-0.05, 0.05), self.wait(2.2, 0.8, 4.5)) } else { (x * 0.4 + self.range(-0.2, 0.2), self.range(-0.1, 0.15), self.range(0.3, 0.6)) }
                }
                State::Idle if self.attend => {
                    if self.rand() < 0.85 {
                        (self.range(-0.12, 0.12), self.range(-0.1, 0.06), self.wait(1.8, 0.6, 4.0))
                    } else {
                        let side = if self.rand() < 0.5 { -1.0 } else { 1.0 };
                        (side * self.range(0.3, 0.6), self.range(-0.2, 0.25), self.range(0.3, 0.7))
                    }
                }
                State::Idle => {
                    let r = self.rand();
                    if r < 0.45 {
                        (self.range(-0.2, 0.2), self.range(-0.15, 0.15), self.wait(1.6, 0.5, 4.0))
                    } else if r < 0.8 {
                        let side = if self.rand() < 0.5 { -1.0 } else { 1.0 };
                        (side * self.range(0.45, 0.95), self.range(-0.3, 0.35), self.range(0.5, 1.6))
                    } else if null {
                        (self.range(-0.2, 0.2), -0.8, self.range(0.8, 1.8))
                    } else {
                        (self.range(-0.5, 0.5), self.range(-0.6, 0.6), self.range(0.4, 1.0))
                    }
                }
            };
            if ((gx - self.gaze.0).abs() > 0.5 || (gy - self.gaze.1).abs() > 0.5) && self.rand() < 0.4 && st != State::Sleep {
                self.next_blink = now;
            }
            self.gaze = (gx, gy);
            self.next_saccade = now + hold;
        }
        if now >= self.next_jitter {
            self.jitter = (self.range(-0.035, 0.035), self.range(-0.025, 0.025));
            self.next_jitter = now + self.range(0.25, 0.7);
        }
        if now >= self.next_blink && st != State::Sleep {
            self.blink_until = now + self.range(0.07, 0.11);
            self.next_blink = if self.rand() < 0.15 { now + 0.24 } else { now + self.wait(3.4, 0.9, 7.5) };
        }
        if now >= self.next_shift {
            self.lean_to = match st {
                State::Sleep => self.range(0.02, 0.05) * if self.rand() < 0.5 { -1.0 } else { 1.0 },
                State::Think => self.range(0.03, 0.06) * self.gaze.0.signum(),
                _ => self.range(-0.035, 0.035),
            };
            self.next_shift = now + self.range(2.2, 6.0);
        }
        if !null && now >= self.next_fidget && st == State::Talk && self.level.is_some_and(|l| l < 0.15) {
            self.next_fidget = now + 0.1;
        } else if !null && now >= self.next_fidget {
            let side = self.rand();
            let amount = match st {
                State::Talk => self.range(0.25, 0.6),
                State::Sleep => 0.0,
                _ => self.range(0.12, 0.3),
            };
            self.arm_kick = ArmKick { left: if side < 0.5 { amount } else { 0.0 }, right: if side >= 0.5 { amount } else { 0.0 }, until: now + self.range(0.3, 0.7) };
            self.next_fidget = now + if st == State::Talk { self.range(0.9, 2.0) } else { self.range(3.5, 9.0) };
        }
        if st == State::Talk && self.level.is_none() && now >= self.syllable.until {
            if self.syllable.level > 0.0 {
                self.syllable = Syllable { level: 0.0, until: now + self.range(0.04, 0.11) };
            } else if self.syllables_left <= 0 {
                self.syllables_left = (self.range(5.0, 12.0)) as i32;
                self.syllable = Syllable { level: 0.0, until: now + self.range(0.25, 0.6) };
            } else {
                self.syllables_left -= 1;
                self.syllable = Syllable { level: self.range(0.35, 1.0), until: now + self.range(0.09, 0.22) };
            }
        }
        if st != State::Talk {
            self.syllable.level = 0.0;
        }
        if null && now >= self.next_tap && now >= self.tap_until {
            let typing = match st {
                _ if now < self.wipe_until => 1.0,
                State::Think => 1.0,
                State::Idle => 0.12,
                _ => 0.0,
            };
            if self.rand() < typing {
                self.tap_side = !self.tap_side;
                self.tap_until = now + 0.08;
                self.next_tap = now + self.range(0.1, 0.22);
            } else {
                self.next_tap = now + self.range(0.4, 1.2);
            }
        }
    }

    pub fn tick(&mut self, now: f32) {
        let dt = (now - self.t).clamp(0.0, 0.05);
        self.t = now;
        if self.finger_up.is_some_and(|t| now - t > 1.4) {
            (self.finger, self.finger_up) = (None, None);
        }
        self.schedule(now);
        let st = self.state;
        let null = self.who == Who::Null;
        let f = self.f;
        self.asleep_for = if st == State::Sleep { self.asleep_for + dt } else { 0.0 };
        if self.happy_until < now && st == State::Happy {
            self.state = State::Idle;
        }

        let (mut gx, mut gy) = (self.gaze.0 + self.jitter.0, self.gaze.1 + self.jitter.1);
        if self.glance.is_some_and(|g| now >= g.2) {
            self.glance = None;
        }
        if let Some((x, y, _)) = self.glance {
            (gx, gy) = (x, y);
        } else if let Some((x, y)) = self.finger {
            (gx, gy) = ((x / 140.0).clamp(-1.0, 1.0), (-y / 170.0).clamp(-1.0, 1.0));
        }
        let happy = self.happy_until > now || st == State::Happy;
        let open_to = if now < self.blink_until { 0.0 } else { 1.0 };
        let lid_to = match (st, self.asleep_for > 4.0) {
            (State::Sleep, false) => 0.55,
            (State::Sleep, true) => 1.0,
            _ => self.hold.drowsy,
        };
        let speaking = match self.level {
            Some(l) if st == State::Talk => ((l - 0.05) * 1.8).clamp(0.0, 1.0),
            _ => self.syllable.level,
        };
        let nod = self.nod.x;
        gy -= 0.35 * nod;
        let arm_up = 0.5;
        let (mut arm_l, mut arm_r) = (f.arm_rest, f.arm_rest);
        match st {
            State::Think => arm_l = f.arm_rest + 0.35,
            State::Sleep => (arm_l, arm_r) = (f.arm_rest - 0.15, f.arm_rest - 0.15),
            _ => {}
        }
        let stretching = now < self.stretch_until;
        if happy || stretching {
            (arm_l, arm_r) = (arm_up, arm_up);
        }
        if now < self.arm_kick.until {
            arm_l += self.arm_kick.left;
            arm_r += self.arm_kick.right;
        }
        let breath_period = match st {
            State::Sleep => 5.2,
            State::Talk => 2.6,
            State::Happy => 2.4,
            State::Think => 3.6,
            State::Idle => 3.2,
        } * (1.0 + 0.12 * (now * 0.23).sin() * (now * 0.071 + 1.3).cos());
        let sway = 0.012 * (now * 0.9).sin() + 0.008 * (now * 0.37 + 1.0).sin() + 0.005 * (now * 1.71 + 2.0).sin();
        let squash_to = if self.launch_at.is_some() { 0.86 } else if self.y > 0.0 { 1.06 } else if stretching { 1.09 } else { 1.0 + 0.025 * speaking - 0.03 * nod };
        let (head_roll_to, head_nod_to) = match st {
            State::Think => (0.1 * gx.signum(), if gy < 0.0 { 0.22 } else { -0.08 }),
            State::Sleep => (0.06, 0.2),
            State::Talk => (0.0, -0.05 * speaking),
            State::Happy => (0.0, -0.08),
            State::Idle if self.hold.work.is_some() && gy < -0.6 => (0.0, 0.3),
            State::Idle => (0.0, if gy < -0.6 { 0.18 } else { 0.0 }),
        };
        let laptop_to = if happy { 0.2 } else if st == State::Sleep { -0.05 } else { 0.0 };
        let droop_to = if st == State::Sleep { 1.0 } else { 0.0 };

        let steps = (dt * 240.0).ceil().max(1.0);
        let h = dt / steps;
        for _ in 0..steps as usize {
            if let Some(at) = self.launch_at {
                if self.t >= at {
                    self.vy = (2.0 * GRAVITY * f.hop).sqrt();
                    self.y = 1e-4;
                    self.launch_at = None;
                    self.arms.0.v += 2.0;
                    self.arms.1.v += 2.0;
                    self.squash.v += 3.0;
                }
            }
            if self.y > 0.0 {
                self.vy -= GRAVITY * h;
                self.y += self.vy * h;
                if self.y <= 0.0 {
                    self.squash.v -= self.vy.abs() * f.land;
                    self.arms.0.v -= 3.0;
                    self.arms.1.v -= 3.0;
                    self.laptop.v -= 1.2;
                    (self.y, self.vy) = (0.0, 0.0);
                }
            }
            self.squash.step(squash_to, f.squash.hz, f.squash.zeta, h);
            self.lean.step(self.lean_to + gx * 0.03 + sway, f.lean.hz * 0.6, f.lean.zeta, h);
            let work = self.hold.work.map_or(0.0, |w| w.0 * 0.3);
            self.turn.step(gx * 0.22 + work + 0.04 * (now * 0.21).sin(), 1.3, 0.75, h);
            self.look.0.step(gx, 7.0, 0.92, h);
            self.look.1.step(gy, 7.0, 0.92, h);
            self.open.step(open_to, 18.0, 1.0, h);
            self.lid.step(lid_to, 1.1, 1.0, h);
            self.happy.step(if happy || self.hold.glad { 1.0 } else { 0.0 }, 5.0, 0.75, h);
            self.think.step(if st == State::Think { 1.0 } else { 0.0 }, 3.0, 1.0, h);
            self.talk.step(if st == State::Talk { 1.0 } else { 0.0 }, 3.0, 1.0, h);
            self.voice.step(speaking, 7.5, 0.75, h);
            self.nod.step(if self.t < self.nod_until { 1.0 } else { 0.0 }, 3.2, 0.8, h);
            self.arm_to.0.step(arm_l, 1.5, 1.0, h);
            self.arm_to.1.step(arm_r, 1.35, 1.0, h);
            self.arms.0.step(self.arm_to.0.x, f.arms.hz, f.arms.zeta, h);
            self.arms.1.step(self.arm_to.1.x, f.arms.hz * 1.07, f.arms.zeta, h);
            let (bright_to, hz) = if st == State::Sleep { (0.35, 0.6) } else { (1.0, 1.8) };
            self.bright.step(bright_to * (1.0 - self.hold.dim), hz, 1.0, h);
            if null {
                self.head.roll.step(head_roll_to - self.lean.x * 0.5, 2.2, 0.55, h);
                self.head.nod.step(head_nod_to + 0.16 * nod, 2.5, 0.6, h);
                self.head.turn.step(gx * 0.12, 1.8, 0.7, h);
                self.laptop.step(laptop_to, 3.5, 0.5, h);
                let tapping = self.t < self.tap_until;
                self.taps.0.step(if tapping && self.tap_side { 1.0 } else { 0.0 }, 9.0, 0.6, h);
                self.taps.1.step(if tapping && !self.tap_side { 1.0 } else { 0.0 }, 9.0, 0.6, h);
                let hx = (self.lean.v + self.head.roll.v) * -1.7 + self.turn.v * 0.3;
                let hy = self.vy + self.squash.v * 1.6;
                let (dx, dy) = (hx - self.head_v.x, hy - self.head_v.y);
                self.head_v = HeadVel { x: hx, y: hy };
                self.ant.side.v -= dx.clamp(-2.0, 2.0) * 1.4;
                self.ant.forward.v -= dy.clamp(-6.0, 6.0) * 0.35;
                self.ant.stretch.v -= dy.clamp(-6.0, 6.0) * 0.6;
                self.droop.step(droop_to, 1.2, 1.0, h);
                self.ant.side.step(1.25 * self.droop.x, 3.2, 0.13, h);
                self.ant.forward.step(0.2 * self.droop.x + 0.08 * self.head.nod.x, 2.8, 0.15, h);
                self.ant.stretch.step(if self.hold.drowsy > 0.0 && self.attend && st != State::Sleep { 0.14 } else { 0.0 }, 5.0, 0.2, h);
                self.ant.side.x = self.ant.side.x.clamp(-1.4, 1.4);
                self.ant.stretch.x = self.ant.stretch.x.clamp(-0.25, 0.25);
            }
        }
        self.breath = (self.breath + dt * TAU / breath_period) % TAU;
    }

    pub fn moving(&self) -> bool {
        let springs = [&self.squash, &self.look.0, &self.look.1, &self.open, &self.happy, &self.arms.0, &self.arms.1, &self.ant.side, &self.voice, &self.nod];
        self.y > 0.0 || self.finger.is_some() || self.glance.is_some() || self.t < self.stretch_until || self.t < self.wipe_until || self.state == State::Talk || self.roll != 0.0 || springs.iter().any(|s| s.v.abs() > 0.05)
    }

    pub fn uniforms(&self) -> Uniforms {
        let deep = if self.state == State::Sleep { 1.7 } else { 1.0 };
        let breath = self.breath.sin() * self.f.breath * deep;
        let lag = (self.breath - 0.6).sin() * 0.05;
        let body = [self.squash.x.clamp(0.8, 1.12) * (1.0 + breath), self.y, self.lean.x, self.turn.x];
        let eyes = [self.look.0.x, self.look.1.x, self.open.x.clamp(0.0, 1.0), self.lid.x.clamp(0.0, 1.0)];
        let happy = self.happy.x.clamp(0.0, 1.0);
        match self.who {
            Who::Null => vec![
                ("uBody", body),
                ("uHead", [self.head.roll.x, self.head.nod.x - breath * 2.0, self.head.turn.x, self.laptop.x + lag * 0.2]),
                ("uAnt", [self.ant.side.x, self.ant.forward.x, self.ant.stretch.x, self.roll]),
                ("uEyes", eyes),
                ("uMood", [happy, self.think.x.clamp(0.0, 1.0), self.talk.x.clamp(0.0, 1.0), self.t % 120.0]),
                ("uHands", [self.taps.0.x.clamp(-0.3, 1.2), self.taps.1.x.clamp(-0.3, 1.2), self.voice.x.clamp(0.0, 1.0), self.bright.x]),
            ],
            Who::Clawd(m) => vec![
                ("uBody", body),
                ("uArms", [self.arms.0.x + lag, self.arms.1.x + lag, 0.0, 0.0]),
                ("uEyes", eyes),
                ("uMood", [happy, (self.voice.x * self.talk.x).clamp(0.0, 1.0), 0.0, 0.0]),
                ("uMat", [m as i32 as f32, 0.0, 0.0, 0.0]),
            ],
        }
    }
}

