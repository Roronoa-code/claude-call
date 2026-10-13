# CallMe

Lets Claude Code phone you when it needs you, for a real two-way voice conversation. Free: no phone number, accounts or API keys.

**How a call works:** the `callme` MCP server (Rust, `src/`, one binary `bin/callme.exe`) is reached through one free Cloudflare quick tunnel shared by every session (`callme.exe gateway`: started hidden by the first server, routes by call token, log in `gateway.log`), and posts a "Claude is calling" alert to a private ntfy topic. The **Claude Call** Android app keeps a connection open to that topic and rings with a full-screen call screen. The whole app is Rust too (`phone/`): the home and call screens drawn with Slint, call audio through AAudio on the phone-call path, the listener, ringing, notifications, settings and the widget. Android must create an app's activities, service and receivers as Java classes, so `Shell.kt` holds one-line shells that forward to Rust (`phone/src/shell.rs`). Calls from Codex (`CALLME_AGENT=Codex`) show its pet Null instead of Clawd (frames from the Codex app's sprite sheet, cleaned by `cargo run --release --example null_atlas`). Your speech is transcribed on this PC by Whisper on the GPU, which uses what Claude just said as context so names are heard correctly.

**Voice:** your own ElevenLabs clip (`voice/flirty.wav`, the first 22 s of `flirty.mp3`) is cloned on the GPU by Qwen3-TTS Base through `faster-qwen3-tts` (`voice/voice_server.py`, venv in `voice/.venv`; the model is read from ThePipeline). The same GPU server also runs the hearing, with Whisper large-v3-turbo. The clip in use is set in `voice/voice.json`.

Timing:
- The server uses about 7.2 GB of VRAM. One copy is shared by all Claude sessions, and it exits after 30 idle minutes.
- A call first loads the server (about 40–70 s if it isn't already running) and prepares the first line. Only then does the phone ring, so there is no wait after answering.
- After that, Claude's voice is streamed while it is made, with a 250 ms cushion on the phone so a late piece is never a gap. Captions follow the real playback position.
- While you talk, your words show live (Whisper every ~0.9 s). Your turn ends 0.7 s after you stop if what you said reads as finished, otherwise after 1.8 s (or press Done talking).
- Talking over Claude: loudness against the measured echo, confirmed by Whisper words; transcripts that only repeat Claude's own line are dropped.
- Debug: `"debug": true` in `config.json` (or `CALLME_DEBUG=1`) logs the phone's mic levels, echo level and interrupts to `timing.log`.

Fallbacks: ElevenLabs for the voice (`ELEVENLABS_API_KEY` + `ELEVENLABS_VOICE_ID`), then captions only. Without the GPU server there is no hearing: type replies instead.

**Tools:** `call_user`, `continue_call`, `hold_call`, `end_call`, `get_contact_mode`, `set_contact_mode`.

**Build:**
- PC: `cargo build --release -p callme` (needs the Visual C++ Build Tools), then copy `target\release\callme.exe` to `bin\`. Registered for all projects with `claude mcp add --scope user callme -- B:\Projects\CallMe\bin\callme.exe`.
- Phone: `cd android; .\gradlew.bat assembleRelease` builds the Rust library first (needs `rustup target add aarch64-linux-android` and `cargo install cargo-ndk`), then the app. Install with `adb install -r app\build\outputs\apk\release\app-release.apk`.

**Check:** `.\gradlew.bat installReleaseAndroidTest` once, then `cargo run --release --example phone_check -- <adb serial>` (with `CALLME_AGENT=Codex` for a call from Codex): a whole call on the real phone without ringing it (private test topic, typed replies), with screenshots, a screen recording and CPU readings in `D:\Outputs\CallMe\`. It restarts the app's call listener at the end.
