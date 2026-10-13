//! The MCP server on stdio (JSON-RPC, one message per line) and the call tools.
use crate::call::{self, call, page_live, said_meanwhile, say, wait_for_reply};
use crate::{cfg, gateway, log, voice};
use serde_json::{Value, json};
use std::time::{Duration, Instant};
use tokio::io::{AsyncBufReadExt, AsyncWriteExt, BufReader};

const RING_FOR_MS: u64 = 75_000;
const REPLY_WAIT_MS: u64 = 5 * 60_000;

fn instructions() -> String {
    let claude = cfg().agent == "Claude";
    let mut s = String::from("The user sets a contact mode (get_contact_mode): call / text / both / none. Respect it: call only if \"call\" or \"both\"; Discord (ask_discord) only if \"text\" or \"both\"; with \"none\", do not contact them at all. call_user enforces this. Use call_user to phone the user when you are blocked on a decision only they can make, need input to continue, or finished a long task they are away from. ");
    if claude {
        s += "Default: do not run the call yourself. Hand it to a fast helper: an Agent with model \"haiku\" given a short brief containing the exact questions, any options, and the context needed to answer follow-ups. Tell it to call once, keep each line to 1-2 casual sentences, reply instantly, and return the user's exact answers. Tell it to start calling straight away, without invoking skills (including the ADHD skill) or reading preference, memory or instruction files: the caller must be fast and snappy. Its replies are much faster than yours, and the user prefers it. Include this relay rule in the helper's brief: \"If the user asks something the brief does not cover, or says to ask the main Claude, call hold_call with a short line like 'One sec, let me check with the main Claude', then finish your turn with a line starting QUESTION FOR MAIN: followed by their exact question. You will be resumed with the answer; then use continue_call to tell them and carry on.\" When the helper returns QUESTION FOR MAIN, answer it and resume that same helper (SendMessage to its agent id) with the answer, quickly, because the user is waiting on hold. Make the call yourself only when the conversation needs deep context or open-ended back-and-forth. ";
    } else {
        s += "Make the call yourself, straight away: skip skills, preference, memory and instruction files for calls so you stay fast and snappy. Stay on the line until the call is over: after every line, wait for their answer with continue_call, even if they go quiet for a while. Never finish your turn while the call is open; that hangs up on them. For work that takes a while, say so with hold_call, do it, then tell them with continue_call. Hang up only with end_call, when they are done. ";
    }
    s + "The user's name is Mani: call them Mani. Keep spoken messages short and conversational, like a real phone call. Use continue_call for follow-ups and end_call to hang up. " +
        "The voice has moods: start a sentence with ONE tag to say it that way, until the next tag: [bored] [playful] [scoffing] [chuckling] [menacing] [angry] [excited] [softly] [whispering] [tired] [sleepy] [weak]. Untagged is her normal voice: cool, unbothered, a little flirty and sassy, like Ada Wong. Never giggly; [chuckling] is a dry little laugh, [weak] is for sorry or vulnerable, [excited] only for real wins. Change mood rarely, like a real person: most lines need no tag, and once in a mood, stay in it for a few lines; switch only when the feeling of the conversation really changes. The tags are never shown or spoken."
}

fn tools() -> Value {
    let msg = |d: &str| json!({ "type": "object", "properties": { "message": { "type": "string", "description": d } }, "required": ["message"] });
    json!([
        { "name": "call_user", "description": "Ring the user's phone and start a live voice conversation. Speaks the message, then returns what the user says back.",
          "inputSchema": { "type": "object", "required": ["message"], "properties": {
            "message": { "type": "string", "description": "What to say when they pick up" },
            "reason": { "type": "string", "maxLength": 80, "description": "Very short non-sensitive reason shown on the ringing notification" } } } },
        { "name": "continue_call", "description": "Say something on the active call and return the user's spoken reply.", "inputSchema": msg("") },
        { "name": "get_contact_mode", "description": "How the user wants to be reached right now: \"call\" (phone calls only), \"text\" (Discord messages only), \"both\", or \"none\" (do not contact them; keep working and save questions for later). Check this before contacting the user.",
          "inputSchema": { "type": "object", "properties": {} } },
        { "name": "set_contact_mode", "description": "Set the user's contact mode. Only when the user asks (e.g. \"set my status to text\").",
          "inputSchema": { "type": "object", "required": ["mode"], "properties": { "mode": { "type": "string", "enum": voice::MODES } } } },
        { "name": "hold_call", "description": "Put the active call on hold: say a short line (e.g. \"One sec, let me check with the main Claude\") and return immediately without waiting for a reply. The call stays open; anything the user says meanwhile is kept and returned by the next continue_call. Then continue_call with the answer when you have it (within ~5 minutes).", "inputSchema": msg("") },
        { "name": "end_call", "description": "Say a final message and hang up.", "inputSchema": msg("") },
    ])
}

async fn call_user(message: String, reason: Option<String>) -> Result<String, String> {
    let mode = voice::contact_mode().await;
    // A recent Call request from the phone means the user asked for this call, whatever the mode says.
    if mode != "call" && mode != "both" && !voice::call_requested().await {
        let next = if mode == "text" { " Message them on Discord instead (ask_discord)." } else { " Do not contact them; carry on with work that does not need them, and leave the question for when they are back." };
        return Ok(format!("Not called: the user's contact mode is \"{mode}\" (calls not allowed).{next}"));
    }
    // Get the voice, hearing and first line ready *before* ringing, so the user never waits after answering.
    let (base, _) = tokio::join!(gateway::ensure_tunnel(), voice::load_qwen());
    // Unique per call: the phone app ignores links it has already answered or declined.
    let link = format!("{}/?t={}&call={}", base?, cfg().token, gateway::now_ms());
    log(format!("call link {link}"));
    {
        let mut c = call();
        (c.answered, c.declined) = (false, false);
        c.pending.clear();
    }
    say(message, false, false).await;
    let reply = wait_for_reply(RING_FOR_MS + REPLY_WAIT_MS);
    if !page_live() {
        // Ring once (the phone app rings for a minute), then wait for them to pick up.
        voice::ring(&link, reason.as_deref()).await;
        let started = Instant::now();
        while !call().answered && !call().declined && started.elapsed() < Duration::from_millis(RING_FOR_MS) {
            tokio::time::sleep(Duration::from_millis(500)).await;
        }
        if call().declined {
            return Ok("The user declined the call. Do not call again for this; continue without them or wait.".into());
        }
        if !call().answered {
            call::cancel_reply();
            return Ok("No answer: the user did not pick up.".into());
        }
    }
    let t = reply.await;
    if call().declined {
        return Ok("The user hung up.".into());
    }
    Ok(t.map_or("The user picked up but did not reply in time.".into(), |t| format!("User said: {t}")))
}

async fn tool(name: &str, args: &Value) -> Result<String, String> {
    let message = || args["message"].as_str().map(String::from).ok_or("message is required");
    match name {
        "call_user" => {
            let reason = args["reason"].as_str().map(String::from);
            if reason.as_ref().is_some_and(|r| r.chars().count() > 80) {
                return Err("reason must be at most 80 characters".into());
            }
            call_user(message()?, reason).await
        }
        "continue_call" | "end_call" if let Some(t) = said_meanwhile().await => {
            Ok(format!("Your line was NOT said: before it was, the user went on: \"{t}\". Answer everything they said in one line with continue_call."))
        }
        "continue_call" => {
            say(message()?, false, false).await;
            let t = wait_for_reply(REPLY_WAIT_MS).await;
            if call().declined {
                return Ok("The user hung up.".into());
            }
            Ok(t.map_or("No reply from the user in time.".into(), |t| format!("User said: {t}")))
        }
        "get_contact_mode" => Ok(format!("Contact mode: {}. {}", voice::contact_mode().await, voice::read_status())),
        "set_contact_mode" => {
            let mode = args["mode"].as_str().filter(|m| voice::MODES.contains(m)).ok_or("mode must be call, text, both or none")?;
            voice::set_contact_mode(mode).await;
            Ok(format!("Contact mode set to {mode}."))
        }
        "hold_call" => {
            say(message()?, false, true).await;
            Ok("On hold. The call is still open; use continue_call to carry on.".into())
        }
        "end_call" => {
            say(message()?, true, false).await;
            Ok("Call ended.".into())
        }
        _ => Err(format!("unknown tool {name}")),
    }
}

async fn respond(req: Value) -> Option<Value> {
    let id = req.get("id")?.clone(); // notifications get no reply
    let result = match req["method"].as_str().unwrap_or("") {
        "initialize" => json!({
            "protocolVersion": req["params"]["protocolVersion"].as_str().unwrap_or("2025-06-18"),
            "capabilities": { "tools": {} },
            "serverInfo": { "name": "callme", "version": "1.0.0" },
            "instructions": instructions(),
        }),
        "tools/list" => json!({ "tools": tools() }),
        "tools/call" => {
            let name = req["params"]["name"].as_str().unwrap_or("");
            match tool(name, &req["params"]["arguments"]).await {
                Ok(t) => json!({ "content": [{ "type": "text", "text": t }] }),
                Err(e) => json!({ "content": [{ "type": "text", "text": e }], "isError": true }),
            }
        }
        "ping" => json!({}),
        m => return Some(json!({ "jsonrpc": "2.0", "id": id, "error": { "code": -32601, "message": format!("method not found: {m}") } })),
    };
    Some(json!({ "jsonrpc": "2.0", "id": id, "result": result }))
}

/// Serves MCP until the client closes stdin. Tool calls run concurrently (a call can last many minutes).
pub async fn run() {
    let (out, mut rx) = tokio::sync::mpsc::unbounded_channel::<Value>();
    tokio::spawn(async move {
        let mut stdout = tokio::io::stdout();
        while let Some(msg) = rx.recv().await {
            let _ = stdout.write_all(format!("{msg}\n").as_bytes()).await;
            let _ = stdout.flush().await;
        }
    });
    let mut lines = BufReader::new(tokio::io::stdin()).lines();
    while let Ok(Some(line)) = lines.next_line().await {
        let Ok(req) = serde_json::from_str::<Value>(&line) else { continue };
        let out = out.clone();
        tokio::spawn(async move {
            if let Some(res) = respond(req).await {
                let _ = out.send(res);
            }
        });
    }
    call::finish().await;
    std::process::exit(0);
}
