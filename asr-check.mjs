// Self-check for asr.mjs: a name Whisper gets wrong without context must come out right with it.
import assert from 'node:assert';
import { KokoroTTS } from 'kokoro-js';
import { transcribe } from './asr.mjs';

const tts = await KokoroTTS.from_pretrained('onnx-community/Kokoro-82M-v1.0-ONNX', { dtype: 'fp32', device: 'cpu' });
const a = await tts.generate('I said red dress Ada Wong, not white dress.', { voice: 'am_michael' });
const ratio = a.sampling_rate / 16000, pcm = new Float32Array(Math.floor(a.audio.length / ratio));
for (let i = 0; i < pcm.length; i++) pcm[i] = a.audio[Math.floor(i * ratio)];

console.log('no context:  ', await transcribe(pcm));
let t = Date.now();
const withCtx = await transcribe(pcm, 'Which Resident Evil 4 character were you thinking of? Ada Wong or Leon?');
console.log('with context:', withCtx, Date.now() - t, 'ms');
assert.match(withCtx, /Ada Wong/);
assert.doesNotMatch(withCtx, /Resident Evil 4 character/); // prompt must not leak into the output
console.log('PASS');
