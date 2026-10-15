//! The live cast's framing (where each character stands in its picture, for every screen that shows one), and the
//! live Clawd and Null on the home screen: real 3D toys drawn by the GPU (live_gl.rs, shaders in art/live3d),
//! not swapped sprites. Each is a Cast (cast.rs: springs, saccades, blinks, breathing, hops); this gives the pair
//! their home life: they act out the contact mode (Null's face shows it, Clawd copies him a beat later; asleep in the
//! sleep modes, where a tap wakes one for a while), a drag on Null rolls his face through the modes, Clawd is now and
//! then lost in thought, and both watch a finger.
use crate::cast::{Cast, Hold, Mat, State, Who};

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

/// Logical px of drag on Null per mode; his face rolls through them in settings::MODES order (call, text, both,
/// none, wake, sleep), dragging up towards Calls only.
pub const STEP: f32 = 48.0;
const LAST: f32 = 5.0;
const LATE: f32 = 0.22; // Clawd turns to see what Null shows a beat later
const DWELL: f32 = 0.3; // a face held this long gets its full reaction (a cheer, Clawd talking)

fn nearest(p: f32) -> usize {
    p.round().clamp(0.0, LAST) as usize
}

pub struct Pair {
    pub clawd: Cast,
    pub null: Cast,
    awake_until: (f32, f32),
    moment: f32, // Clawd's next idle moment starts
    moment_end: f32,
    rng: u64,
    /// The mode the pair act out (an index into settings::MODES): the stored one, or the one a drag shows.
    shown: usize,
    shown_at: f32,
    acted: (usize, usize), // the mode each one last reacted to
    copy: usize,           // the mode Clawd copies, a beat after Null shows it
    talk_until: f32,
    dial: Option<(usize, f32)>, // a drag on Null: the mode it started from, and where it is now (a fractional mode)
    roll: (f32, f32),           // the face's place (a fractional mode) and speed, springing to `shown` after a drag
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

    /// A drag on Null, `dy` logical px from where it began (negative: up): the mode his face now shows.
    pub fn drag(&mut self, dy: f32) -> usize {
        let from = self.dial.map_or(self.shown.min(5), |d| d.0);
        let mut p = from as f32 + dy / STEP;
        // past either end it gives a little, like rubber
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

    /// The finger lifts (`keep`), or the drag is cancelled: the mode to keep. The face springs home.
    pub fn release(&mut self, keep: bool) -> usize {
        match self.dial.take() {
            Some((_, p)) if keep => nearest(p),
            Some((from, _)) => from,
            None => self.shown,
        }
    }

    /// A small nudge of Null's face, up and back, as a hint that it rolls: `t` seconds into it (it lasts 1.4 s).
    pub fn nudge(&mut self, t: f32) {
        if self.dial.is_none() && (0.0..1.4).contains(&t) && self.shown <= 5 {
            let k = (t / 1.4 * std::f32::consts::TAU).sin() * (1.0 - t / 1.4);
            self.roll = (self.shown as f32 - 0.32 * k, 0.0);
        }
    }

    /// Advances both to `now` (seconds), acting out `mode` (an index into settings::MODES) unless a drag shows another.
    pub fn tick(&mut self, now: f32, mode: usize) {
        let dt = (now - self.last).clamp(0.0, 0.05);
        self.last = now;
        let m = self.dial.map_or(mode.min(5), |d| nearest(d.1));
        if self.shown > 5 {
            // opening the app: already in the mode, no reactions
            (self.shown, self.shown_at, self.acted, self.copy, self.roll) = (m, now - DWELL, (m, m), m, (m as f32, 0.0));
        }
        if m != self.shown {
            if self.dial.is_none() {
                self.roll = (m as f32, 0.0); // changed elsewhere: the face changes in place
            }
            (self.shown, self.shown_at) = (m, now);
        }
        // after a drag the face springs home with a little overshoot
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

    /// Null's face is the mode: a voice wave, typing dots, happy, heads down at the laptop, on watch, dark.
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
            n.set_state(state); // a cheer runs its course first
        }
        if settled && self.acted.1 != m {
            self.acted.1 = m;
            if m == 2 {
                n.set_state(State::Happy); // a hop for Calls and messages
            }
        }
    }

    /// Clawd copies what Null shows, a beat later: talks to you, reads Null's laptop, cheers, turns to his work, sleeps.
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
            c.set_state(State::Happy); // a cheer, arms up
        }
        // Now and then, while just waiting for you, Clawd looks up, thinking.
        if (m == 0 || m == 2) && now >= self.moment && c.state() == State::Idle {
            c.set_state(State::Think);
            self.moment_end = now + 1.6 + self.rand() * 1.8;
            self.moment = self.moment_end + 12.0 + self.rand() * 16.0;
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn run(p: &mut Pair, from: f32, to: f32, mode: usize) {
        let mut now = from;
        while now < to {
            now += 1.0 / 60.0;
            p.tick(now, mode);
        }
    }

    #[test]
    fn each_mode_has_its_own_look() {
        let mut p = Pair::new(7);
        let want = [State::Talk, State::Think, State::Idle, State::Idle, State::Idle, State::Sleep];
        let mut t = 0.0;
        for (m, s) in want.into_iter().enumerate() {
            run(&mut p, t, t + 4.0, m);
            t += 4.0;
            assert_eq!(p.null.state(), s, "mode {m}");
            assert_eq!(p.null.roll, 0.0);
        }
        // Quiet: eyes on their work, not on you
        let eyes = |c: &Cast| c.uniforms().iter().find(|u| u.0 == "uEyes").unwrap().1[1];
        run(&mut p, t, t + 1.0, 3);
        let (mut null, mut clawd, mut now) = (0.0, 0.0, t + 1.0);
        for _ in 0..600 {
            now += 1.0 / 60.0;
            p.tick(now, 3);
            (null, clawd) = (null + eyes(&p.null) / 600.0, clawd + eyes(&p.clawd) / 600.0);
        }
        assert!(null < -0.5 && clawd < -0.45, "{null} {clawd}");
        let t = now;
        // the cheer for Calls and messages eases back to idle with the happy face kept
        run(&mut p, t, t + 4.0, 2);
        assert!(p.null.hold.glad && p.clawd.hold.glad);
        // Asleep, wake me: Clawd sleeps, Null keeps watch
        run(&mut p, t + 4.0, t + 10.0, 4);
        assert_eq!((p.clawd.state(), p.null.state()), (State::Sleep, State::Idle));
    }

    #[test]
    fn a_drag_rolls_through_the_modes_and_letting_go_keeps_one() {
        let mut p = Pair::new(3);
        run(&mut p, 0.0, 1.0, 5);
        assert_eq!(p.drag(-10.0), 5); // under half a step: still Asleep
        assert_eq!(p.drag(-STEP * 2.0), 3);
        run(&mut p, 1.0, 1.2, 5); // the stored mode is still Asleep, the face shows Quiet
        assert_eq!(p.null.state(), State::Idle);
        assert_eq!(p.drag(-STEP * 20.0), 0); // past the end: Calls only, and no further
        assert_eq!(p.drag(-STEP * 2.4), 3); // back down, mid-roll towards Messages only
        run(&mut p, 1.2, 1.3, 5);
        assert!(p.null.roll < -0.3);
        assert_eq!(p.release(true), 3);
        run(&mut p, 1.3, 3.0, 3);
        assert_eq!(p.null.roll, 0.0); // sprung home
        p.drag(STEP * 2.0);
        assert_eq!(p.release(false), 3); // cancelled: back to where it began
    }

    #[test]
    fn sleep_modes_sleep_and_a_tap_wakes_one_for_a_while() {
        let mut p = Pair::new(3);
        run(&mut p, 0.0, 2.0, 5);
        assert_eq!((p.clawd.state(), p.null.state()), (State::Sleep, State::Sleep));
        p.hop(false, 2.0);
        run(&mut p, 2.0, 4.0, 5);
        assert_ne!(p.clawd.state(), State::Sleep);
        assert_eq!(p.null.state(), State::Sleep);
        run(&mut p, 4.0, 2.0 + WAKE_FOR + 0.5, 5);
        assert_eq!(p.clawd.state(), State::Sleep);
        run(&mut p, 11.0, 12.0, 0);
        assert_ne!(p.clawd.state(), State::Sleep);
    }
}
