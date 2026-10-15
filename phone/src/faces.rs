//! The open chat's character, alive (chat.slint, actor.slint): small by its name, and big in the middle while the
//! chat is still empty. It thinks while the PC works on a message, talks for a moment when an answer arrives, and
//! idles otherwise. Nothing is drawn while no chat is open (the list shows still pictures).
use crate::{
    Chats, HomeScreen, alive,
    cast::{Cast, State, Who},
    live_gl::{Slot, texture_px},
};
use slint::ComponentHandle;

const TALK_FOR: f32 = 2.2; // seconds of talk when an answer arrives

/// PREVIEW (branch preview/chats): the small and big sizes on each chat direction (debug.callme.chats; 0 as shipped:
/// chat.slint's header Actor and the empty chat's). The .slint files draw them at these sizes. In the directions the
/// agent stands in the home screen's spotlight (stage.rs 1), lit by it, with its shadow and reflection on the floor.
const LOOK_K: [[f32; 2]; 5] = [[0.21, 0.7], [0.36, 0.62], [0.36, 0.62], [0.21, 0.62], [0.42, 0.62]];

pub fn look() -> usize {
    crate::stage::prop(c"debug.callme.chats", LOOK_K.len() - 1)
}

fn ks() -> [f32; 2] {
    LOOK_K[look()]
}

/// The most floor drawn under a picture, at k = 1 (the spotlight's 110 px, less what the picture already has, for a
/// solo Null drawn larger).
fn floor(k: f32) -> f32 {
    if look() > 0 { crate::stage::STAGES[1].floor * k * alive::NULL_SOLO } else { 0.0 }
}

/// The textures' sizes (logical px): the widest and tallest picture (a Clawd's) with room above, at each size.
pub fn sizes() -> [(f32, f32); 2] {
    ks().map(|k| {
        let (w, h) = alive::solo_size(k);
        (w, h + floor(k))
    })
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
            let view = ks().map(|k| alive::view(who, alive::solo(who, k), sf).1);
            let below = ks().map(|k| if look() > 0 { (crate::stage::STAGES[1].below(who) * alive::solo(who, k) * sf).round() as i32 } else { 0 });
            c.set_face_w(view[0].0);
            c.set_face_h(view[0].1 + below[0]);
            c.set_face_th(tex[0].1);
            c.set_big_w(view[1].0);
            c.set_big_h(view[1].1 + below[1]);
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
        for (i, (cast, k)) in self.casts.iter_mut().zip(ks()).enumerate() {
            cast.set_state(state);
            cast.tick(now);
            let shown = i == 0 || now < self.big_until;
            if shown && (self.drawn[i] < 0.0 || alive::due(cast, self.drawn[i], now)) {
                let (res, mut view) = alive::view(who, alive::solo(who, k), sf);
                let mut frame = cast.uniforms();
                if look() > 0 {
                    // PREVIEW: in the spotlight, its floor drawn under the picture (screen.rs does the same on a call)
                    let st = &crate::stage::STAGES[1];
                    let below = (st.below(who) * alive::solo(who, k) * sf).round() as i32;
                    view.1 += below;
                    let mut u = st.uniforms(who, res.1, view.1, sf, 1.0);
                    for (name, v) in u.iter_mut() {
                        if *name == "uShadow" {
                            v[2] = st.shadow[2] * k * sf; // its fade, at this size
                        }
                    }
                    frame.extend(u);
                }
                slots[i] = Slot { who, res, view, frame, fresh: true };
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
