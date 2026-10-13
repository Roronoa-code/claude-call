//! The user's voice: when they start and stop talking, live captions while they talk, the end of their turn,
//! and talking over Claude.
use super::*;

// ---- Text helpers ----
fn words_of(t: &str) -> Vec<String> {
    t.to_lowercase()
        .split(|c: char| !(c.is_ascii_alphanumeric() || c == '\''))
        .filter(|w| !w.is_empty())
        .map(String::from)
        .collect()
}

/// "mm-hmm", "uh-huh", "mhm", "hmm": not a real interruption.
fn backchannel(t: &str) -> bool {
    let t = t.trim().to_lowercase();
    let t = t.strip_suffix(['.', '!']).unwrap_or(&t);
    let only = |s: &str, set: &str| !s.is_empty() && s.chars().all(|c| set.contains(c));
    let mhm = || {
        // m+ -? h? m+
        let b = t.as_bytes();
        let mut i = 0;
        while i < b.len() && b[i] == b'm' {
            i += 1;
        }
        if i == 0 {
            return false;
        }
        if i < b.len() && b[i] == b'-' {
            i += 1;
        }
        if i < b.len() && b[i] == b'h' {
            i += 1;
        }
        let rest = &t[i..];
        only(rest, "m")
    };
    t == "uh-huh" || t == "uhhuh" || t == "mhm" || (t.starts_with('h') && t.len() >= 3 && only(&t[1..], "m")) || mhm()
}

/// Real words from the user: not Claude's own voice leaking back, not a "mm-hmm".
fn real_words(text: &str, line: Option<&str>) -> bool {
    if text.trim().is_empty() || backchannel(text) {
        return false;
    }
    let own: Vec<String> = line.map(words_of).unwrap_or_default();
    words_of(text).iter().any(|w| !own.contains(w))
}

const CONT: [&str; 44] = [
    "and", "but", "so", "or", "because", "cause", "the", "a", "an", "to", "of", "er", "like", "if", "then", "with", "for", "that", "which", "my",
    "your", "our", "is", "are", "was", "were", "just", "maybe", "also", "well", "what", "when", "where", "how", "why", "i", "i'm", "we", "you",
    "it's", "in", "on", "at", "about",
];

/// Does the live transcript read as a finished turn? ("Yes." / "Push it now." but not "I think that we should and")
fn looks_complete(text: &str) -> bool {
    let s = text.trim();
    let core = s.trim_end_matches(['"', '\'', ')', ']']);
    if !core.ends_with(['.', '!', '?']) || s.ends_with("...") || s.ends_with('…') {
        return false;
    }
    let bare = s.trim_end_matches(['.', '!', '?', '"', '\'', ')', ']']);
    let last = bare.rsplit(|c: char| !(c.is_alphanumeric() || c == '\'')).next().unwrap_or("").to_lowercase();
    let filler = |w: &str, a: char, b: char| w.len() >= 2 && w.starts_with(a) && w[1..].chars().all(|c| c == b); // um+, uh+
    !(CONT.contains(&last.as_str()) || filler(&last, 'u', 'm') || filler(&last, 'u', 'h'))
}

impl Engine {
    pub(super) fn new_utt(s: &mut Inner, now: f64) -> u64 {
        let id = s.next_utt;
        s.next_utt += 1;
        let mut u = Utt { id: format!("{:x}{:04x}", now as u64, id), born: now, last_hot: now, ..Default::default() };
        for f in &s.preroll {
            u.unsent.extend_from_slice(f);
            u.raw += f.len();
        }
        s.utts.insert(id, u);
        id
    }

    /// The audio not sent yet, as 16-bit PCM bytes, and where it starts.
    pub(super) fn take(u: &mut Utt) -> (usize, Vec<u8>) {
        let body: Vec<u8> = u.unsent.drain(..).flat_map(|x| ((x.clamp(-1.0, 1.0) * 32767.0) as i16).to_le_bytes()).collect();
        let off = u.sent16;
        u.sent16 += body.len() / 2;
        (off, body)
    }

    /// Sends what's new and gets the transcript so far (one request at a time per utterance).
    pub(super) fn ask(self: &Arc<Self>, s: &mut Inner, id: u64, now: f64) {
        let Some(u) = s.utts.get_mut(&id) else { return };
        if u.closed {
            return;
        }
        if u.inflight {
            u.again = true;
            return;
        }
        u.last_ask = now;
        u.inflight = true;
        let (off, body) = Self::take(u);
        let covers = u.raw;
        let url = format!("{}&u={}&off={off}&partial=1", self.api("listen"), u.id);
        let me = self.clone();
        RT.spawn(async move {
            let t0 = me.now();
            let r: Option<Value> = async { HTTP.post(url).body(body).send().await.ok()?.json().await.ok() }.await;
            let mut s = me.lock();
            let now = me.now();
            let text = r.as_ref().and_then(|r| r["text"].as_str()).map(String::from);
            let Some(u) = s.utts.get_mut(&id) else { return };
            u.inflight = false;
            let closed = u.closed;
            let again = std::mem::take(&mut u.again);
            let uid = u.id.clone();
            if let Some(text) = text.filter(|_| !closed) {
                u.partial = Some((text.clone(), covers));
                if s.barge == Some(id) {
                    let msg = format!("barge words \"{text}\" ({}ms)", (now - t0) as i64);
                    me.dbg(&mut s, &msg);
                    let line = s.line.as_ref().map(|l| l.text.clone());
                    if real_words(&text, line.as_deref()) {
                        me.interrupt(&mut s, "words", now);
                    }
                } else if s.utt == Some(id) && !text.is_empty() {
                    s.view.partial(&uid, &text);
                }
            }
            if closed {
                s.utts.remove(&id);
            } else if again {
                me.ask(&mut s, id, now);
            }
        });
    }

    pub fn on_frame(self: &Arc<Self>, f: Vec<f32>) {
        let now = self.now();
        let mut s = self.lock();
        let s = &mut *s;
        let rms = (f.iter().map(|x| x * x).sum::<f32>() / f.len() as f32).sqrt();
        s.stats.0 += 1;
        s.stats.1 += rms;
        s.stats.2 = s.stats.2.max(rms);
        s.preroll.push_back(f.clone());
        while s.preroll.len() > PREROLL_FRAMES {
            s.preroll.pop_front();
        }
        if s.muted || s.ended {
            return;
        }
        match s.mode {
            Mode::Claude => self.barge_frame(s, f, rms, now),
            Mode::Idle => s.noise_floor = s.noise_floor * 0.95 + rms * 0.05,
            Mode::Listen => {
                // Speech has to last 3 frames (120 ms): a knock or a breath doesn't start a turn.
                if rms > (0.012f32).max(s.noise_floor * 3.0) {
                    s.speech_run += 1;
                    if s.speech_run >= 3 {
                        s.speech_run = 0;
                        let u = Self::new_utt(s, now);
                        self.start_talking(s, u, false, now);
                    }
                } else {
                    s.speech_run = 0;
                    s.noise_floor = s.noise_floor * 0.95 + rms * 0.05;
                }
            }
            Mode::Talk => {
                let speech = rms > (0.012f32).max(s.noise_floor * 3.0);
                let Some(id) = s.utt else { return };
                s.view.mic_level = (rms * 12.0).min(1.0);
                let (ask, early) = {
                    let u = s.utts.get_mut(&id).unwrap();
                    u.unsent.extend_from_slice(&f);
                    u.raw += f.len();
                    if speech {
                        s.quiet_since = now;
                        u.speech_end_raw = u.raw;
                        u.asked = false;
                    }
                    let quiet = now - s.quiet_since;
                    let ask = if quiet < PAUSE_MS { now - u.last_ask > PARTIAL_MS } else { !u.asked };
                    if ask {
                        u.asked = quiet >= PAUSE_MS;
                    }
                    let early = quiet > EARLY_MS && u.partial.as_ref().is_some_and(|(t, covers)| *covers >= u.speech_end_raw && looks_complete(t));
                    (ask, early)
                };
                if ask {
                    self.ask(s, id, now);
                }
                let quiet = now - s.quiet_since;
                if early || quiet > SILENCE_MS || now - s.started_at > MAX_MS {
                    self.finish(s, if early { "early" } else { "silence" });
                }
            }
        }
    }

    pub(super) fn barge_frame(self: &Arc<Self>, s: &mut Inner, f: Vec<f32>, rms: f32, now: f64) {
        let low = BARGE_MIN.max(s.echo * BARGE_ECHO).max(s.noise_floor * 2.0);
        let high = LOUD_MIN.max(s.echo * LOUD_ECHO);
        (s.stats.3, s.stats.4) = (low, high);
        // Learns how loud Claude's own voice comes back: falls fast, rises slowly (so the user's voice doesn't count).
        if s.barge.is_none() {
            s.echo = if rms < s.echo { s.echo * 0.9 + rms * 0.1 } else { s.echo * 0.99 + rms * 0.01 };
        }
        // Clearly loud speech stops Claude at once (not in a line's first moments, while the echo is still being learnt).
        s.loud_run = if rms > high { s.loud_run + 1 } else { 0 };
        if s.loud_run >= 4 && s.line.as_ref().is_some_and(|l| l.started && now - l.start_at > 800.0) {
            return self.interrupt(s, "loud", now);
        }
        s.hot.push_back(rms > low);
        if s.hot.len() > 7 {
            s.hot.pop_front();
        }
        let Some(id) = s.barge else {
            if s.hot.iter().filter(|h| **h).count() >= 5 {
                s.barge = Some(Self::new_utt(s, now));
                let msg = format!("barge? rms={rms:.4} thr={low:.4}");
                self.dbg(s, &msg);
            }
            return;
        };
        let u = s.utts.get_mut(&id).unwrap();
        u.unsent.extend_from_slice(&f);
        u.raw += f.len();
        if rms > low {
            u.last_hot = now;
        }
        if now - u.last_hot > 700.0 {
            u.closed = true;
            if !u.inflight {
                s.utts.remove(&id);
            }
            s.barge = None;
            self.dbg(s, "barge dropped");
            return;
        }
        if now - u.born > 200.0 && now - u.last_ask > 500.0 {
            self.ask(s, id, now);
        }
    }

    pub(super) fn interrupt(self: &Arc<Self>, s: &mut Inner, reason: &str, now: f64) {
        s.player.stop();
        if let Some(l) = &mut s.line {
            l.cut = true;
            self.post(format!("{}&seq={}", self.api("hush"), l.id), vec![]); // stop making the rest of the line
        }
        self.line_stop.notify_waiters();
        let id = s.barge.take().unwrap_or_else(|| Self::new_utt(s, now));
        s.hot.clear();
        s.loud_run = 0;
        self.start_talking(s, id, true, now);
        let msg = format!("interrupt ({reason})");
        self.dbg(s, &msg);
    }

    pub(super) fn start_talking(self: &Arc<Self>, s: &mut Inner, id: u64, interrupted: bool, now: f64) {
        s.utt = Some(id);
        s.mode = Mode::Talk;
        s.cut_in = interrupted;
        s.started_at = now;
        s.quiet_since = now;
        let u = s.utts.get_mut(&id).unwrap();
        u.speech_end_raw = u.raw;
        let (uid, text) = (u.id.clone(), u.partial.as_ref().map(|p| p.0.clone()).unwrap_or_default());
        s.last_turn = Some(uid.clone());
        s.view.partial(&uid, &text);
        s.view.status("Hearing you…");
        s.view.can_finish = true;
    }

    pub(super) fn listen(&self, s: &mut Inner) {
        if s.ended {
            return;
        }
        if s.muted || s.mic.is_none() {
            s.mode = Mode::Idle;
            s.view.status(if s.mic.is_some() { "Mic muted, type a reply" } else { "Type your reply" });
            return;
        }
        s.mode = Mode::Listen;
        s.view.status("Listening… take your time");
    }

    pub(super) fn drop_utterance(s: &mut Inner) {
        if let Some(id) = s.utt.take() {
            if let Some(u) = s.utts.get_mut(&id) {
                u.closed = true;
            }
        }
        s.mode = Mode::Idle;
        s.view.can_finish = false;
        s.view.mic_level = 0.0;
    }

    /// The user's turn is over: send the rest of what they said and get the final transcript, in order.
    pub(super) fn finish(self: &Arc<Self>, s: &mut Inner, why: &str) {
        if s.mode != Mode::Talk {
            return;
        }
        let now = self.now();
        let Some(id) = s.utt else { return };
        let quiet = (now - s.quiet_since).round() as i64;
        let was_cut = s.cut_in;
        Self::drop_utterance(s);
        s.cut_in = false;
        s.view.status("Got it…");
        let Some(mut u) = s.utts.remove(&id) else { return };
        let msg = format!("turn end ({why}) after {quiet}ms quiet, caption \"{}\"", u.partial.as_ref().map_or("", |p| p.0.as_str()));
        self.dbg(s, &msg);
        let (off, body) = Self::take(&mut u);
        s.speech_pending += 1;
        let url = format!(
            "{}&u={}&off={off}&end={}&quiet={quiet}{}",
            self.api("speech"),
            u.id,
            u.speech_end_raw,
            if was_cut { "&interrupted=1" } else { "" }
        );
        let me = self.clone();
        let uid = u.id;
        RT.spawn(async move {
            let r: Option<Value> = {
                let _in_order = me.speech_order.lock().await; // successive turns reach Claude in order
                if me.lock().ended {
                    None
                } else {
                    async { HTTP.post(url).body(body).timeout(Duration::from_millis(MAX_MS as u64)).send().await.ok()?.json().await.ok() }.await
                }
            };
            let mut s = me.lock();
            let s = &mut *s;
            s.speech_pending -= 1;
            if s.ended {
                return;
            }
            let mine = s.last_turn.as_deref() == Some(uid.as_str());
            match r {
                Some(r) => {
                    let text = r["text"].as_str().unwrap_or("").to_string();
                    s.view.heard(&uid, &text, false);
                    if mine && !s.muted && s.mode == Mode::Idle && s.line.is_none() {
                        if !text.is_empty() {
                            s.view.status("Claude is thinking…");
                        } else {
                            me.listen(s);
                        }
                    }
                }
                None => {
                    if mine && s.mode != Mode::Talk && s.mode != Mode::Claude {
                        s.view.status("Could not send audio, type instead");
                    }
                }
            }
            // Keep hearing followups during agent thinking without replacing its thinking caption.
            if !s.muted && s.mic.is_some() && s.line.is_none() && s.mode == Mode::Idle && mine {
                s.mode = Mode::Listen;
            }
        });
    }

}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    pub(super) fn turn_end_reading() {
        assert!(looks_complete("Yes."));
        assert!(looks_complete("Push it now!"));
        assert!(!looks_complete("I think that we should and."));
        assert!(!looks_complete("Well..."));
        assert!(!looks_complete("so the"));
        assert!(backchannel("Mm-hmm."));
        assert!(backchannel("uh-huh"));
        assert!(backchannel("Hmm"));
        assert!(!backchannel("Hang on"));
        assert!(real_words("Hang on, stop", Some("So the plan is to move")));
        assert!(!real_words("the plan", Some("So the plan is to move")));
    }
}
