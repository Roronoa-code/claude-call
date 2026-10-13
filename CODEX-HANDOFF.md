# CallMe: handoff to Codex

Written 2026-10-13 by Claude. The user ran out of Claude usage and will finish the open issues with Codex. Read this, then `README.md` (how it works) and `LAPTOP.md` (second machine).

## 1. What this is

Claude (and Codex) can phone the user for a real two-way voice conversation when they need input. Fully free: no phone number, no accounts, no paid APIs.

- The phone rings like a real call (Android app **Claude Call**, package `fyi.mani.callme`, Samsung S25).
- Claude speaks in the user's **own cloned voice** (made from their ElevenLabs clips, cloned locally on the RTX 5080).
- The user's speech is heard by **Whisper** on the PC GPU, prompted with what Claude just said.
- Earpiece by default (Speaker toggle), live captions both ways, and the user can **interrupt** Claude mid-sentence.
- Call screen is design **"D. Your mix"** with the **Clawd** pixel mascot (Playful/Wild mix). The user approved this look. Do not restyle it without showing previews first (see section 8).

Related features that exist and work:
- **Contact mode** (`call` / `text` / `both` / `none`) in `B:\Projects\CallMe\status.json`, enforced for every agent. Shown by the **Aura** home-screen widget on the phone and synced over ntfy.
- **Two-way Discord**: the user can message the bot anytime and a headless Claude (or `codex exec` for "codex ..." messages) acts. Lives in `B:\Projects\Discord Input MCP` (Scheduled Task "Discord Claude Watcher"). Owned by the separate "Claude CLI update" session. Leave it alone.
- **Codex can call too**: `C:\Users\abdul\.codex\config.toml`, `[mcp_servers.callme]`, env `CALLME_AGENT="Codex"` and `tool_timeout_sec = 600`. **Keep both.** Backup in `D:\Backups\Codex config\`.

## 2. Architecture (one call, start to finish)

1. An agent calls the MCP tool `call_user` in **`server.mjs`** (stdio MCP server). Tools: `call_user`, `continue_call`, `hold_call`, `end_call`, `get_contact_mode`, `set_contact_mode`.
2. `server.mjs` checks the contact mode, registers with **`gateway.mjs`**, and prepares the first line of speech.
3. **`gateway.mjs`** owns ONE Cloudflare quick tunnel shared by every session and proxies by call token. It waits 45 s for a new tunnel to settle (phone DNS lags), retries on HTTP 429, exits after 2 h idle. (Per-server tunnels hit 429 rate limits, which is why it exists.) Log: `gateway.log`.
4. The phone is rung through a private **ntfy.sh topic** (title must be exactly `Claude is calling`; body `<AGENT> needs you`). The topic is in gitignored `config.json` and the app's gitignored `topic.xml`.
5. The Android app's **ListenService** (foreground service) streams that topic; **Ringer** posts a full-screen CallStyle notification, plays ringtone, vibrates. Answering opens **CallActivity**, a WebView of the call page.
6. The call page is **`page.html`**, built from `call-ui/` (React + voice-glow + thinking-orbs). Its logic is **`call-engine.js`** (captions, end-of-turn, interrupt detection, streaming playback).
7. **Voice and hearing** run in **`voice/voice_server.py`** (pythonw, hidden, shared by all sessions, exits after 30 min idle, ~7.2 GB VRAM):
   - TTS: faster-qwen3-tts (Qwen3-TTS Base 1.7B) cloning the voice in `voice/news.mp3` (chosen in `voice/voice.json`), streaming through `/stream`.
   - STT: Whisper large-v3-turbo through `/stt` (final) and `/partial` (live captions).
   - Fallbacks, in order: ElevenLabs (env keys, used on the laptop), Kokoro, CPU Whisper (`asr.mjs`), phone browser speech.
8. **Native audio on the phone** is `CallAudio.kt` (a JS bridge): AudioTrack streaming on the earpiece, `MODE_IN_COMMUNICATION`, a route watcher, proximity wakelock. The tunnel buffers about 128 KB, so voice is streamed in rolling pieces.

Timing rules the engine uses today: live captions every ~0.9 s; the user's turn ends 0.7 s after they stop if the sentence reads finished, otherwise 1.8 s (or the Done button); interrupt detection uses echo-relative loudness thresholds plus word confirmation (so Claude's own echo and "mm-hmm" don't cut it off).

## 3. File map

| Path | What |
|---|---|
| `server.mjs` | MCP server, contact-mode gating, speech parts/streaming, pending queue, `timing.log` marks, ElevenLabs fallback. Keep the `CALLME_AGENT` label and the skip-skills/skip-files instruction lines (another session added them). |
| `gateway.mjs` | Shared tunnel + token proxy |
| `call-engine.js` | Call page logic (client side API) |
| `call-ui/` | Source of the call screen. `cd call-ui && npm install && npm run build` writes `page.html`. **Edit `call-ui/`, never `page.html` directly.** |
| `page.html` | Build output |
| `voice/voice_server.py` | GPU TTS + STT. venv in `voice/.venv` (needs transformers 5.15.1) |
| `asr.mjs` | CPU Whisper fallback |
| `test-call.mjs`, `phone-sim.mjs` | Full call test in headless Edge with a fake mic (see section 6) |
| `asr-check.mjs`, `voice/server_check.mjs` | Self-checks for hearing and voice speed |
| `speech-check.mjs`, `call-ui/check.mjs` | Offline packet/transcript and screen regression checks, with fake I/O and no call services |
| `design-preview/` | Preview site for UI alternatives (`npm start`, http://127.0.0.1:5178) |
| `android/` | Kotlin app, no libraries: `ListenService`, `Ringer`, `CallActivity`, `CallAudio`, `ModeWidget`, `MainActivity`, `Receivers` |
| `README.md` / `LAPTOP.md` | How it works / second-machine setup |

## 3b. Claude status update (historical, 2026-10-13; current status is in 3c)

Claude then edited code for items 1, 4 and 8. **Written and built, NOT tested** (the user said not to run tests):
- **Item 8 (Whisper vocabulary):** `VOCAB` in `voice/voice_server.py` is now always put in front of the prompt in `hear()`. Needs a voice-server restart to apply (it is a long-running hidden pythonw process). Then check with `node asr-check.mjs`.
- **Item 1 (giant bubble):** `call-ui/src/call.css` (`.d-thread .d-bubble.claude`: max-height 190px, scrolls, hides `.w.unsaid`) and a scroll-to-bottom `useLayoutEffect` in `call-ui/src/callD.jsx`. `page.html` was rebuilt.
- **Item 4 (empty caption):** `caption()` in `callD.jsx` adds tone `pre` until the first word is heard, and `.d-label.pre { visibility: hidden }` in `call.css`.
- **Item 7 (route flip), written, not built or installed:** `CallAudio.kt` route watcher now also reacts when the system clears the device (null), and `route()` re-asserts `MODE_IN_COMMUNICATION`. Needs `gradlew assembleRelease` + install.
- **Item 5 (handover), partly done, untested:** `/speech` in `server.mjs` now uses the live transcript immediately when it already covers the speech, instead of first waiting for an older in-flight partial. If handover is still slow, measure with `timing.log` marks ("turn sent" to "transcribed" to "handed to Claude"); the rest of the 3.3 s may be Claude's own turn time or the `/stt` fallback.
- **Items 2 and 3, finding from the recording** (frames pulled with ffmpeg at 10 fps): the glitch is NOT only Clawd. For single frames the whole `.d-stage` (Clawd, "Claude" label and caption) goes blank while the header, status line, controls and glow stay, roughly every half second during a line. That looks like a compositor/layer flicker on the phone, not an animation bug. Guard added: `.d-stage { isolation: isolate; transform: translateZ(0) }` in `call.css` (page.html rebuilt). UNVERIFIED: it can only be confirmed on the phone with a new screen recording. If it still flickers, next suspects: `.mascot-host { will-change: transform }` plus its per-frame `svg.innerHTML` rewrite in `mascot.jsx`, and the 250 ms `useNow` re-render of the whole screen. The control row was steady in the frames I checked; the "disappears at turn end" part is still unlocated.
- **Item 6, one cheap try written, UNVERIFIED:** `call-engine.js` now opens the mic with `echoCancellation: false` inside the app on the earpiece, and turns it on again (via `applyConstraints`) when Speaker is chosen; in a plain browser it stays on. Idea: the browser echo canceller may eat quiet speech while Claude plays. If the mic is still about 0 during playback on the earpiece (check `rms` with debug on), the mute is the Samsung call-mode hardware path, and the next things to try are `AudioSource.VOICE_RECOGNITION` / `UNPROCESSED` capture natively in `CallAudio.kt` and sending that audio to the page, or accepting loud-only interrupts on the earpiece.
- **Still open:** real-phone verification of 6, the "control row disappears at turn end" part of item 3 (not seen in the frames I checked), and verifying everything above on the real phone.
- Claude subsequently committed and pushed these changes in `9941ae4`, added the CPU vocabulary hint in `86c0f85`, and updated the README in `38b0f58`.

## 3c. Codex continuation, 2026-10-13

**Current boundary: the user is expecting a call and explicitly prohibited phone tests.** No ADB, Android build/install, real calls, microphone access, model startup, contact-mode changes or service restarts were performed in this continuation. Keep this boundary until the user reauthorizes phone work.

The pasted Claude handoff reports that the app was built and installed over the S25 app, its listener restarted, and the old voice server stopped after the earlier section 3b was written. These are prior-session reports; Codex has not independently checked the device or service state.

Changes made here:
- **Handover:** `/speech` used to remove the utterance from the shared map before waiting for delayed `/listen` packets. Late packets then went into a different utterance, leaving the final request waiting up to three seconds and transcribing gaps. The same utterance now survives through final transcription and is cleaned up in `finally`. Empty final packets preserve their offset, so missing preceding audio is detected. Permanently missing audio fails instead of submitting incomplete speech.
- **Transcript completeness:** live transcripts of incomplete audio are skipped, and rolling 28-second captions record their starting offset. Only a caption covering the whole reply can replace final transcription. A complete available caption still bypasses redundant in-flight recognition.
- **First interrupted frame:** `CallView.status()` marks live Claude words as cut in the same update that enters hearing, before asynchronous playback cleanup. Unspoken words are hidden from the first thread render.
- **Bubble scrolling/layout:** Claude's bubble no longer resets manual scrolling on every 250ms clock tick. Both bubbles can shrink and scroll within the stage; the user's bubble scrolls to new text, and the thinking row retains its height. The approved design/motion remains in place.

Verification completed:
- `node speech-check.mjs` **PASS**: delayed and empty final packets, complete-caption reuse while a newer recognition is pending, long replies, gaps, failure cleanup. Evaluates the actual request handler with fake I/O; no server startup or network.
- `cd call-ui; npm run build; node check.mjs` **PASS**: real built page with mocked engine in headless Edge, all requests intercepted. First interrupted render, hidden initial label, manual scroll retention, long live/final replies at 412x892, 412x740 and 412x640, and retained mascot/control DOM nodes. Browser closed afterwards.
- Read-only second review found no concrete regression in the changed source or checks.

Remaining: the S25 stage/control flicker's cause is still unlocated; continuously mounted DOM nodes do not prove glitch-free phone rendering. Quiet earpiece interrupts, route stability, actual Whisper accuracy and real-call timing are unverified. No claim of reaching the 0.3s handover target. The full audio integration checks were not run because they can use the shared voice service.

Delivery: `page.html` was rebuilt and will be read by an existing call server on the next page load. The `server.mjs` fixes require a newly started CallMe MCP server process; no existing server was restarted. No Android source changed, so this batch needs no APK update.

## 3d. Full-frame flicker review, 2026-10-13

The user supplied `Screen_Recording_20261013_141027(1).mp4` and requested frame-by-frame review. It is byte-identical to the original recording (SHA-256 `47a17d228b45df5cf4c8d296087266eb8d79b45f5e476477707401503e053f29`), so it does not show the newer fixes. All **809 original frames** over 26.703 seconds were extracted with their original timestamps, pixel-screened and reviewed on contact sheets; flagged controls were also reviewed as enlarged crops. No frame-rate resampling was used.

This corrects the earlier 10fps review in section 3b:
- **The controls do disappear:** 26 frames in 14 episodes, including frames 87–88 at 2.156–2.224s and 229–231 at 5.411–5.478s. This is not confined to one turn ending.
- Clawd disappears independently during steady speech as well as during transitions. Orange-pixel screening flags 90 frames across 57 episodes. The whole stage is blank in 13 frames, but other failures remove only parts of the stage or controls.
- Some missing regions have abrupt rectangular edges, including parts of the glow. This supports investigating rendering/compositing, but the recording alone does not identify the responsible layer or establish whether recording contributes.

**Change:** `CallD` now puts VoiceBeam in an absolute background sibling of the foreground. Its per-frame inherited CSS variables previously cascaded through Clawd, captions and controls. Separating them reduced style-recalculation time from 340/372ms to 181/178ms in alternating two-second desktop samples. This is a measured rendering-cost reduction, **not a confirmed fix for S25 flicker**. The existing stage isolation guard remains. The closing selector excludes the background glow, preserving the prior foreground fade.

**Verification:** rebuilt `page.html`; extended `call-ui/check.mjs` passes with all traffic intercepted and engine mocked. It checks glow bounds and cascade isolation, existing caption/layout cases, mute/speaker/captions/typing interactions, and closing. Both the actual screen and the shared design-preview layout retain their foreground geometry; preview screen and glow retain 412x892 bounds and 40px rounding. Frozen before/after images have identical header/mascot/caption/control pixels; minor lower-glow/corner differences remain, so this is not full-image pixel identity.

A separate **pre-separation** desktop replay inspected 1,287 screencast deliveries (1,266 distinct images) over 15.51s without losing mascot/control pixels. It did not reproduce the phone bug; do not use it as proof of a fix. All helper browsers closed. No phone, microphone, voice models or call services were used.

Local evidence is in `D:\Outputs\CallMe\recording-20261013\` (frames, timestamps, metrics, contact sheets, four-frame evidence strips and second-opinion comparisons), with the offline replay in `D:\Outputs\CallMe\offline-flicker-probe\`. Recording/screenshots were not added to Git. Phone confirmation remains deferred under the user's no-phone-test boundary. No APK update is needed for this page-only change.

## 4. Open issues (the work left). Recording: `C:\Users\abdul\Downloads\Screen_Recording_20261013_141027.mp4`

All came from the first real call on the S25. Items 1 to 4 are call-screen (`call-ui/`); no app rebuild needed for those.

1. **Giant bubble after an interrupt.** After the user cuts in, the whole long line, including the words never spoken, becomes one giant Claude bubble that pushes the user's live bubble behind the status line.
   Fix: keep only the *spoken* words in the bubble (the engine already reports `spoken` with a cut index), cap the bubble height, and scroll so the user's bubble always stays visible.
2. **Clawd vanishes for a frame or two**, including during steady speech. The full-frame review and glow separation are documented in section 3d; the S25 result remains unverified.
3. **Control row disappears repeatedly.** The full-frame review found 26 missing-control frames in 14 episodes. Source/offline checks show it stays mounted; glow style updates no longer cascade through it. The phone-specific cause and result remain unconfirmed.
4. **Empty "Claude" caption** flashes before a line's first words. Don't render the caption until there is text.
5. **Turn handover takes 3.3 s** even when it reuses the live transcript; target about **0.3 s**. Trace from "user stopped" (engine) to `/speech` sent to the agent receiving the text to the next line arriving. `timing.log` has marks; `node test-call.mjs` prints "you stopped -> turn sent X ms, transcript back Y ms". Find the first stage that waits longer than it needs to (likely a final `/stt` request being awaited when the last live partial is already complete).
6. **Quiet interrupts fail on the earpiece.** Samsung's call mode mutes the mic (rms about 0) while Claude is playing, so only loud speech gets through (one normal-volume interrupt worked). Needs a real-device experiment: try a different `AudioSource` (e.g. `VOICE_COMMUNICATION` vs `MIC`), mic gain, or different audio mode/route during playback. This needs the phone, not the simulator.
7. **Route flipped earpiece to speaker by itself once.** Look at the route watcher in `CallAudio.kt`; probably a device-change callback that should re-assert the earpiece unless the user chose Speaker.
8. **Mishearing: "call screen" heard as "cold screen".** Add a fixed vocabulary hint (Claude, Clawd, Codex, call screen, ntfy, Discord, ...) to Whisper's prompt in `voice/voice_server.py`. Check with `node asr-check.mjs` and add the failing phrase there.

## 5. Icon work (UPDATE: Codex has since committed it, as `0cf86b4`; this section is history)

Codex (an earlier Codex session) was changing the app icon and left these uncommitted:
- `android/app/src/main/AndroidManifest.xml`: `android:icon` / `roundIcon` now `@drawable/ic_call_logo`
- new `android/app/src/main/res/drawable/ic_call_logo.xml`

They are not mine. Review them, build, and commit them if they are done and look right on the phone; do not discard them.

## 6. How to test (never ring the user's real phone)

- While phone/service work is restricted, use only `node speech-check.mjs` and `cd call-ui; npm run build; node check.mjs`. They use fake I/O and do not load voice models or start call services.
- `node test-call.mjs` runs a whole call against the real call page in headless Edge with a fake mic (`phone-sim.mjs`). It uses a private test topic, no tunnel, and its own status file. It sets `CALLME_STATUS` / `CALLME_LOCAL` so it **never touches the real `status.json` or the real ntfy topic**. Expect PASS plus latency lines.
- `node test-call.mjs fallback` checks the no-GPU voices.
- `node asr-check.mjs` checks that context fixes misheard names.
- `node voice/server_check.mjs` checks voice server speed and audio.
- `CALLME_SHOTS=<folder> node test-call.mjs` also saves a phone-sized screenshot of each call state, useful for items 1 to 4.
- Debug logging: set `"debug": true` in `config.json` (it was removed at wrap-up) or `CALLME_DEBUG=1` to log mic levels, echo level and interrupts to `timing.log`.
- A real call test needs the user's phone and the user. Ask them (Discord, or call them if mode allows). Tests that publish to the real topic previously flipped the user's status to "both", so never publish from tests.

## 7. Build and install the phone app

```
cd B:\Projects\CallMe\android
.\gradlew.bat assembleRelease
.\gradlew.bat --stop
adb connect 192.168.0.210:<port>
adb install -r ClaudeCall.apk
```

- The wireless-debugging port changes every time. Last seen: `192.168.0.210:36555`. Ask the user for the new port (Settings, Developer options, Wireless debugging).
- Install **over** the normal app with `-r`. Do not uninstall and do not make a separate Dev app: the user wants their existing app and its settings kept.
- The build is signed with the PC's `debug.keystore`, which is also backed up in the private repo. A different keystore will fail to install over the existing app.
- `android/app/src/main/res/values/topic.xml` (gitignored) must exist or the app won't build. Template: `android/app/topic.example.xml`.
- Stop Gradle and any emulator when finished. Do not leave heavy processes running.

## 8. User rules that apply here (from their own preference file)

- **Plain, short English. Lead with the answer.** No long reports.
- **No shortcuts.** Verify on the real device at real speed where it matters; don't claim runtime success from a syntax check.
- **UI and motion: show real previews and get explicit approval before building** any new look or animation. The approved call screen design is "D. Your mix". Motion default is "Liquid" (Gentle is the alternative). Previews live in `design-preview/`. Fix visual imperfections (jumps, clipped or orphaned elements, mid-animation mess) before showing anything.
- **Pixel art:** keep Clawd's established pixel rendering.
- **Do not add AI co-author lines to commits.** The user said this explicitly.
- **Windows:** keep background helper shells hidden (the voice server already uses `pythonw`). Run tests quietly: no taking over the real mouse or keyboard, no windows stuck on top. Put test windows on the second monitor if there is one.
- **Drives:** project code and anything read at run time stay on `B:`. `D:` is slow storage for backups and old files. Keep `C:` clean.
- Stop heavy background processes you started as soon as you are done. (The voice server and gateway also exit by themselves when idle.)
- Flag any check that can't fit the time limit; up to five minutes is fine for reliability tests.

## 9. Secrets and repos

- Never commit: `config.json` (ntfy topic), `status.json`, `topic.xml`, `*.apk`, `voice/*.mp3`, `voice/*.wav`, `voice/voice.json`, `timing.log`, ElevenLabs keys. All are in `.gitignore`.
- GitHub (user `Roronoa-code`):
  - `claude-call` is public: code only, no secrets.
  - `claude-call-private` is private: holds `config.json`, `topic.xml`, `debug.keystore`.
  - `discord-input-mcp` is private.
- Baseline before the Codex continuation: `38b0f58`; use `git log` for the latest delivered continuation commit.

## 10. Suggested order

1. Items 1 to 4 together in `call-ui/` (one batch), check with `CALLME_SHOTS` screenshots and `node test-call.mjs`.
2. Item 8 (Whisper vocabulary hint). Small, quick win.
3. Item 5 (handover latency), measured with `test-call.mjs` before and after.
4. Items 6 and 7 together on the phone. They need the user present for a real call.
5. Rebuild and install the app only if `android/` changed (items 5 to 7 and the icon). Items 1 to 4 and 8 need no app rebuild.
6. Update the README "Open issues" section as each one is fixed, then commit and push.
