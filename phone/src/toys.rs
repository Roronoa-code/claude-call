//! The vinyl Clawd and Null's life on the home screen (toys.slint): random blinks and glances, Null thinking at its
//! laptop now and then, sleep in the quiet modes, and a squash-and-hop when tapped. Each behaviour is a short list of
//! timed steps (pose, squash, lift, lean); Slint springs between them.
use std::collections::VecDeque;

// Frame order matches toys.slint.
pub const C_IDLE: i32 = 0;
pub const C_BLINK: i32 = 1;
pub const C_LOOK_LEFT: i32 = 2;
pub const C_LOOK_RIGHT: i32 = 3;
pub const C_HAPPY: i32 = 7;
pub const C_SLEEP: i32 = 8;
pub const N_IDLE: i32 = 0;
pub const N_BLINK: i32 = 1;
pub const N_THINK: i32 = 2;
pub const N_HAPPY: i32 = 3;
pub const N_SLEEP: i32 = 4;

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Body {
    pub pose: i32,
    pub squash: f32,
    pub lift: f32,
    pub lean: f32, // degrees
}

#[derive(Clone, Copy)]
struct Step {
    at: f32,
    pose: Option<i32>,
    squash: f32,
    lift: f32,
    lean: f32,
}
const fn step(at: f32, pose: Option<i32>, squash: f32, lift: f32, lean: f32) -> Step {
    Step { at, pose, squash, lift, lean }
}

pub struct Toy {
    pub body: Body,
    codex: bool,
    steps: VecDeque<Step>,
    next: f32, // when the next idle behaviour starts
    rng: u64,
    asleep: bool,
}

impl Toy {
    pub fn new(codex: bool, seed: u64) -> Self {
        let idle = if codex { N_IDLE } else { C_IDLE };
        Toy { body: Body { pose: idle, squash: 1.0, lift: 0.0, lean: 0.0 }, codex, steps: VecDeque::new(), next: 1.5, rng: seed | 1, asleep: false }
    }

    fn rand(&mut self) -> f32 {
        // xorshift: plenty for picking blinks
        self.rng ^= self.rng << 13;
        self.rng ^= self.rng >> 7;
        self.rng ^= self.rng << 17;
        (self.rng % 10_000) as f32 / 10_000.0
    }

    fn idle_pose(&self) -> i32 {
        match (self.codex, self.asleep) {
            (false, false) => C_IDLE,
            (false, true) => C_SLEEP,
            (true, false) => N_IDLE,
            (true, true) => N_SLEEP,
        }
    }

    fn play(&mut self, now: f32, steps: &[Step]) {
        self.steps = steps.iter().map(|s| Step { at: now + s.at, ..*s }).collect();
    }

    /// Squash, hop into the happy pose, land, settle; then back to rest.
    pub fn hop(&mut self, now: f32) {
        let (happy, rest) = (if self.codex { N_HAPPY } else { C_HAPPY }, self.idle_pose());
        self.play(now, &[
            step(0.0, None, 0.86, 0.0, 0.0),
            step(0.09, Some(happy), 1.08, 16.0, 0.0),
            step(0.34, None, 0.94, 0.0, 0.0),
            step(0.46, None, 1.0, 0.0, 0.0),
            step(1.1, Some(rest), 0.97, 0.0, 0.0),
            step(1.22, None, 1.0, 0.0, 0.0),
        ]);
        self.next = now + 2.5;
    }

    /// Advances to `now` (seconds); true when the body changed.
    pub fn tick(&mut self, now: f32, asleep: bool) -> bool {
        let before = self.body;
        if asleep != self.asleep {
            self.asleep = asleep;
            let rest = self.idle_pose();
            // nodding off (or waking): a slow sink or a small stretch
            self.play(now, &[step(0.0, Some(rest), if asleep { 0.95 } else { 1.04 }, 0.0, 0.0), step(0.3, None, 1.0, 0.0, 0.0)]);
        }
        if self.steps.is_empty() && now >= self.next {
            self.idle_behaviour(now);
        }
        while self.steps.front().is_some_and(|s| s.at <= now) {
            let s = self.steps.pop_front().unwrap();
            if let Some(p) = s.pose {
                self.body.pose = p;
            }
            (self.body.squash, self.body.lift, self.body.lean) = (s.squash, s.lift, s.lean);
        }
        self.body != before
    }

    fn idle_behaviour(&mut self, now: f32) {
        let (r, rest) = (self.rand(), self.idle_pose());
        if self.asleep {
            self.next = now + 4.0 + self.rand() * 4.0; // asleep: just breathing (in Slint)
            return;
        }
        match (self.codex, r) {
            // Clawd: mostly blinks, sometimes glances (often towards Null on its right), now and then a double blink,
            // a shift of his weight or a small happy bounce on the spot.
            (false, r) if r < 0.45 => self.play(now, &[step(0.0, Some(C_BLINK), 1.0, 0.0, 0.0), step(0.11, Some(rest), 1.0, 0.0, 0.0)]),
            (false, r) if r < 0.53 => {
                let side = if self.rand() < 0.5 { -2.0 } else { 2.0 };
                self.play(now, &[step(0.0, None, 0.98, 0.0, side), step(0.5, None, 1.0, 0.0, side * 0.6), step(1.6, None, 1.0, 0.0, 0.0)]);
            }
            (false, r) if r < 0.58 => self.play(now, &[
                step(0.0, None, 0.95, 0.0, 0.0),
                step(0.14, None, 1.03, 0.0, 0.0),
                step(0.3, None, 1.0, 0.0, 0.0),
            ]),
            (false, r) if r < 0.85 => {
                let right = self.rand() < 0.65;
                let (look, lean) = if right { (C_LOOK_RIGHT, 2.5) } else { (C_LOOK_LEFT, -2.5) };
                let hold = 1.2 + self.rand();
                self.play(now, &[step(0.0, Some(look), 1.0, 0.0, lean), step(hold, Some(rest), 1.0, 0.0, 0.0)]);
            }
            (false, _) => self.play(now, &[
                step(0.0, Some(C_BLINK), 1.0, 0.0, 0.0),
                step(0.1, Some(rest), 1.0, 0.0, 0.0),
                step(0.24, Some(C_BLINK), 1.0, 0.0, 0.0),
                step(0.34, Some(rest), 1.0, 0.0, 0.0),
            ]),
            // Null: blinks its screen, thinks at its laptop, now and then a small happy beat.
            (true, r) if r < 0.6 => self.play(now, &[step(0.0, Some(N_BLINK), 1.0, 0.0, 0.0), step(0.14, Some(rest), 1.0, 0.0, 0.0)]),
            (true, r) if r < 0.88 => {
                let hold = 1.8 + self.rand();
                self.play(now, &[step(0.0, Some(N_THINK), 1.0, 0.0, -1.5), step(hold, Some(rest), 1.0, 0.0, 0.0)]);
            }
            (true, _) => self.play(now, &[
                step(0.0, None, 0.93, 0.0, 0.0),
                step(0.08, Some(N_HAPPY), 1.04, 6.0, 0.0),
                step(0.22, None, 1.0, 0.0, 0.0),
                step(0.9, Some(rest), 1.0, 0.0, 0.0),
            ]),
        }
        // Uneven gaps, like a living thing: mostly short, now and then a long still moment.
        self.next = now + if self.codex { 2.6 + self.rand() * 2.8 } else { 1.0 + self.rand().powi(2) * 6.0 };
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_hop_squashes_lifts_lands_and_returns_to_rest() {
        let mut t = Toy::new(false, 7);
        t.tick(0.0, false);
        t.hop(1.0);
        t.tick(1.0, false);
        assert_eq!((t.body.squash, t.body.lift), (0.86, 0.0)); // wind-up
        t.tick(1.1, false);
        assert_eq!((t.body.pose, t.body.lift), (C_HAPPY, 16.0)); // in the air, happy
        t.tick(1.4, false);
        assert_eq!(t.body.lift, 0.0); // landed
        t.tick(2.5, false);
        assert_eq!(t.body.pose, C_IDLE);
    }

    #[test]
    fn idle_life_always_comes_back_to_rest_and_sleep_stays_asleep() {
        let mut t = Toy::new(true, 99);
        let mut now = 0.0;
        while now < 120.0 {
            t.tick(now, false);
            now += 0.033;
        }
        while !t.steps.is_empty() {
            now += 0.033;
            t.tick(now, false);
        }
        assert_eq!(t.body.pose, N_IDLE);
        t.tick(now, true);
        for _ in 0..3000 {
            now += 0.033;
            t.tick(now, true);
        }
        assert_eq!(t.body.pose, N_SLEEP);
    }
}
