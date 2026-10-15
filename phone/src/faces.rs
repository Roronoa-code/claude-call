//! The open chat's character, alive (chat.slint, actor.slint): small by its name, and big in the middle while the
//! chat is still empty. It thinks while the PC works on a message, talks for a moment when an answer arrives, and
//! idles otherwise. Nothing is drawn while no chat is open (the list shows still pictures).
use crate::{
    Chats, HomeScreen, alive,
    cast::{Cast, State, Who},
    live_gl::{Slot, texture_px},
};
use slint::ComponentHandle;

pub const SMALL_K: f32 = 0.21; // chat.slint's header Actor
pub const BIG_K: f32 = 0.7; // ...and the empty chat's
const TALK_FOR: f32 = 2.2; // seconds of talk when an answer arrives

/// The textures' sizes (logical px): the widest and tallest picture (a Clawd's) with room above, at each size.
pub fn sizes() -> [(f32, f32); 2] {
    let f = alive::frame(Who::Clawd(crate::cast::Mat::Vinyl));
    [SMALL_K, BIG_K].map(|k| (f.w * k, (f.h + alive::HEAD) * k))
}

pub struct Faces {
    who: Option<Who>,
    casts: [Cast; 2],
    drawn: [f32; 2],
    replies: i32,
    talk_until: f32,
    big_until: f32,
    seed: u64,
}

impl Faces {
    pub fn new() -> Self {
        let c = || Cast::new(Who::Null, 1);
        Faces { who: None, casts: [c(), c()], drawn: [-1.0; 2], replies: 0, talk_until: 0.0, big_until: 0.0, seed: 0x2545_f491 }
    }

    pub fn poke(&mut self, now: f32) {
        self.casts.iter_mut().for_each(|c| c.hop(now));
    }

    /// Advances to `now` and marks a new frame in `slots` (small, big) when one is due. True if any was.
    pub fn tick(&mut self, ui: &HomeScreen, slots: &mut [Slot], now: f32, sf: f32) -> bool {
        let c = ui.global::<Chats>();
        if !c.get_open() || c.get_current().is_empty() || c.get_viewing() {
            self.who = None;
            return false;
        }
        let who = Who::named(&c.get_face());
        let replies = c.get_replies();
        if self.who != Some(who) {
            // a chat opened: its character, fresh (each copy on its own clock so they never move in step)
            self.seed = self.seed.rotate_left(7) ^ (now.to_bits() as u64);
            self.casts = [Cast::new(who, self.seed), Cast::new(who, self.seed.rotate_left(31) | 1)];
            self.who = Some(who);
            (self.replies, self.talk_until, self.drawn) = (replies, 0.0, [-1.0; 2]);
            let tex = sizes().map(|s| texture_px(s, sf));
            let view = [SMALL_K, BIG_K].map(|k| alive::view(who, k, sf).1);
            c.set_face_w(view[0].0);
            c.set_face_h(view[0].1);
            c.set_face_th(tex[0].1);
            c.set_big_w(view[1].0);
            c.set_big_h(view[1].1);
            c.set_big_th(tex[1].1);
        }
        if replies > self.replies {
            self.talk_until = now + TALK_FOR;
        }
        self.replies = replies;
        let state = if now < self.talk_until {
            State::Talk
        } else if c.get_waiting() {
            State::Think
        } else {
            State::Idle
        };
        if c.get_empty() {
            self.big_until = now + 0.3; // drawn until it has faded out
        }
        let mut any = false;
        for (i, (cast, k)) in self.casts.iter_mut().zip([SMALL_K, BIG_K]).enumerate() {
            cast.set_state(state);
            cast.tick(now);
            let shown = i == 0 || now < self.big_until;
            if shown && (self.drawn[i] < 0.0 || alive::due(cast, self.drawn[i], now)) {
                let (res, view) = alive::view(who, k, sf);
                slots[i] = Slot { who, res, view, frame: cast.uniforms(), fresh: true };
                self.drawn[i] = now;
                any = true;
            }
        }
        if now >= self.big_until {
            self.drawn[1] = -1.0; // drawn at once when it shows again
        }
        any
    }
}
