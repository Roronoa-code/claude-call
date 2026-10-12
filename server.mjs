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
const page = readFileSync(path.join(dir, 'page.html'), 'utf8');
const log = (...a) => console.error('[callme]', ...a);
// Latency marks for each reply turn, in timing.log: shows where the gap between "you stop" and "Claude speaks" goes.
let turnStart = 0;
const mark = (what, reset = false) => {
  if (reset) turnStart = Date.now();
  if (turnStart) appendFileSync(path.join(dir, 'timing.log'), `${new Date().toISOString()} +${Date.now() - turnStart}ms ${what}\n`);
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
const call = { seq: 0, say: '', end: false, parts: [], audio: [], answered: false, lastSeen: 0, waiter: null, pending: [] };

// Main voice + hearing on the GPU (voice/voice_server.py): the user's own voice cloned by Qwen3-TTS, and
// Whisper large. One instance is shared by all Claude sessions and exits itself after 30 idle minutes.
// Kokoro / CPU Whisper / the phone's voice are fallbacks.
const QWEN_PORT = 47811;
const qwenUp = () => fetch(`http://127.0.0.1:${QWEN_PORT}/health`).then(r => r.ok, () => false);
let qwenPromise = null;
const loadQwen = () => qwenPromise ??= (async () => {
  if (await qwenUp()) return true;
  // Machines without the GPU voice set up (e.g. the laptop) skip straight to ElevenLabs / Kokoro.
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
async function hear(raw, pcm, context) {
  const r = await gpu('/stt', { body: raw, headers: { 'x-context': encodeURIComponent(context) } });
  if (r) return (await r.text()).trim();
  return (await loadAsr()).transcribe(pcm.slice(), context);
}
let asrPromise = null;
const loadAsr = () => asrPromise ??= import('./asr.mjs');
// Whisper invents these on near-silence.
const NOISE = /^\s*(\[.*\]|\(.*\)|you|thank you\.?|thanks for watching!?|\.+)?\s*$/i;

const server = http.createServer((req, res) => {
  const url = new URL(req.url, 'http://x');
  if (url.searchParams.get('t') !== token) { res.writeHead(404).end(); return; }
  if (url.pathname === '/') { res.writeHead(200, { 'content-type': 'text/html; charset=utf-8' }).end(page); return; }
  if (url.pathname === '/state') {
    call.lastSeen = Date.now();
    res.writeHead(200, { 'content-type': 'application/json', 'cache-control': 'no-store' })
      .end(JSON.stringify({ seq: call.seq, say: call.say, end: call.end, parts: call.parts, ready: call.audio.length }));
    return;
  }
  const part = call.audio[Number(url.searchParams.get('part') ?? 0)];
  if (url.pathname === '/audio' && Number(url.searchParams.get('seq')) === call.seq && part) {
    if (url.searchParams.get('part') === '0') mark('phone fetched first audio');
    res.writeHead(200, { 'content-type': part.type, 'cache-control': 'no-store' }).end(part.data);
    return;
  }
  if (url.pathname === '/answer' && req.method === 'POST') { call.answered = true; res.writeHead(204).end(); return; }
  if (url.pathname === '/decline' && req.method === 'POST') {
    call.declined = true; res.writeHead(204).end();
    const w = call.waiter; call.waiter = null; w?.(null);
    return;
  }
  if (url.pathname === '/speech' && req.method === 'POST') {
    // Body: mono 16 kHz Float32 PCM, little-endian.
    const chunks = []; let size = 0;
    mark('speech upload started (you stopped talking ~1.8s before this)', true);
    req.on('data', c => { chunks.push(c); size += c.length; if (size > 16_000 * 4 * 120) req.destroy(); });
    req.on('end', async () => {
      try {
        const buf = Buffer.concat(chunks);
        mark(`speech uploaded (${(buf.length / 64000).toFixed(1)}s of audio)`);
        const pcm = new Float32Array(buf.buffer, buf.byteOffset, Math.floor(buf.length / 4));
        const heard = await hear(buf, pcm, call.say);
        mark('transcribed -> handed to Claude');
        const ok = !NOISE.test(heard);
        res.writeHead(200, { 'content-type': 'application/json' }).end(JSON.stringify({ text: ok ? heard : '' }));
        if (ok) deliver(url.searchParams.get('interrupted') ? `(interrupting you) ${heard}` : heard);
      } catch (e) {
        log('asr failed', e.message);
        res.writeHead(500).end();
      }
    });
    return;
  }
  if (url.pathname === '/reply' && req.method === 'POST') {
    let body = '';
    req.on('data', c => { body += c; if (body.length > 20_000) req.destroy(); });
    req.on('end', () => { res.writeHead(204).end(); deliver(body.trim()); });
    return;
  }
  res.writeHead(404).end();
});
await new Promise(r => server.listen(0, '127.0.0.1', r));
const port = server.address().port;

// Opened when the server starts (not at call time) and kept for this session. A new trycloudflare address
// takes ~30s to resolve on the phone, and a too-early lookup gets cached as "not found", so calls only ring
// once the address is at least TUNNEL_SETTLE_MS old.
const TUNNEL_SETTLE_MS = 45_000;
let tunnelPromise = null, tunnel = null, tunnelBornAt = 0;
const ensureTunnel = () => {
  if (!tunnelPromise || tunnel?.exitCode !== null) tunnelPromise = openTunnel().catch(e => { tunnelPromise = null; throw e; });
  return tunnelPromise.then(async url => {
    const wait = tunnelBornAt + TUNNEL_SETTLE_MS - Date.now();
    if (wait > 0) await new Promise(r => setTimeout(r, wait));
    return url;
  });
};
async function openTunnel() {
  tunnel = spawn(path.join(dir, 'bin', 'cloudflared.exe'),
    ['tunnel', '--no-autoupdate', '--url', `http://127.0.0.1:${port}`], { windowsHide: true });
  const tunnelUrl = await new Promise((resolve, reject) => {
    const timer = setTimeout(() => reject(new Error('tunnel did not start in 30s')), 30_000);
    const onData = d => {
      const m = String(d).match(/https:\/\/[a-z0-9-]+\.trycloudflare\.com/);
      if (m) { clearTimeout(timer); resolve(m[0]); }
    };
    tunnel.stderr.on('data', onData); tunnel.stdout.on('data', onData);
    tunnel.on('exit', c => { clearTimeout(timer); reject(new Error('cloudflared exited ' + c)); });
  });
  // Quick tunnels need a moment before the hostname resolves publicly.
  for (let i = 0; i < 20; i++) {
    try { if ((await fetch(`${tunnelUrl}/?t=${token}`)).ok) break; } catch {}
    await new Promise(r => setTimeout(r, 1500));
  }
  tunnelBornAt = Date.now();
  log('tunnel', tunnelUrl);
  return tunnelUrl;
}
const killTunnel = () => tunnel?.kill();
process.on('exit', killTunnel);
process.stdin.on('close', () => { killTunnel(); process.exit(0); });

async function ring(link, reason) {
  await fetch(`https://ntfy.sh/${topic}`, {
    method: 'POST',
    // ponytail: message text is not sent through ntfy (public server); only a generic alert + private link.
    body: reason ? `Claude needs you: ${reason}` : 'Claude needs to talk to you',
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
    const audio = await qwenSpeak(text) ?? await eleven(text);
    if (audio) return audio;
    const tts = await loadTts();
    if (tts) return { data: Buffer.from((await tts.generate(text, { voice: VOICE })).toWav()), type: 'audio/wav' };
  } catch (e) { log('tts failed', e.message); }
  return null; // page falls back to the phone's own voice for this part
}
// Speaks sentence by sentence so the page starts playing the first sentence while the rest are made.
// Resolves once the first sentence is ready.
function say(text, end = false) {
  mark('Claude replied (thinking done), making voice');
  // Two pieces: the first sentence (so playback starts fast), then the rest in one go (so the voice stays even).
  const sentences = text.match(/[^.!?]+(?:[.!?]+|$)/g)?.map(s => s.trim()).filter(Boolean) ?? [text];
  let n = 1;
  while (n < sentences.length && sentences.slice(0, n).join(' ').length < 25) n++; // don't start on a tiny fragment
  const parts = [sentences.slice(0, n).join(' '), sentences.slice(n).join(' ')].filter(Boolean);
  const seq = ++call.seq;
  Object.assign(call, { say: text, end, parts, audio: [] });
  let firstReady;
  const first = new Promise(r => { firstReady = r; });
  (async () => {
    for (const p of parts) {
      const a = await synth(p);
      if (call.seq !== seq) return; // superseded by a newer line
      call.audio.push(a);
      if (call.audio.length === 1) mark('first piece of voice ready');
      firstReady();
    }
  })();
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
const pageLive = () => Date.now() - call.lastSeen < 10_000;

const mcp = new McpServer({ name: 'callme', version: '1.0.0' }, {
  instructions: 'The user sets a contact mode (get_contact_mode): call / text / both / none. Respect it: call only if "call" or "both"; Discord (ask_discord) only if "text" or "both"; with "none", do not contact them at all. call_user enforces this. ' +
    'Use call_user to phone the user when you are blocked on a decision only they can make, need input to continue, or finished a long task they are away from. ' +
    'Default: do not run the call yourself. Hand it to a fast helper: an Agent with model "haiku" given a short brief containing the exact questions, any options, and the context needed to answer follow-ups. Tell it to call once, keep each line to 1-2 casual sentences, reply instantly, and return the user\'s exact answers. Its replies are much faster than yours, and the user prefers it. ' +
    'Include this relay rule in the helper\'s brief: "If the user asks something the brief does not cover, or says to ask the main Claude, call hold_call with a short line like \'One sec, let me check with the main Claude\', then finish your turn with a line starting QUESTION FOR MAIN: followed by their exact question. You will be resumed with the answer; then use continue_call to tell them and carry on." When the helper returns QUESTION FOR MAIN, answer it and resume that same helper (SendMessage to its agent id) with the answer, quickly, because the user is waiting on hold. ' +
    'Make the call yourself only when the conversation needs deep context or open-ended back-and-forth. ' +
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
  // Publish too, so the phone app shows the new mode.
  await fetch(`https://ntfy.sh/${topic}`, { method: 'POST', body: mode, headers: { Title: 'Contact mode', Priority: '1' } }).catch(() => {});
  return text(`Contact mode set to ${mode}.`);
});

mcp.registerTool('hold_call', {
  description: 'Put the active call on hold: say a short line (e.g. "One sec, let me check with the main Claude") and return immediately without waiting for a reply. The call stays open; anything the user says meanwhile is kept and returned by the next continue_call. Then continue_call with the answer when you have it (within ~5 minutes).',
  inputSchema: { message: z.string() },
}, async ({ message }) => { await say(message); return text('On hold. The call is still open; use continue_call to carry on.'); });

mcp.registerTool('end_call', {
  description: 'Say a final message and hang up.',
  inputSchema: { message: z.string() },
}, async ({ message }) => { await say(message, true); return text('Call ended.'); });

await mcp.connect(new StdioServerTransport());
ensureTunnel().catch(e => log('tunnel failed', e.message)); // open early so it has settled before any call
log('ready on port', port, 'topic', topic);
