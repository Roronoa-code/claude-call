# CallMe

Lets Claude Code phone you when it needs you, for a real two-way voice conversation. Free: no phone number, accounts or API keys.

**How a call works:** the `callme` MCP server (`server.mjs`) opens a free Cloudflare quick tunnel and posts a "Claude is calling" alert to a private ntfy topic. The **Claude Call** Android app keeps a connection open to that topic and rings with a full-screen call screen. The call page (`page.html`) plays Claude's voice and records you. Your speech is transcribed on this PC by Whisper (`asr.mjs`), which uses what Claude just said as context so names are heard correctly.

**Voice:** your own ElevenLabs clips (`voice/*.mp3`) are cloned on the GPU by Qwen3-TTS Base through `faster-qwen3-tts` (`voice/voice_server.py`, venv in `voice/.venv`; the model is read from ThePipeline). The same GPU server (`voice/voice_server.py`) also runs the hearing, with Whisper large-v3-turbo. The clip in use is set in `voice/voice.json`.

Timing:
- The server uses about 7.2 GB of VRAM. One copy is shared by all Claude sessions, and it exits after 30 idle minutes.
- A call first loads the server (about 40–70 s if it isn't already running) and prepares the first line. Only then does the phone ring, so there is no wait after answering.
- After that, each sentence takes about 1.5 s to say, and hearing a reply takes about 0.6 s.

Fallbacks: Kokoro for the voice, small Whisper on the CPU for hearing, and the phone's own voice.

**Tools:** `call_user`, `continue_call`, `end_call`. Registered for all projects: `claude mcp add --scope user callme -- node B:\Projects\CallMe\server.mjs`.

**Checks**
- `node test-call.mjs` uses a private test topic, so it never rings the real phone. Run it with `node phone-sim.mjs <link>` acting as the phone. Set `CALLME_TOPIC` to the topic in `config.json` to ring the real phone.
- `node asr-check.mjs`: checks that context fixes misheard names.
- `node voice/server_check.mjs`: checks the voice server's speed and audio.

**Android app:** `android/` (Kotlin, no libraries). Build with `gradlew assembleRelease`, then install with `adb install -r ClaudeCall.apk`.
