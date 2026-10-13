// Offline rendering of the real built page. The engine is replaced; no mic, audio, servers or phone.
import assert from 'node:assert/strict';
import { readFileSync } from 'node:fs';
import { chromium } from 'playwright-core';

const html = readFileSync(new URL('../page.html', import.meta.url), 'utf8');
const browser = await chromium.launch({ channel: 'msedge', headless: true });
try {
  const page = await browser.newPage({ viewport: { width: 412, height: 892 } });
  const errors = [];
  page.on('pageerror', error => errors.push(error.message));
  await page.route('**/*', route => {
    const url = new URL(route.request().url());
    if (url.origin !== 'https://callme.invalid') return route.abort();
    if (url.pathname === '/') return route.fulfill({ contentType: 'text/html', body: html });
    if (url.pathname === '/call-engine.js') return route.fulfill({ contentType: 'text/javascript', body: `
      window.CallEngine = ({ on }) => {
        window.events = on;
        return { hasSpeaker: true, answer() {}, finish() {}, sendTyped() {}, setMuted: x => x, setSpeaker: x => x };
      };
    ` });
    return route.abort();
  });
  await page.goto('https://callme.invalid/');
  await page.waitForFunction(() => window.__call && document.querySelector('.d-mascot'));
  await page.evaluate(() => {
    window.originalMascot = document.querySelector('.d-mascot');
    window.originalControls = document.querySelector('.a-controls');
    window.__call.answer();
    events.claude({ id: 1, text: 'Hello Abdul. These words must remain unheard.' });
  });
  await page.waitForFunction(() => document.querySelector('.d-label.pre'));
  assert.equal(await page.locator('.d-label.pre').evaluate(el => getComputedStyle(el).visibility), 'hidden');
  const cut = await page.evaluate(() => {
    events.playing({ id: 1, text: 'Hello Abdul. These words must remain unheard.', shown: 'Hello Abdul.' });
    const firstHearing = [];
    const unsub = window.__call.subscribe(() => {
      if (window.__call.view.phase === 'hearing') firstHearing.push(window.__call.view.msgs.at(-1).cut);
    });
    events.status('Hearing you…');
    unsub();
    return firstHearing;
  });
  assert.deepEqual(cut, [true], 'unheard words must already be cut on the first hearing update');
  await page.waitForFunction(() => document.querySelector('.screen').dataset.phase === 'hearing');
  assert.equal((await page.locator('.d-thread .claude').innerText()).trim(), 'Hello Abdul.');

  await page.evaluate(() => {
    events.spoken({ id: 1, cut: true });
    events.heard({ id: 'first', text: 'A short reply.' });
    events.status('Claude is thinking…');
    const text = 'A complete spoken line with enough words to need scrolling. '.repeat(20);
    events.claude({ id: 2, text });
    events.playing({ id: 2, text, shown: text });
    events.spoken({ id: 2, cut: false });
    events.status('Hearing you…');
    events.partial({ text: 'A short reply.' });
  });
  await page.waitForFunction(() => document.querySelector('.d-thread .claude')?.scrollTop > 0);
  await page.locator('.d-thread .claude').evaluate(el => { el.scrollTop = 0; window.clockBeforeScroll = document.querySelector('.d-time').textContent; });
  await page.waitForFunction(() => document.querySelector('.d-time').textContent !== window.clockBeforeScroll);
  assert.equal(await page.locator('.d-thread .claude').evaluate(el => el.scrollTop), 0, 'clock ticks must not undo manual scrolling');

  for (const height of [892, 740, 640]) {
    await page.setViewportSize({ width: 412, height });
    await page.evaluate(() => events.partial({ text: 'A very long live reply that should scroll above the controls. '.repeat(30) }));
    await page.waitForFunction(() => document.querySelector('.d-bubble.me')?.textContent.includes('very long'));
    for (const thinking of [false, true]) {
      if (thinking) await page.evaluate(() => {
        events.heard({ id: 'long', text: 'A very long final reply that should scroll above the controls. '.repeat(30) });
        events.status('Claude is thinking…');
      });
      // Let the existing approved transition settle before measuring its resting geometry.
      await page.waitForTimeout(550);
      const layout = await page.evaluate(() => {
        const me = document.querySelector('.d-bubble.me'), stage = document.querySelector('.d-stage');
        const bottom = el => el.getBoundingClientRect().bottom;
        return { meBottom: bottom(me), stageBottom: bottom(stage), statusTop: document.querySelector('.d-status').getBoundingClientRect().top,
          pendingBottom: document.querySelector('.d-pending') ? bottom(document.querySelector('.d-pending')) : 0,
          scrollable: me.scrollHeight > me.clientHeight, latest: Math.abs(me.scrollHeight - me.clientHeight - me.scrollTop) <= 2,
          sameMascot: window.originalMascot === document.querySelector('.d-mascot'), sameControls: window.originalControls === document.querySelector('.a-controls'),
          controlsOpacity: getComputedStyle(document.querySelector('.a-controls')).opacity };
      });
      assert.ok(layout.meBottom <= layout.stageBottom + 1, `reply escaped stage at height ${height}: ${JSON.stringify(layout)}`);
      assert.ok(layout.pendingBottom <= layout.statusTop, `thinking label overlaps status at height ${height}: ${JSON.stringify(layout)}`);
      assert.ok(layout.scrollable && layout.latest, 'long user reply must scroll to its latest words');
      assert.ok(layout.sameMascot && layout.sameControls, 'mascot and controls must remain mounted');
      assert.equal(layout.controlsOpacity, '1');
    }
    await page.evaluate(() => events.status('Hearing you…'));
  }
  assert.deepEqual(errors, []);
  console.log('PASS: first interrupt render, scroll retention, long replies at three sizes, persistent mascot/controls (offline Edge)');
} finally {
  await browser.close();
}
