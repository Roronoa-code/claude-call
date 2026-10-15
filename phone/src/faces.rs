//! The open chat's character, alive (chat.slint, actor.slint): small by its name, and big in the middle while the
//! chat is still empty. It thinks while the PC works on a message, talks for a moment when an answer arrives, and
//! idles otherwise. Nothing is drawn while no chat is open (the list shows still pictures).
use crate::{
    Chats, HomeScreen, alive,
    cast::{Cast, State, Who},
    live_gl::{Slot, texture_px},
};
use slint::ComponentHandle;

pub const BIG_K: f32 = 0.7; // ...and the empty chat's
const TALK_FOR: f32 = 2.2; // seconds of talk when an answer arrives

/// The textures' sizes (logical px): the widest and tallest picture (a Clawd's) with room above, at each size.
pub fn sizes() -> [(f32, f32); 2] {
    [crate::chats4::small_k(), BIG_K].map(alive::solo_size)
}

pub struct Faces {
    who: Option<Who>,
    casts: [Cast; 2],
    drawn: [f32; 2],
    replies: i32,
    talk_until: f32,
    big_until: f32,
    seed: u64,
    // PREVIEW (chats-4): in the list, the chat being worked on (its character thinks on its row), then a moment of
    // talk when its answer comes in; and when the small one last had something to do (it rests still after).
    list_busy: Option<String>,
    list_talk: Option<String>,
    list_who: Option<Who>,
    quiet_from: f32,
}

impl Faces {
    pub fn new() -> Self {
        let c = || Cast::new(Who::Null, 1);
        Faces { who: None, casts: [c(), c()], drawn: [-1.0; 2], replies: 0, talk_until: 0.0, big_until: 0.0, seed: 0x2545_f491, list_busy: None, list_talk: None, list_who: None, quiet_from: 0.0 }
    }

    pub fn poke(&mut self, now: f32) {
        self.casts.iter_mut().for_each(|c| c.hop(now));
    }

    /// Advances to `now` and marks a new frame in `slots` (small, big) when one is due. True if any was.
    pub fn tick(&mut self, ui: &HomeScreen, slots: &mut [Slot], now: f32, sf: f32) -> bool {
        let c = ui.global::<Chats>();
        let look = crate::chats4::look();
        let list = look > 0 && c.get_open() && c.get_current().is_empty() && !c.get_picker();
        if list {
            return self.list(ui, slots, now, sf);
        }
        if !c.get_open() || c.get_current().is_empty() || c.get_viewing() {
            self.who = None;
            if !c.get_live_row().is_empty() {
                c.set_live_row("".into());
            }
            return false;
        }
        if !c.get_live_row().is_empty() {
            c.set_live_row("".into());
        }
        self.list_who = None;
        let who = Who::named(&c.get_face());
        let replies = c.get_replies();
        if self.who != Some(who) {
            // a chat opened: its character, fresh (each copy on its own clock so they never move in step)
            self.seed = self.seed.rotate_left(7) ^ (now.to_bits() as u64);
            self.casts = [Cast::new(who, self.seed), Cast::new(who, self.seed.rotate_left(31) | 1)];
            self.who = Some(who);
            (self.replies, self.talk_until, self.drawn, self.quiet_from) = (replies, 0.0, [-1.0; 2], now + 0.8);
            self.frame(ui, who, sf);
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
        if state != State::Idle {
            self.quiet_from = now.max(self.talk_until) + 0.8;
        }
        // PREVIEW (chats-4): big too while an agent's question waits (it stands over the question, looking at you)
        if c.get_empty() || (look > 0 && c.get_ask() == "waiting") {
            self.big_until = now + 0.5; // drawn until it has faded out
        }
        let mut any = false;
        for (i, (cast, k)) in self.casts.iter_mut().zip([crate::chats4::small_k(), BIG_K]).enumerate() {
            cast.set_state(state);
            cast.attend(look > 0 && c.get_ask() == "waiting"); // PREVIEW (chats-4): eyes on you while it asks
            cast.tick(now);
            let shown = i == 0 || now < self.big_until;
            // PREVIEW (chats-4): the small one rests still once it has nothing to do (no frames for a quiet screen)
            let resting = look > 0 && i == 0 && self.drawn[0] >= 0.0 && now > self.quiet_from && !cast.moving();
            if shown && !resting && (self.drawn[i] < 0.0 || alive::due(cast, self.drawn[i], now)) {
                let (res, view) = alive::view(who, alive::solo(who, k), sf);
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

    /// PREVIEW (chats-4): the list. The chat the PC is working on has its character alive on its row, thinking; when
    /// its answer comes in it talks for a moment, then rests as a still again. Nothing is drawn otherwise.
    fn list(&mut self, ui: &HomeScreen, slots: &mut [Slot], now: f32, sf: f32) -> bool {
        use slint::Model;
        let c = ui.global::<Chats>();
        self.who = None; // a chat opened from here starts its character afresh
        let rows = c.get_rows();
        let busy = rows.iter().find(|r| r.busy).map(|r| r.id.to_string());
        if let Some(prev) = self.list_busy.take() {
            if busy.as_ref() != Some(&prev) && rows.iter().any(|r| r.id == prev.as_str() && !r.busy) {
                (self.list_talk, self.talk_until) = (Some(prev), now + TALK_FOR);
            }
        }
        self.list_busy = busy.clone();
        if self.list_talk.is_some() && now > self.talk_until + 0.8 && !self.casts.iter().any(|c| c.moving()) {
            self.list_talk = None;
        }
        let (id, state) = match (busy, &self.list_talk) {
            (Some(b), _) => (b, State::Think),
            (None, Some(t)) => (t.clone(), if now < self.talk_until { State::Talk } else { State::Idle }),
            _ => (String::new(), State::Idle),
        };
        if c.get_live_row() != id.as_str() {
            c.set_live_row(id.as_str().into());
        }
        let Some(face) = rows.iter().find(|r| r.id == id.as_str()).map(|r| r.face) else {
            self.list_who = None;
            return false;
        };
        let who = Who::named(&face);
        if self.list_who != Some(who) {
            self.seed = self.seed.rotate_left(7) ^ (now.to_bits() as u64);
            self.casts = [Cast::new(who, self.seed), Cast::new(who, self.seed.rotate_left(31) | 1)];
            (self.list_who, self.drawn) = (Some(who), [-1.0; 2]);
            self.frame(ui, who, sf);
        }
        // Companion's hero stands big at the top of the list (the big texture); the others' portraits are small.
        let (i, k) = if look_hero() { (1, BIG_K) } else { (0, crate::chats4::small_k()) };
        let cast = &mut self.casts[i];
        cast.set_state(state);
        cast.tick(now);
        if self.drawn[i] < 0.0 || alive::due(cast, self.drawn[i], now) {
            let (res, view) = alive::view(who, alive::solo(who, k), sf);
            slots[i] = Slot { who, res, view, frame: cast.uniforms(), fresh: true };
            self.drawn[i] = now;
            return true;
        }
        false
    }

    /// A character, fresh (each copy on its own clock so they never move in step), and its textures' framing.
    fn frame(&mut self, ui: &HomeScreen, who: Who, sf: f32) {
        let c = ui.global::<Chats>();
        {
            let tex = sizes().map(|s| texture_px(s, sf));
            let view = [crate::chats4::small_k(), BIG_K].map(|k| alive::view(who, alive::solo(who, k), sf).1);
            c.set_face_w(view[0].0);
            c.set_face_h(view[0].1);
            c.set_face_th(tex[0].1);
            c.set_big_w(view[1].0);
            c.set_big_h(view[1].1);
            c.set_big_th(tex[1].1);
        }
    }
}

/// PREVIEW (chats-4): whether the list's live character is Companion's big one at the top.
fn look_hero() -> bool {
    crate::chats4::look() == 2
}
