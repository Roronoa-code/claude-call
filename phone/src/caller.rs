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
/// PREVIEW (branch preview/call-screen): its size on each call screen direction (debug.callme.call; 0 as shipped).
/// The directions place it themselves (call_looks.slint) and never perch it.
/// Round 2 (branch preview/call-screen-2, looks 5-8): Subtitles, Script, Your light, and Camera (its closest shot; it
/// moves between shots, `shot`).
pub const LOOK_K: [f32; 9] = [BIG_K, 1.0, 1.5, 0.85, 0.6, 1.0, 0.72, 0.95, 1.15];
pub const CAMERA: usize = 8;
const MOVE_S: f32 = 0.5; // the ease between the two
const DOLLY_S: f32 = 1.3; // PREVIEW (Camera): a camera move between shots
const RING_EVERY: f32 = 1.5; // a hop per ring
const SHOW_AFTER: f32 = 0.6; // fades in once the call says who is calling, or after this long

/// PREVIEW: the most floor drawn under a picture at k = 1: the record's (stage.rs), or the spotlight's in round 2.
pub fn floor(look: usize) -> f32 {
    if look >= 5 { 82.0 } else { 45.0 }
}

/// PREVIEW (Camera): how close the camera is in each state: wide while it rings, close while the caller talks,
/// further back for your turn and while it works, wider on hold and at the end.
fn shot(phase: Option<Phase>) -> f32 {
    match phase {
        None | Some(Phase::Incoming) => 0.62,
        Some(Phase::Connecting | Phase::Talking) => 1.15,
        Some(Phase::Listening | Phase::Hearing) => 0.9,
        Some(Phase::Thinking) => 0.98,
        Some(Phase::Hold) => 0.8,
        Some(Phase::Ended) => 0.7,
    }
}

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
    dolly: bool, // PREVIEW (Camera): the current move is a camera move (slower, eased in and out)
    perched: bool,
    look: usize,
    phase: Option<Phase>,
    next_ring: f32,
    next_nod: f32,
    me: (f32, f32), // your voice: how loud lately, and when you last went quiet after speaking
}

fn place(who: Who, perched: bool, look: usize, phase: Option<Phase>) -> Place {
    if look == CAMERA {
        return Place { x: 0.0, y: 0.0, k: shot(phase) };
    }
    if look > 0 {
        return Place { x: 0.0, y: 0.0, k: LOOK_K[look] };
    }
    let (k, left, y) = if perched { (SMALL_K, 30.0, 73.0) } else { (BIG_K, 24.0, 108.0) };
    Place { x: left + alive::frame(who).body_w * alive::solo(who, k) / 2.0, y, k }
}

fn ease_out(t: f32) -> f32 {
    1.0 - (1.0 - t.clamp(0.0, 1.0)).powi(3)
}

impl Caller {
    pub fn new(seed: u64, look: usize) -> Self {
        let who = Who::named("Claude");
        Caller {
            cast: Cast::new(who, seed),
            seed,
            shown_at: None,
            swap_at: None,
            from: place(who, false, look, None),
            to: place(who, false, look, None),
            moved_at: -1e9,
            dolly: false,
            perched: false,
            look,
            phase: None,
            next_ring: 0.0,
            next_nod: 0.0,
            me: (0.0, -1e9),
        }
    }

    /// The texture's size (logical px): the widest and tallest picture at its big size, with room above, and (the
    /// preview directions) a record's floor below.
    pub fn size(look: usize) -> (f32, f32) {
        let (w, h) = alive::solo_size(LOOK_K[look]);
        (w, h + if look > 0 { floor(look) * LOOK_K[look] * alive::NULL_SOLO } else { 0.0 })
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
        let e = if self.dolly {
            let t = ((now - self.moved_at) / DOLLY_S).clamp(0.0, 1.0);
            if t < 0.5 { 4.0 * t * t * t } else { 1.0 - (-2.0 * t + 2.0).powi(3) / 2.0 }
        } else {
            ease_out((now - self.moved_at) / MOVE_S)
        };
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
            self.to = place(self.cast.who, perched, self.look, Some(phase));
            (self.perched, self.moved_at, self.dolly) = (perched, now, false);
        }

        let was = self.phase.replace(phase);
        // PREVIEW (Camera): a new state can be a new shot; the camera moves there from wherever it is
        if self.look == CAMERA && shot(Some(phase)) != self.to.k && self.shown_at.is_some() {
            self.from = self.place(now);
            self.to = place(self.cast.who, self.perched, self.look, Some(phase));
            (self.moved_at, self.dolly) = (now, true);
        }
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
        self.from = place(who, self.perched, self.look, self.phase);
        self.to = self.from;
    }
}

/// PREVIEW (call screen direction 2, Close-up): the light on the caller tells the call's state, easing between
/// moods: warm while it talks (brighter with its voice), cool while it's your turn, dimmer while it works, lilac on
/// hold, out at the end. Offsets to the record stage's key light colour and ambient (the shaders add them to 1).
pub struct Mood {
    now: [f32; 4],
    at: f32,
    spot: bool, // round 2: offsets to the spotlight's key colour instead (stage.rs STAGES[1])
}

impl Mood {
    pub fn new(spot: bool) -> Self {
        Mood { now: if spot { [-0.05, -0.12, -0.2, -0.45] } else { [0.06, -0.02, -0.14, -0.3] }, at: 0.0, spot }
    }

    pub fn ease(&mut self, phase: Phase, voice: f32, now: f32) {
        let v = voice.clamp(0.0, 1.0);
        // Round 2, in the spotlight: as home while it rings, a touch warmer and brighter with its voice, the light a
        // little cooler for your turn, lower while it works, lower and lilac on hold, out at the end.
        let target = if self.spot {
            match phase {
                Phase::Incoming | Phase::Connecting => [-0.05, -0.12, -0.2, -0.45],
                Phase::Talking => [0.0, -0.1, -0.2, -0.42 + 0.14 * v],
                Phase::Listening | Phase::Hearing => [-0.12, -0.12, -0.08, -0.47],
                Phase::Thinking => [-0.28, -0.3, -0.32, -0.6],
                Phase::Hold => [-0.32, -0.38, -0.12, -0.62],
                Phase::Ended => [-0.75, -0.75, -0.75, -0.85],
            }
        } else {
            match phase {
                Phase::Incoming | Phase::Connecting => [0.06, -0.02, -0.14, -0.3],
                Phase::Talking => [0.12, 0.0, -0.12, -0.28 + 0.22 * v],
                Phase::Listening | Phase::Hearing => [-0.3, -0.1, 0.2, -0.3],
                Phase::Thinking => [-0.18, -0.24, -0.1, -0.52],
                Phase::Hold => [-0.2, -0.3, 0.22, -0.5],
                Phase::Ended => [-0.55, -0.55, -0.5, -0.7],
            }
        };
        let k = 1.0 - (-(now - self.at).clamp(0.0, 0.1) * 6.0).exp(); // about 400 ms
        self.at = now;
        for (v, t) in self.now.iter_mut().zip(target) {
            *v += (t - *v) * k;
        }
    }

    pub fn apply(&self, frame: &mut crate::cast::Uniforms) {
        if let Some(u) = frame.iter_mut().find(|(n, _)| *n == "uKeyCol") {
            u.1 = self.now;
        }
    }
}
