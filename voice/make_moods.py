"""Builds the voice's moods from the user's ElevenLabs sample lines (voice/lines, cut by pause and transcribed).

Each mood joins a few lines spoken in that mood into one reference clip (voice/moods/<mood>.wav) and records it, with
its exact words, under "moods" in voice.json. The voice server clones from it when a line starts with [<mood>].
  python make_moods.py
"""
import json
from pathlib import Path

import numpy as np
import soundfile as sf

HERE = Path(__file__).parent
LINES = HERE / "lines"
MOODS = {
    "smug": ["sample2-01d", "sample2-02", "sample3-00", "sample3-01d", "sample3-02", "sample3-06e", "sample3-06f"],
    "tired": ["sample2-00", "sample2-01b", "sample2-04e", "sample2-05b", "sample3-03", "sample3-06a"],
    "cold": ["sample2-05d", "sample2-05e", "sample2-05f", "sample3-04"],
    "whisper": ["sample2-03", "sample2-04a", "sample2-04b"],
    "excited": ["sample2-05h"],
}
GAP = 0.25  # seconds of quiet between joined lines


def main():
    words = {r["file"][:-4]: r["text"] for r in json.loads((LINES / "lines.json").read_text(encoding="utf-8"))}
    (HERE / "moods").mkdir(exist_ok=True)
    voice = json.loads((HERE / "voice.json").read_text(encoding="utf-8"))
    voice["moods"] = {}
    for mood, names in MOODS.items():
        parts, sr = [], 24000
        for n in names:
            audio, sr = sf.read(LINES / f"{n}.wav", dtype="float32")
            parts += [audio, np.zeros(int(GAP * sr), dtype=np.float32)]
        clip = np.concatenate(parts[:-1])
        sf.write(HERE / "moods" / f"{mood}.wav", clip, sr, subtype="PCM_16")
        voice["moods"][mood] = {"clip": f"moods/{mood}.wav", "text": " ".join(words[n] for n in names)}
        print(f"{mood:8} {len(clip) / sr:5.1f}s  {voice['moods'][mood]['text']}")
    (HERE / "voice.json").write_text(json.dumps(voice, indent=1), encoding="utf-8")


if __name__ == "__main__":
    main()
