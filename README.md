# CallMe

Lets Claude Code phone you when it needs you, for a real two-way voice conversation. Free: no phone number, accounts or API keys.

**How a call works:** the `callme` MCP server (`server.mjs`) is reached through one free Cloudflare quick tunnel shared by every session (`gateway.mjs`: started hidden by the first server, routes by call token, log in `gateway.log`), and posts a "Claude is calling" alert to a private ntfy topic. The **Claude Call** Android app keeps a connection open to that topic and rings with a full-screen call screen. The call page (`page.html`, built from `call-ui/` with `cd call-ui && npm install && npm run build`; its call logic is `call-engine.js`) plays Claude's voice and records you. Your speech is transcribed on this PC by Whisper (`asr.mjs`), which uses what Claude just said as context so names are heard correctly.

**Voice:** your own ElevenLabs clips (`voice/*.mp3`) are cloned on the GPU by Qwen3-TTS Base through `faster-qwen3-tts` (`voice/voice_server.py`, venv in `voice/.venv`; the model is read from ThePipeline). The same GPU server (`voice/voice_server.py`) also runs the hearing, with Whisper large-v3-turbo. The clip in use is set in `voice/voice.json`.

Timing:
- The server uses about 7.2 GB of VRAM. One copy is shared by all Claude sessions, and it exits after 30 idle minutes.
- A call first loads the server (about 40–70 s if it isn't already running) and prepares the first line. Only then does the phone ring, so there is no wait after answering.
- After that, Claude's voice is streamed while it is made: the first sound reaches the phone about 0.2 s after a line is ready (the app plays it natively on the earpiece). Captions appear in step with the voice.
- While you talk, your words show live (Whisper every ~0.9 s). Your turn ends 0.7 s after you stop if what you said reads as finished, otherwise after 1.8 s (or press Done talking).
- Interrupt detection uses loudness and Whisper word confirmation to reject Claude's echo and "mm-hmm". Quiet human interruption on the earpiece still needs verification.
- Debug: `"debug": true` in `config.json` (or `CALLME_DEBUG=1`) logs the phone's mic levels, echo level and interrupts to `timing.log`.

Fallbacks: Kokoro for the voice, small Whisper on the CPU for hearing, and the phone's own voice.

**Tools:** `call_user`, `continue_call`, `end_call`. Registered for all projects: `claude mcp add --scope user callme -- node B:\Projects\CallMe\server.mjs`.

**Test:** `node test-call.mjs` runs a whole call against the real call page in headless Edge with a fake mic (`phone-sim.mjs`). Private test topic, no tunnel, own status file: it never rings the real phone. `node test-call.mjs fallback` checks the no-GPU voices.

**Known issues:** the phone runs hot and feels laggy during calls (call screen drawing cost). Quiet interruptions on the earpiece are unverified.

**Android app:** `android/` (Kotlin, no libraries). Build with `gradlew assembleRelease`, then install with `adb install -r ClaudeCall.apk`.
