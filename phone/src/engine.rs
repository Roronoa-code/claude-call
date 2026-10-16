use crate::{
    android,
    audio::{self, Mic, Player},
    net::{HTTP, RT},
    view::View,
};
use serde_json::Value;
use std::{
    collections::{HashMap, VecDeque},
    sync::{Arc, Mutex, MutexGuard},
    time::{Duration, Instant},
};
use tokio::sync::Notify;

mod hearing;
mod speaking;


const PREROLL_FRAMES: usize = 12;
const SILENCE_MS: f64 = 1800.0;
const EARLY_MS: f64 = 700.0;
const PARTIAL_MS: f64 = 900.0;
const PAUSE_MS: f64 = 250.0;
const MAX_MS: f64 = 90_000.0;
const LOST_MS: f64 = 30_000.0;
const BARGE_MIN: f32 = 0.006;
const BARGE_ECHO: f32 = 3.0;
const LOUD_MIN: f32 = 0.03;
const LOUD_ECHO: f32 = 8.0;
const MS_PER_CHAR: f64 = 60.0;

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Mode {
    Idle,
    Listen,
    Talk,
    Claude,
}

#[derive(Default)]
struct Utt {
    id: String,
    unsent: Vec<f32>,
    raw: usize,
    sent16: usize,
    speech_end_raw: usize,
    partial: Option<(String, usize)>,
    inflight: bool,
    again: bool,
    last_ask: f64,
    asked: bool,
    closed: bool,
    born: f64,
    last_hot: f64,
}

pub struct Line {
    pub id: u64,
    pub text: String,
    pub started: bool,
    pub start_at: f64,
    pub start_frame: Option<u64>,
    pub rate: u32,
    pub dur: f64,
    pub cut: bool,
}

pub struct Inner {
    pub view: View,
    pub mode: Mode,
    pub ended: bool,
    pub muted: bool,
    debug: bool,
    mic: Option<Mic>,
    noise_floor: f32,
    echo: f32,
    preroll: VecDeque<Vec<f32>>,
    utts: HashMap<u64, Utt>,
    next_utt: u64,
    utt: Option<u64>,
    barge: Option<u64>,
    cut_in: bool,
    quiet_since: f64,
    started_at: f64,
    last_turn: Option<String>,
    speech_pending: u32,
    hot: VecDeque<bool>,
    speech_run: u32,
    loud_run: u32,
    pub line: Option<Line>,
    pub player: Player,
    last_hold: (bool, String),
    stats: (u32, f32, f32, f32, f32),
    dq: Vec<String>,
}

pub struct Engine {
    s: Mutex<Inner>,
    base: String,
    token: String,
    t0: Instant,
    line_stop: Notify,
    speech_order: tokio::sync::Mutex<()>,
}

impl Engine {
    pub fn new(link: &str) -> Arc<Engine> {
        let (base, query) = link.split_once("/?").unwrap_or((link, ""));
        let token = query.split('&').find_map(|kv| kv.strip_prefix("t=")).unwrap_or("").to_string();
        Arc::new(Engine {
            s: Mutex::new(Inner {
                view: View::new(),
                mode: Mode::Idle,
                ended: false,
                muted: false,
                debug: false,
                mic: None,
                noise_floor: 0.01,
                echo: 0.004,
                preroll: VecDeque::new(),
                utts: HashMap::new(),
                next_utt: 1,
                utt: None,
                barge: None,
                cut_in: false,
                quiet_since: 0.0,
                started_at: 0.0,
                last_turn: None,
                speech_pending: 0,
                hot: VecDeque::new(),
                speech_run: 0,
                loud_run: 0,
                line: None,
                player: Player::new(),
                last_hold: (false, String::new()),
                stats: (0, 0.0, 0.0, 0.0, 0.0),
                dq: vec![],
            }),
            base: base.to_string(),
            token,
            t0: Instant::now(),
            line_stop: Notify::new(),
            speech_order: tokio::sync::Mutex::new(()),
        })
    }

    pub fn now(&self) -> f64 {
        self.t0.elapsed().as_secs_f64() * 1000.0
    }
    pub fn lock(&self) -> MutexGuard<'_, Inner> {
        self.s.lock().unwrap()
    }
    fn api(&self, path: &str) -> String {
        format!("{}/{path}?t={}", self.base, self.token)
    }
    fn post(&self, url: String, body: Vec<u8>) {
        RT.spawn(async move {
            let _ = HTTP.post(url).body(body).send().await;
        });
    }
    fn dbg(&self, s: &mut Inner, msg: &str) {
        if s.debug {
            s.dq.push(format!("{:.2} {msg}", self.now() / 1000.0));
        }
    }

    async fn run(self: Arc<Self>) {
        let (mut v, mut last_seq, mut lost_since) = (-1i64, None::<u64>, None::<f64>);
        loop {
            if self.lock().ended {
                return;
            }
            let Some(mut st) = self.get_state(v).await else {
                if self.now() - *lost_since.get_or_insert(self.now()) > LOST_MS {
                    self.end();
                    return;
                }
                self.lock().view.status("Reconnecting…");
                tokio::time::sleep(Duration::from_secs(1)).await;
                continue;
            };
            lost_since = None;
            v = st["v"].as_i64().unwrap_or(-1);
            self.note_state(&st);
            if st["say"].as_str().unwrap_or("").is_empty() {
                self.lock().view.status("Connecting…");
                continue;
            }
            if st["seq"].as_u64() == last_seq {
                continue;
            }
            let busy = |s: &Inner| s.mode == Mode::Talk || s.speech_pending > 0;
            if busy(&self.lock()) {
                let until = self.now() + MAX_MS;
                loop {
                    let stop = { let s = self.lock(); s.ended || s.muted || !busy(&s) };
                    if stop || self.now() > until {
                        break;
                    }
                    tokio::select! {
                        n = self.get_state(v) => if let Some(n) = n { v = n["v"].as_i64().unwrap_or(v); self.note_state(&n); st = n; if st["end"] == true { break; } },
                        _ = tokio::time::sleep(Duration::from_millis(50)) => {}
                    }
                }
                if self.lock().ended {
                    return;
                }
                if st["end"] == true && busy(&self.lock()) {
                    return self.end();
                }
                let Some(n) = self.get_state(-1).await else { continue };
                v = n["v"].as_i64().unwrap_or(-1);
                self.note_state(&n);
                st = n;
                let still = { let s = self.lock(); s.mode == Mode::Talk || (!s.muted && s.speech_pending > 0) };
                if still {
                    v = -1;
                    continue;
                }
                if st["say"].as_str().unwrap_or("").is_empty() || st["seq"].as_u64() == last_seq {
                    continue;
                }
            }
            let seq = st["seq"].as_u64().unwrap_or(0);
            last_seq = Some(seq);
            self.play_line(seq, st["say"].as_str().unwrap_or("").to_string()).await;
            if st["end"] == true {
                return self.end();
            }
            let mut guard = self.lock();
            let s = &mut *guard;
            if s.mode != Mode::Talk {
                if let Some(b) = s.barge.take() {
                    let now = self.now();
                    self.start_talking(s, b, false, now);
                } else {
                    self.listen(s);
                }
            }
            drop(guard);
        }
    }

    fn note_state(&self, st: &Value) {
        let mut s = self.lock();
        if let Some(a) = st["agent"].as_str().filter(|a| !a.is_empty()) {
            s.view.agent = a.to_string();
        }
        s.debug = st["debug"] == true;
        s.view.usage = st["usage"].as_str().unwrap_or("").to_string();
        s.last_hold = (st["hold"] == true, st["holdLabel"].as_str().unwrap_or("").to_string());
    }

    pub fn peek(self: &Arc<Self>) {
        let me = self.clone();
        RT.spawn(async move {
            if let Some(st) = me.get_state(-1).await {
                me.note_state(&st);
            }
        });
    }

    pub fn hang_up(&self) {
        if !self.lock().ended {
            self.end();
        }
    }

    pub fn call_back(self: &Arc<Self>, min: u32) {
        {
            let mut s = self.lock();
            if s.ended || s.view.asking_back {
                return;
            }
            s.view.asking_back = true;
            s.view.notice = "Calling you back…".into();
        }
        let (me, url) = (self.clone(), format!("{}&back={min}", self.api("decline")));
        RT.spawn(async move {
            let ok = HTTP.post(url).timeout(Duration::from_secs(10)).send().await.is_ok_and(|r| r.status().is_success());
            if ok {
                me.end();
            } else {
                let mut s = me.lock();
                s.view.asking_back = false;
                s.view.notice = "Couldn't ask for a call back. Try again".into();
            }
        });
    }

    fn end(&self) {
        let mut s = self.lock();
        if s.ended {
            return;
        }
        s.ended = true;
        s.mode = Mode::Idle;
        let now = self.now();
        s.view.status("Call ended");
        s.view.can_finish = false;
        s.mic = None;
        if let Some(l) = &mut s.line {
            l.cut = true;
        }
        self.line_stop.notify_waiters();
        s.player.close();
        android::log("call ended: voice closed");
        s.view.ended(now);
    }

    async fn debug_log(self: Arc<Self>) {
        loop {
            tokio::time::sleep(Duration::from_secs(1)).await;
            let body = {
                let mut s = self.lock();
                if s.ended {
                    return;
                }
                if !s.debug {
                    continue;
                }
                let (n, sum, max, low, high) = std::mem::take(&mut s.stats);
                let line = format!(
                    "{:.2} {:?} frames={n} rms avg={:.4} max={max:.4} echo={:.4} floor={:.4} out={}{}{}",
                    self.now() / 1000.0,
                    s.mode,
                    sum / n.max(1) as f32,
                    s.echo,
                    s.noise_floor,
                    s.player.take_peak(),
                    if s.view.quiet { " quiet" } else { "" },
                    if s.mode == Mode::Claude { format!(" thr={low:.4}/{high:.4}") } else { String::new() }
                );
                s.dq.push(line);
                s.dq.drain(..).collect::<Vec<_>>().join("\n")
            };
            let _ = HTTP.post(self.api("debug")).body(body).send().await;
        }
    }

    fn open_mic(self: &Arc<Self>) -> Option<Mic> {
        let me = self.clone();
        Mic::open(move |f| me.on_frame(f))
    }

    pub fn answer(self: &Arc<Self>) {
        let now = self.now();
        {
            let mut s = self.lock();
            if s.view.phase != crate::view::Phase::Incoming {
                return;
            }
            s.view.answered(now);
            s.view.status("Connected");
        }
        android::route(false);
        android::call_began();
        let mic = self.open_mic();
        {
            let mut s = self.lock();
            if mic.is_none() {
                s.muted = true;
                s.view.muted = true;
            }
            s.mic = mic;
        }
        self.post(self.api("answer"), vec![]);
        RT.spawn(self.clone().run());
        RT.spawn(self.clone().debug_log());
    }

    pub fn send_typed(self: &Arc<Self>, text: &str) {
        let text = text.trim().to_string();
        if text.is_empty() {
            return;
        }
        let now = self.now();
        let mut s = self.lock();
        if s.mode == Mode::Talk || s.mode == Mode::Listen {
            Self::drop_utterance(&mut s);
        }
        let id = format!("typed-{}", now as u64);
        s.last_turn = Some(id.clone());
        s.view.heard(&id, &text, true);
        s.view.status("Claude is thinking…");
        drop(s);
        let (me, url) = (self.clone(), self.api("reply"));
        RT.spawn(async move {
            let ok = HTTP.post(url).body(text.clone()).timeout(Duration::from_secs(15)).send().await.is_ok_and(|r| r.status().is_success());
            let mut s = me.lock();
            if ok {
                return;
            }
            if s.ended {
                android::log("call: a typed reply didn't go, and the call has ended");
                return;
            }
            let bubble = format!("u{id}");
            s.view.msgs.retain(|m| m.id != bubble);
            s.view.status("Listening");
            s.view.notice = "Your reply didn't go. It's back in the box".into();
            if !s.view.unsent.is_empty() {
                s.view.unsent.push(' ');
            }
            s.view.unsent.push_str(&text);
        });
    }

    pub fn toggle_mute(self: &Arc<Self>) {
        let mut s = self.lock();
        if s.ended {
            return;
        }
        if s.muted && s.mic.is_none() {
            drop(s);
            let mic = self.open_mic();
            s = self.lock();
            if mic.is_none() {
                s.view.status("Mic blocked, type a reply");
                return;
            }
            s.mic = mic;
        }
        let s = &mut *s;
        s.muted = !s.muted;
        s.view.muted = s.muted;
        if s.muted {
            if s.mode == Mode::Talk || s.mode == Mode::Listen {
                Self::drop_utterance(s);
            }
            if let Some(b) = s.barge.take() {
                s.utts.remove(&b);
            }
            s.view.status("Mic muted, type a reply");
        } else if s.mode == Mode::Idle && s.line.is_none() {
            self.listen(s);
        }
    }

    pub fn toggle_speaker(self: &Arc<Self>) {
        let on = {
            let mut s = self.lock();
            if s.ended {
                return;
            }
            s.view.speaker = !s.view.speaker;
            s.view.speaker
        };
        android::route(on);
    }

    pub fn toggle_quiet(self: &Arc<Self>) {
        let mut s = self.lock();
        if s.ended {
            return;
        }
        s.view.quiet = !s.view.quiet;
        let on = s.view.quiet;
        s.player.quiet(on);
    }

    /// The system sometimes moves the call audio.
    pub fn keep_route(&self) {
        let s = self.lock();
        if s.view.phase != crate::view::Phase::Incoming && !s.ended {
            let on = s.view.speaker;
            drop(s);
            android::route(on);
        }
    }

    pub fn claude_level(&self) -> f32 {
        let s = self.lock();
        if s.line.as_ref().is_none_or(|l| !l.started || l.cut) {
            return 0.0;
        }
        s.player.level()
    }
}

