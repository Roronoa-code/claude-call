// Fake phone for checks: answers a live call link, fetches Claude's voice, and replies twice with real speech audio.
// Usage: node phone-sim.mjs <call link>
import { KokoroTTS } from 'kokoro-js';
const link = new URL(process.argv[2]);
const t = link.searchParams.get('t'), base = link.origin;
const api = p => `${base}/${p}?t=${t}`;
if (!link.searchParams.get('call')) throw new Error('call link must be unique per call (missing &call=)');

const tts = await KokoroTTS.from_pretrained('onnx-community/Kokoro-82M-v1.0-ONNX', { dtype: 'fp32', device: 'cpu' });
async function speech(text) {
  const a = await tts.generate(text, { voice: 'am_michael' });
  const ratio = a.sampling_rate / 16000, pcm = new Float32Array(Math.floor(a.audio.length / ratio));
  for (let i = 0; i < pcm.length; i++) pcm[i] = a.audio[Math.floor(i * ratio)];
  return pcm.buffer;
}
const replies = [await speech('Yes, I can hear you fine.'), await speech('Keep the old layout please.')];

await fetch(api('answer'), { method: 'POST' });
let seq = null, n = 0;
while (n < 3) {
  const s = await (await fetch(api('state'))).json();
  if (s.say && s.seq !== seq) {
    seq = s.seq;
    const t0 = Date.now();
    let secs = 0;
    for (let i = 0; i < s.parts.length; i++) {
      let st = s;
      while (st.ready <= i) { await new Promise(r => setTimeout(r, 150)); st = await (await fetch(api('state'))).json(); }
      const a = await fetch(`${api('audio')}&seq=${s.seq}&part=${i}`);
      if (i === 0) console.log(`first sentence ready to play after ${Date.now() - t0}ms`);
      secs += (await a.arrayBuffer()).byteLength / 48000;
    }
    console.log(`claude said (${s.parts.length} parts): "${s.say}" | ${secs.toFixed(1)}s audio`);
    if (s.end) break;
    const r = await (await fetch(api('speech'), { method: 'POST', body: replies[n++ % 2] })).json();
    console.log('phone heard as:', r.text);
  }
  await new Promise(r => setTimeout(r, 500));
}
