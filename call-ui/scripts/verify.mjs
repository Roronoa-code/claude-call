// Real-call check of the built page (dist/page.html), without touching the real page.html:
// runs test-call.mjs (private test topic, never rings the phone) from a side copy of the server whose
// page.html is the new one, answers in headless Edge with a fake microphone that says a line and then
// stays quiet, and screenshots every state the real call goes through into shots/.
// Usage: node build.mjs && node scripts/verify.mjs
import { chromium } from 'playwright-core';
import { spawn } from 'node:child_process';
import { copyFileSync, existsSync, mkdirSync, rmSync, symlinkSync, writeFileSync } from 'node:fs';
import { join } from 'node:path';

const ui = join(import.meta.dirname, '..'), app = join(ui, '..'), side = join(ui, '.verify'), shots = join(ui, 'shots');
mkdirSync(side, { recursive: true }); mkdirSync(shots, { recursive: true });
for (const f of ['server.mjs', 'test-call.mjs', 'asr.mjs', 'package.json']) copyFileSync(join(app, f), join(side, f));
copyFileSync(join(ui, 'dist/page.html'), join(side, 'page.html'));
for (const d of ['node_modules', 'voice', 'bin']) if (!existsSync(join(side, d))) symlinkSync(join(app, d), join(side, d), 'junction');

// The fake microphone: a short reply, then silence (Edge loops the file).
const wav = join(side, 'mic.wav');
if (!existsSync(wav)) {
  const { KokoroTTS } = await import('kokoro-js');
  const tts = await KokoroTTS.from_pretrained('onnx-community/Kokoro-82M-v1.0-ONNX', { dtype: 'fp32', device: 'cpu' });
  const a = await tts.generate('Yes, I can hear you fine. Keep the last three backups.', { voice: 'am_michael' });
  const sr = a.sampling_rate, pad = new Float32Array(sr * 9), pcm = new Float32Array(a.audio.length + pad.length);
  pcm.set(new Float32Array(sr * 0.5).fill(0)); pcm.set(a.audio, sr * 0.5);
  const b = Buffer.alloc(44 + pcm.length * 2);
  b.write('RIFF', 0); b.writeUInt32LE(36 + pcm.length * 2, 4); b.write('WAVEfmt ', 8); b.writeUInt32LE(16, 16); b.writeUInt16LE(1, 20); b.writeUInt16LE(1, 22);
  b.writeUInt32LE(sr, 24); b.writeUInt32LE(sr * 2, 28); b.writeUInt16LE(2, 32); b.writeUInt16LE(16, 34); b.write('data', 36); b.writeUInt32LE(pcm.length * 2, 40);
  pcm.forEach((v, i) => b.writeInt16LE(Math.max(-32768, Math.min(32767, Math.round(v * 32767 * 0.9))), 44 + i * 2));
  writeFileSync(wav, b);
}

const test = spawn(process.execPath, ['test-call.mjs'], { cwd: side, env: { ...process.env, CALLME_TOPIC: 'claude-test-' + Date.now() }, windowsHide: true });
let log = '';
const link = await new Promise((res, rej) => {
  const on = d => { log += d; process.stdout.write(d); const m = /call link\s+(\S+)/.exec(log); if (m) res(m[1]); };
  test.stdout.on('data', on); test.stderr.on('data', on);
  test.on('exit', c => rej(new Error('test-call exited ' + c)));
});
const done = new Promise(r => test.on('exit', c => r(c)));

const browser = await chromium.launch({ channel: 'msedge', headless: true, args: ['--use-fake-ui-for-media-stream', '--use-fake-device-for-media-stream', `--use-file-for-fake-audio-capture=${wav}`, '--autoplay-policy=no-user-gesture-required'] });
const ctx = await browser.newContext({ viewport: { width: 412, height: 892 }, deviceScaleFactor: 2, isMobile: true, hasTouch: true });
await ctx.grantPermissions(['microphone']);
const p = await ctx.newPage();
p.on('pageerror', e => console.log('PAGE ERROR', e.message));
await p.goto(link);
await p.waitForTimeout(1500);
const seen = new Set();
const shoot = async name => { await p.screenshot({ path: join(shots, `real-${name}.png`) }); console.log('shot', name); };
await shoot('incoming');
await p.getByRole('button', { name: 'Answer call' }).click();
let panels = false;
for (const end = Date.now() + 240_000; Date.now() < end;) {
  const v = await p.evaluate(() => { const v = window.__call.view; return { phase: v.phase, sub: v.sub, closed: v.closed, spoken: v.msgs.at(-1)?.spoken }; }).catch(() => null);
  if (!v) break;
  const key = v.phase === 'thinking' ? `thinking-${v.sub}` : v.phase === 'talking' && v.spoken?.[1] > 2 ? 'talking' : v.phase === 'talking' ? 'talking-start' : v.phase;
  if (!seen.has(key)) {
    seen.add(key);
    if (key === 'talking' || key === 'hearing') await p.waitForTimeout(500);
    await shoot(key);
    if (key === 'listening' && !panels) { // the two panels, on the real transcript
      panels = true;
      await p.getByRole('button', { name: 'Captions' }).click(); await p.waitForTimeout(700); await shoot('transcript');
      await p.keyboard.press('Escape'); await p.waitForTimeout(500);
    }
  }
  if (v.closed) break;
  await p.waitForTimeout(120);
}
await shoot('closed');
console.log('test-call exit', await Promise.race([done, new Promise(r => setTimeout(() => r('still running'), 30_000))]));
await browser.close();
test.kill();
rmSync(join(side, 'page.html'));
