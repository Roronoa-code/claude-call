// Real engine + CallView with fake I/O and a controlled clock; no sockets, models, builds or devices.
import assert from 'node:assert/strict';
import { readFileSync } from 'node:fs';
import vm from 'node:vm';

const engine = readFileSync(new URL('./call-engine.js', import.meta.url), 'utf8');
assert.ok(engine.includes('    function end() {'));
const instrumented = engine.replace('    function end() {', '    window.__turnCheck = { onFrame, end };\n    function end() {');
const view = readFileSync(new URL('./call-ui/src/call.js', import.meta.url), 'utf8')
  .replace("import { words } from './text.js';", "const words = t => t.split(/\\s+/).filter(Boolean);")
  .replace('export class CallView', 'window.CallView = class CallView');
const settle = async () => { for (let i = 0; i < 40; i++) await Promise.resolve(); };

function harness() {
  let now = 0, timerId = 0, state = { v: 1, seq: 1, say: 'Welcome.', mode: 'stream', debug: false, end: false };
  const timers = new Map(), watchers = new Set(), audioBytes = new Map(), speech = [], played = [], events = [], tracks = [];
  const timeout = (fn, ms) => { timers.set(++timerId, { fn, due: now + ms }); return timerId; };
  const window = {};
  window.CallMeApp = { begin() {}, setSpeaker() {}, ended() {}, stream(url, id) {
    played.push(id);
    queueMicrotask(() => { window.__audioStart(id); window.__audioDur(id, 100); window.__audioPos(id, 100); window.__played(id, true); });
  } };
  const response = value => ({ ok: true, json: async () => value });
  const fetch = (url, options = {}) => {
    const parsed = new URL(url, 'https://offline.invalid/');
    if (parsed.pathname === '/state') {
      if (Number(parsed.searchParams.get('v')) < state.v) return Promise.resolve(response({ ...state }));
      return new Promise((resolve, reject) => {
        const watcher = () => { watchers.delete(watcher); resolve(response({ ...state })); };
        watchers.add(watcher);
        options.signal?.addEventListener('abort', () => { watchers.delete(watcher); reject(new Error('aborted state watch')); }, { once: true });
      });
    }
    const id = parsed.searchParams.get('u');
    if (['/speech', '/listen'].includes(parsed.pathname)) audioBytes.set(id, (audioBytes.get(id) || 0) + options.body.byteLength);
    if (parsed.pathname === '/speech') return new Promise((resolve, reject) => {
      speech.push({ id, bytes: options.body.byteLength, totalBytes: audioBytes.get(id), resolve: text => resolve(response({ text })), reject });
      options.signal?.addEventListener('abort', () => reject(new Error('speech timeout')), { once: true });
    });
    if (parsed.pathname === '/listen') return Promise.resolve(response({ text: `Live words for ${id}.` }));
    return Promise.resolve(response({}));
  };
  class AudioContext {
    constructor() { this.sampleRate = 48000; this.destination = {}; }
    resume() {} close() {}
    createScriptProcessor() { return { connect() {}, disconnect() {} }; }
    createMediaStreamSource() { return { connect() {}, disconnect() {} }; }
  }
  const context = vm.createContext({ window, location: { search: '?t=offline', href: 'https://offline.invalid/?t=offline' }, URL, URLSearchParams, AbortController,
    AbortSignal: { timeout: ms => { const controller = new AbortController(); timeout(() => controller.abort(), ms); return controller.signal; } },
    AudioContext, Audio: class { play() { return Promise.resolve(); } },
    navigator: { mediaDevices: { getUserMedia: async () => { const track = { stopped: false, stop() { this.stopped = true; } }; tracks.push(track); return { getTracks: () => [track] }; } } },
    performance: { now: () => now }, console, setInterval: () => 0, clearInterval() {},
    setTimeout: timeout, clearTimeout: id => timers.delete(id), fetch,
  });
  vm.runInContext(instrumented, context);
  const actual = window.CallEngine;
  window.CallEngine = options => actual({ ...options, on: Object.fromEntries(Object.entries(options.on).map(([name, fn]) => [name, x => { events.push({ name, x }); fn(x); }])) });
  vm.runInContext(view, context);
  const screen = new window.CallView();
  screen.answer();
  const advance = async ms => {
    now += ms;
    for (const [id, timer] of [...timers]) if (timer.due <= now) { timers.delete(id); timer.fn(); }
    await settle();
  };
  const frame = async () => { now += 100; window.__turnCheck.onFrame(new Float32Array(2048).fill(0.1), now); await settle(); };
  return { screen, speech, played, events, tracks, advance, frame,
    speak: async () => { await frame(); now += 1000; await frame(); },
    publish: async (seq, say, end = false) => { state = { ...state, v: state.v + 1, seq, say, end }; for (const watcher of [...watchers]) watcher(); await settle(); },
    end: () => window.__turnCheck.end(),
  };
}

{
  const h = harness(); await settle();
  await h.speak(); const first = h.screen.call.finish(); await settle();
  h.screen.call.finish(); await settle(); assert.equal(h.speech.length, 1, 'Done must not submit a turn twice');
  h.speech[0].resolve('First reply.'); await first;
  assert.equal(h.screen.view.phase, 'thinking');
  await h.speak();
  assert.equal(h.screen.view.phase, 'hearing', 'speech during agent thinking must be captured');
  const partial = h.screen.view.partial;
  await h.publish(2, 'Obsolete answer.');
  assert.deepEqual(h.played, [1000], 'an arriving answer must not force-finish or speak over the user');
  assert.equal(h.speech.length, 1, 'an arriving answer must not submit unfinished speech');
  const second = h.screen.call.finish(); await settle();
  assert.equal(h.speech.length, 2); assert.ok(h.speech[1].totalBytes > 0);
  await h.publish(3, 'Latest answer.');
  assert.deepEqual(h.played, [1000], 'wait until the final user transcript is submitted');
  h.speech[1].resolve('Followup reply.'); await second; await h.advance(50);
  assert.deepEqual(h.played, [1000, 3000], 'refresh state and play only the newest held answer');
  assert.ok(partial.startsWith('Live words'));
  assert.deepEqual(Array.from(h.screen.view.msgs.filter(m => m.who === 'me'), m => m.text), ['First reply.', 'Followup reply.']);
  assert.notEqual(h.speech[0].id, h.speech[1].id);
  await h.advance(2000); assert.equal(h.speech.length, 2, 'silence must not create an endless reply loop');
  h.end();
}

{
  const h = harness(); await settle(); await h.speak();
  const first = h.screen.call.finish(); await settle();
  await h.screen.call.setMuted(true); await h.screen.call.setMuted(false);
  await h.speak(); const partial = h.screen.view.partial;
  h.speech[0].resolve('Older reply.'); await first;
  assert.equal(h.screen.view.phase, 'hearing', 'old final status must not overwrite newer live speech');
  assert.equal(h.screen.view.partial, partial, 'old final text must not clear a newer caption');
  const second = h.screen.call.finish(); await settle();
  h.speech[1].resolve('Newer reply.'); await second;
  assert.equal(h.screen.view.partial, '');
  h.end();
}

{
  const h = harness(); await settle(); await h.speak();
  const first = h.screen.call.finish(); await settle();
  await h.screen.call.setMuted(true); await h.screen.call.setMuted(false);
  await h.speak(); const second = h.screen.call.finish(); await settle();
  assert.equal(h.speech.length, 1, 'successive final uploads must retain submission order');
  h.end(); h.speech[0].resolve('Too late.'); await first; await second;
  assert.equal(h.speech.length, 1, 'an upload queued behind another must not start after end');
  assert.equal(h.screen.view.phase, 'ended');
  assert.equal(h.events.filter(e => e.name === 'heard').length, 0, 'late transcripts must not resurrect an ended call');
  assert.ok(h.tracks.every(t => t.stopped));
}

for (const stop of ['end', 'mute']) {
  const h = harness(); await settle(); await h.speak();
  await h.publish(2, 'Held answer.');
  if (stop === 'end') {
    await h.publish(3, 'Goodbye.', true); await h.advance(50);
    assert.equal(h.screen.view.phase, 'ended', 'server hangup must wake the hold without cutting/uploading speech');
    assert.deepEqual(h.played, [1000]);
  } else {
    await h.screen.call.setMuted(true); await h.advance(50);
    assert.deepEqual(h.played, [1000, 2000], 'mute must release the hold after discarding its active utterance as before');
  }
  assert.equal(h.speech.length, 0); h.end();
}

{
  const h = harness(); await settle(); await h.speak();
  const finishing = h.screen.call.finish(); await settle();
  await h.publish(2, 'Held answer.');
  await h.advance(90_000); await finishing;
  assert.ok(h.events.some(e => e.name === 'status' && e.x.startsWith('Could not send audio')), 'hung final transcription must time out');
  assert.deepEqual(h.played, [1000, 2000], 'a timed-out transcript must not hold the answer forever');
  h.end();
}
console.log('PASS: thinking followups, newest answer, no forced/duplicate submission, protected newer captions/status, ordered uploads, queued/end cleanup and mute-aware hold (offline)');
