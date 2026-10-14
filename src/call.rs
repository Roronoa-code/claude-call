//! The call: its state, Claude's voice for each line, and the web server the phone talks to (through the shared
//! tunnel).
use crate::speech::{UTTS, final_transcript, partial, take_audio};
use crate::voice::{self, Audio};
use crate::{cfg, debug_on, log, log_line};
use axum::{
    body::{Body, to_bytes},
    extract::Request,
    http::{Method, StatusCode, header},
    response::{IntoResponse, Response},
};
use serde_json::json;
use std::{
    collections::HashMap,
    future::Future,
    sync::{Arc, LazyLock, Mutex, MutexGuard, OnceLock},
    time::{Duration, Instant},
};
use tokio::{
    sync::{oneshot, watch},
    time::timeout,
};

pub static PORT: OnceLock<u16> = OnceLock::new();

/// Claude's line being made while it plays (16-bit mono PCM).
pub struct Stream {
    rate: u32,
    data: Mutex<Vec<u8>>,
    progress: watch::Sender<(usize, bool)>, // bytes so far, done
}
/// Marks the stream done however its maker ends (finished, broke, or the line was replaced or talked over).
struct DoneGuard(Arc<Stream>);
impl Drop for DoneGuard {
    fn drop(&mut self) {
        let n = self.0.data.lock().unwrap().len();
        self.0.progress.send_replace((n, true));
    }
}

#[derive(Default)]
pub struct Call {
    v: u64,
    seq: u64,
    pub say: String,
    end: bool,
    hold: bool,
    /// How the current line is played: "stream" (sent while it is being made), "parts" (whole files), None = not decided.
    mode: Option<&'static str>,
    parts: Vec<String>,
    audio: Vec<Option<Arc<Audio>>>, // None = that part failed: the phone says it with its own voice
    stream: Option<Arc<Stream>>,
    tts: Option<tokio::task::AbortHandle>,
    pub answered: bool,
    pub declined: bool,
    pub call_back: Option<Instant>, // the user hung up asking to be called back then
    last_seen: Option<Instant>,
    watchers: u32,
    waiter: Option<oneshot::Sender<Option<String>>>,
    pub pending: Vec<String>,
    heard_at: Option<Instant>, // the user's audio last arrived (they are mid-sentence while it keeps coming)
    turns_in_flight: u32,      // turns still being transcribed
    usage: String,             // what the call has used so far, e.g. "$0.04" (usage.rs)
}

static CALL: LazyLock<Mutex<Call>> = LazyLock::new(Default::default);
// The call page waits on /state until something changes (long poll), so a new line reaches it at once.
static CHANGED: LazyLock<watch::Sender<u64>> = LazyLock::new(|| watch::channel(0).0);

pub fn call() -> MutexGuard<'static, Call> {
    CALL.lock().unwrap()
}
fn changed(c: &mut Call) {
    c.v += 1;
    CHANGED.send_replace(c.v);
}

// Latency marks for each reply turn, in timing.log: where the gap between "you stop" and "Claude speaks" goes.
static TURN_START: Mutex<Option<Instant>> = Mutex::new(None);
pub fn mark(what: &str, reset: bool) {
    let mut start = TURN_START.lock().unwrap();
    if reset {
        *start = Some(Instant::now());
    }
    if let Some(s) = *start {
        log_line(&format!("+{}ms {what}", s.elapsed().as_millis()));
    }
}

fn fire(first: &mut Option<oneshot::Sender<()>>) {
    if let Some(t) = first.take() {
        let _ = t.send(());
    }
}

/// Starts saying a line; returns once its first sound is ready.
pub async fn say(text: String, end: bool, hold: bool) {
    mark("Claude replied (thinking done), making voice", false);
    let seq = {
        let mut c = call();
        if let Some(t) = c.tts.take() {
            t.abort(); // a newer line replaces one still being made
        }
        c.seq += 1;
        let shown = crate::speech::without_tags(&text); // the [mood] tags only steer the voice
        (c.say, c.end, c.hold, c.mode) = (shown.clone(), end, hold, None);
        (c.parts, c.audio, c.stream) = (vec![shown], vec![], None);
        changed(&mut c);
        c.seq
    };
    let (tx, first_ready) = oneshot::channel();
    let task = tokio::spawn(async move {
        let mut first = Some(tx);
        if !stream_line(seq, &text, &mut first).await && call().seq == seq {
            speak_parts(seq, &text, &mut first).await;
        }
    });
    if call().seq == seq {
        call().tts = Some(task.abort_handle());
    }
    let _ = first_ready.await; // also returns if the line failed or was replaced
}

/// Main path: the cloned voice streamed as it is made (first sound ~0.2 s after the line is ready), one stream per
/// line so the voice stays even. False if the GPU voice is unavailable.
async fn stream_line(seq: u64, text: &str, first: &mut Option<oneshot::Sender<()>>) -> bool {
    let Some(mut r) = voice::gpu("/tts_stream", text.as_bytes().to_vec(), None).await else { return false };
    if call().seq != seq {
        return true;
    }
    let rate = r.headers().get("x-sample-rate").and_then(|v| v.to_str().ok()?.parse().ok()).unwrap_or(24_000);
    let st = Arc::new(Stream { rate, data: Mutex::new(vec![]), progress: watch::channel((0, false)).0 });
    let _done = DoneGuard(st.clone());
    {
        let mut c = call();
        (c.stream, c.mode) = (Some(st.clone()), Some("stream"));
        changed(&mut c);
    }
    mark("first voice chunk ready", false);
    fire(first);
    loop {
        match r.chunk().await {
            Ok(None) => break,
            Ok(Some(b)) => {
                let n = {
                    let mut d = st.data.lock().unwrap();
                    d.extend_from_slice(&b);
                    d.len()
                };
                st.progress.send_replace((n, false));
            }
            Err(e) => {
                log(format!("voice stream broke {e}"));
                break;
            }
        }
    }
    true
}

/// Fallback: whole files, in two pieces: the first sentence (so playback starts fast), then the rest in one go.
async fn speak_parts(seq: u64, text: &str, first: &mut Option<oneshot::Sender<()>>) {
    let mut sentences = vec![];
    let mut cur = String::new();
    let mut chars = text.chars().peekable();
    while let Some(ch) = chars.next() {
        cur.push(ch);
        if ".!?".contains(ch) && !chars.peek().is_some_and(|n| ".!?".contains(*n)) {
            sentences.push(std::mem::take(&mut cur));
        }
    }
    sentences.push(cur);
    let sentences: Vec<String> = sentences.iter().map(|s| s.trim().to_string()).filter(|s| !s.is_empty()).collect();
    let mut n = 1;
    while n < sentences.len() && sentences[..n].join(" ").len() < 25 {
        n += 1; // don't start on a tiny fragment
    }
    let parts: Vec<String> =
        [sentences[..n.min(sentences.len())].join(" "), sentences[n.min(sentences.len())..].join(" ")].into_iter().filter(|s| !s.is_empty()).collect();
    {
        let mut c = call();
        (c.parts, c.mode) = (parts.iter().map(|p| crate::speech::without_tags(p)).collect(), Some("parts"));
        changed(&mut c);
    }
    for p in parts {
        let a = voice::synth(&p).await;
        {
            let mut c = call();
            if c.seq != seq {
                return; // superseded by a newer line
            }
            c.audio.push(a.map(Arc::new));
            changed(&mut c);
            if c.audio.len() == 1 {
                mark("first piece of voice ready", false);
            }
        }
        fire(first);
    }
}

/// What the user says goes to whoever is waiting; if nobody is (e.g. while on hold), it is kept for the next turn.
pub fn deliver(t: String) {
    let mut c = call();
    match c.waiter.take() {
        Some(w) => {
            if let Err(Some(t)) = w.send(Some(t)) {
                c.pending.push(t);
            }
        }
        None if !t.is_empty() => c.pending.push(t),
        None => {}
    }
}

/// Just before Claude's line is said: waits while the user is still talking or their words are still being
/// transcribed, then takes anything they said after Claude got their turn (e.g. a correction). Saying the stale answer
/// and then the next one, talking over itself, is what made corrections a mess.
pub async fn said_meanwhile() -> Option<String> {
    let talking = || {
        let c = call();
        c.turns_in_flight > 0 || c.heard_at.is_some_and(|t| t.elapsed() < Duration::from_millis(1200))
    };
    let since = Instant::now();
    while talking() && since.elapsed() < Duration::from_secs(30) {
        tokio::time::sleep(Duration::from_millis(50)).await;
    }
    let mut c = call();
    (!c.pending.is_empty()).then(|| c.pending.drain(..).collect::<Vec<_>>().join(" / "))
}

/// Registers for the user's next reply now (so nothing said meanwhile is lost) and waits up to `ms`.
pub fn wait_for_reply(ms: u64) -> impl Future<Output = Option<String>> {
    let ready = {
        let mut c = call();
        if c.pending.is_empty() {
            let (tx, rx) = oneshot::channel();
            c.waiter = Some(tx);
            Err(rx)
        } else {
            Ok(c.pending.drain(..).collect::<Vec<_>>().join(" / "))
        }
    };
    async move {
        match ready {
            Ok(t) => Some(t),
            Err(rx) => match timeout(Duration::from_millis(ms), rx).await {
                Ok(r) => r.ok().flatten(),
                Err(_) => {
                    call().waiter = None;
                    None
                }
            },
        }
    }
}

pub fn set_usage(u: String) {
    let mut c = call();
    if c.usage != u {
        c.usage = u;
        changed(&mut c);
    }
}

/// Ends the wait of whoever is waiting for a reply (hung up / declined / no answer).
pub fn cancel_reply() {
    if let Some(w) = call().waiter.take() {
        let _ = w.send(None);
    }
}

/// The agent's session closed during a call (a headless run that ended its turn): let the last line reach the phone,
/// say goodbye if it never hung up, and stay up until the phone has hung up, instead of cutting the line off and
/// leaving the phone waiting until it gives up on the PC.
pub async fn finish() {
    if !call().answered || call().declined {
        return;
    }
    wait_line().await;
    if !call().end {
        say("[playful] Okay, I've got to run. Call me back if you need me.".into(), true, false).await;
        wait_line().await;
    }
    let since = Instant::now();
    while page_live() && since.elapsed() < Duration::from_secs(60) {
        tokio::time::sleep(Duration::from_millis(500)).await;
    }
}

/// Waits until the line being made is all made (the phone fetches it as it goes).
async fn wait_line() {
    let stream = call().stream.clone();
    if let Some(st) = stream {
        let _ = timeout(Duration::from_secs(60), st.progress.subscribe().wait_for(|&(_, done)| done)).await;
    }
}

pub fn page_live() -> bool {
    let c = call();
    c.watchers > 0 || c.last_seen.is_some_and(|t| t.elapsed() < Duration::from_secs(10))
}

// ---- The web server: the call page, Claude's voice, and the user's speech. ----
pub async fn serve() -> u16 {
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.expect("bind");
    let port = listener.local_addr().unwrap().port();
    let _ = PORT.set(port);
    let app = axum::Router::new().fallback(|req: Request| async move {
        let path = req.uri().path().to_string();
        handle(req).await.unwrap_or_else(|e| {
            log(format!("request failed {path} {e}"));
            StatusCode::INTERNAL_SERVER_ERROR.into_response()
        })
    });
    tokio::spawn(async move { axum::serve(listener, app).await.unwrap() });
    port
}

/// Counts the page as watching while a /state poll is open, even if the phone drops the request.
struct Watching;
impl Watching {
    fn new() -> Self {
        let mut c = call();
        c.watchers += 1;
        c.last_seen = Some(Instant::now());
        Watching
    }
}
impl Drop for Watching {
    fn drop(&mut self) {
        let mut c = call();
        c.watchers -= 1;
        c.last_seen = Some(Instant::now());
    }
}

fn json_res(v: serde_json::Value) -> Response {
    ([(header::CONTENT_TYPE, "application/json"), (header::CACHE_CONTROL, "no-store")], v.to_string()).into_response()
}

async fn handle(req: Request) -> Result<Response, String> {
    let path = req.uri().path().to_string();
    let query = axum::extract::Query::<HashMap<String, String>>::try_from_uri(req.uri()).map(|q| q.0).unwrap_or_default();
    let q = |k: &str| query.get(k).map(String::as_str);
    let num = |k: &str| q(k).and_then(|v| v.parse::<u64>().ok());
    if q("t") != Some(cfg().token.as_str()) {
        return Ok(StatusCode::NOT_FOUND.into_response());
    }
    let post = req.method() == Method::POST;
    let body = |limit: usize| async move { to_bytes(req.into_body(), limit).await.map_err(|e| e.to_string()) };
    let no_content = || StatusCode::NO_CONTENT.into_response();

    Ok(match path.as_str() {
        "/state" => {
            let want = q("v").and_then(|v| v.parse::<i64>().ok()).unwrap_or(-1);
            let _watching = Watching::new();
            let mut rx = CHANGED.subscribe();
            if (call().v as i64) <= want {
                let _ = timeout(Duration::from_secs(20), rx.wait_for(|v| *v as i64 > want)).await;
            }
            let c = call();
            json_res(json!({ "v": c.v, "seq": c.seq, "say": c.say, "end": c.end, "hold": c.hold,
                "holdLabel": if c.hold { c.say.as_str() } else { "" }, "mode": c.mode, "parts": c.parts,
                "ready": c.audio.len(), "debug": debug_on(), "agent": cfg().agent, "usage": c.usage }))
        }
        "/stream" => {
            // Claude's line as 16-bit PCM while it is being made, in rolling pieces: each request returns the audio made
            // since byte `from` (waiting until there is some), with x-done: 1 on the last. Whole responses, because the
            // quick tunnel holds back a streamed response until ~128 KB have built up. 409 = play the parts instead.
            let (seq, from) = (num("seq").unwrap_or(0), num("from").unwrap_or(0) as usize);
            let mut rx = CHANGED.subscribe();
            let st = loop {
                {
                    let c = call();
                    if c.seq != seq {
                        return Ok(StatusCode::NOT_FOUND.into_response());
                    }
                    match c.mode {
                        Some("stream") => break c.stream.clone().unwrap(),
                        Some(_) => return Ok(StatusCode::CONFLICT.into_response()),
                        None => {}
                    }
                }
                let _ = timeout(Duration::from_secs(30), rx.changed()).await;
            };
            let mut prx = st.progress.subscribe();
            loop {
                let (bytes, done) = *prx.borrow_and_update();
                if bytes > from || done {
                    break;
                }
                let _ = timeout(Duration::from_secs(15), prx.changed()).await;
            }
            let (bytes, done) = *st.progress.borrow();
            let piece = st.data.lock().unwrap().get(from..).map(<[u8]>::to_vec).unwrap_or_default();
            if from == 0 && !piece.is_empty() {
                mark("first audio byte sent to phone", false);
            }
            let last = done && from + piece.len() >= bytes;
            Response::builder()
                .header(header::CONTENT_TYPE, "application/octet-stream")
                .header(header::CACHE_CONTROL, "no-store")
                .header("x-sample-rate", st.rate.to_string())
                .header("x-done", if last { "1" } else { "0" })
                .body(Body::from(piece))
                .unwrap()
        }
        "/audio" => {
            let part = num("part").unwrap_or(0) as usize;
            let a = {
                let c = call();
                if num("seq") != Some(c.seq) { None } else { c.audio.get(part).cloned().flatten() }
            };
            match a {
                Some(a) => {
                    if part == 0 {
                        mark("first audio byte sent to phone", false);
                    }
                    ([(header::CONTENT_TYPE, a.kind), (header::CACHE_CONTROL, "no-store")], a.data.clone()).into_response()
                }
                None => StatusCode::NOT_FOUND.into_response(),
            }
        }
        "/hush" if post => {
            // The user talked over Claude: stop making the rest of the line.
            let mut c = call();
            if num("seq") == Some(c.seq) {
                if let Some(t) = c.tts.take() {
                    t.abort();
                }
            }
            no_content()
        }
        "/answer" if post => {
            call().answered = true;
            no_content()
        }
        "/decline" if post => {
            let mut c = call();
            c.declined = true;
            if let Some(m) = num("back") {
                c.call_back = Some(Instant::now() + Duration::from_secs(m * 60));
            }
            if let Some(t) = c.tts.take() {
                t.abort(); // hung up mid-line: stop making the rest
            }
            drop(c);
            cancel_reply();
            no_content()
        }
        "/listen" if post => {
            // A piece of what the user is saying; with partial=1, also returns the transcript so far (live captions).
            let (u, off, live) = (q("u").unwrap_or("").to_string(), num("off").unwrap_or(0) as usize, q("partial").is_some());
            let cell = take_audio(&u, off, &body(16_000 * 2 * 30).await?);
            call().heard_at = Some(Instant::now());
            if live {
                json_res(partial(&cell).await.map_or(json!({ "text": null }), |p| json!(p)))
            } else {
                json_res(json!({}))
            }
        }
        "/speech" if post => {
            mark(&format!("turn sent (you stopped talking {}ms before)", q("quiet").unwrap_or("?")), true);
            let (u, off, end) = (q("u").unwrap_or("").to_string(), num("off").unwrap_or(0) as usize, num("end").map(|e| e as usize));
            let interrupted = q("interrupted").is_some();
            call().turns_in_flight += 1;
            let result = match body(16_000 * 2 * 120).await {
                Ok(b) => final_transcript(&take_audio(&u, off, &b), end).await,
                Err(e) => Err(e),
            };
            UTTS.lock().unwrap().0.remove(&u);
            let (secs, heard, reuse) = result.inspect_err(|_| call().turns_in_flight -= 1)?;
            mark(&format!("transcribed ({secs:.1}s of audio{}) -> handed to Claude", if reuse { ", reused live transcript" } else { "" }), false);
            let ok = !voice::is_noise(&heard);
            if ok {
                deliver(if interrupted { format!("(interrupting you) {heard}") } else { heard.clone() });
            } else if interrupted {
                // Cut off by a noise, not words: without this the agent would wait for a reply that never comes.
                deliver("(you were cut off by a noise, not words: carry on from where you were)".into());
            }
            call().turns_in_flight -= 1;
            json_res(json!({ "text": if ok { heard } else { String::new() } }))
        }
        "/reply" if post => {
            let text = String::from_utf8_lossy(&body(20_000).await?).trim().to_string();
            deliver(text);
            no_content()
        }
        "/debug" if post => {
            let text = String::from_utf8_lossy(&body(20_000).await?).to_string();
            if debug_on() {
                for l in text.lines().filter(|l| !l.is_empty()) {
                    log_line(&format!("DEBUG {}", l.chars().take(400).collect::<String>()));
                }
            }
            no_content()
        }
        _ => StatusCode::NOT_FOUND.into_response(),
    })
}
