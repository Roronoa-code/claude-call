const PHRASE_CHARS: usize = 42;
const CLAUSE_ENDS: [char; 5] = [',', ';', ':', '—', '–'];

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Phase {
    Incoming,
    Connecting,
    Talking,
    Listening,
    Hearing,
    Thinking,
    Hold,
    Ended,
}

#[derive(Clone, Copy, Default, Debug, PartialEq)]
pub struct Spoken {
    pub phrase: usize,
    pub words: usize,
}

#[derive(Clone, Debug)]
pub struct Msg {
    pub id: String,
    pub me: bool,
    pub text: String,
    pub parts: Vec<String>,
    pub spoken: Spoken,
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
    pub quiet: bool,
    pub can_finish: bool,
    pub call_start: f64,
    pub usage: String,
    pub ended_at: f64,
    pub hold_start: f64,
    pub hold_label: String,
    pub notice: String,
    pub partial: String,
    partial_id: Option<String>,
    hold_after: bool,
    pub voice: Voice,
    pub mic_level: f32,
    pub agent: String,
    pub unsent: String,
    pub asking_back: bool,
}

pub fn words(t: &str) -> Vec<&str> {
    t.split_whitespace().collect()
}

fn sentences(text: &str) -> Vec<String> {
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
    out.iter().map(|s| words(s).join(" ")).filter(|s| !s.is_empty()).collect()
}

pub fn phrases(text: &str) -> Vec<String> {
    let out: Vec<String> = sentences(text).iter().flat_map(|s| clauses(s)).collect();
    if out.is_empty() { vec![text.trim().to_string()] } else { out }
}

fn span(ws: &[&str]) -> usize {
    ws.iter().map(|w| w.chars().count()).sum::<usize>() + ws.len().saturating_sub(1)
}

fn clauses(sentence: &str) -> Vec<String> {
    let ws = words(sentence);
    let ends = (1..=ws.len()).filter(|&e| e == ws.len() || ws[e - 1].ends_with(CLAUSE_ENDS) || ws[e - 1] == "-");
    let mut out = vec![];
    let (mut start, mut prev) = (0, 0);
    for e in ends {
        if prev > start && span(&ws[start..e]) > PHRASE_CHARS {
            out.extend(balanced(&ws[start..prev]));
            start = prev;
        }
        prev = e;
    }
    out.extend(balanced(&ws[start..]));
    out
}

fn balanced(ws: &[&str]) -> Vec<String> {
    let total = span(ws);
    let pieces = total.div_ceil(PHRASE_CHARS).max(1);
    let mut out = vec![];
    let mut cur: Vec<&str> = vec![];
    for &w in ws {
        let n = w.chars().count();
        let fits = span(&cur) + 1 + n <= PHRASE_CHARS;
        let past_goal = (span(&cur) + 1 + n / 2) * pieces > total;
        if !cur.is_empty() && (!fits || (out.len() + 1 < pieces && past_goal)) {
            out.push(cur.join(" "));
            cur.clear();
        }
        cur.push(w);
    }
    if !cur.is_empty() {
        out.push(cur.join(" "));
    }
    out
}

fn spoken_at(parts: &[String], shown: &str) -> Spoken {
    let mut n = words(shown).len();
    for (i, p) in parts.iter().enumerate() {
        let w = words(p).len();
        if n < w {
            return Spoken { phrase: i, words: n };
        }
        n -= w;
    }
    Spoken { phrase: parts.len(), words: 0 }
}

impl View {
    pub fn new() -> View {
        View {
            phase: Phase::Incoming,
            transcribing: false,
            msgs: vec![],
            muted: false,
            speaker: false,
            quiet: false,
            can_finish: false,
            call_start: 0.0,
            usage: String::new(),
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
            unsent: String::new(),
            asking_back: false,
        }
    }

    fn idle_phase(&self) -> Phase {
        if self.hold_after { Phase::Hold } else { Phase::Listening }
    }

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
            (self.phase, self.transcribing, self.notice) = (Phase::Hearing, false, String::new());
            for m in self.msgs.iter_mut().filter(|m| m.live) {
                m.cut = true;
            }
        } else if s.starts_with("Got it") {
            self.voice = Voice::None;
            let phase = if self.hold_after { Phase::Hold } else { Phase::Thinking };
            (self.phase, self.transcribing, self.notice) = (phase, true, String::new());
        } else if s.starts_with("Claude is thinking") {
            self.voice = Voice::None;
            let phase = if self.hold_after { Phase::Hold } else { Phase::Thinking };
            (self.phase, self.transcribing, self.notice) = (phase, false, String::new());
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

    pub fn claude(&mut self, id: u64, text: &str) {
        self.hold_after = false;
        self.voice = Voice::Claude;
        self.msgs.push(Msg { id: id.to_string(), me: false, text: String::new(), parts: phrases(text), spoken: Spoken::default(), cut: false, live: true });
        (self.phase, self.transcribing, self.notice) = (Phase::Talking, false, String::new());
        self.partial.clear();
    }

    pub fn playing(&mut self, id: u64, text: &str, shown: &str) {
        let id = id.to_string();
        let Some(m) = self.msgs.iter_mut().find(|m| m.id == id) else { return };
        if m.parts.join(" ") != words(text).join(" ") {
            m.parts = phrases(text);
        }
        m.spoken = spoken_at(&m.parts, shown);
    }

    pub fn spoken(&mut self, id: u64, cut: bool, hold: (bool, String), now: f64) {
        let id = id.to_string();
        if let Some(m) = self.msgs.iter_mut().find(|m| m.id == id) {
            m.live = false;
            m.cut = cut;
            if !cut {
                m.spoken = Spoken { phrase: m.parts.len(), words: 0 };
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

    pub fn heard(&mut self, id: &str, text: &str, typed: bool) {
        let clear = self.partial_id.as_deref().is_none_or(|p| p == id) || typed;
        if clear {
            self.partial_id = None;
            self.partial.clear();
        }
        if !text.is_empty() {
            self.msgs.push(Msg { id: format!("u{id}"), me: true, text: text.to_string(), parts: vec![], spoken: Spoken::default(), cut: false, live: false });
        }
    }

    pub fn ended(&mut self, now: f64) {
        self.voice = Voice::None;
        (self.phase, self.ended_at, self.can_finish) = (Phase::Ended, now, false);
        self.notice.clear();
    }
}

pub fn clock(ms: f64) -> String {
    let s = (ms / 1000.0).floor().max(0.0) as u64;
    format!("{}:{:02}", s / 60, s % 60)
}

pub fn lit(m: &Msg, only: Option<usize>) -> (String, String) {
    let Spoken { phrase: p, words: w } = m.spoken;
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
#[path = "view_tests.rs"]
mod tests;
