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

**Checks**
- `node speech-check.mjs`: offline checks of audio packet ordering and final transcript reuse. No sockets, models, microphone or notifications.
- `node mic-check.mjs`: offline checks of microphone replacement, failed requests, rapid route changes and call-end cleanup. No device or microphone access.
- `node turn-check.mjs`: actual engine/CallView with fake I/O and a controlled clock; followups during thinking, protected live captions, ordered submissions, latest held answer, timeouts and hangup/mute cleanup.
- `cd call-ui; npm run build; node check.mjs`: offline screen checks in headless Edge with a fake engine and all requests intercepted. Covers interrupted captions, scrolling, long replies at three viewport heights, glow isolation, controls and closing; no phone or call service involved.
- `node test-call.mjs`: a whole call against the real call page in headless Edge with a fake mic (`phone-sim.mjs`, started by the test). Private test topic, no tunnel, its own status file: it never rings the real phone. Prints the latencies. `node test-call.mjs fallback` checks the no-GPU voices.
- `node asr-check.mjs`: checks that context fixes misheard names.
- `node voice/server_check.mjs`: checks the voice server's speed and audio.

**Issue status (from the first real call on the S25, 2026-10-13; recording `Downloads\Screen_Recording_20261013_141027.mp4`)**
Status: the normal S25 app was updated without clearing data, but the user still reports lag. The user stopped this session to hand work to Claude; the current batch remains uncommitted. Native keyboard transitions passed geometry and a 1,464-frame review without foreground loss, but that review found brief keyboard overlap. A final clearance adjustment passes geometry; its visual clearance and human-call acceptance remain pending. See `CODEX-HANDOFF.md` section 3f for the exact handoff and evidence.
- Call screen (`call-ui/`):
  - Interrupted captions now keep only the spoken words, cap the bubble height and preserve scrolling. Offline layout checks pass.
  - Clawd/control flicker: the glow stays separate, its full-screen stroke is removed, and distortion mirrors are disabled. Earlier native frame reviews passed those replay scenarios. The newer user recording shows keyboard jumps and detached icons, so the UI is not declared fully fixed.
  - Keyboard: the WebView/glow viewport now stays fixed while native IME insets resize the foreground. Closing waits for its restored button position; the 200ms focus delay is removed. All three native geometry checks pass, with zero tile warnings; final frame review is pending.
  - The empty "Claude" label stays hidden until its first words arrive.
- In the later human call, final transcription took 195ms and voice preparation 339ms; the 40.978s pause before the next prompt was Codex's own orchestration delay. It is not an app response-time measurement. The engine now keeps capturing spoken followups during thinking and holds answers until user speech finishes; offline checks pass, real-call verification remains pending.
- Quiet earpiece interruptions remain unverified with human speech. Automatic checks received live mic frames during playback; that does not establish quiet-speech sensitivity.
- The route stayed earpiece→speaker→earpiece as selected during the automatic check. Changing Speaker replaces the microphone capture so the S25 actually applies the new echo-cancellation setting; the previous capture stays live until its replacement is ready.
- Both GPU and CPU Whisper paths now receive the vocabulary hint (Claude, Clawd, Codex, call screen…). Human recognition accuracy remains unverified.
- Home, Back and the launcher now retain the active call page and microphone; repeated native checks pass. The ongoing notification has Return/End actions and a timer. Live Update promotion is confirmed on the S25, using a BigTextStyle fallback because its framework excludes CallStyle from promotion. Lock-screen appearance and human Return/End acceptance remain unchecked.

**Android app:** `android/` (Kotlin, no libraries). Build with `gradlew assembleRelease`, then install with `adb install -r ClaudeCall.apk`.
