use crate::{
    Chats, HomeScreen, alive,
    cast::{Cast, State, Who},
    live_gl::{Slot, texture_px},
};
use slint::ComponentHandle;

pub const SMALL_K: f32 = 0.26;
pub const BIG_K: f32 = 0.7;
pub const HERO_K: f32 = 0.36;
const TALK_FOR: f32 = 2.2;

pub fn sizes() -> [(f32, f32); 2] {
    [SMALL_K, BIG_K].map(alive::solo_size)
}

pub struct Faces {
    who: Option<Who>,
    casts: [Cast; 2],
    drawn: [f32; 2],
    replies: i32,
    talk_until: f32,
    big_until: f32,
    seed: u64,
    list_busy: Option<String>,
    list_talk: Option<String>,
    list_who: Option<Who>,
    quiet_from: f32,
    pleased_at: Option<f32>,
}

impl Faces {
    pub fn new() -> Self {
        let c = || Cast::new(Who::Null, 1);
        Faces { who: None, casts: [c(), c()], drawn: [-1.0; 2], replies: 0, talk_until: 0.0, big_until: 0.0, seed: 0x2545_f491, list_busy: None, list_talk: None, list_who: None, quiet_from: 0.0, pleased_at: None }
    }

    pub fn poke(&mut self, now: f32) {
        self.casts.iter_mut().for_each(|c| c.hop(now));
    }

    pub fn typed(&mut self, text: &str, now: f32) {
        let c = &mut self.casts[0];
        match text {
            "" => {}
            "\n" => {
                c.hop(now);
                c.glance(0.3, 0.85, now + 1.3);
            }
            t => {
                c.glance(0.45, -0.8, now + 2.2);
                if t.ends_with(' ') {
                    c.nod(now);
                } else {
                    c.sway(0.12);
                }
            }
        }
        self.quiet_from = self.quiet_from.max(now + 2.4);
    }

    pub fn tick(&mut self, ui: &HomeScreen, slots: &mut [Slot], now: f32, sf: f32) -> bool {
        let c = ui.global::<Chats>();
        if c.get_open() && c.get_current().is_empty() && !c.get_picker() {
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
            self.seed = self.seed.rotate_left(7) ^ (now.to_bits() as u64);
            self.casts = [Cast::new(who, self.seed), Cast::new(who, self.seed.rotate_left(31) | 1)];
            self.who = Some(who);
            (self.replies, self.talk_until, self.drawn, self.quiet_from) = (replies, 0.0, [-1.0; 2], now + 0.8);
            self.frame(ui, who, sf);
        }
        if replies > self.replies {
            self.talk_until = now + TALK_FOR;
            self.pleased_at = Some(self.talk_until + 0.2);
        }
        if self.pleased_at.is_some_and(|t| now >= t) && !c.get_waiting() {
            self.pleased_at = None;
            self.casts.iter_mut().for_each(|c| c.pleased(now));
            self.quiet_from = now + 2.4;
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
        if c.get_empty() {
            self.big_until = now + 0.5;
        }
        let mut any = false;
        for (i, (cast, k)) in self.casts.iter_mut().zip([SMALL_K, BIG_K]).enumerate() {
            cast.set_state(state);
            cast.attend(c.get_ask() == "waiting");
            cast.tick(now);
            let shown = i == 0 || now < self.big_until;
            let resting = i == 0 && self.drawn[0] >= 0.0 && now > self.quiet_from && !cast.moving();
            if shown && !resting && (self.drawn[i] < 0.0 || alive::due(cast, self.drawn[i], now)) {
                let (res, view) = alive::view(who, alive::solo(who, k), sf);
                slots[i] = Slot { who, res, view, frame: cast.uniforms(), fresh: true };
                self.drawn[i] = now;
                any = true;
            }
        }
        if now >= self.big_until {
            self.drawn[1] = -1.0;
        }
        any
    }

    fn list(&mut self, ui: &HomeScreen, slots: &mut [Slot], now: f32, sf: f32) -> bool {
        use slint::Model;
        let c = ui.global::<Chats>();
        self.who = None;
        let rows = c.get_rows();
        let busy = rows.iter().find(|r| r.busy).map(|r| r.id.to_string());
        if let Some(prev) = self.list_busy.take() {
            if busy.as_ref() != Some(&prev) && rows.iter().any(|r| r.id == prev.as_str() && !r.busy) {
                (self.list_talk, self.talk_until) = (Some(prev), now + TALK_FOR);
                self.pleased_at = Some(now + TALK_FOR + 0.2);
            }
        }
        self.list_busy = busy.clone();
        if self.list_talk.is_some() && now > self.talk_until + 0.8 && !self.casts.iter().any(|c| c.moving()) {
            self.list_talk = None;
        }
        let (mut id, state) = match (busy, &self.list_talk) {
            (Some(b), _) => (b, State::Think),
            (None, Some(t)) => (t.clone(), if now < self.talk_until { State::Talk } else { State::Idle }),
            _ => (String::new(), State::Idle),
        };
        if id != c.get_hero().id.as_str() {
            id.clear();
        }
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
            let view = alive::view(who, alive::solo(who, HERO_K), sf).1;
            c.set_big_w(view.0);
            c.set_big_h(view.1);
        }
        let (i, k) = (1, HERO_K);
        let cast = &mut self.casts[i];
        if self.pleased_at.is_some_and(|t| now >= t) {
            self.pleased_at = None;
            cast.pleased(now);
        }
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

    fn frame(&mut self, ui: &HomeScreen, who: Who, sf: f32) {
        let c = ui.global::<Chats>();
        {
            let tex = sizes().map(|s| texture_px(s, sf));
            let view = [SMALL_K, BIG_K].map(|k| alive::view(who, alive::solo(who, k), sf).1);
            c.set_face_w(view[0].0);
            c.set_face_h(view[0].1);
            c.set_face_th(tex[0].1);
            c.set_big_w(view[1].0);
            c.set_big_h(view[1].1);
            c.set_big_th(tex[1].1);
        }
    }
}
