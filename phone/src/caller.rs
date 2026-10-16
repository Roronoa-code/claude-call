use crate::{
    alive,
    cast::{Cast, State, Who},
    view::Phase,
};

const CLOSEST_K: f32 = 1.15;
const FLOOR: f32 = 82.0;
const MOVE_S: f32 = 0.5;
const DOLLY_S: f32 = 1.3;
const RING_EVERY: f32 = 1.5;
const SHOW_AFTER: f32 = 0.6;

fn shot(phase: Option<Phase>) -> f32 {
    match phase {
        None | Some(Phase::Incoming) => 0.62,
        Some(Phase::Connecting | Phase::Talking) => CLOSEST_K,
        Some(Phase::Listening | Phase::Hearing) => 0.9,
        Some(Phase::Thinking) => 0.98,
        Some(Phase::Hold) => 0.8,
        Some(Phase::Ended) => 0.7,
    }
}

#[derive(Clone, Copy, PartialEq)]
pub struct Place {
    pub x: f32,
    pub y: f32,
    pub k: f32,
}

pub struct Caller {
    pub cast: Cast,
    seed: u64,
    shown_at: Option<f32>,
    swap_at: Option<f32>,
    from: Place,
    to: Place,
    moved_at: f32,
    dolly: bool,
    perched: bool,
    phase: Option<Phase>,
    next_ring: f32,
    next_nod: f32,
    me: (f32, f32),
}

fn place(phase: Option<Phase>) -> Place {
    Place { x: 0.0, y: 0.0, k: shot(phase) }
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
            from: place(None),
            to: place(None),
            moved_at: -1e9,
            dolly: false,
            perched: false,
            phase: None,
            next_ring: 0.0,
            next_nod: 0.0,
            me: (0.0, -1e9),
        }
    }

    pub fn size() -> (f32, f32) {
        let (w, h) = alive::solo_size(CLOSEST_K);
        (w, h + FLOOR * CLOSEST_K * alive::NULL_SOLO)
    }

    pub fn poke(&mut self, now: f32) {
        self.cast.hop(now);
    }

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

    pub fn tick(&mut self, now: f32, agent: &str, phase: Phase, perched: bool, voice: f32, me: f32) {
        let who = Who::named(agent);
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
            self.to = place(Some(phase));
            (self.perched, self.moved_at, self.dolly) = (perched, now, false);
        }

        let was = self.phase.replace(phase);
        if shot(Some(phase)) != self.to.k && self.shown_at.is_some() {
            self.from = self.place(now);
            self.to = place(Some(phase));
            (self.moved_at, self.dolly) = (now, true);
        }
        let connected = matches!(phase, Phase::Talking | Phase::Listening | Phase::Hearing | Phase::Thinking | Phase::Hold);
        if connected && matches!(was, Some(Phase::Incoming | Phase::Connecting)) {
            self.cast.hop(now);
        }
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
        self.from = place(self.phase);
        self.to = self.from;
    }
}

pub struct Mood {
    now: [f32; 4],
    at: f32,
}

impl Mood {
    pub fn new() -> Self {
        Mood { now: crate::stage::SPOT.key_col, at: 0.0 }
    }

    pub fn ease(&mut self, phase: Phase, voice: f32, now: f32) {
        let v = voice.clamp(0.0, 1.0);
        let target = match phase {
            Phase::Incoming | Phase::Connecting => crate::stage::SPOT.key_col,
            Phase::Talking => [0.0, -0.1, -0.2, -0.42 + 0.14 * v],
            Phase::Listening | Phase::Hearing => [-0.12, -0.12, -0.08, -0.47],
            Phase::Thinking => [-0.28, -0.3, -0.32, -0.6],
            Phase::Hold => [-0.32, -0.38, -0.12, -0.62],
            Phase::Ended => [-0.75, -0.75, -0.75, -0.85],
        };
        let k = 1.0 - (-(now - self.at).clamp(0.0, 0.1) * 6.0).exp();
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
