# Claude Call: handoff (2026-10-17)

Written for whoever carries on (GPT/Codex). Read this all before touching anything.

## Continuation status (2026-10-17, Codex)

This section overrides the older snapshot below.

### Latest acceptance audit

- Latest shared client passed the cached Android ARM64 compile check: `cargo ndk -t arm64-v8a -P 29 check -p callme-phone --lib --release --locked -j 2`, using `target-desk-android` and the existing Android 36 jar. Completed in 18.73 seconds; one dead-code warning for the test-used `apply_reply` wrapper. No APK build, Gradle run, phone connection, install or emulator. This supersedes the earlier statement that the latest shared changes had not been Android compile-checked.
- Installed PC source is delivered; core chat, UI and motion code is shared, with target-specific OS adapters. Android compilation plus Windows checks does not establish equal runtime smoothness or update the installed phone.
- PC client remains stopped. Server health responds and reports a tunnel URL; no live call or notification was triggered. The expression preview and six-behavior approval README still exist; the named original motion-notes file remains absent.

| Handoff item | Current result / remaining gate |
| --- | --- |
| 1. Phone | Earlier authorized install completed; latest instruction prohibits further phone work. Device acceptance deferred. |
| 2. Stop other ringing | Server patch deployed and integration-tested; two-device real acceptance still needs permitted device use. |
| 3. Real PC call | Not done: would interrupt the user's video; no new permission to call. Audio, echo, latency, typed reply, mute/hang-up and toast-click checks remain. |
| 4. Icons | Approved option installed and inspected for exe/window/shortcut; toast registration set, actual notification appearance/click deferred. |
| 5. Expressions | Six previews prepared and offline-checked; no approval to change native motion/shaders. |
| 6. Speed | Quiet PC CPU/memory samples saved. Frame timing blocked by trace privileges; thermal telemetry unavailable. Phone benchmark prohibited. |
| 7. Smoothness | Home/chat minimum and wide layouts checked. Call resizing, real high-resolution wheel behavior and frame pacing remain. Existing shared 60 fps active cap unchanged. |
| 8. PC gaps | Drop/paste and HEIC implemented; codec/unit checks passed. Native drop/paste interaction still needs acceptance. Signed recent-history mirroring verified with synthetic data; full old phone history requires a separate transfer/device access. |
| 9. Cleanup | Eleven merged worktrees and fourteen old caches archived/cleared, about 43 GB recovered. Main merge is gated on device acceptance. Unmerged previews/local changes kept. The 21 listed refactors explicitly require phone checks, so remain deferred. |

No new question was sent: the user explicitly asked to stop repeated questions while watching a video. Remaining acceptance requires a change in that interruption/device boundary, motion approval, or trace/thermal availability. Do not repeat unchanged successful checks or treat compile results as runtime acceptance.

### Performance and cleanup audit

- Cleared fourteen retired Cargo cache directories after preserving 542 custom files/final binaries in `D:\Backups\CallMe\old-cache-extras-20261017\preserved.tar` (SHA-256 `d62624d9e84a8ff011656995c7497e31407b0da0a08463a88929938def1a9f6d`). Retired names: target-bugs, call2, caps, chats2, chatstest, chatstest-b2, drum, ghost, nullface, qol3, qol3test, receipts, ring-android and typing (each with the `target-` prefix). Custom APKs, scripts, screenshots/recordings/logs and direct release/debug EXE/DLL/SO/APK outputs are preserved. Regenerable compiler output was deliberately discarded. Manifest records original file inventories and preserved hashes; source inventory, archive and preserved source hashes were rechecked before native PowerShell deletion, with path/reparse/process guards. About 40 GB more free space on B:; no phone/emulator or active build was used.
- Retained current/shared `target`, `target-desk`, `target-desk-android`, `target-ring`, plus the unmerged-preview `target-controls2` and retained-checkout `target-ship`. The installed PC executable still matches its retained release build. No existing GitHub Actions workflows are available; the subsequent cached local Android compile check is recorded above.
- Archived and removed eleven old merged worktrees: bugs, call2, caps, chats2, drum, ghost, nullface, qol3, receipts, ring and typing. Full backup is `D:\Backups\CallMe\merged-worktrees-20261017\checkouts.tar`, SHA-256 `202e2c153d7fb9e926c8cc9b1b4956373c129399bffcfa169f7515393a3d87ca`. All 7,610 files (3,136,969,589 source bytes), including ignored configuration and binaries, were hash-verified against the archive and rechecked before removal. Manifest and restore instructions are beside it. Git branches remain. About 3 GB was recovered on B:.
- Registered worktrees now: main, desktop, controls2 and ship. `CallMe-chats2` has no files or Git registration left, but Windows denies removing its empty directory; leave it until the external handle is released. No processes were stopped for cleanup. `CallMe-controls2` keeps three unmerged preview commits and `CallMe-ship` keeps its untracked build log. Retained build caches are listed above.
- Confirmed both targets compile the same `alive`, `cast`, `home`, `screen`, chat logic, Slint UI and shaders. `alive::due` retains 60 fps while moving, 20 idle and 10 asleep. This is shared implementation, not verified equal device performance.
- PresentMon 2.6.0 from the official release was downloaded to `B:\Projects\CallMe-work\PresentMon-2.6.0-x64.exe`; SHA-256 matches the published digest `B2A706BC6AD475749E3B7E3409263AA1E6906D45BDCF993F6DBC0F660188F1AF`. Its one-second privilege probe failed: `failed to start trace session: access denied`. No frame capture, elevation, UAC prompt, group change or app launch occurred. A future capture needs an already authorized/elevated environment or Performance Log Users membership.
- Wheel diagnosis: `phone/ui/toys.slint` steps a mode on every nonzero vertical scroll event, regardless of delta size. The existing test covers discrete steps, not high-resolution input. Accumulation would change interaction behavior; it remains unapplied pending a preview and approval.
- Removed only `B:\Projects\CallMe-server-deploy`, the clean temporary staging checkout made during this continuation. Verified no untracked/ignored files and identical stable patch IDs for staging commit `9d2ad65` and deployed main commit `e21f60e` before `git worktree remove`. Branch retained.
- Main/desktop merge remains deferred with device acceptance and motion decisions. Old merged checkout configuration and build files are preserved in the verified archive above.
- No phone access, real calls, notifications, clipboard changes, new motion or heavy benchmarks during this audit. Installed PC hash remains `605996DB49F49196FDF82DCEB8F8146723713DE3D272CEB9D3A305CB7FCDCFC3`; app is stopped.

### Expression preview prepared (not approved or applied)

- Self-contained preview: `C:\Users\abdul\Documents\Codex\2026-10-17\i-ve-written-the-handoff-to\outputs\callme-expression-preview.html` (SHA-256 `1EFB325318FA27AE703A485949869ECBAF4B66B44037A0BE21C073493C9B138D`). Static overview beside it: `callme-expression-options.png`.
- Six proposals using the actual `phone/ui/toys` sprites and the existing approved preview's eye-lifting method: surprised/wide eyes, worried, listening, sleepy yawn, a proper downward screen wipe for Null, and pleased. Each has subtle/expressive variants, play/pause/reset, neutral comparison and a frame scrubber. The yawn proposal explicitly adds a small mouth; it is not an approved character change.
- No autoplay; playback stops when hidden or finished. Continuous eye shapes avoid threshold jumps. Nothing was added to the approved Alive collection, native shaders, shared animation scheduler or installed app.
- `B:\Projects\CallMe-work\check-expression-preview.cjs` runs the exact preview script offline with the installed `@napi-rs/canvas` package. All 12 variants, 48 intermediate-frame checks, neutral end-state equality, and control handlers passed. The rendered overview was visually inspected.
- Browser verification remains incomplete: the browser security policy rejected the local `file:` URL and explicitly prohibited workarounds. No alternate protocol/server/browser was used to bypass that decision. Offline rendering is not a claim of browser layout or native motion acceptance.
- User has not approved these new proposals. Keep them separate; the handoff's explicit approval-before-motion rule still applies. Do not send repeated Discord questions while he is watching a video.

### Shared chat receive continuation

- Found the history gap at the shared receive boundary: `net::listen` ignored signed `Chat` messages, while replies required the corresponding outgoing line locally.
- Added HMAC-verified mirroring of outgoing messages through `inbox`/`Store`, preserving IDs, agent/model, original message dates and attachment validation. Imported messages are marked as mirrored and excluded from automatic re-submission/retry; replay does not replace a local draft, photo pick, original outbox state or completed reply. Deleted ordinary chats retain a bounded tombstone so cached messages cannot recreate them.
- A first start now asks for up to 12 hours of cached events rather than 75 seconds. Historical replies/notes do not produce toast notifications, and retain their original dates. Calls retain the existing 75-second freshness gate. Subsequent starts retain the bounded cursor behavior.
- This is **recent-history catch-up and ongoing mirroring**, not a complete old-history migration. Events outside the relay cache, expired file links, and phone-only drafts/history still need a separate transfer. No phone was accessed. The receiving fix is shared source code, but only the PC build is being installed; the installed phone will not gain this new receive behavior until a later authorized update.
- Verification: 26 shared chat host tests passed, including existing storage compatibility, signatures, deletion/undo, attachments and the new mirrored-message cases. The first-start cursor check passed on Windows. An isolated Windows inbox integration test rejected a forged message, accepted the signed message/reply once, persisted them without notifications, and retained the historical reply date. The native app displayed this synthetic history on monitor 2; thread layouts at minimum and wide sizes were inspected, with foreground unchanged. No live chat was sent and no real account history was fetched.
- Synthetic fixture and quiet UI check: `B:\Projects\CallMe-work\sync-check` and `check-sync.ps1`. The ignored inbox test deliberately requires a fresh `chats.json` location and the literal test key, so it cannot overwrite an existing history fixture accidentally.
- Final release build succeeded and was installed without launching the normal app. Current build/installed SHA-256: `605996DB49F49196FDF82DCEB8F8146723713DE3D272CEB9D3A305CB7FCDCFC3` (36,935,680 bytes), superseding the earlier attachment-build hash below. Desktop app remains stopped. Shared motion and shaders were not changed.

- Latest user direction: **work on the PC only; leave the phone alone. Keep features and animation behavior shared between platforms.** He is watching a video: no repeated questions, calls, pop-ups, focus changes or physical input. Do not ask again about the completed approvals below.
- Shared client remains one `callme-phone` crate: `home`, `screen`, chat/store, attachment state, `alive`, `cast`, `faces`, Slint UI and shaders serve both targets. Native OS input, notifications, audio and image decoding stay in `phone/src/desk/`. No new animation, expression or refresh-rate changes in this continuation.
- Phone install was completed earlier with explicit permission, using `install -r`; no launch or tests. APK SHA-256 `BD7D4AF644C567468B8C34FBFFEBDE61E305EF2652B7E62FED5A44963A870EED`. Private topic restored to the dummy afterward, built APK removed, Gradle stopped. **No further phone work is authorized now.**
- Server ring state/wakeup fix deployed from main commit `e21f60e` and pushed. Integration coverage includes answer/decline waking ring long-polls, auth rejection and watcher accounting; seven server tests passed. Installed server hash `30C60013D5F5F89079B1586A34A67D3C7ABB2A80D7630B7D5B05457BBC1823FA`. Gateway/tunnel healthy and a fresh stdio MCP initialize/contact-mode check passed. Old MCP sessions disconnected by the approved restart need reconnecting; do not restart Codex. No actual call made.
- Icon option 3 (close-up Clawd) was explicitly approved through Discord. Commit `86cddba` pushed: multi-size exe icon, window/taskbar icon, toast registration and Start menu icon. Actual window and extracted exe icon inspected. No toast was displayed.
- Windows file drop and Ctrl+V photo/file paste now use the shared attachment loading/result/store flow. Text-only clipboard paste remains with Slint; attachments are accepted only in an editable open chat. One attachment at a time; existing 10 MiB limit remains. Picker, drop and file paste share `read_file`.
- HEIC/HEIF now converts through Windows image decoding to JPEG quality 90 with the same 4096-edge sampling policy as Android. EXIF orientation and sRGB conversion are applied. Requires installed Windows HEIF/HEVC support; errors are shown in the existing attachment UI. Both extensions exist on this PC. Tested against `strukturag/libheif`'s public `examples/example.heic` (SHA-256 `7F8B363E4936C0666A25F64F3A92FDA10BD8E5453BE4592530B65A55DD98F3F2`), plus synthetic scale/orientation, malformed input, clipboard RGBA and file-size cases. Fixture is in `B:\Projects\CallMe-work`, not committed.
- Windows suite before HEIC: 41 passed. Focused suite after HEIC: all five passed including the normally ignored native-codec fixture test. Final release built successfully and installed; source and installed SHA-256 both `A757ADE15E1785B50C9847D432C04A5E66A97000E765EABC56CCD81C26E9D2AD` (36,897,792 bytes). Native Explorer drop / Ctrl+V end-to-end remain unchecked to avoid changing the user's clipboard or using real input.
- Resize check found clipping/overlap at 320x480 outer size. Added desktop-only minimum 412x600 logical client size in the shared UI. Verified Windows min-track bounds 428x639 outer; home screenshots at 428x639, 760x639 and 428x931 are usable. Chat/call resizing remains unchecked. One initial resize-test launch briefly became foreground despite `Start-Process -WindowStyle Hidden`; stopped it and corrected the Windows attributes hook to open isolated `CALLME_DESK_DATA` runs without activation. Retest confirmed foreground unchanged both on launch and throughout resizing. Do not rely on the shell Hidden option alone.
- Repeatable resource sampler: `scripts/bench-desktop.ps1 -AppProcessId <pid> -Scenario <name> -Seconds 60`. CSVs in `D:\Outputs\CallMe\bench`. Offline isolated test app on monitor 2, never foreground: home (60 s) 106.71 MiB working set / 110.88 MiB private / 5.81% of one core; chat open (60 s) 188.89 / 224.15 / 3.64%; minimized (15 s) 1.35 / 222.83 / 0.10%. Working-set trimming does not free the private allocation. These are passive resource readings, not frame pacing or a thermal verdict.
- Temperature reader returned unavailable; existing elevated HWiNFO has shared memory off. Did not restart it or trigger UAC. Builds limited to two Cargo jobs. No stress test, PresentMon capture or phone benchmark.
- Contrary to the old snapshot, default `%LOCALAPPDATA%\ClaudeCall` and the startup registry entry were absent when checked. Install now seeds only home/call positions on monitor 2 in default prefs and registers the installed executable at login. Start menu target/icon and matching installed bytes verified; toast IconUri restored to `B:\Projects\CallMe\bin\claude-call.png`. App left stopped. Test runs used isolated data/config; no personal chat history was copied or deleted.
- Missing motion notes were not found in the named path or usual project/output/backup locations. The six-behavior approval still exists at `D:\Inspiration\Alive-2026-10-17\README.md`. No replacement notes were invented.
- Remaining: real call/audio/ring and toast-click acceptance when interruption is welcome; native drop/paste interaction acceptance; frame pacing/high-refresh assessment; approval of prepared expression previews; high-resolution wheel accumulation; history older than the relay cache; main/desktop merge and remaining cache cleanup. The 21 refactors require phone acceptance, so leave them deferred. Worktree cleanup is recorded above; archived data remains recoverable.

Native codec check (no phone, no UI):
```powershell
$env:CARGO_TARGET_DIR='B:\Projects\CallMe\target-desk'
$env:CALLME_HEIC_FIXTURE='B:\Projects\CallMe-work\example.heic'
cargo test -p callme-phone --lib --release --target x86_64-pc-windows-msvc -j 2 desk_attach::tests -- --include-ignored
```

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
