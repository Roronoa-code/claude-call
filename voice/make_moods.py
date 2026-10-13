"""Builds the voice's moods: several reference takes per mood, which the voice server clones from (one picked per
sentence, so the same mood never sounds the same twice).

Takes come from two places:
  - the user's own ElevenLabs sample lines (voice/lines, cut by pause and transcribed), joined per mood below;
  - mood_lines.json, generated in the user's ElevenLabs voice (needs ELEVENLABS_API_KEY; the voice is found by name).
Each take is transcribed with the local Whisper (voice server on :47811) so the clone gets its exact words.
Generated takes are kept in voice/moods and never generated twice (they cost ElevenLabs credits).
  python make_moods.py
"""
import io
import json
import os
import subprocess
import urllib.request
from pathlib import Path

import numpy as np
import soundfile as sf

HERE = Path(__file__).parent
LINES, MOODS_DIR = HERE / "lines", HERE / "moods"
VOICE_NAME = "American woman"
MODEL = "eleven_v4"  # performs the [bracketed] tags; the user's samples were made with it
SR = 24000
SETTINGS = {"stability": 0.5, "similarity_boost": 0.75, "speed": 1.0}  # as the user's own samples (sp100 s50 sb75)
# The user's own sample lines, joined into one take per mood.
FROM_SAMPLES = {
    "smug": ["sample2-01d", "sample2-02", "sample3-00", "sample3-01d", "sample3-02", "sample3-06e", "sample3-06f"],
    "tired": ["sample2-00", "sample2-01b", "sample2-04e", "sample2-05b", "sample3-03", "sample3-06a"],
    "cold": ["sample2-05d", "sample2-05e", "sample2-05f", "sample3-04"],
    "whisper": ["sample2-03", "sample2-04a", "sample2-04b"],
    "excited": ["sample2-05h"],
}


def api(path, body=None, raw=False):
    req = urllib.request.Request(f"https://api.elevenlabs.io{path}", method="POST" if body else "GET",
                                 data=json.dumps(body).encode() if body else None,
                                 headers={"xi-api-key": os.environ["ELEVENLABS_API_KEY"], "content-type": "application/json"})
    data = urllib.request.urlopen(req, timeout=180).read()
    return data if raw else json.loads(data)


def hear(audio):
    pcm = subprocess.run(["ffmpeg", "-hide_banner", "-loglevel", "error", "-f", "f32le", "-ar", str(SR), "-ac", "1", "-i", "-",
                          "-ar", "16000", "-f", "f32le", "-"], input=audio.astype("<f4").tobytes(), capture_output=True).stdout
    req = urllib.request.Request("http://127.0.0.1:47811/stt", data=pcm, method="POST")
    return urllib.request.urlopen(req, timeout=120).read().decode().strip()


def generated(mood, i, line, voice_id):
    path = MOODS_DIR / f"{mood}-{i + 1}.wav"
    if not path.exists():
        pcm = api(f"/v1/text-to-speech/{voice_id}?output_format=pcm_24000", {"text": line, "model_id": MODEL, "voice_settings": SETTINGS}, raw=True)
        sf.write(path, np.frombuffer(pcm, dtype="<i2").astype(np.float32) / 32768, SR, subtype="PCM_16")
        print(f"  made {path.name}")
    return path


def main():
    MOODS_DIR.mkdir(exist_ok=True)
    takes: dict[str, list[Path]] = {}
    words = {r["file"][:-4]: r["text"] for r in json.loads((LINES / "lines.json").read_text(encoding="utf-8"))}
    texts = {}
    for mood, names in FROM_SAMPLES.items():
        parts = []
        for n in names:
            parts += [sf.read(LINES / f"{n}.wav", dtype="float32")[0], np.zeros(int(0.25 * SR), dtype=np.float32)]
        path = MOODS_DIR / f"{mood}.wav"
        sf.write(path, np.concatenate(parts[:-1]), SR, subtype="PCM_16")
        takes.setdefault(mood, []).append(path)
        texts[path] = " ".join(words[n] for n in names)
    if os.environ.get("ELEVENLABS_API_KEY"):
        voice_id = next(v["voice_id"] for v in api("/v2/voices?page_size=100")["voices"] if v["name"].strip().lower() == VOICE_NAME.lower())
        for mood, lines in json.loads((HERE / "mood_lines.json").read_text(encoding="utf-8")).items():
            if not mood.startswith("_"):
                takes.setdefault(mood, []).extend(generated(mood, i, line, voice_id) for i, line in enumerate(lines))
        sub = api("/v1/user/subscription")
        print(f"ElevenLabs credits left: {sub['character_limit'] - sub['character_count']}")
    voice = json.loads((HERE / "voice.json").read_text(encoding="utf-8"))
    voice["moods"] = {}
    for mood, paths in takes.items():
        for p in paths:
            text = texts.get(p) or hear(sf.read(p, dtype="float32")[0])
            voice["moods"].setdefault(mood, []).append({"clip": f"moods/{p.name}", "text": text})
            print(f"{mood:12} {p.name:16} {text[:90]}")
    (HERE / "voice.json").write_text(json.dumps(voice, indent=1), encoding="utf-8")


if __name__ == "__main__":
    main()
