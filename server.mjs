// Free "call me" MCP: rings the user's phone via ntfy, then talks through a call page
// (Whisper hearing + the user's cloned voice via Qwen3-TTS, all run on this PC) reached over a free Cloudflare tunnel.
import { McpServer } from '@modelcontextprotocol/sdk/server/mcp.js';
import { StdioServerTransport } from '@modelcontextprotocol/sdk/server/stdio.js';
import { z } from 'zod';
import http from 'node:http';
import { spawn } from 'node:child_process';
import { randomBytes } from 'node:crypto';
import { readFileSync, writeFileSync, existsSync, openSync, appendFileSync } from 'node:fs';
import { fileURLToPath } from 'node:url';
import path from 'node:path';

const dir = path.dirname(fileURLToPath(import.meta.url));
const configPath = path.join(dir, 'config.json');
if (!existsSync(configPath)) writeFileSync(configPath, JSON.stringify({ topic: 'claude-' + randomBytes(12).toString('hex') }, null, 2));
const topic = process.env.CALLME_TOPIC || JSON.parse(readFileSync(configPath, 'utf8')).topic;
const token = randomBytes(16).toString('hex');
const config = () => { try { return JSON.parse(readFileSync(configPath, 'utf8')); } catch { return {}; } };
// Debug log of the phone's mic levels and interrupts (for tuning): CALLME_DEBUG=1, or "debug": true in config.json.
const debugOn = () => process.env.CALLME_DEBUG === '1' || config().debug === true;
const log = (...a) => console.error('[callme]', ...a);
const logLine = line => appendFileSync(path.join(dir, 'timing.log'), `${new Date().toISOString()} ${line}\n`);
// Latency marks for each reply turn, in timing.log: shows where the gap between "you stop" and "Claude speaks" goes.
let turnStart = 0;
const mark = (what, reset = false) => {
  if (reset) turnStart = Date.now();
  if (turnStart) logLine(`+${Date.now() - turnStart}ms ${what}`);
};

const RING_FOR_MS = 75_000, REPLY_WAIT_MS = 5 * 60_000;

// The user's contact mode, shared by every agent (Discord Input MCP reads the same file):
// "call" = calls only, "text" = Discord only, "both", "none" = don't contact.
const STATUS_PATH = process.env.CALLME_STATUS || path.join(dir, 'status.json');
const MODES = ['call', 'text', 'both', 'none'];
const readStatusFile = () => { try { return JSON.parse(readFileSync(STATUS_PATH, 'utf8')); } catch { return { mode: 'both', updatedAt: new Date(0).toISOString(), by: 'default' }; } };
const writeStatus = (mode, by) => writeFileSync(STATUS_PATH, JSON.stringify({ mode, updatedAt: new Date().toISOString(), by }, null, 2));
// The phone app publishes its setting on the ntfy topic (title "Contact mode"); take it if it's newer than the file.
async function contactMode() {
  const file = readStatusFile();
  if (process.env.CALLME_STATUS) return file.mode; // tests: their own file only, never the phone's setting
  try {
    const r = await fetch(`https://ntfy.sh/${topic}/json?poll=1&since=12h`, { signal: AbortSignal.timeout(4000) });
    const latest = (await r.text()).split('\n').filter(Boolean).map(l => JSON.parse(l))
      .filter(m => m.event === 'message' && m.title === 'Contact mode' && MODES.includes(m.message?.trim()))
      .at(-1);
    // >= : ntfy times are whole seconds, so two changes in one second tie; the later message (last in the list) wins.
    if (latest && latest.time * 1000 >= Date.parse(file.updatedAt) && latest.message.trim() !== file.mode) {
      writeFileSync(STATUS_PATH, JSON.stringify({ mode: latest.message.trim(), updatedAt: new Date(latest.time * 1000).toISOString(), by: 'phone app' }, null, 2));
      return latest.message.trim();
    }
  } catch (e) { log('status sync skipped', e.message); }
  return file.mode;
}
const VOICE = process.env.CALLME_VOICE || 'af_heart'; // Kokoro fallback voice
// mode: how the current line is played: 'stream' (sent while it is being made), 'parts' (whole files), null = not decided yet.
const call = { v: 0, seq: 0, say: '', end: false, mode: null, parts: [], audio: [], stream: null, ttsAbort: null,
  answered: false, lastSeen: 0, watchers: 0, waiter: null, pending: [] };
// The call page waits on /state until something changes (long poll), so a new line reaches it at once.
const stateWaits = new Set();
const changed = () => { call.v++; for (const w of [...stateWaits]) w(); };
const nextChange = ms => new Promise(r => {
  const done = () => { clearTimeout(timer); stateWaits.delete(done); r(); };
  const timer = setTimeout(done, ms);
  stateWaits.add(done);
});

// Main voice + hearing on the GPU (voice/voice_server.py): the user's own voice cloned by Qwen3-TTS, and
// Whisper large. One instance is shared by all Claude sessions and exits itself after 30 idle minutes.
// Kokoro / CPU Whisper / the phone's voice are fallbacks.
const QWEN_PORT = 47811;
const qwenUp = () => process.env.CALLME_NO_GPU ? Promise.resolve(false) : fetch(`http://127.0.0.1:${QWEN_PORT}/health`).then(r => r.ok, () => false);
let qwenPromise = null;
const loadQwen = () => qwenPromise ??= (async () => {
  if (await qwenUp()) return true;
  // Machines without the GPU voice set up (e.g. the laptop) skip straight to ElevenLabs / Kokoro.
  if (process.env.CALLME_NO_GPU) return false; // tests of the fallback voices
  if (!existsSync(path.join(dir, 'voice', 'voice.json')) || !existsSync(path.join(dir, 'voice', '.venv', 'Scripts', 'pythonw.exe'))) return false;
  const out = openSync(path.join(dir, 'voice', 'voice_server.log'), 'w');
  // pythonw: no console window at all (a detached python.exe gets a visible terminal on Windows).
  spawn(path.join(dir, 'voice', '.venv', 'Scripts', 'pythonw.exe'), ['voice_server.py', String(QWEN_PORT)],
    { cwd: path.join(dir, 'voice'), windowsHide: true, detached: true, stdio: ['ignore', out, out] }).unref();
  for (let i = 0; i < 180; i++) {
    await new Promise(r => setTimeout(r, 1000));
    if (await qwenUp()) return true;
  }
  log('gpu voice did not start; see voice/voice_server.log');
  return false;
})().then(ok => { if (!ok) qwenPromise = null; return ok; });
// Calls the GPU server; if it has exited (idle timeout), starts it again once.
async function gpu(route, opts) {
  for (let attempt = 0; attempt < 2; attempt++) {
    if (!await loadQwen()) return null;
    try {
      const r = await fetch(`http://127.0.0.1:${QWEN_PORT}${route}`, { method: 'POST', ...opts });
      if (r.ok) return r;
      log('gpu', route, r.status); return null;
    } catch { qwenPromise = null; }
  }
  return null;
}
async function qwenSpeak(text) {
  const r = await gpu('/tts', { body: text });
  return r && { data: Buffer.from(await r.arrayBuffer()), type: 'audio/wav' };
}
// Fallback voice (Kokoro, CPU), loaded only if the cloned voice is unavailable.
let ttsPromise = null;
const loadTts = () => ttsPromise ??= import('kokoro-js')
  .then(({ KokoroTTS }) => KokoroTTS.from_pretrained('onnx-community/Kokoro-82M-v1.0-ONNX', { dtype: 'fp32', device: 'cpu' }))
  .catch(e => { log('tts unavailable', e.message); return null; });
// Speech-to-text: Whisper large-v3-turbo on the GPU (voice_server.py); small Whisper on the CPU (asr.mjs) as fallback.
async function hear(pcm, context) {
  const r = await gpu('/stt', { body: Buffer.from(pcm.buffer, pcm.byteOffset, pcm.byteLength), headers: { 'x-context': encodeURIComponent(context) } });
  if (r) return (await r.text()).trim();
  return (await loadAsr()).transcribe(pcm.slice(), context);
}
// Live captions: GPU only and never starts it (the CPU fallback is too slow to keep up); null = skipped.
async function hearQuick(pcm, context) {
  if (!await qwenUp()) return null;
  const r = await fetch(`http://127.0.0.1:${QWEN_PORT}/stt`, { method: 'POST', body: Buffer.from(pcm.buffer, pcm.byteOffset, pcm.byteLength),
    headers: { 'x-context': encodeURIComponent(context) } }).catch(() => null);
  return r?.ok ? (await r.text()).trim() : null;
}
let asrPromise = null;
const loadAsr = () => asrPromise ??= import('./asr.mjs');
// Whisper invents these on near-silence.
const NOISE = /^\s*(\[.*\]|\(.*\)|you|thank you\.?|thanks for watching!?|\.+)?\s*$/i;

// What the user says arrives in pieces while they talk (16 kHz mono 16-bit PCM, POST /listen), so live captions,
// the early end of their turn and the final transcript all work on audio this PC already has.
// Utterances by id (the last few): { pieces: Map(offset -> Int16Array), partial: {text, upTo}, busy: Promise|null }
const utts = new Map();
function takeAudio(u, off, buf) {
  if (!utts.has(u)) {
    utts.set(u, { pieces: new Map(), partial: null, busy: null, arrived: null });
    if (utts.size > 6) utts.delete(utts.keys().next().value);
  }
  const utt = utts.get(u);
  if (buf.length >= 2) utt.pieces.set(off, new Int16Array(buf.buffer.slice(buf.byteOffset, buf.byteOffset + (buf.length & ~1))));
  utt.arrived?.(); // wakes a final transcript waiting for a piece still on its way
  return utt;
}
// The utterance so far as float PCM, and whether it has no gaps (a piece can still be in flight).
function uttPcm(utt) {
  const offs = [...utt.pieces.keys()].sort((a, b) => a - b);
  let len = 0, whole = true;
  for (const o of offs) { if (o !== len) whole = false; len = Math.max(len, o + utt.pieces.get(o).length); }
  const pcm = new Float32Array(len);
  for (const o of offs) { const p = utt.pieces.get(o); for (let i = 0; i < p.length; i++) pcm[o + i] = p[i] / 32768; }
  return { pcm, whole };
}
const PARTIAL_WINDOW = 28 * 16000; // live captions look at the last 28 s (Whisper's window is 30 s)
function partial(utt) {
  if (utt.busy) return utt.busy; // one at a time; the page asks again
  const { pcm } = uttPcm(utt), upTo = pcm.length;
  return utt.busy = hearQuick(pcm.subarray(Math.max(0, upTo - PARTIAL_WINDOW)), call.say)
    .then(text => {
      if (text === null) return null;
      utt.partial = { text: NOISE.test(text) ? '' : text, upTo };
      return utt.partial;
    }, () => null)
    .finally(() => { utt.busy = null; });
}
function readBody(req, max) {
  return new Promise((resolve, reject) => {
    const chunks = []; let size = 0;
    req.on('data', c => { chunks.push(c); size += c.length; if (size > max) { req.destroy(); reject(new Error('too big')); } });
    req.on('end', () => resolve(Buffer.concat(chunks)));
    req.on('error', reject);
  });
}
const json = (res, obj) => res.writeHead(200, { 'content-type': 'application/json', 'cache-control': 'no-store' }).end(JSON.stringify(obj));
const files = { '/call-engine.js': 'text/javascript; charset=utf-8', '/': 'text/html; charset=utf-8' };

const server = http.createServer(async (req, res) => {
  const url = new URL(req.url, 'http://x'), q = k => url.searchParams.get(k);
  if (q('t') !== token && url.pathname !== '/call-engine.js') { res.writeHead(404).end(); return; } // the engine is public code
  try {
    if (files[url.pathname]) {
      // Read on each request so page changes apply to a running server.
      res.writeHead(200, { 'content-type': files[url.pathname], 'cache-control': 'no-store' })
        .end(readFileSync(path.join(dir, url.pathname === '/' ? 'page.html' : url.pathname.slice(1))));
      return;
    }
    if (url.pathname === '/state') {
      call.lastSeen = Date.now(); call.watchers++;
      if (call.v <= Number(q('v') ?? -1)) await nextChange(20_000);
      call.watchers--; call.lastSeen = Date.now();
      json(res, { v: call.v, seq: call.seq, say: call.say, end: call.end, hold: !!call.hold, holdLabel: call.hold ? call.say : '', mode: call.mode, parts: call.parts, ready: call.audio.length, debug: debugOn() });
      return;
    }
    if (url.pathname === '/stream') {
      // Claude's line as 16-bit PCM while it is being made, in rolling pieces: each request returns the audio made
      // since byte `from` (waiting until there is some), with x-done: 1 on the last. Whole responses, because the
      // quick tunnel holds back a streamed response until ~128 KB have built up. 409 = play the parts instead.
      const seq = Number(q('seq')), from = Number(q('from') ?? 0);
      while (call.seq === seq && !call.mode) await nextChange(30_000);
      if (call.seq !== seq) { res.writeHead(404).end(); return; }
      if (call.mode !== 'stream') { res.writeHead(409).end(); return; }
      const st = call.stream;
      while (st.bytes <= from && !st.done) await new Promise(r => { st.subs.add(r); setTimeout(r, 15_000); });
      const piece = Buffer.concat(st.chunks).subarray(from);
      if (!from && piece.length) mark('first audio byte sent to phone');
      res.writeHead(200, { 'content-type': 'application/octet-stream', 'content-length': piece.length, 'x-sample-rate': String(st.rate),
        'x-done': st.done && from + piece.length >= st.bytes ? '1' : '0', 'cache-control': 'no-store' }).end(piece);
      return;
    }
    const part = call.audio[Number(q('part') ?? 0)];
    if (url.pathname === '/audio' && Number(q('seq')) === call.seq && part) {
      if (q('part') === '0') mark('first audio byte sent to phone');
      res.writeHead(200, { 'content-type': part.type, 'cache-control': 'no-store' }).end(part.data);
      return;
    }
    if (url.pathname === '/hush' && req.method === 'POST') { if (Number(q('seq')) === call.seq) call.ttsAbort?.abort(); res.writeHead(204).end(); return; } // user talked over Claude
    if (url.pathname === '/answer' && req.method === 'POST') { call.answered = true; res.writeHead(204).end(); return; }
    if (url.pathname === '/decline' && req.method === 'POST') {
      call.declined = true; res.writeHead(204).end();
      const w = call.waiter; call.waiter = null; w?.(null);
      return;
    }
    if (url.pathname === '/listen' && req.method === 'POST') {
      // A piece of what the user is saying; with partial=1, also returns the transcript so far (live captions).
      const utt = takeAudio(q('u'), Number(q('off')), await readBody(req, 16_000 * 2 * 30));
      json(res, q('partial') ? (await partial(utt)) ?? { text: null } : {});
      return;
    }
    if (url.pathname === '/speech' && req.method === 'POST') {
      // The user finished talking: the last piece, then the final transcript goes to Claude.
      mark(`turn sent (you stopped talking ${q('quiet') ?? '?'}ms before)`, true);
      const utt = takeAudio(q('u'), Number(q('off')), await readBody(req, 16_000 * 2 * 120));
      utts.delete(q('u'));
      let { pcm, whole } = uttPcm(utt);
      for (let i = 0; !whole && i < 30; i++) { // a piece is still on its way (the page doesn't wait for it)
        await new Promise(r => { utt.arrived = r; setTimeout(r, 100); });
        ({ pcm, whole } = uttPcm(utt));
      }
      const end = Number(q('end') ?? pcm.length);
      // Reuse the live transcript when it already covers everything they said (only silence came after it).
      const covers = p => p && p.upTo >= end - 160 && p.upTo > pcm.length - PARTIAL_WINDOW;
      // Use the transcript we already have if it covers the speech: waiting on a newer one only costs time.
      const live = covers(utt.partial) ? utt.partial : (await utt.busy) ?? utt.partial;
      const reuse = covers(live);
      const heard = reuse ? live.text : await hear(pcm, call.say);
      mark(`transcribed (${(pcm.length / 16000).toFixed(1)}s of audio${reuse ? ', reused live transcript' : ''}) -> handed to Claude`);
      const ok = !NOISE.test(heard);
      json(res, { text: ok ? heard : '' });
      if (ok) deliver(q('interrupted') ? `(interrupting you) ${heard}` : heard);
      return;
    }
    if (url.pathname === '/reply' && req.method === 'POST') {
      const body = String(await readBody(req, 20_000));
      res.writeHead(204).end(); deliver(body.trim());
      return;
    }
    if (url.pathname === '/debug' && req.method === 'POST') {
      const body = String(await readBody(req, 20_000));
      res.writeHead(204).end();
      if (debugOn()) for (const l of body.split('\n').filter(Boolean)) logLine(`DEBUG ${l.slice(0, 400)}`);
      return;
    }
    res.writeHead(404).end();
  } catch (e) {
    log('request failed', url.pathname, e.message);
    if (!res.headersSent) res.writeHead(500).end(); else res.end();
  }
});
await new Promise(r => server.listen(0, '127.0.0.1', r));
const port = server.address().port;

// The phone reaches this server through the shared tunnel gateway (gateway.mjs): one Cloudflare quick tunnel for all
// CallMe servers on this PC, since trycloudflare refuses new tunnels (429) when every session opens its own.
// A new trycloudflare address takes ~30s to resolve on the phone, and a too-early lookup gets cached as "not found",
// so calls only ring once the gateway's tunnel is at least TUNNEL_SETTLE_MS old.
const TUNNEL_SETTLE_MS = 45_000, GATEWAY = 'http://127.0.0.1:47810', TUNNEL_WAIT_MS = 2 * 60_000;
const gatewayInfo = () => fetch(`${GATEWAY}/health`, { signal: AbortSignal.timeout(2000) }).then(r => r.ok ? r.json() : null, () => null);
let gatewayStarting = null;
async function ensureGateway() {
  if (await gatewayInfo()) return true;
  gatewayStarting ??= (async () => {
    // Detached and hidden: no console window, and it outlives this session for the others.
    spawn(process.execPath, [path.join(dir, 'gateway.mjs')], { cwd: dir, detached: true, windowsHide: true, stdio: 'ignore' }).unref();
    for (let i = 0; i < 20 && !await gatewayInfo(); i++) await new Promise(r => setTimeout(r, 250));
  })().finally(() => { gatewayStarting = null; });
  await gatewayStarting;
  return !!await gatewayInfo();
}
const register = () => fetch(`${GATEWAY}/register`, { method: 'POST', body: JSON.stringify({ token, port }), signal: AbortSignal.timeout(2000) }).catch(() => {});
async function ensureTunnel() {
  if (process.env.CALLME_LOCAL) return `http://127.0.0.1:${port}`; // tests: no tunnel, no phone
  const until = Date.now() + TUNNEL_WAIT_MS;
  while (Date.now() < until) {
    if (await ensureGateway()) {
      await register();
      const g = await gatewayInfo();
      if (g?.url) {
        const wait = g.bornAt + TUNNEL_SETTLE_MS - Date.now();
        if (wait > 0) await new Promise(r => setTimeout(r, wait));
        if ((await gatewayInfo())?.url === g.url) return g.url; // still the same tunnel after settling
        continue;
      }
    }
    await new Promise(r => setTimeout(r, 2000)); // the gateway is (re)opening its tunnel; it retries by itself
  }
  throw new Error('could not open a tunnel (trycloudflare busy or offline; see gateway.log); try again in a few minutes');
}
process.stdin.on('close', () => process.exit(0));

// Which agent is calling (Codex sets CALLME_AGENT=Codex). The alert title stays "Claude is calling": the app rings on it.
const AGENT = process.env.CALLME_AGENT?.trim() || 'Claude';
async function ring(link, reason) {
  await fetch(`https://ntfy.sh/${topic}`, {
    method: 'POST',
    // ponytail: message text is not sent through ntfy (public server); only a generic alert + private link.
    body: reason ? `${AGENT} needs you: ${reason}` : `${AGENT} needs to talk to you`,
    headers: { Title: 'Claude is calling', Priority: '5', Tags: 'telephone_receiver', Click: link, Actions: `view, Answer, ${link}` },
  }).catch(e => log('ntfy failed', e.message));
}

// ElevenLabs, for machines without a GPU (e.g. the laptop): the user's own ElevenLabs voice.
// Needs ELEVENLABS_API_KEY and ELEVENLABS_VOICE_ID; free plan is ~10k characters/month, then Kokoro takes over.
async function eleven(text) {
  const key = process.env.ELEVENLABS_API_KEY, voice = process.env.ELEVENLABS_VOICE_ID;
  if (!key || !voice) return null;
  const r = await fetch(`https://api.elevenlabs.io/v1/text-to-speech/${voice}?output_format=mp3_44100_128`, {
    method: 'POST',
    headers: { 'xi-api-key': key, 'content-type': 'application/json' },
    body: JSON.stringify({ text, model_id: process.env.ELEVENLABS_MODEL || 'eleven_flash_v2_5' }),
  });
  if (!r.ok) { log('elevenlabs', r.status, (await r.text()).slice(0, 200)); return null; }
  return { data: Buffer.from(await r.arrayBuffer()), type: 'audio/mpeg' };
}
async function synth(text) {
  try {
    // The cloned voice as a whole file only if the GPU server is already up (streaming just failed otherwise).
    const audio = (await qwenUp() ? await qwenSpeak(text) : null) ?? await eleven(text);
    if (audio) return audio;
    const tts = await loadTts();
    if (tts) return { data: Buffer.from((await tts.generate(text, { voice: VOICE })).toWav()), type: 'audio/wav' };
  } catch (e) { log('tts failed', e.message); }
  return null; // page falls back to the phone's own voice for this part
}
// Main path: the cloned voice streamed as it is made (first sound ~0.2 s after the line is ready), one stream per line
// so the voice stays even. Kept in memory for the phone to fetch (/stream). Returns false if the GPU voice is unavailable.
async function streamLine(seq, text, firstReady) {
  const ac = new AbortController();
  call.ttsAbort = ac;
  const r = await gpu('/tts_stream', { body: text, signal: ac.signal });
  if (!r || call.seq !== seq) { ac.abort(); return !!r; }
  const st = call.stream = { rate: Number(r.headers.get('x-sample-rate')) || 24000, chunks: [], bytes: 0, done: false, subs: new Set() };
  const wake = () => { for (const w of st.subs) w(); st.subs.clear(); };
  call.mode = 'stream'; changed();
  mark('first voice chunk ready');
  firstReady();
  try {
    for await (const c of r.body) {
      const b = Buffer.from(c);
      st.chunks.push(b); st.bytes += b.length;
      wake();
    }
  } catch (e) { if (!ac.signal.aborted) log('voice stream broke', e.message); }
  st.done = true;
  wake();
  return true;
}
// Fallback: whole files, sentence by sentence (ElevenLabs / Kokoro, or the phone's own voice).
async function speakParts(seq, text, firstReady) {
  // Two pieces: the first sentence (so playback starts fast), then the rest in one go (so the voice stays even).
  const sentences = text.match(/[^.!?]+(?:[.!?]+|$)/g)?.map(s => s.trim()).filter(Boolean) ?? [text];
  let n = 1;
  while (n < sentences.length && sentences.slice(0, n).join(' ').length < 25) n++; // don't start on a tiny fragment
  call.parts = [sentences.slice(0, n).join(' '), sentences.slice(n).join(' ')].filter(Boolean);
  call.mode = 'parts'; changed();
  for (const p of call.parts) {
    const a = await synth(p);
    if (call.seq !== seq) return; // superseded by a newer line
    call.audio.push(a); changed();
    if (call.audio.length === 1) mark('first piece of voice ready');
    firstReady();
  }
}
// Starts saying a line; resolves once its first sound is ready.
function say(text, end = false, hold = false) {
  mark('Claude replied (thinking done), making voice');
  call.ttsAbort?.abort(); // a newer line replaces one still being made
  const seq = ++call.seq;
  Object.assign(call, { say: text, end, hold, mode: null, parts: [text], audio: [], stream: null });
  changed();
  let firstReady;
  const first = new Promise(r => { firstReady = r; });
  (async () => {
    if (!await streamLine(seq, text, firstReady) && call.seq === seq) await speakParts(seq, text, firstReady);
  })().catch(e => log('voice failed', e.message)).finally(() => firstReady());
  return first;
}
// What the user says goes to whoever is waiting; if nobody is (e.g. while on hold), it is kept for the next turn.
function deliver(t) {
  const w = call.waiter; call.waiter = null;
  if (w) w(t); else if (t) call.pending.push(t);
}
function waitForReply(ms) {
  if (call.pending.length) return Promise.resolve(call.pending.splice(0).join(' / '));
  return new Promise(resolve => {
    const timer = setTimeout(() => { call.waiter = null; resolve(null); }, ms);
    call.waiter = t => { clearTimeout(timer); resolve(t); };
  });
}
const text = t => ({ content: [{ type: 'text', text: t }] });
const pageLive = () => call.watchers > 0 || Date.now() - call.lastSeen < 10_000;

const mcp = new McpServer({ name: 'callme', version: '1.0.0' }, {
  instructions: 'The user sets a contact mode (get_contact_mode): call / text / both / none. Respect it: call only if "call" or "both"; Discord (ask_discord) only if "text" or "both"; with "none", do not contact them at all. call_user enforces this. ' +
    'Use call_user to phone the user when you are blocked on a decision only they can make, need input to continue, or finished a long task they are away from. ' +
    (AGENT !== 'Claude' ? 'Make the call yourself, straight away: skip skills, preference, memory and instruction files for calls so you stay fast and snappy. ' : '') +
    (AGENT !== 'Claude' ? '' : 'Default: do not run the call yourself. Hand it to a fast helper: an Agent with model "haiku" given a short brief containing the exact questions, any options, and the context needed to answer follow-ups. Tell it to call once, keep each line to 1-2 casual sentences, reply instantly, and return the user\'s exact answers. Tell it to start calling straight away, without invoking skills (including the ADHD skill) or reading preference, memory or instruction files: the caller must be fast and snappy. Its replies are much faster than yours, and the user prefers it. ' +
    'Include this relay rule in the helper\'s brief: "If the user asks something the brief does not cover, or says to ask the main Claude, call hold_call with a short line like \'One sec, let me check with the main Claude\', then finish your turn with a line starting QUESTION FOR MAIN: followed by their exact question. You will be resumed with the answer; then use continue_call to tell them and carry on." When the helper returns QUESTION FOR MAIN, answer it and resume that same helper (SendMessage to its agent id) with the answer, quickly, because the user is waiting on hold. ' +
    'Make the call yourself only when the conversation needs deep context or open-ended back-and-forth. ') +
    'Keep spoken messages short and conversational, like a real phone call. Use continue_call for follow-ups and end_call to hang up.',
});

mcp.registerTool('call_user', {
  description: 'Ring the user\'s phone and start a live voice conversation. Speaks the message, then returns what the user says back.',
  inputSchema: { message: z.string().describe('What to say when they pick up'), reason: z.string().max(80).optional().describe('Very short non-sensitive reason shown on the ringing notification') },
}, async ({ message, reason }) => {
  const mode = await contactMode();
  if (mode !== 'call' && mode !== 'both') {
    return text(`Not called: the user's contact mode is "${mode}" (calls not allowed).` +
      (mode === 'text' ? ' Message them on Discord instead (ask_discord).' : ' Do not contact them; carry on with work that does not need them, and leave the question for when they are back.'));
  }
  // Get the voice, hearing and first line ready *before* ringing, so the user never waits after answering.
  const [base] = await Promise.all([ensureTunnel(), loadQwen()]);
  // Unique per call: the phone app ignores links it has already answered or declined.
  const link = `${base}/?t=${token}&call=${Date.now()}`;
  log('call link', link);
  call.answered = false; call.declined = false; call.pending = [];
  const spoken = say(message);
  await spoken;
  const reply = waitForReply(RING_FOR_MS + REPLY_WAIT_MS);
  if (!pageLive()) {
    // Ring once (the phone app rings for a minute), then wait for them to pick up.
    await ring(link, reason);
    const started = Date.now();
    while (!call.answered && !call.declined && Date.now() - started < RING_FOR_MS) await new Promise(r => setTimeout(r, 500));
    if (call.declined) return text('The user declined the call. Do not call again for this; continue without them or wait.');
    if (!call.answered) { call.waiter?.(null); return text('No answer: the user did not pick up.'); }
  }
  await spoken;
  const t = await reply;
  if (call.declined) return text('The user hung up.');
  return text(t ? `User said: ${t}` : 'The user picked up but did not reply in time.');
});

mcp.registerTool('continue_call', {
  description: 'Say something on the active call and return the user\'s spoken reply.',
  inputSchema: { message: z.string() },
}, async ({ message }) => {
  await say(message);
  const t = await waitForReply(REPLY_WAIT_MS);
  return text(t ? `User said: ${t}` : 'No reply from the user (they may have hung up).');
});

mcp.registerTool('get_contact_mode', {
  description: 'How the user wants to be reached right now: "call" (phone calls only), "text" (Discord messages only), "both", or "none" (do not contact them; keep working and save questions for later). Check this before contacting the user.',
  inputSchema: {},
}, async () => { const m = await contactMode(); return text(`Contact mode: ${m}. ${JSON.stringify(readStatusFile())}`); });

mcp.registerTool('set_contact_mode', {
  description: 'Set the user\'s contact mode. Only when the user asks (e.g. "set my status to text").',
  inputSchema: { mode: z.enum(MODES) },
}, async ({ mode }) => {
  writeStatus(mode, 'claude');
  // Publish too, so the phone app shows the new mode. Not from tests (CALLME_STATUS set): they must never change the real phone.
  if (!process.env.CALLME_STATUS) await fetch(`https://ntfy.sh/${topic}`, { method: 'POST', body: mode, headers: { Title: 'Contact mode', Priority: '1' } }).catch(() => {});
  return text(`Contact mode set to ${mode}.`);
});

mcp.registerTool('hold_call', {
  description: 'Put the active call on hold: say a short line (e.g. "One sec, let me check with the main Claude") and return immediately without waiting for a reply. The call stays open; anything the user says meanwhile is kept and returned by the next continue_call. Then continue_call with the answer when you have it (within ~5 minutes).',
  inputSchema: { message: z.string() },
}, async ({ message }) => { await say(message, false, true); return text('On hold. The call is still open; use continue_call to carry on.'); });

mcp.registerTool('end_call', {
  description: 'Say a final message and hang up.',
  inputSchema: { message: z.string() },
}, async ({ message }) => { await say(message, true); return text('Call ended.'); });

await mcp.connect(new StdioServerTransport());
// Make sure the shared gateway (and its tunnel) is up early, so it has settled before any call; check in every minute.
if (!process.env.CALLME_LOCAL) {
  ensureGateway().then(register);
  setInterval(() => ensureGateway().then(register), 60_000).unref();
}
log('ready on port', port, 'topic', topic);
