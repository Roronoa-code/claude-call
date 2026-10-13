// Fake phone for checks: opens the real call page (page.html + call-engine.js) in headless Edge with a fake mic,
// and talks back with real speech audio. Never touches the real phone: it only opens the link it is given.
// Usage: node phone-sim.mjs <call link> [basic]   (test-call.mjs starts it by itself; basic = only reply to line 1)
// Script, by Claude's line number: 1 reply · 2 talk over it quietly (confirmed by words) · 3 (hold) reply while on hold
// · 4 nothing (the held reply is returned) · 5 talk over it loudly · 6 end.
import { chromium } from 'playwright-core';
import { KokoroTTS } from 'kokoro-js';

const link = new URL(process.argv[2]), basic = process.argv[3] === 'basic';
if (!link.searchParams.get('call')) throw new Error('call link must be unique per call (missing &call=)');
const tts = await KokoroTTS.from_pretrained('onnx-community/Kokoro-82M-v1.0-ONNX', { dtype: 'fp32', device: 'cpu' });
// Real speech, with Kokoro's silent lead-in and tail cut off so "started/stopped talking" times are exact.
const speech = async (text, gain = 1) => {
  const a = await tts.generate(text, { voice: 'am_michael' }), loud = v => Math.abs(v) > 0.02;
  const pcm = a.audio.slice(a.audio.findIndex(loud), a.audio.findLastIndex(loud) + 1);
  return { pcm: Array.from(pcm, v => v * gain), rate: a.sampling_rate };
};
const lines = {
  reply: await speech('Yes, I can hear you fine.'),
  bargeQuiet: await speech('Hang on, keep the old layout please.', 0.25),
  held: await speech('Also make the button blue.'),
  bargeLoud: await speech('Stop, stop. That is wrong.', 1.6),
};

const browser = await chromium.launch({ channel: 'msedge', headless: true, args: ['--autoplay-policy=no-user-gesture-required'] });
const page = await browser.newPage();
page.on('pageerror', e => console.log('PAGE ERROR', e.message));
await page.addInitScript(() => {
  // Fake mic: whatever window.__mic.say() plays; silence otherwise.
  const ctx = new AudioContext(), dest = ctx.createMediaStreamDestination();
  window.__mic = {
    say(pcm, rate) {
      ctx.resume();
      const b = ctx.createBuffer(1, pcm.length, rate); b.copyToChannel(Float32Array.from(pcm), 0);
      const s = ctx.createBufferSource(); s.buffer = b; s.connect(dest); s.start();
      return new Promise(r => { s.onended = () => r(performance.now()); });
    },
  };
  navigator.mediaDevices.getUserMedia = async () => dest.stream;
  // Record the engine's events with times, next to the page's own handlers.
  window.__ev = [];
  let engine;
  Object.defineProperty(window, 'CallEngine', {
    get: () => engine,
    set: f => { engine = opts => f({ ...opts, on: new Proxy(opts.on, { get: (o, k) => x => { window.__ev.push({ k, x: x === undefined ? null : JSON.parse(JSON.stringify(x)), t: performance.now() }); return o[k]?.(x); } }) }); },
  });
});
const reqs = [];
page.on('request', r => { const u = new URL(r.url()); if (['/speech', '/stream', '/listen'].includes(u.pathname)) reqs.push({ p: u.pathname, t: Date.now(), partial: u.searchParams.has('partial') }); });
await page.goto(link.href + '&auto=1');
const now = () => page.evaluate(() => performance.now());
const say = l => page.evaluate(([pcm, rate]) => window.__mic.say(pcm, rate), [l.pcm, l.rate]);
const events = () => page.evaluate(() => window.__ev);
const waitFor = async (test, ms = 60_000) => {
  const t = Date.now();
  for (;;) { const e = (await events()).find(test); if (e) return e; if (Date.now() - t > ms) throw new Error('timed out'); await new Promise(r => setTimeout(r, 50)); }
};
const report = [];
let n = 0, bargeAt = 0;
for (;;) {
  const st = await waitFor(e => e.k === 'state' && e.x.say && e.x.seq > n, 300_000);
  n = st.x.seq;
  const start = await waitFor(e => e.k === 'claude' && e.x.id === n);
  const msg = `line ${n}${st.x.hold ? ' (hold)' : ''}: first sound ${Math.round(start.t - st.t)}ms after the line was ready`;
  console.log(msg); report.push(msg);
  if (!basic && (n === 2 || n === 5)) {
    await new Promise(r => setTimeout(r, 1200));
    const t0 = bargeAt = await now();
    say(n === 2 ? lines.bargeQuiet : lines.bargeLoud);
    const cut = await waitFor(e => e.k === 'interrupted' && e.t > t0, 15_000);
    const done = await waitFor(e => e.k === 'spoken' && e.x.id === n);
    const m = `  talked over it: Claude stopped ${Math.round(cut.t - t0)}ms after the user started (${cut.x.reason}), line cut=${done.x.cut}`;
    console.log(m); report.push(m);
  } else {
    const done = await waitFor(e => e.k === 'spoken' && e.x.id === n);
    const shown = (await events()).filter(e => e.k === 'playing' && e.x.id === n);
    const early = shown.find(e => e.x.shown.length > 0);
    console.log(`  captions: first words "${early?.x.shown}" at ${Math.round((early?.t ?? 0) - start.t)}ms, ${shown.length} updates, cut=${done.x.cut}`);
    if (st.x.end) break;
    if (n === 4) continue;
    await new Promise(r => setTimeout(r, 400));
  }
  if (n === 1 || (!basic && n === 3)) {
    const stopped = await say(n === 1 ? lines.reply : lines.held), wall = Date.now();
    const heard = await waitFor(e => e.k === 'heard' && e.t > stopped, 20_000);
    const sent = reqs.find(r => r.p === '/speech' && r.t >= wall - 2000);
    const parts = (await events()).filter(e => e.k === 'partial' && e.t > stopped - 4000).map(e => e.x.text);
    const m = `  you stopped -> turn sent ${sent ? sent.t - wall : '?'}ms, transcript back ${Math.round(heard.t - stopped)}ms: "${heard.x.text}" | live captions: ${parts.map(p => `"${p}"`).join(' > ')}`;
    console.log(m); report.push(m);
  } else if (!basic && (n === 2 || n === 5)) {
    const heard = await waitFor(e => e.k === 'heard' && e.t > bargeAt && e.x.text, 20_000);
    const m = `  heard: "${heard.x.text}"`;
    console.log(m); report.push(m);
  }
}
await waitFor(e => e.k === 'ended', 10_000);
console.log('SIM DONE');
await browser.close();
