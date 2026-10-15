//! Life for the 3D cast (shaders in art/live3d): Null, and the Clawd family in its four materials. The app sets
//! a state (idle, thinking, talking, happy, asleep), taps and finger positions; this turns them into the shaders'
//! uniforms every frame. Everything moves on springs, so nothing snaps: eyes saccade and blink on their own uneven
//! clock, the head follows the eyes a beat later, breathing drifts, weight shifts now and then, landings jiggle,
//! and Null's antenna lags and wobbles behind his head.
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
    /// Whoever an agent or model name means: Codex is Null; Haiku, Sonnet and Opus are their Clawds; anything else
    /// (Claude, unknown) the vinyl Clawd.
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

    /// Its name in the UI (still pictures in ui/faces, actor.slint): null, clawd, haiku, sonnet or opus.
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

/// A look held on top of the state, set by the home screen's mode (alive.rs): a happy face kept without the hop and
/// raised arms, eyes kept on their work rather than on you, lids partly down while awake, Null's screen taken down.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct Hold {
    pub glad: bool,
    pub work: Option<(f32, f32)>, // where the eyes rest: gaze x, y
    pub drowsy: f32,              // lids, 0..1, while awake
    pub dim: f32,                 // Null's screen: 0 full, 1 dark
}

/// The uniforms for one frame, by name (each a vec4), ready for glUniform4f.
pub type Uniforms = Vec<(&'static str, [f32; 4])>;

/// A damped spring: `hz` is how fast it moves, `zeta` below 1 overshoots and wobbles before settling.
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

/// How each character moves: Haiku snappy and bouncy, Sonnet soft and jiggly, Opus heavy and stiff.
#[derive(Clone, Copy)]
struct Feel {
    squash: (f32, f32), // spring hz, zeta
    land: f32,          // squash kick per unit of landing speed
    hop: f32,           // hop height, world units
    lean: (f32, f32),
    arms: (f32, f32),
    arm_rest: f32,
    breath: f32, // breathing depth
}
fn feel(who: Who) -> Feel {
    match who {
        Who::Clawd(Mat::Vinyl) => Feel { squash: (5.0, 0.55), land: 0.3, hop: 0.22, lean: (2.0, 0.45), arms: (3.0, 0.35), arm_rest: -0.6, breath: 0.016 },
        Who::Clawd(Mat::Cartoon) => Feel { squash: (6.5, 0.38), land: 0.42, hop: 0.28, lean: (2.6, 0.4), arms: (4.0, 0.3), arm_rest: -0.85, breath: 0.02 },
        Who::Clawd(Mat::Plush) => Feel { squash: (2.6, 0.2), land: 0.75, hop: 0.16, lean: (1.3, 0.42), arms: (1.8, 0.2), arm_rest: -0.8, breath: 0.024 },
        Who::Clawd(Mat::Chrome) => Feel { squash: (7.5, 0.7), land: 0.16, hop: 0.15, lean: (1.8, 0.6), arms: (3.5, 0.5), arm_rest: -0.55, breath: 0.01 },
        Who::Null => Feel { squash: (5.5, 0.5), land: 0.25, hop: 0.16, lean: (2.2, 0.5), arms: (6.0, 0.5), arm_rest: 0.0, breath: 0.012 },
    }
}

const GRAVITY: f32 = 54.0;

pub struct Cast {
    pub who: Who,
    f: Feel,
    state: State,
    t: f32,
    rng: u64,
    // body
    squash: Spring,
    lean: Spring,
    turn: Spring,
    y: f32,
    vy: f32,
    launch_at: Option<f32>,
    breath: f32,
    lean_to: f32,
    next_shift: f32,
    // eyes
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
    // feelings, eased in and out
    happy: Spring,
    happy_until: f32,
    think: Spring,
    talk: Spring,
    voice: Spring,
    syllable: (f32, f32), // (level, until)
    syllables_left: i32,
    level: Option<f32>, // a real voice's loudness (calls): it moves the mouth instead of made-up syllables
    attend: bool,       // listening to you: eyes mostly on you, now and then a short glance away
    nod: Spring,
    nod_until: f32,
    arms: (Spring, Spring),
    arm_to: (Spring, Spring), // where the arms are heading, eased, so big swings start softly and still overshoot
    arm_kick: (f32, f32, f32), // extra target per arm, until
    next_fidget: f32,
    // Null only
    head: (Spring, Spring, Spring), // roll, nod, turn
    laptop: Spring,
    taps: (Spring, Spring),
    next_tap: f32,
    tap_side: bool,
    tap_until: f32,
    ant: (Spring, Spring, Spring), // bend sideways, forwards, stretch
    droop: Spring,                  // asleep, the antenna flops over; eased so waking lifts it, not flicks it
    head_v: (f32, f32),             // the head's velocity a substep ago, for the antenna's push
    bright: Spring,
    pub hold: Hold,
    /// Null's face rolled between two faces by the home screen's mode dial, -0.5..0.5 (negative: up).
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
            happy: Spring::default(),
            happy_until: 0.0,
            think: Spring::default(),
            talk: Spring::default(),
            voice: Spring::default(),
            syllable: (0.0, 0.0),
            syllables_left: 0,
            level: None,
            attend: false,
            nod: Spring::default(),
            nod_until: 0.0,
            arms: (Spring::at(f.arm_rest), Spring::at(f.arm_rest)),
            arm_to: (Spring::at(f.arm_rest), Spring::at(f.arm_rest)),
            arm_kick: (0.0, 0.0, 0.0),
            next_fidget: 4.0,
            head: (Spring::default(), Spring::default(), Spring::default()),
            laptop: Spring::default(),
            taps: (Spring::default(), Spring::default()),
            next_tap: 0.0,
            tap_side: false,
            tap_until: 0.0,
            ant: (Spring::default(), Spring::default(), Spring::default()),
            droop: Spring::default(),
            head_v: (0.0, 0.0),
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
    /// A wait with an exponential spread (like real blinks): mostly near `mean`, now and then much longer.
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
        self.next_saccade = self.t + 0.05; // look somewhere that suits the new state straight away
    }

    /// A tap: a crouch, a throw up, a happy face, a squashed landing.
    pub fn hop(&mut self, now: f32) {
        if self.y > 0.0 || self.launch_at.is_some() {
            return;
        }
        self.launch_at = Some(now + 0.08);
        self.happy_until = self.happy_until.max(now + 1.0);
        if self.state == State::Sleep {
            self.state = State::Idle; // a tap wakes him
        }
    }

    /// A look held on top of the state; a new one is looked at straight away.
    pub fn set_hold(&mut self, h: Hold) {
        if h != self.hold {
            self.hold = h;
            self.next_saccade = self.t + 0.05;
        }
    }

    /// The real voice's loudness, 0..1, while talking (None: made-up syllables).
    pub fn set_voice(&mut self, level: Option<f32>) {
        self.level = level;
    }

    /// Listening to you: the gaze stays on you.
    pub fn attend(&mut self, on: bool) {
        if on != self.attend {
            self.attend = on;
            self.next_saccade = self.t + 0.05;
        }
    }

    /// A small nod: a dip of the eyes and body (Null's head), eased in and out.
    pub fn nod(&mut self, now: f32) {
        self.nod_until = now + 0.22;
    }

    /// The finger, in logical pixels from the middle of the image; `down` false when it lifts.
    pub fn point(&mut self, x: f32, y: f32, down: bool, now: f32) {
        self.finger = Some((x, y));
        self.finger_up = if down { None } else { Some(now) };
    }

    fn schedule(&mut self, now: f32) {
        let st = self.state;
        let null = self.who == Who::Null;
        // Saccades: the gaze jumps to a new spot and holds; how long and where depends on the mood.
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
                    // now and then a short glance up from it
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
                        (self.range(-0.2, 0.2), -0.8, self.range(0.8, 1.8)) // a look down at the laptop
                    } else {
                        (self.range(-0.5, 0.5), self.range(-0.6, 0.6), self.range(0.4, 1.0))
                    }
                }
            };
            // a big jump of the eyes often comes with a blink
            if ((gx - self.gaze.0).abs() > 0.5 || (gy - self.gaze.1).abs() > 0.5) && self.rand() < 0.4 && st != State::Sleep {
                self.next_blink = now;
            }
            self.gaze = (gx, gy);
            self.next_saccade = now + hold;
        }
        // tiny fixational wobbles, so held eyes are never perfectly still
        if now >= self.next_jitter {
            self.jitter = (self.range(-0.035, 0.035), self.range(-0.025, 0.025));
            self.next_jitter = now + self.range(0.25, 0.7);
        }
        // Blinks: an uneven clock, sometimes a double; none while asleep or beaming.
        if now >= self.next_blink && st != State::Sleep {
            self.blink_until = now + self.range(0.07, 0.11);
            self.next_blink = if self.rand() < 0.15 { now + 0.24 } else { now + self.wait(3.4, 0.9, 7.5) };
        }
        // Weight shifts: a new lean every few seconds.
        if now >= self.next_shift {
            self.lean_to = match st {
                State::Sleep => self.range(0.02, 0.05) * if self.rand() < 0.5 { -1.0 } else { 1.0 },
                State::Think => self.range(0.03, 0.06) * self.gaze.0.signum(),
                _ => self.range(-0.035, 0.035),
            };
            self.next_shift = now + self.range(2.2, 6.0);
        }
        // Clawds fidget an arm now and then, and gesture while talking.
        if !null && now >= self.next_fidget && st == State::Talk && self.level.is_some_and(|l| l < 0.15) {
            self.next_fidget = now + 0.1; // with a real voice, gestures land on words, not on pauses
        } else if !null && now >= self.next_fidget {
            let side = self.rand();
            let amount = match st {
                State::Talk => self.range(0.25, 0.6),
                State::Sleep => 0.0,
                _ => self.range(0.12, 0.3),
            };
            self.arm_kick = (if side < 0.5 { amount } else { 0.0 }, if side >= 0.5 { amount } else { 0.0 }, now + self.range(0.3, 0.7));
            self.next_fidget = now + if st == State::Talk { self.range(0.9, 2.0) } else { self.range(3.5, 9.0) };
        }
        // Talking: syllables of uneven length and loudness, with short gaps and longer pauses between phrases.
        if st == State::Talk && self.level.is_none() && now >= self.syllable.1 {
            if self.syllable.0 > 0.0 {
                self.syllable = (0.0, now + self.range(0.04, 0.11));
            } else if self.syllables_left <= 0 {
                self.syllables_left = (self.range(5.0, 12.0)) as i32;
                self.syllable = (0.0, now + self.range(0.25, 0.6));
            } else {
                self.syllables_left -= 1;
                self.syllable = (self.range(0.35, 1.0), now + self.range(0.09, 0.22));
            }
        }
        if st != State::Talk {
            self.syllable.0 = 0.0;
        }
        // Null types while thinking (and a little while idle): taps on alternating hands in bursts.
        if null && now >= self.next_tap && now >= self.tap_until {
            let typing = match st {
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

    /// Advances to `now` (seconds).
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
        if let Some((x, y)) = self.finger {
            (gx, gy) = ((x / 140.0).clamp(-1.0, 1.0), (-y / 170.0).clamp(-1.0, 1.0));
        }
        let happy = self.happy_until > now || st == State::Happy;
        let open_to = if now < self.blink_until { 0.0 } else { 1.0 };
        let lid_to = match (st, self.asleep_for > 4.0) {
            (State::Sleep, false) => 0.55,
            (State::Sleep, true) => 1.0,
            _ => self.hold.drowsy,
        };
        // the real voice opens the mouth from a whisper's loudness (~0.05) to fully open near 0.6
        let speaking = match self.level {
            Some(l) if st == State::Talk => ((l - 0.05) * 1.8).clamp(0.0, 1.0),
            _ => self.syllable.0,
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
        if happy {
            (arm_l, arm_r) = (arm_up, arm_up);
        }
        if now < self.arm_kick.2 {
            arm_l += self.arm_kick.0;
            arm_r += self.arm_kick.1;
        }
        let breath_period = match st {
            State::Sleep => 5.2,
            State::Talk => 2.6,
            State::Happy => 2.4,
            State::Think => 3.6,
            State::Idle => 3.2,
        } * (1.0 + 0.12 * (now * 0.23).sin() * (now * 0.071 + 1.3).cos()); // never quite the same breath twice
        let sway = 0.012 * (now * 0.9).sin() + 0.008 * (now * 0.37 + 1.0).sin() + 0.005 * (now * 1.71 + 2.0).sin();
        let squash_to = if self.launch_at.is_some() { 0.86 } else if self.y > 0.0 { 1.06 } else { 1.0 + 0.025 * speaking - 0.03 * nod };
        let (head_roll_to, head_nod_to) = match st {
            State::Think => (0.1 * gx.signum(), if gy < 0.0 { 0.22 } else { -0.08 }),
            State::Sleep => (0.06, 0.2),
            State::Talk => (0.0, -0.05 * speaking),
            State::Happy => (0.0, -0.08),
            State::Idle if self.hold.work.is_some() && gy < -0.6 => (0.0, 0.3), // head down over the laptop
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
                    self.squash.v -= self.vy.abs() * f.land; // the landing squashes, then jiggles
                    self.arms.0.v -= 3.0;
                    self.arms.1.v -= 3.0;
                    self.laptop.v -= 1.2;
                    (self.y, self.vy) = (0.0, 0.0);
                }
            }
            self.squash.step(squash_to, f.squash.0, f.squash.1, h);
            self.lean.step(self.lean_to + gx * 0.03 + sway, f.lean.0 * 0.6, f.lean.1, h);
            let work = self.hold.work.map_or(0.0, |w| w.0 * 0.3); // turned to their work
            self.turn.step(gx * 0.22 + work + 0.04 * (now * 0.21).sin(), 1.3, 0.75, h); // the body turns after the eyes
            self.look.0.step(gx, 7.0, 0.92, h); // saccades: fast (about 3 frames at 30 fps), barely overshooting
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
            self.arms.0.step(self.arm_to.0.x, f.arms.0, f.arms.1, h);
            self.arms.1.step(self.arm_to.1.x, f.arms.0 * 1.07, f.arms.1, h);
            // the screen fades slowly as he falls asleep, and comes up quicker
            let (bright_to, hz) = if st == State::Sleep { (0.35, 0.6) } else { (1.0, 1.8) };
            self.bright.step(bright_to * (1.0 - self.hold.dim), hz, 1.0, h);
            if null {
                // the head trails the body's lean a little, then nods and tilts with the mood
                self.head.0.step(head_roll_to - self.lean.x * 0.5, 2.2, 0.55, h);
                self.head.1.step(head_nod_to + 0.16 * nod, 2.5, 0.6, h);
                self.head.2.step(gx * 0.12, 1.8, 0.7, h);
                self.laptop.step(laptop_to, 3.5, 0.5, h);
                let tapping = self.t < self.tap_until;
                self.taps.0.step(if tapping && self.tap_side { 1.0 } else { 0.0 }, 9.0, 0.6, h);
                self.taps.1.step(if tapping && !self.tap_side { 1.0 } else { 0.0 }, 9.0, 0.6, h);
                // the antenna: a wobbly spring pushed by the head's acceleration, so it lags, overshoots and settles
                let hx = (self.lean.v + self.head.0.v) * -1.7 + self.turn.v * 0.3;
                let hy = self.vy + self.squash.v * 1.6;
                let (dx, dy) = (hx - self.head_v.0, hy - self.head_v.1);
                self.head_v = (hx, hy);
                // a pendulum about 0.7 long: the head's change of speed swings it the other way
                self.ant.0.v -= dx.clamp(-2.0, 2.0) * 1.4;
                self.ant.1.v -= dy.clamp(-6.0, 6.0) * 0.35;
                self.ant.2.v -= dy.clamp(-6.0, 6.0) * 0.6;
                self.droop.step(droop_to, 1.2, 1.0, h);
                self.ant.0.step(1.25 * self.droop.x, 3.2, 0.13, h);
                self.ant.1.step(0.2 * self.droop.x + 0.08 * self.head.1.x, 2.8, 0.15, h);
                self.ant.2.step(if self.hold.drowsy > 0.0 && self.attend && st != State::Sleep { 0.14 } else { 0.0 }, 5.0, 0.2, h); // perked up, on watch
                self.ant.0.x = self.ant.0.x.clamp(-1.4, 1.4);
                self.ant.2.x = self.ant.2.x.clamp(-0.25, 0.25);
            }
        }
        self.breath = (self.breath + dt * TAU / breath_period) % TAU;
    }

    /// True while anything is still moving beyond breathing and drifting: then draw at the full frame rate.
    pub fn moving(&self) -> bool {
        let springs = [&self.squash, &self.look.0, &self.look.1, &self.open, &self.happy, &self.arms.0, &self.arms.1, &self.ant.0, &self.voice, &self.nod];
        self.y > 0.0 || self.finger.is_some() || self.state == State::Talk || self.roll != 0.0 || springs.iter().any(|s| s.v.abs() > 0.05)
    }

    pub fn uniforms(&self) -> Uniforms {
        let deep = if self.state == State::Sleep { 1.7 } else { 1.0 };
        let breath = self.breath.sin() * self.f.breath * deep;
        let lag = (self.breath - 0.6).sin() * 0.05; // the arms rise a beat after the body
        let body = [self.squash.x.clamp(0.8, 1.12) * (1.0 + breath), self.y, self.lean.x, self.turn.x];
        let eyes = [self.look.0.x, self.look.1.x, self.open.x.clamp(0.0, 1.0), self.lid.x.clamp(0.0, 1.0)];
        let happy = self.happy.x.clamp(0.0, 1.0);
        match self.who {
            Who::Null => vec![
                ("uBody", body),
                ("uHead", [self.head.0.x, self.head.1.x - breath * 2.0, self.head.2.x, self.laptop.x + lag * 0.2]),
                ("uAnt", [self.ant.0.x, self.ant.1.x, self.ant.2.x, self.roll]),
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

#[cfg(test)]
#[path = "cast_tests.rs"]
mod tests;
