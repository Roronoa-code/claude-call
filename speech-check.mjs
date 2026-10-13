// Offline HTTP-handler checks: no server startup, sockets, model loading, microphone or notifications.
import assert from 'node:assert/strict';
import { readFileSync } from 'node:fs';
import { EventEmitter } from 'node:events';
import vm from 'node:vm';

// The server starts services on import. Evaluate only its audio store and request handler with fake I/O.
const source = readFileSync(new URL('./server.mjs', import.meta.url), 'utf8');
const start = source.indexOf('const NOISE ='), end = source.indexOf('await new Promise(r => server.listen');
assert.ok(start >= 0 && end > start, 'offline handler boundaries must exist');
function harness() {
  let handle;
  const state = { waits: 0, quick: 0, final: [], delivered: [], onWait: null, quickResult: 'Complete reply.' };
  vm.runInNewContext(source.slice(start, end), {
    Buffer, URL, clearTimeout,
    setTimeout: (fn, ms) => { assert.equal(ms, 100); state.waits++; state.onWait?.(); return setTimeout(fn, 1); },
    http: { createServer: fn => { handle = fn; return {}; } },
    token: 'offline', call: { say: 'What should I change?', audio: [] },
    hearQuick: async () => { state.quick++; return state.quickResult; },
    hear: async pcm => { state.final.push(Array.from(pcm)); return 'Full reply.'; },
    log: () => {}, mark: () => {}, deliver: text => state.delivered.push(text),
  });
  state.request = async (url, samples, gate) => {
    const req = new EventEmitter();
    req.url = `${url}&t=offline`; req.method = 'POST';
    const result = {};
    const res = { headersSent: false,
      writeHead(status) { result.status = status; this.headersSent = true; return this; },
      end(body) { result.body = body ? JSON.parse(body) : null; },
    };
    const done = handle(req, res);
    if (gate) await gate;
    req.emit('data', Buffer.from(samples.buffer, samples.byteOffset, samples.byteLength));
    req.emit('end');
    await done;
    return result;
  };
  return state;
}
const audio = n => new Int16Array(n).fill(8192), empty = new Int16Array();

for (const tail of [audio(1600), empty]) {
  const h = harness();
  await h.request('/listen?u=late&off=0', audio(1600));
  let release;
  const gate = new Promise(r => { release = r; });
  const late = h.request('/listen?u=late&off=1600', audio(1600), gate);
  h.onWait = release;
  const result = await h.request('/speech?u=late&off=3200&end=3200', tail);
  release(); await late;
  assert.equal(result.status, 200);
  assert.equal(h.final[0].length, 3200 + tail.length, 'late audio must stay in the same utterance, including an empty final packet');
  assert.ok(h.final[0].every(n => n === 0.25), 'missing packets must not become silence');
  assert.ok(h.waits < 30, 'arrival should wake the final request before the retry limit');
}

{
  const h = harness();
  await h.request('/listen?u=short&off=0&partial=1', audio(1600));
  // A newer caption request must not delay an already complete transcript.
  h.quickResult = new Promise(() => {});
  void h.request('/listen?u=short&off=1600&partial=1', audio(160));
  const result = await Promise.race([
    h.request('/speech?u=short&off=1760&end=1600', empty),
    new Promise((_, reject) => { const timer = setTimeout(() => reject(new Error('waited for a redundant caption')), 1000); timer.unref(); }),
  ]);
  assert.equal(result.body.text, 'Complete reply.');
  assert.equal(h.final.length, 0, 'a complete live caption should avoid a final transcription');
}

{
  const h = harness(), length = 29 * 16000;
  h.quickResult = 'Only the tail.';
  await h.request('/listen?u=long&off=0&partial=1', audio(length));
  const result = await h.request(`/speech?u=long&off=${length}&end=${length}`, empty);
  assert.equal(result.body.text, 'Full reply.');
  assert.equal(h.final[0].length, length, 'a rolling 28-second caption must not replace a longer reply');
}

{
  const h = harness();
  await h.request('/listen?u=gap&off=1600&partial=1', audio(1600));
  assert.equal(h.quick, 0, 'do not cache a transcript of audio with a gap');
  await h.request('/listen?u=gap&off=0', audio(1600));
  await h.request('/speech?u=gap&off=3200&end=3200', empty);
  assert.ok(h.final[0].every(n => n === 0.25));
}

{
  const h = harness();
  const result = await h.request('/speech?u=lost&off=1600&end=3200', audio(1600));
  assert.equal(result.status, 500, 'a permanently missing packet must fail instead of submitting incomplete speech');
  assert.equal(h.final.length, 0);
  assert.equal(h.delivered.length, 0);
  // Failure must release the old utterance too.
  await h.request('/speech?u=lost&off=0&end=1600', audio(1600));
  assert.equal(h.final[0].length, 1600);
}
console.log('PASS: late/empty packets, complete-caption reuse, long replies, gaps and failure cleanup (offline)');
