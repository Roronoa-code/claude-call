//! Real-phone check of the call screen, without ringing: places a call on a private test topic (nobody listens to it),
//! opens it on the phone over adb (answered, no ringing), answers with typed replies, and saves
//! screenshots, screen recordings and CPU readings to D:/Outputs/CallMe.
//!   cargo run --release --example phone_check -- [adb serial]      (CALLME_AGENT=Codex: a call from Codex)
//! Needs: callme built (cargo build --release) and the app installed.
use serde_json::{Value, json};
use std::{
    collections::HashMap,
    process::{Command, Stdio},
    sync::{Arc, Mutex},
    time::{Duration, Instant, SystemTime},
};
use tokio::{
    io::{AsyncBufReadExt, AsyncWriteExt, BufReader},
    sync::oneshot,
    time::sleep,
};

type Pending = Arc<Mutex<HashMap<u64, oneshot::Sender<Value>>>>;

struct Check {
    device: Option<String>,
    out: std::path::PathBuf,
    t0: Instant,
}

impl Check {
    fn say(&self, msg: impl AsRef<str>) {
        println!("[{:.1}s] {}", self.t0.elapsed().as_secs_f32(), msg.as_ref());
    }
    fn adb_cmd(&self, args: &[&str]) -> Command {
        let mut c = Command::new("adb");
        if let Some(d) = &self.device {
            c.args(["-s", d]);
        }
        c.args(args);
        c
    }
    fn adb(&self, args: &[&str]) -> String {
        let out = self.adb_cmd(args).output().expect("adb");
        String::from_utf8_lossy(&out.stdout).into_owned()
    }
    fn shot(&self, name: &str) {
        let png = self.adb_cmd(&["exec-out", "screencap", "-p"]).output().expect("adb").stdout;
        std::fs::write(self.out.join(format!("{name}.png")), png).unwrap();
        self.say(format!("shot {name}"));
    }
    fn cpu(&self) -> String {
        let pid = self.adb(&["shell", "pidof", "fyi.mani.callme"]).trim().to_string();
        let top = self.adb(&["shell", "top", "-b", "-n", "1", "-p", &pid]);
        let line = top.lines().find(|l| l.contains(&pid)).unwrap_or("");
        line.split_whitespace().nth(8).unwrap_or("?").to_string()
    }
    fn record(&self, secs: u32, file: &str) -> std::process::Child {
        self.adb_cmd(&["shell", "screenrecord", "--time-limit", &secs.to_string(), &format!("/sdcard/{file}")])
            .stdout(Stdio::null())
            .spawn()
            .expect("adb")
    }
    fn pull(&self, file: &str, name: &str) {
        self.adb(&["pull", &format!("/sdcard/{file}"), &self.out.join(name).to_string_lossy()]);
    }
    /// Only ever taps the call screen (a tap anywhere else would be a real tap on the user's phone).
    fn tap(&self, x: f32, y: f32, what: &str) {
        let focus = self.adb(&["shell", "dumpsys", "window"]);
        let focus = focus.lines().find(|l| l.contains("mCurrentFocus=")).unwrap_or("");
        assert!(focus.contains("fyi.mani.callme/fyi.mani.callme.CallActivity"), "call screen not in front; not tapping {what}");
        self.adb(&["shell", "input", "tap", &(x.round() as i32).to_string(), &(y.round() as i32).to_string()]);
        self.say(format!("tap {what}"));
    }
    fn keyboard_up(&self) -> bool {
        self.adb(&["shell", "dumpsys", "input_method"]).contains("mInputShown=true")
    }
}

/// A minimal MCP client for callme.exe: newline-delimited JSON-RPC, calls may overlap.
struct Mcp {
    stdin: tokio::sync::Mutex<tokio::process::ChildStdin>,
    pending: Pending,
    next: Mutex<u64>,
}

impl Mcp {
    async fn request(&self, method: &str, params: Value) -> Value {
        let id = {
            let mut n = self.next.lock().unwrap();
            *n += 1;
            *n
        };
        let (tx, rx) = oneshot::channel();
        self.pending.lock().unwrap().insert(id, tx);
        let msg = json!({ "jsonrpc": "2.0", "id": id, "method": method, "params": params });
        self.stdin.lock().await.write_all(format!("{msg}\n").as_bytes()).await.unwrap();
        rx.await.unwrap()
    }
    async fn call(&self, name: &str, message: &str) -> String {
        let r = self.request("tools/call", json!({ "name": name, "arguments": { "message": message } })).await;
        r["result"]["content"][0]["text"].as_str().unwrap_or("").to_string()
    }
}

/// Waits (up to 60 s) until the phone's debug log in timing.log shows Claude's voice reaching the speaker, after `from`.
async fn wait_voice(from: u64) -> bool {
    let log = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("timing.log");
    for _ in 0..300 {
        let s = std::fs::read(&log).unwrap_or_default();
        let new = String::from_utf8_lossy(s.get(from as usize..).unwrap_or_default()).into_owned();
        let loud = new.lines().any(|l| l.contains(" Claude ") && l.split("out=").nth(1).and_then(|v| v.split_whitespace().next()?.parse::<i32>().ok()).is_some_and(|v| v > 1000));
        if loud {
            return true;
        }
        sleep(Duration::from_millis(200)).await;
    }
    false
}
fn log_len() -> u64 {
    std::fs::metadata(std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("timing.log")).map_or(0, |m| m.len())
}

async fn reply(link: &str, text: &str) {
    let (base, query) = link.split_once("/?").unwrap();
    let token = query.split('&').next().unwrap();
    reqwest::Client::new().post(format!("{base}/reply?{token}")).body(text.to_string()).send().await.unwrap();
}

#[tokio::main]
async fn main() {
    let stamp: String = humantime::format_rfc3339_seconds(SystemTime::now()).to_string().chars().filter(char::is_ascii_digit).take(12).collect();
    let c = Arc::new(Check {
        device: std::env::args().nth(1),
        out: std::path::PathBuf::from(format!("D:/Outputs/CallMe/phone-check-{stamp}")),
        t0: Instant::now(),
    });
    std::fs::create_dir_all(&c.out).unwrap();
    let now_ms = SystemTime::now().duration_since(std::time::UNIX_EPOCH).unwrap().as_millis();
    let exe = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("target/release/callme.exe");
    let topic = format!("claude-check-{now_ms}");
    let mut child = tokio::process::Command::new(exe)
        .env("CALLME_TOPIC", &topic)
        .env("CALLME_STATUS", std::env::temp_dir().join(format!("callme-check-{now_ms}.json")))
        .env("CALLME_DEBUG", "1")
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .kill_on_drop(true)
        .spawn()
        .expect("callme.exe (cargo build --release first)");

    // The call link appears on callme's log.
    let links = Arc::new(Mutex::new(None::<String>));
    let mut err = BufReader::new(child.stderr.take().unwrap()).lines();
    tokio::spawn({
        let links = links.clone();
        async move {
            while let Ok(Some(line)) = err.next_line().await {
                if let Some(l) = line.split("call link ").nth(1).and_then(|s| s.split_whitespace().next()) {
                    *links.lock().unwrap() = Some(l.to_string()); // the latest call
                }
            }
        }
    });
    let pending: Pending = Arc::default();
    let mut out = BufReader::new(child.stdout.take().unwrap()).lines();
    tokio::spawn({
        let pending = pending.clone();
        async move {
            while let Ok(Some(line)) = out.next_line().await {
                let Ok(v) = serde_json::from_str::<Value>(&line) else { continue };
                if let Some(tx) = v["id"].as_u64().and_then(|id| pending.lock().unwrap().remove(&id)) {
                    let _ = tx.send(v);
                }
            }
        }
    });
    let mcp = Arc::new(Mcp { stdin: tokio::sync::Mutex::new(child.stdin.take().unwrap()), pending, next: Mutex::new(0) });
    mcp.request("initialize", json!({ "protocolVersion": "2025-06-18", "capabilities": {}, "clientInfo": { "name": "phone-check", "version": "1" } })).await;

    let mut report = vec![];
    let spawn_call = |name: &'static str, msg: &'static str| {
        let mcp = mcp.clone();
        tokio::spawn(async move { mcp.call(name, msg).await })
    };
    let first = spawn_call("call_user", "Hi, it is Claude. This is a quick test of the new call screen.");
    let link = loop {
        if let Some(l) = links.lock().unwrap().clone() {
            break l;
        }
        sleep(Duration::from_millis(200)).await;
    };
    // Open the call screen when the call rings (on the test topic, which no phone listens to), answered at once:
    // only adb and the app itself may open it.
    while !reqwest::get(format!("https://ntfy.sh/{topic}/json?poll=1&since=all")).await.unwrap().text().await.unwrap().contains("Claude is calling") {
        sleep(Duration::from_millis(500)).await;
    }
    c.say("ringing: opening the call screen on the phone");
    c.adb(&["shell", &format!("am start -n fyi.mani.callme/.CallActivity --es link '{link}' --ez answer true")]);
    sleep(Duration::from_secs(9)).await;
    c.shot("1-talking");
    report.push(format!("ongoing-call notification: {}", c.adb(&["shell", "dumpsys", "notification"]).contains("channel=ongoing_calls")));
    sleep(Duration::from_secs(3)).await;
    reply(&link, "Looks good so far.").await;
    c.say(format!("REPLY 1 {}", first.await.unwrap()));
    sleep(Duration::from_millis(800)).await;
    c.shot("2-thinking");

    // The round buttons' centres: five columns across the screen less 16 dp each side.
    let size = c.adb(&["shell", "wm", "size"]);
    let (w, h) = size.rsplit(' ').next().unwrap().trim().split_once('x').map(|(w, h)| (w.parse::<f32>().unwrap(), h.parse::<f32>().unwrap())).unwrap();
    let dp = c.adb(&["shell", "wm", "density"]).rsplit(' ').next().unwrap().trim().parse::<f32>().unwrap() / 160.0;
    let col = |i: f32| 16.0 * dp + (w - 32.0 * dp) / 5.0 * (i + 0.5);
    let row_y = h * 0.7077; // the round buttons' centre row (measured on the S25 screenshots)

    // A long line: captions word by word, the glow and the mascot's dance; recorded and measured. Quiet silences her
    // voice for a few seconds mid-line (the debug log's out= shows what reached the speaker).
    let mut rec = c.record(20, "callme-check.mp4");
    let long = spawn_call(
        "continue_call",
        "Great. So the plan is to move the settings into a side panel, change the colours to match the new theme, and rename a few of the buttons so they read more clearly. Then I will run the tests and push it.",
    );
    let mut cpus = vec![];
    for i in 0..6 {
        sleep(Duration::from_millis(2500)).await;
        cpus.push(c.cpu());
        if i == 0 {
            c.tap(col(1.0), row_y, "Quiet");
        }
        if i == 1 {
            c.tap(col(1.0), row_y, "Quiet (off)");
        }
        if i == 2 {
            c.shot("3-talking-long");
        }
    }
    report.push(format!("cpu while talking: {}%", cpus.join(", ")));
    reply(&link, "Sounds right.").await;
    c.say(format!("REPLY 2 {}", long.await.unwrap()));
    rec.wait().unwrap();
    c.pull("callme-check.mp4", "talking.mp4");

    c.say(mcp.call("hold_call", "One sec, let me check with the main Claude.").await);
    sleep(Duration::from_secs(5)).await;
    c.shot("4-hold");
    let mut idle = vec![];
    for _ in 0..4 {
        sleep(Duration::from_secs(2)).await;
        idle.push(c.cpu());
    }
    report.push(format!("cpu on hold: {}%", idle.join(", ")));

    // Home, then the app icon: during a call it brings the call back (the app opening its own call screen).
    c.adb(&["shell", "input", "keyevent", "KEYCODE_HOME"]);
    sleep(Duration::from_secs(1)).await;
    c.adb(&["shell", "am", "start", "-n", "fyi.mani.callme/.MainActivity"]);
    sleep(Duration::from_secs(2)).await;
    let back = c.adb(&["shell", "dumpsys", "window"]).lines().any(|l| l.contains("mCurrentFocus=") && l.contains("CallActivity"));
    report.push(format!("app icon returns to the call: {back}"));

    // The two sheets, tapped like a person would, on a recording: Captions (the transcript) and Type (the keyboard).
    let mut sheets = c.record(30, "callme-sheets.mp4");
    sleep(Duration::from_secs(1)).await;
    c.tap(col(3.0), row_y, "Captions");
    sleep(Duration::from_millis(1500)).await;
    c.shot("6-captions-open");
    c.tap(col(3.0), row_y, "Captions (close)");
    sleep(Duration::from_millis(1500)).await;
    c.tap(col(4.0), row_y, "Type");
    sleep(Duration::from_millis(1800)).await;
    c.shot("7-type-open");
    c.adb(&["shell", "input", "text", "hello%sthere"]);
    sleep(Duration::from_secs(1)).await;
    c.shot("8-type-text");
    c.say(format!("keyboard shown: {}", c.keyboard_up()));
    if c.keyboard_up() {
        c.adb(&["shell", "input", "keyevent", "KEYCODE_BACK"]);
    }
    c.say("Back (keyboard away: the reply field should close)");
    sleep(Duration::from_millis(1500)).await;
    c.shot("9-type-back");
    c.tap(col(4.0), row_y, "Type again");
    sleep(Duration::from_millis(1500)).await;
    c.adb(&["shell", "input", "text", "sent%sfrom%sthe%sphone"]);
    c.adb(&["shell", "input", "keyevent", "KEYCODE_ENTER"]);
    c.say("Enter (sends)");
    sleep(Duration::from_millis(1800)).await;
    c.shot("10-type-sent");
    sheets.wait().unwrap();
    c.pull("callme-sheets.mp4", "sheets.mp4");
    let back = spawn_call("continue_call", "Okay, I am back. The main Claude says yes.");
    reply(&link, "Also make the button blue.").await;
    c.say(format!("REPLY 3 {}", back.await.unwrap()));
    // The user hangs up (the red button beside Done talking) in the middle of a long line: her voice must stop at
    // once, and no part of the line may be played after (logcat: AAudio streams opened after the hang-up).
    let from = log_len();
    let last = loop {
        let t = spawn_call(
            "continue_call",
            "Anything else before I go? I can also tidy up the old branches, write the release notes, and check the build on the laptop, so just tell me which of those you would like me to do first.",
        );
        sleep(Duration::from_secs(2)).await;
        if !t.is_finished() {
            break t;
        }
        c.say(format!("not said yet: {}", t.await.unwrap())); // a reply was still waiting: say it again
    };
    let talking = wait_voice(from).await;
    sleep(Duration::from_millis(1500)).await;
    c.shot("5-hang-up");
    report.push(format!("her voice was playing at the hang-up: {talking}"));
    c.adb(&["logcat", "-c"]);
    c.tap(w * 0.6719, h * 0.8558, "Hang up");
    c.say(format!("REPLY 4 {}", last.await.unwrap()));
    sleep(Duration::from_secs(5)).await;
    let log = c.adb(&["logcat", "-d", "-v", "epoch"]);
    let pid = c.adb(&["shell", "pidof", "fyi.mani.callme"]).trim().to_string();
    let mine: Vec<&str> = log.lines().filter(|l| l.split_whitespace().nth(1) == Some(pid.as_str())).collect();
    let at = |l: &str| l.split_whitespace().next().and_then(|t| t.parse::<f64>().ok()).unwrap_or(0.0);
    let ended = mine.iter().find(|l| l.contains("call ended: voice closed")).map(|l| at(l));
    let opened = mine.iter().filter(|l| l.contains("AAudio") && l.to_lowercase().contains("open")).filter(|l| ended.is_some_and(|e| at(l) > e)).count();
    std::fs::write(c.out.join("hang-up-logcat.txt"), mine.join("\n")).unwrap();
    report.push(format!("hang-up mid-line: voice closed {}; audio streams opened after it: {opened}", if ended.is_some() { "yes" } else { "NOT SEEN" }));
    let closed = !c.adb(&["shell", "dumpsys", "activity", "activities"]).lines().any(|l| l.contains("Hist") && l.contains("fyi.mani.callme/.CallActivity"));
    report.push(format!("call screen closed: {closed}"));
    let listening = c.adb(&["shell", "dumpsys", "activity", "services", "fyi.mani.callme"]).contains("ListenService");
    c.say(format!("call screen closed: {closed}; call listener still running: {listening}"));

    // A new call; while she talks, "Call back in 5 min" (top right): it hangs up and the agent is told to ring back.
    sleep(Duration::from_secs(3)).await;
    let from = log_len();
    let again = spawn_call("call_user", "Hi again, it is Claude. I have one more question about the release notes, if you have a minute.");
    let link2 = loop {
        if let Some(l) = links.lock().unwrap().clone().filter(|l| *l != link) {
            break l;
        }
        sleep(Duration::from_millis(200)).await;
    };
    sleep(Duration::from_secs(4)).await;
    c.adb(&["shell", &format!("am start -n fyi.mani.callme/.CallActivity --es link '{link2}' --ez answer true")]);
    let talking = wait_voice(from).await;
    sleep(Duration::from_millis(800)).await;
    c.shot("11-call-back");
    c.tap(w - 94.0 * dp, h * 0.0635, "Call back in 5 min");
    let said = again.await.unwrap();
    c.say(format!("REPLY 5 {said}"));
    report.push(format!("call back in 5 (voice playing {talking}): {said}"));
    std::fs::write(c.out.join("report.txt"), report.join("\n") + "\n").unwrap();
    c.say(format!("saved to {}", c.out.display()));
}
