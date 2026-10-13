# CallMe

Lets Claude Code phone you when it needs you, for a real two-way voice conversation. Free: no phone number, accounts or API keys.

**How a call works:** the `callme` MCP server (`server.mjs`) is reached through one free Cloudflare quick tunnel shared by every session (`gateway.mjs`: started hidden by the first server, routes by call token, log in `gateway.log`), and posts a "Claude is calling" alert to a private ntfy topic. The **Claude Call** Android app keeps a connection open to that topic and rings with a full-screen call screen. The call page (`page.html`, built from `call-ui/` with `cd call-ui && npm install && npm run build`; its call logic is `call-engine.js`) plays Claude's voice and records you. Your speech is transcribed on this PC by Whisper (`asr.mjs`), which uses what Claude just said as context so names are heard correctly.

**Voice:** your own ElevenLabs clips (`voice/*.mp3`) are cloned on the GPU by Qwen3-TTS Base through `faster-qwen3-tts` (`voice/voice_server.py`, venv in `voice/.venv`; the model is read from ThePipeline). The same GPU server (`voice/voice_server.py`) also runs the hearing, with Whisper large-v3-turbo. The clip in use is set in `voice/voice.json`.

Timing:
- The server uses about 7.2 GB of VRAM. One copy is shared by all Claude sessions, and it exits after 30 idle minutes.
- A call first loads the server (about 40–70 s if it isn't already running) and prepares the first line. Only then does the phone ring, so there is no wait after answering.
- After that, Claude's voice is streamed while it is made: the first sound reaches the phone about 0.2 s after a line is ready (the app plays it natively on the earpiece). Captions appear in step with the voice.
- While you talk, your words show live (Whisper every ~0.9 s). Your turn ends 0.7 s after you stop if what you said reads as finished, otherwise after 1.8 s (or press Done talking).
- You can talk over Claude: clearly loud speech stops it at once; quieter speech stops it as soon as Whisper hears real words (not Claude's own echo or a "mm-hmm").
- Debug: `"debug": true` in `config.json` (or `CALLME_DEBUG=1`) logs the phone's mic levels, echo level and interrupts to `timing.log`.

Fallbacks: Kokoro for the voice, small Whisper on the CPU for hearing, and the phone's own voice.

**Tools:** `call_user`, `continue_call`, `end_call`. Registered for all projects: `claude mcp add --scope user callme -- node B:\Projects\CallMe\server.mjs`.

**Checks**
- `node speech-check.mjs`: offline checks of audio packet ordering and final transcript reuse. No sockets, models, microphone or notifications.
- `node mic-check.mjs`: offline checks of microphone replacement, failed requests, rapid route changes and call-end cleanup. No device or microphone access.
- `cd call-ui; npm run build; node check.mjs`: offline screen checks in headless Edge with a fake engine and all requests intercepted. Covers interrupted captions, scrolling, long replies at three viewport heights, glow isolation, controls and closing; no phone or call service involved.
- `node test-call.mjs`: a whole call against the real call page in headless Edge with a fake mic (`phone-sim.mjs`, started by the test). Private test topic, no tunnel, its own status file: it never rings the real phone. Prints the latencies. `node test-call.mjs fallback` checks the no-GPU voices.
- `node asr-check.mjs`: checks that context fixes misheard names.
- `node voice/server_check.mjs`: checks the voice server's speed and audio.

**Issue status (from the first real call on the S25, 2026-10-13; recording `Downloads\Screen_Recording_20261013_141027.mp4`)**
Status: the user subsequently authorized S25 tests. The normal app was updated without clearing data. Final native recordings retained Clawd and the controls in all 2,820 scripted frames and showed no unexplained disappearance in 2,586 real-audio frames. The keyboard now resizes the page and leaves the reply field visible. Native playback completion no longer waits another clip duration after sound finishes. See `CODEX-HANDOFF.md` section 3e for evidence and limits, including six transient drawing warnings during the real-audio/keyboard run. Quiet human interruptions and end-to-end conversation latency still need a human call.
- Call screen (`call-ui/`):
  - Interrupted captions now keep only the spoken words, cap the bubble height and preserve scrolling. Offline layout checks pass.
  - Clawd/control flicker: reproduced drawing-memory exhaustion on the S25. The glow stays separate, its full-screen stroke is removed, and distortion mirrors are disabled. The main animated glow remains. Final frame review passed; this is not a universal frame-rate guarantee.
  - Keyboard overlay: native window insets resize the WebView itself; the compact typing layout keeps captions scrollable above the field.
  - The empty "Claude" label stays hidden until its first words arrive.
- Turn handover took 3.3 s even when reusing the live transcript; it should be about 0.3 s.
- Quiet earpiece interruptions remain unverified with human speech. Automatic checks received live mic frames during playback; that does not establish quiet-speech sensitivity.
- The route stayed earpiece→speaker→earpiece as selected during the automatic check. Changing Speaker replaces the microphone capture so the S25 actually applies the new echo-cancellation setting; the previous capture stays live until its replacement is ready.
- Both GPU and CPU Whisper paths now receive the vocabulary hint (Claude, Clawd, Codex, call screen…). Human recognition accuracy remains unverified.

**Android app:** `android/` (Kotlin, no libraries). Build with `gradlew assembleRelease`, then install with `adb install -r ClaudeCall.apk`.
