# Claude: pick up CallMe here

17 October 2026. This is the current handoff. Read this before the older `HANDOFF.md`, which retains historical snapshots and superseded to-do items.

## What Abdul wants now

- **PC only. Leave the phone alone.** No ADB, installs, phone tests or emulators. Keep core feature/UI/motion code shared between Windows and Android, but do not claim equal device performance from shared source.
- His latest request was: “js make a handoff for claude to pick up bruh icl”. Before that: “show me the motion previews then keep it pc only”. The six previews exist; **none of these new motions is approved**.
- Earlier he said: “bro pls stop asking me sm i wanna watch a video just do the work”. Do not send repeated questions/Discord messages or interrupt him with calls, toasts, focus changes or test windows. This was not blanket approval of new motion.
- Read `C:\Users\abdul\AgentPreferences\LEARNING.md` explicitly. Use the installed Ponytail/Caveman guidance and applicable AGENTS instructions. No subagents were used/authorized here.
- Quiet tests: second monitor only; no real mouse/keyboard input, no topmost windows. PostMessage helpers exist, but the latest test's foreground guard failed: **leave live UI tests stopped until an appropriate testing window**.
- No motion/shader/refresh-rate change without a working preview and explicit approval. Small layout fixes are allowed. Preserve user data and unrelated Git changes.
- Cargo maximum 6 jobs; this session used **2**. Gradle maximum 4; stop its daemons after use. Temperature telemetry is unavailable; no stress tests or UAC/HWiNFO restart.
- Do not repeat passed tests or reopen completed work for optional polish. The previous goal was marked **blocked, not complete**, after independent work was exhausted under these boundaries. Do not restart the entire audit loop.

## Repositories and installed app

| Item | Current location/state |
| --- | --- |
| Shared client source | `B:\Projects\CallMe-desk`, branch `feat/desktop`, remote `https://github.com/Roronoa-code/claude-call.git` |
| Last implementation/tooling commit before this handoff | `20b8632` — GPU sampling and benchmark limitations; branch pushed, working tree clean before handoff |
| Main/live server checkout | `B:\Projects\CallMe`, main `e21f60e` — deployed ring wakeup fix |
| Installed PC client | `B:\Projects\CallMe\bin\ClaudeCall.exe` |
| Matching release build | `B:\Projects\CallMe\target-desk\x86_64-pc-windows-msvc\release\callme-desk.exe` |
| Installed/build SHA-256 | `605996DB49F49196FDF82DCEB8F8146723713DE3D272CEB9D3A305CB7FCDCFC3`, 36,935,680 bytes |
| PC data | `%LOCALAPPDATA%\ClaudeCall`; initially absent, seeded only monitor-2 home/call positions. Do not replace it with fixtures. |
| Main config | `B:\Projects\CallMe\config.json`; private, do not print credentials |
| Scratch scripts/fixtures | `B:\Projects\CallMe-work` |
| Test/benchmark results | `D:\Outputs\CallMe\bench` and `D:\Outputs\CallMe\desktop` |
| User-facing artifacts | `C:\Users\abdul\Documents\Codex\2026-10-17\i-ve-written-the-handoff-to\outputs` |

The desktop app was stopped after testing. Startup registry and Start menu shortcut point to the installed executable. Default positions are monitor 2, x2900/y-200; monitor bounds start x2560/y-592. Toast IconUri must point to `B:\Projects\CallMe\bin\claude-call.png` (restored after tests).

The live server health endpoint is `http://127.0.0.1:47810/health`; it responded with a tunnel URL during the last audit. Its keys include `backends`, `bornAt`, `url`, not `tunnelReady`. Multiple `callme.exe` processes are legitimate MCP connections. **Do not restart/kill them casually.** The earlier restart was specifically approved. Existing MCP sessions were disconnected by that restart; a fresh stdio initialize/contact-mode check worked. Do not restart Codex to fix old sessions.

## Delivered and verified

1. **Server ring coordination:** answer/decline wake long polls; `ring=1` polls do not count as live call watchers. Seven server tests passed, including auth and wakeup behavior. Deployed on main `e21f60e`. Installed server SHA-256: `30C60013D5F5F89079B1586A34A67D3C7ABB2A80D7630B7D5B05457BBC1823FA`. No real call/two-device acceptance test was performed.
2. **Approved icon option 3:** close-up Clawd, embedded multi-size ICO, window/taskbar icon, Start menu icon and toast registration. Actual executable/window icon inspected. No toast displayed. Commit `86cddba`.
3. **Windows attachments:** picker/drop/file-paste share `phone/src/desk/attach.rs::read_file`; Ctrl+V handles file/image clipboard data and leaves text-only paste to Slint. Input hooks accept attachments only in an editable, ready open chat. Existing 10 MiB limit retained.
4. **HEIC/HEIF:** Windows BitmapDecoder converts in memory to oriented sRGB JPEG quality 90, bounded to 4096 pixels. Five focused attachment checks passed, including a real HEIC fixture, malformed data and RGBA encoding. Native Explorer drop and actual clipboard interaction remain unverified. Commit `905f6f1` includes these changes.
5. **Desktop minimum size:** 412×600 logical (428×639 outer in tests). Home and synthetic chat layouts checked at minimum/wide sizes. Call-window resizing remains untested.
6. **Shared chat receive:** HMAC-verified mirroring of outgoing chat events, original IDs/dates preserved, no mirrored auto-resubmission, replay preserves local drafts/picks/completed replies, tombstones prevent deleted chats resurrecting. First start replays up to 12 hours of relay cache; old replies/notes are silent, calls retain their 75-second freshness limit. Commit `7b442de`.
   **This is recent-history catch-up, not full old-history transfer.** Expired media, old phone-only history and drafts do not migrate. Only the PC is updated with this receive fix.
7. **Checks:** 26 shared chat tests, Windows cursor check and isolated inbox integration passed. The latter rejected a forged event and saved signed message/reply once with original dates and no notification. Native PC displayed the synthetic conversation. Earlier Windows suite: 41 passed before the HEIC addition, then focused attachment checks passed. Do not present this as a newly rerun full suite.
8. **Cross-target compile:** latest shared client passed `cargo ndk ... check` for Android ARM64 in 18.73 seconds with two jobs. One dead-code warning for test-used `apply_reply`. No device access, APK build or install. Shared source compiles for both targets; runtime equality is unverified.

Core `home`, `screen`, `alive`, `cast`, `faces`, chat/store, Slint screens and shaders remain shared. Windows OS adapters live in `phone/src/desk`. Existing scheduler remains 60 fps moving / 20 idle / 10 asleep. No new native motion was applied.

## Motion previews: next review, not approved work

User-facing output directory contains:

- `callme-expression-preview.html` — self-contained, actual character artwork, no autoplay, play/pause/reset, scrubber and Subtle/Expressive variants.
- `callme-expression-options.png` — static overview.
- Six proposals: **surprised/wide eyes, worried, listening, sleepy yawn, Null's downward screen wipe, pleased**. Yawn introduces a small mouth and is explicitly only a proposal.
- HTML SHA-256: `1EFB325318FA27AE703A485949869ECBAF4B66B44037A0BE21C073493C9B138D`.
- `B:\Projects\CallMe-work\check-expression-preview.cjs` rendered the exact JS offline using installed `@napi-rs/canvas`: 12 variants, 48 intermediate frames, neutral endpoint and control checks passed. This is not browser/native acceptance.

Codex browser automation rejected `file:` navigation and explicitly prohibited alternative-protocol/browser workarounds. It was not bypassed. Abdul was given a direct link to the saved HTML. Do not claim it was opened or approved. His next requested action before this handoff was to see the previews, PC-only.

The existing six “alive” behaviors were approved earlier and are separate. Approval record: `D:\Inspiration\Alive-2026-10-17\README.md`. The old `My UI and motion preferences.txt` at the named September Codex output path was not found. No replacement preference file was invented or memory updated.

## Performance evidence and limitations

- Earlier 60-second samples: home ~107 MiB working set; chats ~189 MiB; minimized 1.35 MiB and 0.10% of one core. Minimized private memory stayed ~223 MiB: working-set trimming is not freed allocations.
- Later 20-second samples: synthetic cursor home 104.35 MiB / CPU 6.07%; first chat-list opening 217.89 MiB / CPU 12.78% (last 10 seconds CPU 1.08%); thread open 222.89 MiB / private 283.94 MiB / CPU 3.09%. CPU percentages are of one core.
- Latest test `check-quiet-performance.ps1` used only isolated `sync-check` data with no network topic. **Typing did not enter the field**; the screenshot and saved empty draft prove that. The CSV was renamed `20261017-203709-211-chat-thread-attempted-typing-no-input.csv`.
- Its final foreground guard failed and stopped the app. No intervening foreground owner was recorded, so cause is unknown. Do not claim focus remained unchanged or assume the app stole focus. **Do not rerun live UI tests while uninterrupted foreground use is required.**
- Initial GPU samples summed engines and contained a 231.209 spike; CSV column is now explicitly `LegacyGpu3DEngineSumPercent`. Not a valid GPU baseline. `scripts/bench-desktop.ps1` now uses busiest 3D engine and leaves invalid/unavailable values empty; `scripts/test-bench-desktop.ps1` passed. Corrected live capture is still pending.
- PresentMon 2.6.0 official download is in `B:\Projects\CallMe-work`. Published hash verified. Probe failed `failed to start trace session: access denied`. No elevation/UAC/group change. Do not claim p50/p95/p99 or 144 Hz smoothness measurements.
- CPU temperature helper unavailable; HWiNFO elevated/shared-memory unavailable. No thermal verdict or stress test.
- Wheel root cause identified: `phone/ui/toys.slint` steps on every nonzero vertical delta, so tiny touchpad events can advance modes. No accumulation/physics change applied; existing test only covers discrete steps.

## Cleanup completed; keep the remaining work

About **43 GB** recovered on B:.

- Eleven merged checkouts archived with every file preserved and hash-checked, then removed. `D:\Backups\CallMe\merged-worktrees-20261017\checkouts.tar`: 7,610 files; SHA-256 `202e2c153d7fb9e926c8cc9b1b4956373c129399bffcfa169f7515393a3d87ca`. Manifest/restore instructions beside it. Branches retained. `CallMe-chats2` is now an empty locked directory, not a registered worktree; don't force another process closed to remove it.
- Fourteen retired Cargo caches cleared. Custom files and final binaries (542 files) preserved in `D:\Backups\CallMe\old-cache-extras-20261017\preserved.tar`, SHA-256 `d62624d9e84a8ff011656995c7497e31407b0da0a08463a88929938def1a9f6d`. Compiler intermediates deliberately discarded; first rebuild of those branches costs time.
- Retained build directories: main `target`, `target-desk`, `target-desk-android`, `target-ring`, `target-controls2`, `target-ship`.
- Remaining registered worktrees: main, desktop, `CallMe-controls2` (three unmerged preview commits), `CallMe-ship` (detached `8867c86`, untracked `build.log`). Preserve them. Temporary server staging checkout was safely removed; its patch is deployed on main.
- **Do not merge desktop into main yet.** Original handoff gates this on device acceptance; there are many accumulated changes, not just this continuation.
- `D:\Outputs\CallMe\cleanup\app-open.md` has the 21 deferred refactors and explicitly calls for phone checks. They remain deferred under PC-only scope. Includes keyboard/focus/gesture/shader/state changes, not harmless wholesale cleanup.

## Commands when a relevant change needs verification

Use PowerShell. Reuse existing passed evidence; do not rerun everything for a handoff/docs edit.

```powershell
$env:PATH = "$env:USERPROFILE\.cargo\bin;$env:PATH"
Set-Location B:\Projects\CallMe-desk
$env:CARGO_TARGET_DIR = 'B:\Projects\CallMe\target-desk'
cargo build -p callme-phone --bin callme-desk --release --target x86_64-pc-windows-msvc --locked -j 2
cargo test -p callme-phone --lib --release --target x86_64-pc-windows-msvc --locked -j 2
# Focused real codec check, if attachment code changed:
$env:CALLME_HEIC_FIXTURE='B:\Projects\CallMe-work\example.heic'
cargo test -p callme-phone --lib --release --target x86_64-pc-windows-msvc -j 2 desk_attach::tests -- --include-ignored
# Benchmark calculation checks; does not launch an app:
& .\scripts\test-bench-desktop.ps1
```

For a later relevant shared-code compile check only (still no phone):

```powershell
$env:CARGO_TARGET_DIR='B:\Projects\CallMe\target-desk-android'
$env:ANDROID_JAR="$env:ANDROID_HOME\platforms\android-36\android.jar"
cargo ndk -t arm64-v8a -P 29 check -p callme-phone --lib --release --locked -j 2
```

PC install: when the client is safely stopped, copy the built exe to the installed path and compare SHA-256. Do not launch the normal config for a quiet test. Isolated checks use `CALLME_DESK_DATA` and `CALLME_CONFIG` pointing to fixtures with no real topic, monitor-2 prefs, and the existing inactive-window hook. Only one client instance is allowed by its named mutex. Restore toast IconUri after fixture runs.

## Still owed — don't call the project finished

1. Show the existing six previews in an allowed way; get explicit approval before native motion work. PC only.
2. At a user-appropriate time: actual PC call/audio/echo/latency/mute/hang-up/typed replies, toast click, drop/paste interaction and call-window resize acceptance. No unsolicited call now.
3. Correct GPU/typing capture, PresentMon frame pacing and valid temperature telemetry before optimizing high-refresh or judging smoothness. Current data is partial.
4. Full old-history transfer remains unresolved; recent relay mirroring is delivered. Do not access the phone to fill this gap under current instructions.
5. Main merge and remaining phone-dependent refactors wait on their real acceptance gates. No new phone work is authorized.

Do the next authorized cohesive PC task, not another blanket audit. Ask only when a concrete remaining decision truly blocks that task, respecting Abdul's current instruction about interruptions.
