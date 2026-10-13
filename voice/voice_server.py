"""Local voice and hearing for calls, on the GPU.

Voice: Qwen3-TTS Base clones the chosen reference clip (voice.json).
Hearing: Whisper large-v3-turbo, prompted with what Claude just said so names are heard right.

Started by server.mjs and shared by every Claude session; exits after 30 idle minutes.
Usage: python voice_server.py <port>
POST /tts         (body: UTF-8 text)                                  ->  audio/wav (whole line)
POST /tts_stream  (body: UTF-8 text)                                  ->  16-bit mono PCM, streamed as it is made
                                                                          (header X-Sample-Rate; 500 if the first chunk fails)
POST /stt         (body: 16 kHz mono float32 PCM; header X-Context)   ->  text/plain
Prints "ready" once both models are loaded and warmed up (first run captures CUDA graphs).
"""
import io
import json
import select
import sys
import threading
import time
from http.server import BaseHTTPRequestHandler, ThreadingHTTPServer
from pathlib import Path
from urllib.parse import unquote, urlparse, parse_qs

import numpy as np
import soundfile as sf
import torch
from faster_qwen3_tts import FasterQwen3TTS
from transformers import AutoProcessor, WhisperForConditionalGeneration

from stretch import Stretch  # a little slower than the model speaks, same pitch

HERE = Path(__file__).parent
MODEL = r"B:\Projects\ThePipeline\models\qwen3-tts-base"
voice = json.loads((HERE / "voice.json").read_text(encoding="utf-8"))
model = FasterQwen3TTS.from_pretrained(MODEL)
# Encode the reference clip once, not on every line.
prompt = model.model.create_voice_clone_prompt(ref_audio=str(HERE / voice["clip"]), ref_text=voice["text"],
                                               x_vector_only_mode=False)

# One voice at a time (the CUDA graphs hold a single generation's state), and one GPU job at a time.
# Streaming takes the GPU per chunk, so hearing (live captions, interrupts) slips in between voice chunks.
tts_lock = threading.Lock()
gpu_lock = threading.Lock()
STREAM_CHUNK = 4      # codec steps per streamed chunk (12 per second of audio): small = first sound sooner
STREAM_GAIN = 1.6     # fixed level for streamed lines (whole lines are peak-normalised; a stream can't look ahead)


def speak(text: str) -> bytes:
    with tts_lock, gpu_lock:
        torch.manual_seed(1234)  # same sampling every time, so pieces of one reply sound like the same person
        wavs, sr = model.generate_voice_clone(text=text, language="English",
                                              voice_clone_prompt=prompt, ref_text=voice["text"])
    audio = Stretch().feed(np.asarray(wavs[0], dtype=np.float32), last=True)
    buf = io.BytesIO()
    sf.write(buf, audio / (float(np.abs(audio).max()) or 1.0) * 0.89, sr, format="WAV", subtype="PCM_16")
    return buf.getvalue()


def speak_stream(text: str, chunk: int = STREAM_CHUNK):
    """Yields (sample_rate, int16 PCM bytes) pieces of one line as they are made."""
    with tts_lock:
        torch.manual_seed(1234)
        gen = model.generate_voice_clone_streaming(text=text, language="English", voice_clone_prompt=prompt,
                                                   ref_text=voice["text"], chunk_size=chunk)
        slow, sr = Stretch(), 24000
        try:
            while True:
                with gpu_lock:
                    item = next(gen, None)
                if item is not None:
                    audio, sr, _ = item
                pcm = slow.feed(audio if item is not None else [], last=item is None)
                if len(pcm):
                    pcm = np.clip(pcm * STREAM_GAIN, -1.0, 1.0)
                    yield sr, (pcm * 32767).astype("<i2").tobytes()
                if item is None:
                    return
        finally:
            gen.close()


last_used = time.time()
ASR = "openai/whisper-large-v3-turbo"
asr_proc = AutoProcessor.from_pretrained(ASR)
asr = WhisperForConditionalGeneration.from_pretrained(ASR, torch_dtype=torch.float16).to("cuda:0").eval()


# Words Whisper keeps mishearing (e.g. "call screen" -> "cold screen"); always part of the prompt.
VOCAB = "Claude, Clawd, Codex, call screen, Discord, ntfy, Whisper, widget."


def hear(pcm: np.ndarray, context: str) -> str:
    long = len(pcm) > 30 * 16000  # over 30 s: Whisper's long-form mode instead of cutting the end off
    inputs = asr_proc(pcm, sampling_rate=16000, return_tensors="pt",
                      **(dict(truncation=False, padding="longest", return_attention_mask=True) if long else {}))
    kw = dict(language="en", task="transcribe", max_new_tokens=220)
    if long:
        kw.update(attention_mask=inputs.attention_mask.to("cuda:0"), return_timestamps=True)
    context = (VOCAB + " " + context.strip()[-500:]).strip() if context.strip() else VOCAB
    if context.strip():
        kw["prompt_ids"] = torch.tensor(asr_proc.get_prompt_ids(context.strip()[-600:])).to("cuda:0")
    with gpu_lock, torch.inference_mode():
        ids = asr.generate(inputs.input_features.to("cuda:0", torch.float16), **kw)
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
        route = urlparse(self.path)
        if route.path not in ("/tts", "/stt", "/tts_stream"):
            self.send_error(404); return
        global last_used
        last_used = time.time()
        body = self.rfile.read(int(self.headers.get("Content-Length", 0)))
        if route.path == "/tts_stream":
            chunk = int(parse_qs(route.query).get("chunk", [STREAM_CHUNK])[0])
            return self.stream(speak_stream(body.decode("utf-8").strip(), chunk))
        try:
            if route.path == "/stt":
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

    def stream(self, pieces):
        try:
            try:
                sr, first = next(pieces)  # make the first piece before answering, so a failure is a clean 500
            except Exception as e:
                self.send_error(500, str(e)[:200]); return
            self.send_response(200)
            self.send_header("Content-Type", "application/octet-stream")
            self.send_header("X-Sample-Rate", str(sr))
            self.end_headers()  # no length: the stream ends when the connection closes
            self.wfile.write(first); self.wfile.flush()
            while not self.hung_up():
                piece = next(pieces, None)
                if piece is None:
                    break
                self.wfile.write(piece[1]); self.wfile.flush()
        except (BrokenPipeError, ConnectionResetError, ConnectionAbortedError):
            pass  # caller hung up (line replaced or interrupted): stop making it
        finally:
            pieces.close()

    def hung_up(self):  # the caller closed the connection (line replaced or interrupted): don't make another chunk
        return bool(select.select([self.connection], [], [], 0)[0])

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
    for _ in speak_stream("Warming up."):
        pass
    hear(np.zeros(16000, dtype=np.float32), "")
    server = ThreadingHTTPServer(("127.0.0.1", int(sys.argv[1])), Handler)
    threading.Thread(target=watchdog, args=(server,), daemon=True).start()
    print("ready", flush=True)
    server.serve_forever()
