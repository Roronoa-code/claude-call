// Self-check for voice_server.py: starts it, times a few lines, and checks real WAV audio comes back.
import { spawn } from 'node:child_process';
import assert from 'node:assert';
const PORT = 47812;
const py = spawn('.venv\\Scripts\\python.exe', ['voice_server.py', String(PORT)], { cwd: import.meta.dirname, windowsHide: true });
py.stderr.on('data', () => {});
const t0 = Date.now();
await new Promise((res, rej) => { py.stdout.on('data', d => String(d).includes('ready') && res()); py.on('exit', c => rej(new Error('exited ' + c))); });
console.log('ready after', ((Date.now() - t0) / 1000).toFixed(1), 's');
for (const line of ['Okay, got it.', 'Nice, that worked. Do you want me to push it now, or wait until tomorrow morning?']) {
  const t = Date.now();
  const r = await fetch(`http://127.0.0.1:${PORT}/tts`, { method: 'POST', body: line });
  const buf = Buffer.from(await r.arrayBuffer());
  assert.equal(r.status, 200);
  assert.equal(buf.toString('ascii', 0, 4), 'RIFF');
  const secs = (buf.length - 44) / 2 / buf.readUInt32LE(24);
  console.log(`${((Date.now() - t) / 1000).toFixed(2)}s for ${secs.toFixed(1)}s audio`);
}
// Hearing: feed a spoken line back through /stt (24 kHz int16 WAV -> 16 kHz float32).
const wav = Buffer.from(await (await fetch(`http://127.0.0.1:${PORT}/tts`, { method: 'POST', body: 'Yeah, I can hear you. Red dress Ada Wong.' })).arrayBuffer());
const sr = wav.readUInt32LE(24), n = (wav.length - 44) / 2, ratio = sr / 16000;
const pcm = new Float32Array(Math.floor(n / ratio));
for (let i = 0; i < pcm.length; i++) pcm[i] = wav.readInt16LE(44 + 2 * Math.floor(i * ratio)) / 32768;
let t = Date.now();
const heard = await (await fetch(`http://127.0.0.1:${PORT}/stt`, {
  method: 'POST', body: Buffer.from(pcm.buffer), headers: { 'x-context': encodeURIComponent('Which Resident Evil character? Ada Wong or Leon?') },
})).text();
console.log(`heard in ${((Date.now() - t) / 1000).toFixed(2)}s: ${heard}`);
assert.match(heard, /I can hear you/i);
assert.match(heard, /Ada Wong/);
assert.doesNotMatch(heard, /Resident Evil/); // context must not leak into the transcript
py.kill();
console.log('PASS');
