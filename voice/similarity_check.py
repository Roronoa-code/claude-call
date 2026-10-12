"""Measures how much the cloned voice sounds like the reference clip, using the model's own speaker encoder.

Compares settings: fixed seed vs free sampling, and single clip vs all three clips joined as the reference.
Score = cosine similarity of speaker embeddings (higher = closer to the user's voice).
"""
import json
from pathlib import Path

import numpy as np
import soundfile as sf
import torch
from faster_qwen3_tts import FasterQwen3TTS

HERE = Path(__file__).parent
voice = json.loads((HERE / "voice.json").read_text(encoding="utf-8"))
refs = json.loads((HERE / "refs.json").read_text(encoding="utf-8"))
model = FasterQwen3TTS.from_pretrained(r"B:\Projects\ThePipeline\models\qwen3-tts-base")
qm = model.model
LINES = ["Hey, it's me. Quick one, the build passed but one test is flaky. Want me to fix it now, or leave it for later?",
         "Okay, all done. I tidied up the settings page and it's working on your phone."]


def emb(audio, sr):
    p = qm.create_voice_clone_prompt(ref_audio=(np.asarray(audio, dtype=np.float32), sr), ref_text="", x_vector_only_mode=True)
    e = p[0].ref_spk_embedding if isinstance(p, list) else p["ref_spk_embedding"][0]
    return torch.as_tensor(e).float().flatten()


def cos(a, b):
    return float(torch.nn.functional.cosine_similarity(a, b, dim=0))


ref_audio, ref_sr = sf.read(HERE / voice["clip"], dtype="float32")
ref_emb = emb(ref_audio, ref_sr)
# Joined reference: all three clips back to back.
joined, texts = [], []
for name in ["news", "ask", "check"]:
    a, sr = sf.read(HERE / f"{name}.mp3", dtype="float32")
    joined += [a, np.zeros(int(sr * 0.4), dtype=np.float32)]
    texts.append(refs[name])
joined = np.concatenate(joined)
sf.write(HERE / "joined.wav", joined, sr)  # the engine takes reference audio as a file path
setups = {
    "single clip, fixed seed": (dict(ref_audio=str(HERE / voice["clip"]), ref_text=voice["text"]), True),
    "single clip, free": (dict(ref_audio=str(HERE / voice["clip"]), ref_text=voice["text"]), False),
    "all 3 clips, free": (dict(ref_audio=str(HERE / "joined.wav"), ref_text=" ".join(texts)), False),
}
for name, (ref, seeded) in setups.items():
    scores = []
    for i, line in enumerate(LINES * 2):
        if seeded: torch.manual_seed(1234)
        wavs, out_sr = model.generate_voice_clone(text=line, language="English", **ref)
        scores.append(cos(emb(wavs[0], out_sr), ref_emb))
    print(f"{name:26s} similarity {np.mean(scores):.3f}  (min {np.min(scores):.3f})", flush=True)
