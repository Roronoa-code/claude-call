//! The live cast's framing (where each character stands in its picture, for every screen that shows one), and the
//! live Clawd and Null on the home screen: real 3D toys drawn by the GPU (live_gl.rs, shaders in art/live3d),
//! not swapped sprites. Each is a Cast (cast.rs: springs, saccades, blinks, breathing, hops); this gives the pair
//! their home life: asleep in the quiet modes (a tap wakes one for a while), Null now and then thinking at his laptop
//! or giving a happy hop, Clawd now and then lost in thought, and both watching a finger.
use crate::cast::{Cast, Mat, State, Who};

/// Each image's size in logical pixels; toys.slint draws them at exactly these sizes.
pub const CLAWD_SIZE: (f32, f32) = (300.0, 250.0);
pub const NULL_SIZE: (f32, f32) = (150.0, 230.0);

/// A character's picture at home size (k = 1), in logical px: its size and its body's width. ui/actor.slint has the
/// same picture sizes, and where the body stands in them.
pub struct Frame {
    pub w: f32,
    pub h: f32,
    pub body_w: f32,
}

pub fn frame(who: Who) -> Frame {
    match who {
        Who::Null => Frame { w: 150.0, h: 230.0, body_w: 80.0 },
        Who::Clawd(_) => Frame { w: 300.0, h: 250.0, body_w: 208.0 },
    }
}

/// Room above the picture for hops and Haiku's tufts. The shaders frame by height from the bottom, so drawing a
/// taller view than `uRes` only adds sky; nothing moves or scales.
pub const HEAD: f32 = 40.0;

/// Null stands beside Clawd at home; alone (calls, chats) he is drawn this much larger, so his thin frame reads at
/// a Clawd's size. actor.slint matches it.
pub const NULL_SOLO: f32 = 1.2;

/// `k` for `who` drawn alone.
pub fn solo(who: Who, k: f32) -> f32 {
    if who == Who::Null { k * NULL_SOLO } else { k }
}

/// The texture (logical px) a character drawn alone at `k` needs: the larger of a Clawd's and a solo Null's.
pub fn solo_size(k: f32) -> (f32, f32) {
    let (c, n) = (frame(Who::Clawd(Mat::Vinyl)), frame(Who::Null));
    (c.w.max(n.w * NULL_SOLO) * k, (c.h + HEAD).max((n.h + HEAD) * NULL_SOLO) * k)
}

/// For drawing `who` at `k` times home size on a screen of `sf` px per logical px: its uRes and the viewport (the
/// picture plus HEAD above it), in px.
pub fn view(who: Who, k: f32, sf: f32) -> ((i32, i32), (i32, i32)) {
    let f = frame(who);
    let px = |v: f32| (v * k * sf).round().max(1.0) as i32;
    ((px(f.w), px(f.h)), (px(f.w), px(f.h + HEAD)))
}

/// Whether a character last drawn at `last` needs a new frame at `now`: 60 fps while it moves, 30 while it only
/// breathes, 15 asleep.
pub fn due(c: &Cast, last: f32, now: f32) -> bool {
    let fps = if c.moving() { 60.0 } else if c.state() == State::Sleep { 15.0 } else { 30.0 };
    now - last >= 1.0 / fps - 0.004
}

const WAKE_FOR: f32 = 8.0; // a tap in a quiet mode wakes him this long

pub struct Pair {
    pub clawd: Cast,
    pub null: Cast,
    awake_until: (f32, f32),
    moment: (f32, f32), // when each one's next idle moment starts
    moment_end: (f32, f32),
    rng: u64,
}

impl Pair {
    pub fn new(seed: u64) -> Self {
        Pair {
            clawd: Cast::new(Who::Clawd(Mat::Vinyl), seed),
            null: Cast::new(Who::Null, seed.rotate_left(29) ^ 0x85eb_ca6b),
            awake_until: (0.0, 0.0),
            moment: (14.0, 5.0),
            moment_end: (0.0, 0.0),
            rng: seed.rotate_left(17) | 1,
        }
    }

    fn rand(&mut self) -> f32 {
        self.rng ^= self.rng << 13;
        self.rng ^= self.rng >> 7;
        self.rng ^= self.rng << 17;
        (self.rng % 10_000) as f32 / 10_000.0
    }

    /// A tap on one of them (`codex` for Null): a hop, and awake for a while even in a quiet mode.
    pub fn hop(&mut self, codex: bool, now: f32) {
        if codex {
            self.awake_until.1 = now + WAKE_FOR;
            self.null.hop(now);
        } else {
            self.awake_until.0 = now + WAKE_FOR;
            self.clawd.hop(now);
        }
    }

    /// The finger, in logical pixels from Clawd's eyes, and how far right Null stands of him; `down` false when it
    /// lifts. Both watch it.
    pub fn point(&mut self, x: f32, y: f32, null_dx: f32, down: bool, now: f32) {
        self.clawd.point(x, y, down, now);
        self.null.point(x - null_dx, y, down, now);
    }

    /// Advances both to `now` (seconds); `asleep` while the contact mode is a quiet one.
    pub fn tick(&mut self, now: f32, asleep: bool) {
        let doze = (asleep && now >= self.awake_until.0, asleep && now >= self.awake_until.1);
        for (c, doze) in [(&mut self.clawd, doze.0), (&mut self.null, doze.1)] {
            match (doze, c.state()) {
                (true, State::Sleep) | (false, State::Idle | State::Think | State::Talk | State::Happy) => {}
                (true, _) => c.set_state(State::Sleep),
                (false, State::Sleep) => c.set_state(State::Idle),
            }
        }
        // Idle moments: Null thinks at his laptop (typing) or gives a happy hop; Clawd now and then looks up, thinking.
        if self.null.state() == State::Think && now >= self.moment_end.1 {
            self.null.set_state(State::Idle);
        }
        if self.clawd.state() == State::Think && now >= self.moment_end.0 {
            self.clawd.set_state(State::Idle);
        }
        if now >= self.moment.1 && self.null.state() == State::Idle {
            if self.rand() < 0.75 {
                self.null.set_state(State::Think);
                self.moment_end.1 = now + 2.5 + self.rand() * 3.0;
            } else {
                self.null.set_state(State::Happy); // a hop; it eases back to idle on its own
                self.moment_end.1 = now + 1.6;
            }
            self.moment.1 = self.moment_end.1 + 5.0 + self.rand() * 9.0;
        }
        if now >= self.moment.0 && self.clawd.state() == State::Idle {
            self.clawd.set_state(State::Think);
            self.moment_end.0 = now + 1.6 + self.rand() * 1.8;
            self.moment.0 = self.moment_end.0 + 12.0 + self.rand() * 16.0;
        }
        self.clawd.tick(now);
        self.null.tick(now);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn run(p: &mut Pair, from: f32, to: f32, asleep: bool) {
        let mut now = from;
        while now < to {
            now += 1.0 / 60.0;
            p.tick(now, asleep);
        }
    }

    #[test]
    fn null_has_moments_and_both_come_back_to_idle() {
        let mut p = Pair::new(7);
        let mut thought = false;
        let mut now = 0.0;
        while now < 60.0 {
            now += 1.0 / 60.0;
            p.tick(now, false);
            thought |= p.null.state() == State::Think;
        }
        assert!(thought);
        run(&mut p, 60.0, 61.0, false);
        assert!(matches!(p.null.state(), State::Idle | State::Think | State::Happy));
    }

    #[test]
    fn quiet_modes_sleep_and_a_tap_wakes_one_for_a_while() {
        let mut p = Pair::new(3);
        run(&mut p, 0.0, 2.0, true);
        assert_eq!((p.clawd.state(), p.null.state()), (State::Sleep, State::Sleep));
        p.hop(false, 2.0);
        run(&mut p, 2.0, 4.0, true);
        assert_ne!(p.clawd.state(), State::Sleep);
        assert_eq!(p.null.state(), State::Sleep);
        run(&mut p, 4.0, 2.0 + WAKE_FOR + 0.5, true);
        assert_eq!(p.clawd.state(), State::Sleep);
        run(&mut p, 11.0, 12.0, false);
        assert_ne!(p.clawd.state(), State::Sleep);
    }
}
