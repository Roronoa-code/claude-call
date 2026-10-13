//! The call as the states the design shows (incoming, connecting, talking, listening, hearing, thinking, hold,
//! ended), built from the engine's events. The engine only ever changes the screen through these methods.
use crate::mascot::Phase;

#[derive(Clone, Debug)]
pub struct Msg {
    pub id: String,
    pub me: bool,
    pub text: String,       // yours
    pub parts: Vec<String>, // Claude's, sentence by sentence
    pub spoken: (usize, usize), // how far it has been said: (sentence, words into it)
    pub cut: bool,
    pub live: bool,
}

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Voice {
    None,
    Claude,
    Me,
}

pub struct View {
    pub phase: Phase,
    pub transcribing: bool,
    pub msgs: Vec<Msg>,
    pub muted: bool,
    pub speaker: bool,
    pub can_finish: bool,
    pub call_start: f64,
    pub ended_at: f64,
    pub hold_start: f64,
    pub hold_label: String,
    pub notice: String,
    pub partial: String,
    partial_id: Option<String>,
    hold_after: bool,
    pub voice: Voice,
    pub mic_level: f32,
    /// Who is calling: "Claude" or "Codex" (from the call server).
    pub agent: String,
}

pub fn words(t: &str) -> Vec<&str> {
    t.split_whitespace().collect()
}

/// Claude's line, split into sentences: the big caption shows one sentence at a time.
pub fn sentences(text: &str) -> Vec<String> {
    let mut out = vec![];
    let mut cur = String::new();
    let chars: Vec<char> = text.chars().collect();
    let mut i = 0;
    while i < chars.len() {
        cur.push(chars[i]);
        if ".!?".contains(chars[i]) {
            while i + 1 < chars.len() && ".!?\"')]".contains(chars[i + 1]) {
                i += 1;
                cur.push(chars[i]);
            }
            out.push(std::mem::take(&mut cur));
        }
        i += 1;
    }
    out.push(cur);
    let out: Vec<String> = out.iter().map(|s| words(s).join(" ")).filter(|s| !s.is_empty()).collect();
    if out.is_empty() { vec![text.trim().to_string()] } else { out }
}

/// How far into the line `shown` (the text revealed in step with the real audio) has got: (sentence, words).
fn spoken_at(parts: &[String], shown: &str) -> (usize, usize) {
    let mut n = words(shown).len();
    for (i, p) in parts.iter().enumerate() {
        let w = words(p).len();
        if n < w {
            return (i, n);
        }
        n -= w;
    }
    (parts.len(), 0)
}

impl View {
    pub fn new() -> View {
        View {
            phase: Phase::Incoming,
            transcribing: false,
            msgs: vec![],
            muted: false,
            speaker: false,
            can_finish: false,
            call_start: 0.0,
            ended_at: 0.0,
            hold_start: 0.0,
            hold_label: String::new(),
            notice: String::new(),
            partial: String::new(),
            partial_id: None,
            hold_after: false,
            voice: Voice::None,
            mic_level: 0.0,
            agent: "Claude".into(),
        }
    }

    fn idle_phase(&self) -> Phase {
        if self.hold_after { Phase::Hold } else { Phase::Listening }
    }

    /// The engine's status line, as the design's states. "Claude is talking…" is left to `claude`, so the screen
    /// only changes once Claude's voice is actually heard.
    pub fn status(&mut self, s: &str) {
        if self.phase == Phase::Ended {
            return;
        }
        if s.starts_with("Listening") || s.starts_with("Mic muted") {
            (self.phase, self.transcribing, self.notice) = (self.idle_phase(), false, String::new());
        } else if s.starts_with("Type your reply") {
            (self.phase, self.notice) = (self.idle_phase(), s.to_string());
        } else if s.starts_with("Hearing you") {
            self.voice = Voice::Me;
            self.hold_after = false;
            (self.phase, self.transcribing, self.notice) = (Phase::Hearing, false, String::new());
            for m in self.msgs.iter_mut().filter(|m| m.live) {
                m.cut = true; // hide unheard words before the first thread render
            }
        } else if s.starts_with("Got it") {
            self.voice = Voice::None;
            (self.phase, self.transcribing, self.notice) = (Phase::Thinking, true, String::new());
        } else if s.starts_with("Claude is thinking") {
            self.voice = Voice::None;
            self.hold_after = false;
            (self.phase, self.transcribing, self.notice) = (Phase::Thinking, false, String::new());
        } else if s.starts_with("Connect") {
            if matches!(self.phase, Phase::Incoming | Phase::Connecting) {
                self.phase = Phase::Connecting;
            }
        } else if s.starts_with("Reconnecting") || s.starts_with("Could not") || s.starts_with("Mic blocked") {
            self.notice = s.to_string();
        }
    }

    pub fn answered(&mut self, now: f64) {
        self.phase = Phase::Connecting;
        self.call_start = now;
    }

    /// A line of Claude's starts sounding.
    pub fn claude(&mut self, id: u64, text: &str) {
        self.hold_after = false;
        self.voice = Voice::Claude;
        self.msgs.push(Msg { id: id.to_string(), me: false, text: String::new(), parts: sentences(text), spoken: (0, 0), cut: false, live: true });
        (self.phase, self.transcribing, self.notice) = (Phase::Talking, false, String::new());
        self.partial.clear();
    }

    pub fn playing(&mut self, id: u64, text: &str, shown: &str) {
        let id = id.to_string();
        let Some(m) = self.msgs.iter_mut().find(|m| m.id == id) else { return };
        if m.parts.join(" ") != words(text).join(" ") {
            m.parts = sentences(text);
        }
        m.spoken = spoken_at(&m.parts, shown);
    }

    /// It finished (`cut`: the user talked over it). A hold line, once said, puts the call on hold.
    pub fn spoken(&mut self, id: u64, cut: bool, hold: (bool, String), now: f64) {
        let id = id.to_string();
        if let Some(m) = self.msgs.iter_mut().find(|m| m.id == id) {
            m.live = false;
            m.cut = cut;
            if !cut {
                m.spoken = (m.parts.len(), 0);
            }
        }
        if self.voice == Voice::Claude {
            self.voice = Voice::None;
        }
        if hold.0 && !cut {
            self.hold_after = true;
            self.hold_start = now;
            self.hold_label = hold.1.trim_end_matches(['.', '!', '?', '…', ' ']).to_string();
        }
    }

    pub fn partial(&mut self, id: &str, text: &str) {
        self.partial_id = Some(id.to_string());
        self.partial = text.to_string();
    }

    /// The user's final words ('' = only noise: drop that bubble).
    pub fn heard(&mut self, id: &str, text: &str, typed: bool) {
        let clear = self.partial_id.as_deref().is_none_or(|p| p == id) || typed;
        if clear {
            self.partial_id = None;
            self.partial.clear();
        }
        if !text.is_empty() {
            self.msgs.push(Msg { id: format!("u{id}"), me: true, text: text.to_string(), parts: vec![], spoken: (0, 0), cut: false, live: false });
        }
    }

    pub fn ended(&mut self, now: f64) {
        self.voice = Voice::None;
        (self.phase, self.ended_at, self.can_finish) = (Phase::Ended, now, false);
        self.notice.clear();
    }

    pub fn status_text(&self) -> String {
        let a = &self.agent;
        match self.phase {
            Phase::Incoming => "Incoming call".into(),
            Phase::Connecting => "Connecting…".into(),
            Phase::Talking => format!("{a} is talking…"),
            Phase::Listening => {
                (if self.muted { "Mic muted, type a reply" } else { "Listening… take your time" }).into()
            }
            Phase::Hearing => "Hearing you…".into(),
            Phase::Thinking => {
                if self.transcribing { "Got it, transcribing…".into() } else { format!("{a} is thinking…") }
            }
            Phase::Hold => "On hold".into(),
            Phase::Ended => "Call ended".into(),
        }
    }
}

pub fn clock(ms: f64) -> String {
    let s = (ms / 1000.0).floor().max(0.0) as u64;
    format!("{}:{:02}", s / 60, s % 60)
}

/// The lit (already said) words of one of Claude's lines: all its parts, or just part `only`.
pub fn lit(m: &Msg, only: Option<usize>) -> (String, String) {
    let (p, w) = m.spoken;
    let mut text = vec![];
    let mut lit = vec![];
    for (i, part) in m.parts.iter().enumerate() {
        if only.is_some_and(|o| o != i) {
            continue;
        }
        for (j, word) in words(part).into_iter().enumerate() {
            text.push(word);
            if i < p || (i == p && j < w) {
                lit.push(word);
            }
        }
    }
    (text.join(" "), lit.join(" "))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn sentences_and_progress() {
        let s = sentences("Hi, it is Claude. This is a test call! Say anything back?");
        assert_eq!(s, vec!["Hi, it is Claude.", "This is a test call!", "Say anything back?"]);
        assert_eq!(spoken_at(&s, "Hi, it is Claude. This"), (1, 1));
        assert_eq!(spoken_at(&s, ""), (0, 0));
        let m = Msg { id: "1".into(), me: false, text: String::new(), parts: s, spoken: (1, 2), cut: false, live: true };
        assert_eq!(lit(&m, Some(1)), ("This is a test call!".into(), "This is".into()));
        assert_eq!(lit(&m, None).1, "Hi, it is Claude. This is");
    }
}
