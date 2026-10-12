# Setting up Claude Call on another Windows machine (laptop)

Written for the Claude Code session on the laptop to follow. The laptop is mainly for **changing the phone app's UI**, and it can also make calls. There is no GPU, so calls use the user's **ElevenLabs** voice, and hearing runs on the CPU (small Whisper).

Work in `B:\Projects\CallMe` if the laptop has a B: drive, otherwise `C:\Projects\CallMe`. Ask the user before choosing.

## 1. Tools
- Git, GitHub CLI (`gh auth login` as Roronoa-code), and Node.js LTS.
- For the Android app: JDK 17 (Eclipse Temurin) and the Android SDK. Android Studio, or command-line tools with `platforms;android-37`, `build-tools;36.0.0` and `platform-tools`. Set `sdk.dir` in `android/local.properties` (this file is not in git).

## 2. Code
```
gh repo clone Roronoa-code/claude-call <folder>
cd <folder>; npm install
```
Download `cloudflared-windows-amd64.exe` from the cloudflare/cloudflared GitHub releases into `bin/cloudflared.exe`.

## 3. Private settings (not in git: ask the user, or copy them from the PC)
- `config.json`: `{ "topic": "<private ntfy channel>" }`. Same value as on the PC (`B:\Projects\CallMe\config.json`), so the same phone app rings.
- `android/app/src/main/res/values/topic.xml`: the same channel. Copy `topic.example.xml.txt` and fill it in. The app won't build without it.
- ElevenLabs: the user's API key and voice ID. Register the server with them:
  ```
  claude mcp add --scope user callme -e ELEVENLABS_API_KEY=<key> -e ELEVENLABS_VOICE_ID=<voice id> -- node <folder>\server.mjs
  ```
  Never commit the key.

## 4. Phone app UI work
- The call screen is `page.html`, served by the call server; there is no app rebuild for it.
- The phone app itself (home screen, widget, ringing) lives in `android/`.
- Build with `cd android; .\gradlew.bat assembleRelease`, then `.\gradlew.bat --stop`. The build is signed with that machine's debug key.
- **Signing matters:** an APK built on the laptop has a different debug key from the PC's. To install over the existing app on the phone, either copy the PC's `%USERPROFILE%\.android\debug.keystore` to the laptop first (recommended; the app keeps its data), or uninstall the app first (it loses its settings).
- Install over wifi adb: on the phone, Settings → Developer options → Wireless debugging; `adb connect <phone ip>:<port>`; `adb install -r ClaudeCall.apk`.
- Design previews: `cd design-preview; npm install; npm start`, then open http://127.0.0.1:5178. Follow the user's process: show previews and get explicit approval before building any UI or motion choice.

## 5. Do NOT run on the laptop
- The Discord watcher (`discord-input-mcp/watch.mjs` scheduled task). Only one machine may run it, or every message gets two replies. It runs on the PC.
- The GPU voice server (`voice/voice_server.py`); it needs an NVIDIA GPU.

## 6. Check
- `node test-call.mjs` (private test channel, never rings the phone) with `node phone-sim.mjs <link>` as the phone. Expect PASS. Without ElevenLabs keys the voice is Kokoro, which is fine for the test.
- Keep work in sync with the PC through git: pull before you start, and commit and push when done.
