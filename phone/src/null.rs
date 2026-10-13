//! Null, Codex's pet, for calls from Codex: its own frames (ui/null.png, cut from the Codex app's sprite sheet) at
//! the Codex app's own timings. Like the app, a one-off animation plays three times, then Null goes back to idle.
use crate::mascot::Phase;

/// (row in null.png, frames, ms per frame, ms on the last frame).
type Anim = (i32, i32, u32, u32);
const WAVING: Anim = (1, 4, 140, 280);
const JUMPING: Anim = (2, 5, 140, 280);
const WAITING: Anim = (3, 6, 150, 260); // the "?" screen: your turn
const RUNNING: Anim = (4, 6, 120, 220); // typing on its laptop: working
const REVIEW: Anim = (5, 6, 150, 280);
const IDLE_MS: [u32; 6] = [1680, 660, 660, 840, 840, 1920]; // row 0; the app's idle, slowed 6x as it does

fn length(a: Anim) -> u32 {
    (a.1 as u32 - 1) * a.2 + a.3
}

fn at(a: Anim, t: u32) -> i32 {
    a.0 * 6 + ((t / a.2) as i32).min(a.1 - 1)
}

fn idle(t: u32) -> i32 {
    let mut t = t % IDLE_MS.iter().sum::<u32>();
    for (i, &d) in IDLE_MS.iter().enumerate() {
        if t < d {
            return i as i32;
        }
        t -= d;
    }
    0
}

#[derive(Default)]
pub struct Null {
    phase: Option<Phase>,
    since: f32,
    poked: Option<f32>,
}

impl Null {
    pub fn poke(&mut self, now: f32) {
        self.poked = Some(now);
    }

    /// The frame to show at `now` (seconds), as an index into null.png (row * 6 + column).
    pub fn frame(&mut self, now: f32, phase: Phase) -> i32 {
        if self.phase != Some(phase) {
            (self.phase, self.since) = (Some(phase), now);
        }
        let ms = |since: f32| ((now - since).max(0.0) * 1000.0) as u32;
        if let Some(p) = self.poked {
            if ms(p) < length(JUMPING) {
                return at(JUMPING, ms(p));
            }
            self.poked = None;
        }
        let t = ms(self.since);
        let (anim, looping) = match phase {
            Phase::Incoming => (JUMPING, true),
            Phase::Talking => (REVIEW, true),
            Phase::Thinking | Phase::Hold => (RUNNING, true),
            Phase::Connecting | Phase::Ended => (WAVING, false),
            Phase::Listening => (WAITING, false),
            Phase::Hearing => return idle(t),
        };
        let len = length(anim);
        if looping || t < 3 * len { at(anim, t % len) } else { idle(t - 3 * len) }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn plays_then_rests() {
        let mut n = Null::default();
        assert_eq!(n.frame(0.0, Phase::Listening), 18); // "?" first frame
        assert_eq!(n.frame(0.149, Phase::Listening), 18);
        assert_eq!(n.frame(0.15, Phase::Listening), 19);
        assert_eq!(n.frame(3.05, Phase::Listening), 0); // three plays (3 x 1010 ms), then idle
        assert_eq!(n.frame(4.75, Phase::Listening), 1);
        assert_eq!(n.frame(100.0, Phase::Thinking), 24); // a new state starts at its first frame
        assert_eq!(n.frame(100.83, Phase::Thinking), 24); // and loops (820 ms)
        n.poke(101.0);
        assert_eq!(n.frame(101.0, Phase::Thinking), 12); // a tap: one jump...
        assert_eq!(n.frame(101.85, Phase::Thinking), 25); // ...then back where it was
    }
}
