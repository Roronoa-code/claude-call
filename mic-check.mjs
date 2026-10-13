// Exercise the real capture lifecycle with deferred getUserMedia; no sockets, models or devices.
import assert from 'node:assert/strict';
import { readFileSync } from 'node:fs';
import vm from 'node:vm';

const original = readFileSync(new URL('./call-engine.js', import.meta.url), 'utf8');
assert.ok(original.includes('    function end() {'), 'test-only end hook must match the real engine');
const source = original.replace('    function end() {', '    window.__micCheckEnd = end;\n    function end() {');
const settle = async () => { for (let i = 0; i < 8; i++) await Promise.resolve(); };
function harness() {
  const pending = [], streams = [], contexts = [], statuses = [], routes = [];
  const window = { CallMeApp: { stream() {}, begin() {}, setSpeaker: on => routes.push(on), ended() {} } };
  class AudioContext {
    constructor() { this.sampleRate = 48000; this.destination = {}; this.processors = []; this.sources = []; this.closed = false; contexts.push(this); }
    resume() { return Promise.resolve(); }
    close() { this.closed = true; return Promise.resolve(); }
    createScriptProcessor() {
      const node = { connected: false, connect() { this.connected = true; }, disconnect() { this.connected = false; } };
      this.processors.push(node); return node;
    }
    createMediaStreamSource(stream) {
      const node = { stream, connected: false, connect() { this.connected = true; }, disconnect() { this.connected = false; } };
      this.sources.push(node); return node;
    }
  }
  vm.runInNewContext(source, { window, AudioContext, Audio: class { play() { return Promise.resolve(); } },
    navigator: { mediaDevices: { getUserMedia: constraints => new Promise((resolve, reject) => pending.push({ constraints, resolve, reject })) } },
    performance: { now: () => 0 }, console, setInterval: () => 1, clearInterval() {}, setTimeout: () => 1, clearTimeout() {},
    fetch: url => url.startsWith('state?') ? new Promise(() => {}) : Promise.resolve({ ok: true }),
  });
  const call = window.CallEngine({ token: 'offline', on: { status: value => statuses.push(value) } });
  const resolve = index => {
    const requested = pending[index].constraints.audio.echoCancellation;
    const tracks = [0, 1].map(() => ({ requested, stopped: false, stop() { this.stopped = true; } }));
    const stream = { tracks, getTracks: () => tracks };
    streams.push(stream); pending[index].resolve(stream); return stream;
  };
  return { call, pending, streams, contexts, statuses, routes, resolve, end: window.__micCheckEnd };
}

{
  const h = harness(), answering = h.call.answer();
  assert.equal(h.pending[0].constraints.audio.echoCancellation, false);
  const initial = h.resolve(0); await answering;
  assert.equal(h.contexts.length, 1);
  const ctx = h.contexts[0]; assert.equal(ctx.processors.length, 1);
  assert.equal(h.call.setSpeaker(true), true, 'speaker API must retain its synchronous boolean result');
  assert.equal(h.pending[1].constraints.audio.echoCancellation, true);
  assert.ok(initial.tracks.every(t => !t.stopped), 'capture must survive while replacement is requested');
  const speaker = h.resolve(1); await settle();
  assert.ok(initial.tracks.every(t => t.stopped));
  assert.equal(ctx.processors.length, 1, 'route changes must reuse the processor and context');
  assert.equal(ctx.sources.filter(n => n.connected).length, 1);
  assert.equal(ctx.sources.find(n => n.connected).stream, speaker);

  h.call.setSpeaker(false); h.pending[2].reject(new Error('capture denied')); await settle();
  assert.ok(speaker.tracks.every(t => !t.stopped), 'failed replacement must preserve every current track');
  assert.equal(ctx.sources.find(n => n.connected).stream, speaker);
  assert.ok(h.statuses.some(s => s.startsWith('Could not update microphone')));

  h.call.setSpeaker(true); h.call.setSpeaker(false);
  const latest = h.resolve(4); await settle();
  const stale = h.resolve(3); await settle();
  assert.ok(stale.tracks.every(t => t.stopped), 'out-of-order stale captures must stop all their tracks');
  assert.equal(ctx.sources.filter(n => n.connected).length, 1);
  assert.equal(ctx.sources.find(n => n.connected).stream, latest);
  assert.ok(speaker.tracks.every(t => t.stopped));
  assert.equal(ctx.processors.length, 1);

  h.call.setSpeaker(true); h.end();
  const late = h.resolve(5); await settle();
  assert.ok(late.tracks.every(t => t.stopped), 'capture resolving after end must be released');
  assert.ok(latest.tracks.every(t => t.stopped));
  assert.ok(ctx.closed && ctx.sources.every(n => !n.connected) && ctx.processors.every(n => !n.connected));
  const requestCount = h.pending.length; h.call.setSpeaker(false); await h.call.setMuted(false);
  assert.equal(h.pending.length, requestCount, 'an ended call must not reacquire capture');
}

{
  const h = harness(), answering = h.call.answer();
  h.call.setSpeaker(true);
  const stale = h.resolve(0); await settle();
  assert.equal(h.contexts.length, 0, 'answer must await a newer pending route before attaching capture');
  const speaker = h.resolve(1); await answering;
  assert.ok(stale.tracks.every(t => t.stopped), 'pending answer must not overwrite a newer speaker choice');
  assert.equal(h.contexts.length, 1);
  assert.equal(h.contexts[0].sources.find(n => n.connected).stream, speaker);
  h.end();
}

{
  const h = harness(), answering = h.call.answer();
  h.pending[0].reject(new Error('initial permission denied')); await answering;
  const unmuting = h.call.setMuted(false);
  h.call.setSpeaker(true);
  const stale = h.resolve(1); await settle();
  assert.ok(stale.tracks.every(t => t.stopped));
  assert.equal(h.contexts.length, 0, 'a stale unmute capture must wait for the current route capture');
  h.resolve(2); assert.equal(await unmuting, false, 'unmute must wait for the latest capture startup after initial failure');
  h.end();
}

for (const unmute of [false, true]) {
  const h = harness();
  let waiting = h.call.answer(), staleIndex = 0;
  if (unmute) {
    h.pending[0].reject(new Error('initial permission denied')); await waiting;
    waiting = h.call.setMuted(false); staleIndex = 1;
  }
  h.call.setSpeaker(true);
  h.pending[staleIndex + 1].reject(new Error('newest capture denied')); await settle();
  const stale = h.resolve(staleIndex); await waiting;
  assert.ok(stale.tracks.every(t => t.stopped), 'stale capture must stay released after the newer capture fails');
  assert.equal(h.call.muted, true, 'answer/unmute must retain failure after the newest request has settled');
  assert.equal(h.contexts.length, 0, 'failed newest capture must not enter listening with a missing context');
  h.end();
}
console.log('PASS: initial capture, route swap, preserved capture on failure, one graph, rapid toggles, pending answer, unmute and late/end cleanup (offline)');
