# Claude Call: handoff (2026-10-17)

Written for whoever carries on (GPT/Codex). Read this all before touching anything.

## 1. Where things are

| What | Where |
|---|---|
| Newest code (phone + PC) | `B:\Projects\CallMe-desk`, branch `feat/desktop` (pushed). Head `099c886`. |
| What it is built on | `feat/picks` (`47a540d`, the build now on Mani's phone) + `feat/both-ring` (`fa875dd`) merged in |
| Main repo checkout | `B:\Projects\CallMe` (branch `main`, old). Live PC server and `config.json` live here. |
| Other worktrees | `B:\Projects\CallMe-*` (chats2 = feat/picks, ring = feat/both-ring, drum/caps/ghost/qol3/bugs/... are old and merged). Safe to remove later with `git worktree remove`. |
| Installed PC app | `B:\Projects\CallMe\bin\ClaudeCall.exe` (Start menu "Claude Call", auto-start via `HKCU\...\Run\ClaudeCall`). Data: `%LOCALAPPDATA%\ClaudeCall` (prefs.json, chats, media, fonts). Currently NOT running (Mani asked for silence); it starts at next login. |
| Phone app | `fyi.mani.callme` on Samsung SM-S938B. Has the `feat/picks` build. Does NOT have `feat/desktop` yet. |
| Plans/design | `D:\Outputs\CallMe\desktop\plan.md` (PC port plan + Mani's answers), `D:\Outputs\CallMe\design\...`, `D:\Inspiration\Alive-2026-10-17\README.md` (approved alive behaviours), `D:\Outputs\CallMe\cleanup\app-open.md` (21 deferred refactors) |
| Previews | Alive picks: https://claude.ai/artifact/9enRYPqQCQ4ZJnTMxRfmrH · Effects picks: https://claude.ai/artifact/McxA5jgWqdqwmuA3HUiTPj |

## 2. Rules Mani cares about (do not break)

- No Android emulators, ever. Verify with builds, host tests, and installs on his real phone.
- Never touch the phone without him saying it's free ("install" / "phone's free"). Install keeps his data (`install -r`), never a separate dev app.
- PC tests run quietly: windows on the second monitor (DISPLAY2, X=2560 Y=-592 1080x1920) or hidden. Never move his mouse, press keys, steal focus, or keep windows on top. Drive test windows with `PostMessage` only (`D:\Outputs\CallMe\desktop\drive.ps1` has Click/Move/Key/Wheel/Shot helpers).
- No console windows or pop-ups while he's using the PC ("pls silent").
- Every motion or animation change needs a clickable preview (artifact/web page) and his explicit OK before building. Small layout tweaks: build the recommended one and mention alternatives.
- Push back on AI-slop looks (libraries.dev orbs/beams, particle dust, dot grids, gradient waves, stock glows). He wants bespoke, character-driven, dark, premium, restrained.
- Code style: near comment-free, smallest complete change, no AI co-author lines in commits.
- PC overheats: keep cargo at `-j 6` or less, gradle `--max-workers=4`, stop gradle daemons after (`gradlew --stop`). Code on B:, old stuff/backups on D:, nothing project-related on C:.
- Contact: urgent → Discord only. Keep messages short with a clear answer to give.

## 3. Build, test, install commands

All cargo commands in PowerShell (git-bash has no cargo):

```powershell
$env:PATH = "$env:USERPROFILE\.cargo\bin;$env:PATH"
Set-Location B:\Projects\CallMe-desk
# PC app
$env:CARGO_TARGET_DIR = 'B:\Projects\CallMe\target-desk'
cargo build -p callme-phone --bin callme-desk --release --target x86_64-pc-windows-msvc -j 6
cargo test -p callme-phone --lib --release --target x86_64-pc-windows-msvc -j 6   # 36 tests incl. WASAPI + image decode self-checks
# shared logic + server tests
cargo test -p hosttest -p callme -j 6                                              # 36 + 1
# phone compile check (no device needed)
$env:CARGO_TARGET_DIR = 'B:\Projects\CallMe\target-desk-android'
$env:ANDROID_JAR = "$env:ANDROID_HOME\platforms\android-36\android.jar"
cargo ndk -t arm64-v8a -P 29 check -p callme-phone --release -j 6
```

Install the PC app: stop `ClaudeCall` if running, copy `B:\Projects\CallMe\target-desk\x86_64-pc-windows-msvc\release\callme-desk.exe` over `B:\Projects\CallMe\bin\ClaudeCall.exe`. Only one copy can run (named mutex `Local\ClaudeCallDesk`), so stop the installed one before a test run. Test runs: set `CALLME_DESK_DATA` to a temp dir and `CALLME_CONFIG` to a missing file (no real ntfy traffic), and pre-write `prefs.json` with `{"home":{"x":2900,"y":-200}}` so the window opens on monitor 2.

Install on the phone (only when Mani says it's free):
1. `android/app/src/main/res/values/topic.xml` in the worktree is a dummy. Copy the real one from `B:\Projects\CallMe\android\app\src\main\res\values\topic.xml` over it (keep a copy of the dummy).
2. `$env:CARGO_TARGET_DIR='B:\Projects\CallMe\target-desk-android'; cd android; .\gradlew.bat assembleRelease --max-workers=4; .\gradlew.bat --stop`
3. `adb -s 'adb-R5CY13S5F8D-7zVWkV._adb-tls-connect._tcp' install -r android\app\build\outputs\apk\release\app-release.apk` (do not launch the app).
4. Put the dummy topic.xml back. Delete the built APK (it contains his private channel key).
5. The worktree needs `android/local.properties` (already copied).

## 4. What is done (this session)

PC app (`phone/src/desk/*`, one shared crate with the phone):
- Windows build with Slint winit + Skia OpenGL; live 3D Clawd/Null drawn on the GPU (GL 3.3 fallback for the ES shaders).
- Portrait 412x892 window, remembers position, close button minimises, single instance, start at login, no console window, dark title bar, Roboto bundled so text matches the phone.
- ntfy listener (asks, updates, mode changes, chat replies), Windows toast pop-ups for chat replies (registered app ID `ClaudeCall.Desktop`; clicking should open the chat, not yet tested).
- Calls: ring window + taskbar flash + looping `Ring05.wav`; uses `http://127.0.0.1:47809` directly when the server is on the same PC; WASAPI audio in "communications" mode so Windows echo cancelling applies; watches `/state?...&ring=1` and closes the ring when another device answers.
- Photos/videos: Windows file picker, shell-thumbnail decode capped at 1800 px (RAM).
- Input: Esc and mouse back button = back, Enter sends (Shift+Enter newline), mouse wheel turns the mode drum, hover no longer counts as a press.
- Memory: ~104 MB on home, ~200 MB with chats open, 8 MB when minimised (working set trimmed), ~0% CPU hidden, ~5% of one core idle visible.

Shared (phone + PC), needs a phone install to ship:
- Both devices ring: server `/state` now returns `answered`/`declined` and wakes long-polls; phone stops ringing when the PC answers (`phone/src/ringwatch.rs`, `ring.rs`, `service.rs`).
- Six approved "alive" behaviours (`cast.rs`, `alive.rs`, `faces.rs`, `home.rs`, `toys.slint`, `home.slint`, `chats/field.slint`): cursor watch, noticing each other (every 45-90 s idle), doze after 2 min idle + startled wake, Call-button hover attention + hop on press, Clawd reads along while typing, rare moments every 2.5-5 min (stretch, Null wipe, shake). Host tests cover doze/wake and idle life.
- Audio and attach code refactored so the phone and PC share buffering and pick logic (Android behaviour meant to be identical, but only compile-checked).

## 5. To do, in order

### 5.1 Ship to the phone (blocked on "phone is free")
- Install `feat/desktop` per section 3.
- Check on the phone at real speed: a full voice call (audio in/out, echo, mute, speaker; the audio refactor touched AAudio setup), attach a photo and a video from Samsung Gallery (attach refactor), drag on characters (pointer change: `move && pressed`), the mode drum swipe, all six alive behaviours, Enter/typing in chats, back gesture.
- Both-ring end to end needs 5.2 first.

### 5.2 Deploy the server change
- `src/call.rs` on `feat/desktop` adds `answered`/`declined` to `/state` and `ring=1` (ring polls don't count as a live call page). The live server is built and run from `B:\Projects\CallMe` (main). Merge `feat/desktop` into main (or cherry-pick `fa875dd`), rebuild `callme`, restart it when no call is active and no other Claude session is mid-call (the `callme` MCP server is used by other sessions; ask Mani before restarting).
- Then test: Claude calls, phone and PC both ring, answer on PC, phone stops; and the reverse; decline on one stops the other.

### 5.3 Live PC call test
- Not yet done. With Mani's OK: make a real call to the PC app (headphones and speakers), check echo, latency, mute/quiet/hang-up, typed replies, the ring window position (it opens at remembered `call` position; first run defaults to monitor 2 because of the seeded prefs, change `%LOCALAPPDATA%\ClaudeCall\prefs.json` if he wants it on the main screen).
- Check a toast click opens the right chat and raises the window.

### 5.4 Icons (none yet on PC)
- PC exe, window, taskbar, toast and Start menu use the default icon. Android uses the vector drawable `android/app/src/main/res/drawable/ic_call_logo.xml`.
- Make a proper multi-size `.ico` (16, 20, 24, 32, 40, 48, 64, 256) from the same mark; embed it in the exe (`embed-resource` or `winresource` build dependency in `phone/build.rs`, Windows only), set the winit window icon (`slint::winit_030` window attributes hook, `with_window_icon`), add `IconUri` under `HKCU\Software\Classes\AppUserModelId\ClaudeCall.Desktop` for toasts, and point the Start menu shortcut at it.
- Icon look is a visual decision: show 2-3 options as a preview first (bespoke, matches the dark/orange Clawd world, not a generic phone glyph).

### 5.5 Expressions (needs preview + approval first)
- States today: Idle, Think, Talk, Happy, Sleep (`phone/src/cast.rs`), eyes/lids/happy driven through `uEyes`/`uMood` uniforms into the SDF shaders (`phone/src/live_gl.rs`, `phone/src/engine/`, art in `art/live3d/`).
- Gaps found while building the alive behaviours:
  - "Wide eyes" (surprise/attention) for the Call hover: shader clamps eye open to 0..1, so it isn't possible yet. Needs an `open > 1` path in the eye SDF.
  - Null "wipes its screen": approximated by switching the LEDs off and fast hand taps. A real screen static/scanline effect on Null's face would read better.
  - Candidates to propose: surprised, worried (for "No answer from the PC", failed sends), listening (during a call while Mani speaks), sleepy yawn (start of doze), proud/pleased after a task finishes.
- Build the preview the same way as the alive picks page (sprites from `phone/ui/toys/*.png`, eyes lifted and redrawn, LEDs redrawn), get numbers back from Mani, then add states/uniforms.

### 5.6 Benchmarks
- Make one repeatable script per platform and save results (CSV) under `D:\Outputs\CallMe\bench\`.
- PC: working set + private bytes + CPU% + GPU 3D% (perf counter `\GPU Engine(pid_*engtype_3D)\Utilization Percentage`) for: home idle 60 s, home with cursor moving, chats list, chat thread typing, a call, minimised. Frame pacing with PresentMon (`-process_name ClaudeCall.exe`) for frame time p50/p95/p99. Baselines now: 104 MB home, ~200 MB chats, 8 MB hidden, 5% of one core idle.
- Phone: `adb shell dumpsys gfxinfo fyi.mani.callme framestats` won't cover a NativeActivity well; use `adb shell dumpsys meminfo fyi.mani.callme`, `top -p <pid>`, and a Perfetto trace (`adb shell perfetto -o /data/misc/perfetto-traces/t.pftrace -t 20s sched freq gfx`) for frame timing; battery via `dumpsys batterystats --reset` then an idle hour.
- Watch heat on the PC while benchmarking (B:\Tools\cpu_watch.py; HWiNFO must run with shared memory on: it currently runs elevated with `SensorsSM` off, so restart it as admin with `SensorsSM=1` in its INI at `C:\Users\abdul\Documents\Codex\2026-10-11\comprehensive-pc-health-check-thermal-audit-3\work\tools\hwinfo\HWiNFO64.INI`).

### 5.7 Making things smooth
- The home tick is a 16 ms Slint timer; characters redraw at 60/20/10 fps by state (`alive::due`). On a 144 Hz PC monitor this caps motion at 60 fps. Consider driving from the display refresh on desktop (or 120 fps when moving), measured with PresentMon first.
- First open of chats warms extra shaders (`s.warm` in `home.rs`); check for a hitch on PC and phone.
- Chats-open RAM (~200 MB): find what loads (Skia caches, face textures, chat thread images) before optimising.
- The window is resizable but only tested at 412x892: either lock the aspect ratio/min size or check every screen at other sizes.
- Drum wheel: one notch = one mode; check it feels right with a high-resolution touchpad (many small wheel deltas could skip modes; may need accumulation).

### 5.8 PC gaps still open
- Chat history doesn't come over from the phone; the PC starts with an empty chat list and only gets new messages. Options: sync via the PC server, or one-time import.
- No drag-and-drop or Ctrl+V paste of images into chats (winit `DroppedFile` + clipboard).
- HEIC photos rejected on PC ("can't be sent from the PC yet").
- No tray icon (by design for now: close = minimise to taskbar).
- Text selection/copy in messages, keyboard shortcuts (new chat, search) not designed for desktop.

### 5.9 Housekeeping
- Merge `feat/desktop` into `main` once the phone install and server deploy check out; delete merged worktrees and old `target-*` caches on B: (several GB).
- 21 deferred refactors: `D:\Outputs\CallMe\cleanup\app-open.md` (each needs a phone check).
- Mani's motion notes file named in his preferences (`C:/Users/abdul/Documents/Codex/2026-09-27/new-chat-3/outputs/My UI and motion preferences.txt`) does not exist on disk. The alive approval is recorded in `D:\Inspiration\Alive-2026-10-17\README.md` instead; ask him where the notes file went.
