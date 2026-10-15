//! Whoever is calling, alive on the call screen (cast.rs, drawn by live_gl.rs): the vinyl Clawd for Claude, Null for
//! Codex, the Haiku, Sonnet and Opus Clawds for agents of those names. It moves with the call: hops while it rings
//! and once more when the call connects, looks at you while it's your turn and nods as you finish a phrase, thinks
//! while the agent works or the call is on hold, and talks with its mouth and arms driven by the real voice. It
//! stands big over the caption, and small on the agent's bubble while the thread shows, easing between the two.
use crate::{
    alive,
    cast::{Cast, State, Who},
    view::Phase,
};

pub const BIG_K: f32 = 0.55; // the size over the caption: a Haiku's hop just clears the header
const SMALL_K: f32 = 0.3; // perched on the bubble
const MOVE_S: f32 = 0.5; // the ease between the two
const RING_EVERY: f32 = 1.5; // a hop per ring
const SHOW_AFTER: f32 = 0.6; // fades in once the call says who is calling, or after this long

/// Where it stands, in the call stage's logical px: the middle of its feet, and its size (k times home size).
#[derive(Clone, Copy, PartialEq)]
pub struct Place {
    pub x: f32,
    pub y: f32,
    pub k: f32,
}

pub struct Caller {
    pub cast: Cast,
    seed: u64,
    shown_at: Option<f32>, // when it began fading in
    swap_at: Option<f32>,  // fading out to change who it is
    from: Place,
    to: Place,
    moved_at: f32,
    perched: bool,
    phase: Option<Phase>,
    next_ring: f32,
    next_nod: f32,
    me: (f32, f32), // your voice: how loud lately, and when you last went quiet after speaking
}

fn place(who: Who, perched: bool) -> Place {
    let (k, left, y) = if perched { (SMALL_K, 30.0, 73.0) } else { (BIG_K, 24.0, 108.0) };
    Place { x: left + alive::frame(who).body_w * k / 2.0, y, k }
}

fn ease_out(t: f32) -> f32 {
    1.0 - (1.0 - t.clamp(0.0, 1.0)).powi(3)
}

impl Caller {
    pub fn new(seed: u64) -> Self {
        let who = Who::named("Claude");
        Caller {
            cast: Cast::new(who, seed),
            seed,
            shown_at: None,
            swap_at: None,
            from: place(who, false),
            to: place(who, false),
            moved_at: -1e9,
            perched: false,
            phase: None,
            next_ring: 0.0,
            next_nod: 0.0,
            me: (0.0, -1e9),
        }
    }

    /// The texture's size (logical px): the widest and tallest picture at its big size, with room above.
    pub fn size() -> (f32, f32) {
        let f = alive::frame(Who::Clawd(crate::cast::Mat::Vinyl));
        (f.w * BIG_K, (f.h + alive::HEAD) * BIG_K)
    }

    pub fn poke(&mut self, now: f32) {
        self.cast.hop(now);
    }

    /// How visible it is, 0..1: it fades in once it knows who it is, and out and in again if that changes.
    pub fn fade(&self, now: f32) -> f32 {
        match (self.swap_at, self.shown_at) {
            (Some(at), _) => 1.0 - ((now - at) / 0.15).clamp(0.0, 1.0),
            (None, Some(at)) => ((now - at) / 0.3).clamp(0.0, 1.0),
            (None, None) => 0.0,
        }
    }

    pub fn place(&self, now: f32) -> Place {
        let e = ease_out((now - self.moved_at) / MOVE_S);
        let mix = |a: f32, b: f32| a + (b - a) * e;
        Place { x: mix(self.from.x, self.to.x), y: mix(self.from.y, self.to.y), k: mix(self.from.k, self.to.k) }
    }

    /// Advances to `now` (seconds). `voice`: the agent's voice level as it plays; `me`: your mic level.
    pub fn tick(&mut self, now: f32, agent: &str, phase: Phase, perched: bool, voice: f32, me: f32) {
        let who = Who::named(agent);
        // Who it is: shown once the call has said (or soon anyway); a change fades out, swaps, and fades back in.
        if self.shown_at.is_none() && (who != self.cast.who || now >= SHOW_AFTER) {
            self.swap(who, now);
            self.shown_at = Some(now);
        } else if who != self.cast.who && self.swap_at.is_none() {
            self.swap_at = Some(now);
        }
        if self.swap_at.is_some_and(|at| now - at >= 0.15) {
            self.swap(who, now);
            (self.swap_at, self.shown_at) = (None, Some(now));
        }
        if perched != self.perched {
            self.from = self.place(now);
            self.to = place(self.cast.who, perched);
            (self.perched, self.moved_at) = (perched, now);
        }

        let was = self.phase.replace(phase);
        let connected = matches!(phase, Phase::Talking | Phase::Listening | Phase::Hearing | Phase::Thinking | Phase::Hold);
        if connected && matches!(was, Some(Phase::Incoming | Phase::Connecting)) {
            self.cast.hop(now); // a happy beat as the call connects
        }
        // Your voice: a nod a moment after you finish a phrase (not every time), the odd one while it's your turn.
        let loud = me > 0.2;
        if loud {
            self.me.0 = now;
        } else if self.me.0 > self.me.1 && now - self.me.0 > 0.35 {
            self.me.1 = now;
            if phase == Phase::Hearing && now >= self.next_nod {
                self.cast.nod(now);
                self.next_nod = now + 1.6;
            }
        }
        if phase == Phase::Listening && now >= self.next_nod {
            if self.next_nod > 0.0 {
                self.cast.nod(now);
            }
            self.next_nod = now + 6.0 + (now * 7.3).sin().abs() * 4.0;
        }
        let state = match phase {
            Phase::Talking => State::Talk,
            Phase::Thinking | Phase::Hold => State::Think,
            _ => State::Idle,
        };
        if self.cast.state() != State::Happy {
            self.cast.set_state(state);
        }
        self.cast.attend(matches!(phase, Phase::Incoming | Phase::Connecting | Phase::Listening | Phase::Hearing));
        self.cast.set_voice((phase == Phase::Talking).then_some(voice));
        if phase == Phase::Incoming && now >= self.next_ring {
            if self.shown_at.is_some() {
                self.cast.hop(now);
            }
            self.next_ring = now + RING_EVERY;
        }
        self.cast.tick(now);
    }

    fn swap(&mut self, who: Who, now: f32) {
        if who != self.cast.who {
            self.seed = self.seed.rotate_left(13) | 1;
            self.cast = Cast::new(who, self.seed);
            self.cast.tick(now);
        }
        self.from = place(who, self.perched);
        self.to = self.from;
    }
}
