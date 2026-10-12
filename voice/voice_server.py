"""Local voice and hearing for calls, on the GPU.

Voice: Qwen3-TTS Base clones the chosen reference clip (voice.json).
Hearing: Whisper large-v3-turbo, prompted with what Claude just said so names are heard right.

Started by server.mjs and shared by every Claude session; exits after 30 idle minutes.
Usage: python voice_server.py <port>
POST /tts  (body: UTF-8 text)                                  ->  audio/wav
POST /stt  (body: 16 kHz mono float32 PCM; header X-Context)   ->  text/plain
Prints "ready" once both models are loaded and warmed up (first run captures CUDA graphs).
"""
import io
import json
import sys
import threading
import time
from http.server import BaseHTTPRequestHandler, HTTPServer
from pathlib import Path
from urllib.parse import unquote

import numpy as np
import soundfile as sf
import torch
from faster_qwen3_tts import FasterQwen3TTS
from transformers import AutoProcessor, WhisperForConditionalGeneration

HERE = Path(__file__).parent
MODEL = r"B:\Projects\ThePipeline\models\qwen3-tts-base"
voice = json.loads((HERE / "voice.json").read_text(encoding="utf-8"))
model = FasterQwen3TTS.from_pretrained(MODEL)
# Encode the reference clip once, not on every line.
prompt = model.model.create_voice_clone_prompt(ref_audio=str(HERE / voice["clip"]), ref_text=voice["text"],
                                               x_vector_only_mode=False)


def speak(text: str) -> bytes:
    torch.manual_seed(1234)  # same sampling every time, so pieces of one reply sound like the same person
    wavs, sr = model.generate_voice_clone(text=text, language="English",
                                          voice_clone_prompt=prompt, ref_text=voice["text"])
    audio = np.asarray(wavs[0], dtype=np.float32)
    buf = io.BytesIO()
    sf.write(buf, audio / (float(np.abs(audio).max()) or 1.0) * 0.89, sr, format="WAV", subtype="PCM_16")
    return buf.getvalue()


last_used = time.time()
ASR = "openai/whisper-large-v3-turbo"
asr_proc = AutoProcessor.from_pretrained(ASR)
asr = WhisperForConditionalGeneration.from_pretrained(ASR, torch_dtype=torch.float16).to("cuda:0").eval()


def hear(pcm: np.ndarray, context: str) -> str:
    feats = asr_proc(pcm, sampling_rate=16000, return_tensors="pt").input_features.to("cuda:0", torch.float16)
    kw = dict(language="en", task="transcribe", max_new_tokens=220)
    if context.strip():
        kw["prompt_ids"] = torch.tensor(asr_proc.get_prompt_ids(context.strip()[-600:])).to("cuda:0")
    with torch.inference_mode():
        ids = asr.generate(feats, **kw)
    text = asr_proc.batch_decode(ids, skip_special_tokens=True)[0].strip()
    # With a prompt, some versions echo it back first; drop it if so.
    if context.strip() and text.startswith(context.strip()[-600:].strip()):
        text = text[len(context.strip()[-600:].strip()):].strip()
    return text


class Handler(BaseHTTPRequestHandler):
    def do_GET(self):  # health check used by server.mjs to find a running instance
        self.send_response(200 if self.path == "/health" else 404)
        self.end_headers()

    def do_POST(self):
        if self.path not in ("/tts", "/stt"):
            self.send_error(404); return
        global last_used
        last_used = time.time()
        body = self.rfile.read(int(self.headers.get("Content-Length", 0)))
        try:
            if self.path == "/stt":
                context = unquote(self.headers.get("X-Context", ""))  # sent URI-encoded
                data, kind = hear(np.frombuffer(body, dtype="<f4").copy(), context).encode("utf-8"), "text/plain; charset=utf-8"
            else:
                data, kind = speak(body.decode("utf-8").strip()), "audio/wav"
        except Exception as e:  # report, keep serving
            self.send_error(500, str(e)[:200]); return
        self.send_response(200)
        self.send_header("Content-Type", kind)
        self.send_header("Content-Length", str(len(data)))
        self.end_headers()
        self.wfile.write(data)

    def log_message(self, *args):
        pass


IDLE_EXIT_S = 30 * 60  # free the GPU after 30 minutes without a call


def watchdog(server):
    while True:
        time.sleep(30)
        if time.time() - last_used > IDLE_EXIT_S:
            server.shutdown(); return


if __name__ == "__main__":
    speak("Warming up.")  # captures CUDA graphs so the first real line is fast
    hear(np.zeros(16000, dtype=np.float32), "")
    server = HTTPServer(("127.0.0.1", int(sys.argv[1])), Handler)
    threading.Thread(target=watchdog, args=(server,), daemon=True).start()
    print("ready", flush=True)
    server.serve_forever()
