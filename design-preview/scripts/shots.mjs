// Screenshots every state of every design into shots/, headless (no windows, no mouse).
// Needs the preview running (npm start). Usage: node scripts/shots.mjs [call|panels|film|mascot|widget|reduced]
import { chromium } from 'playwright-core';
import { mkdir } from 'node:fs/promises';
import { join } from 'node:path';

const out = join(import.meta.dirname, '..', 'shots');
await mkdir(join(out, 'frames'), { recursive: true });
const only = process.argv[2] || '';
const run = k => !only || only === k;
const browser = await chromium.launch({ channel: 'msedge', headless: true });
const STATES = ['incoming', 'talking', 'listening', 'hearing', 'thinking', 'hold', 'ended'];
const WAIT = { incoming: 900, talking: 3200, listening: 1200, hearing: 1500, thinking: 2200, hold: 2200, ended: 700 };
const DESIGNS = ['d', 'a', 'b', 'c'];

async function page(reduced = false) {
  const ctx = await browser.newContext({ viewport: { width: 1500, height: 1100 }, deviceScaleFactor: 2, reducedMotion: reduced ? 'reduce' : 'no-preference' });
  const p = await ctx.newPage();
  p.on('pageerror', e => console.log('PAGE ERROR', e.message));
  p.on('console', m => m.type() === 'error' && !m.text().includes('404') && console.log('CONSOLE', m.text().slice(0, 200)));
  await p.goto('http://127.0.0.1:5178/');
  await p.waitForTimeout(1200);
  return p;
}
const frame = (p, d) => p.locator(`.frame[aria-label^="${d.toUpperCase()}."]`);
const shoot = (loc, name) => loc.screenshot({ path: join(out, `${name}.png`), animations: 'allow' });
const state = (p, k) => p.evaluate(k => window.__preview.state(k), k);
const film = async (p, loc, name, n, gap) => { for (let t = 0; t < n; t++) { await loc.screenshot({ path: join(out, 'frames', `${name}-${String(t).padStart(2, '0')}.png`), scale: 'css' }); await p.waitForTimeout(gap); } };

const p = await page();
if (run('call')) {
  for (const mascot of ['clawd', 'spark']) {
    await p.evaluate(m => window.__preview.mascot(m), mascot);
    for (const k of STATES) {
      await state(p, k);
      await p.waitForTimeout(WAIT[k]);
      for (const d of DESIGNS) await shoot(frame(p, d), `call-${d}-${k}-${mascot}`);
    }
  }
}
if (run('panels')) {
  await p.evaluate(() => window.__preview.mascot('clawd'));
  await state(p, 'listening');
  await p.waitForTimeout(900);
  const [D, A, B, C] = DESIGNS.map(d => frame(p, d));
  await D.getByRole('button', { name: 'Captions' }).click();
  await A.getByRole('button', { name: 'Captions' }).click();
  await B.getByRole('button', { name: 'Transcript' }).click();
  await C.getByRole('textbox', { name: 'Type a reply' }).fill('Also, rename the backup folder to Notebook backups.');
  await C.getByRole('button', { name: 'Send' }).click();
  await p.waitForTimeout(900);
  await shoot(D, 'call-d-transcript-open'); await shoot(A, 'call-a-captions-open'); await shoot(B, 'call-b-transcript-open'); await shoot(C, 'call-c-typed-reply');
  await p.keyboard.press('Escape');
  await p.waitForTimeout(600);
  for (const F of [D, A]) {
    await F.getByRole('button', { name: 'Type', exact: true }).click();
    await p.waitForTimeout(800);
    await F.getByRole('textbox', { name: 'Type a reply' }).fill('Keep the last three.');
  }
  await shoot(D, 'call-d-type-open'); await shoot(A, 'call-a-type-open');
  await p.keyboard.press('Escape');
  await state(p, 'listening');
  await p.waitForTimeout(600);
  for (const d of DESIGNS) {
    await frame(p, d).getByRole('button', { name: /mute/i }).first().click();
    await p.waitForTimeout(500);
    await shoot(frame(p, d), `call-${d}-muted`);
    await frame(p, d).getByRole('button', { name: /mute/i }).first().click();
  }
}
if (run('film')) {
  // D's caption -> bubble (you start talking) and bubble -> caption (Claude answers), CSS and
  // Web Animations slowed 10x so each capture is about 30 ms of motion apart.
  const cdp = await p.context().newCDPSession(p);
  await cdp.send('Animation.enable');
  for (const mascot of ['clawd', 'spark']) {
    await p.evaluate(m => window.__preview.mascot(m), mascot);
    const D = frame(p, 'd');
    await state(p, 'listening');
    await p.waitForTimeout(1500);
    await cdp.send('Animation.setPlaybackRate', { playbackRate: 0.1 });
    await p.evaluate(() => window.__preview.sims[0].jump(4, false));
    await film(p, D, `d-caption-to-bubble-${mascot}`, 12, 250);
    await cdp.send('Animation.setPlaybackRate', { playbackRate: 1 });
    await p.evaluate(() => window.__preview.sims[0].jump(5, false));
    await p.waitForTimeout(2200);
    await cdp.send('Animation.setPlaybackRate', { playbackRate: 0.1 });
    await p.evaluate(() => window.__preview.sims[0].jump(6, false));
    await film(p, D, `d-bubble-to-caption-${mascot}`, 12, 250);
    await cdp.send('Animation.setPlaybackRate', { playbackRate: 1 });
  }
}
if (run('mascot')) {
  // The mascot itself while talking and thinking, both looks, in A (the biggest one).
  for (const mascot of ['clawd', 'spark']) {
    await p.evaluate(m => window.__preview.mascot(m), mascot);
    const M = frame(p, 'a').locator('.a-avatar');
    for (const k of ['talking', 'thinking', 'hearing']) {
      await state(p, k);
      await p.waitForTimeout(k === 'talking' ? 1200 : 1800);
      await film(p, M, `mascot-${k}-${mascot}`, 10, 60);
    }
  }
}
if (run('widget')) {
  const W = p.locator('.row').last().locator('.frame');
  for (const m of ['call', 'text', 'both', 'none']) {
    await p.evaluate(m => window.__preview.mode(m), m);
    await p.waitForTimeout(800);
    for (const [i, d] of ['glass', 'aura', 'ink'].entries()) await shoot(W.nth(i), `widget-${d}-${m}`);
  }
}
if (run('reduced')) {
  const r = await page(true);
  for (const mascot of ['clawd', 'spark']) {
    await r.evaluate(m => window.__preview.mascot(m), mascot);
    for (const k of ['talking', 'hearing', 'thinking']) {
      await state(r, k);
      await r.waitForTimeout(WAIT[k]);
      await shoot(frame(r, 'd'), `reduced-motion-d-${k}-${mascot}`);
    }
  }
}
await browser.close();
console.log('done');
