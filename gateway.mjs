// Shared tunnel gateway: ONE Cloudflare quick tunnel for every CallMe server on this PC (one per Claude/Codex session).
// trycloudflare rate-limits new tunnels (HTTP 429), so servers no longer open their own: each registers {token, port}
// here, and public requests are passed to the server whose call token (?t=) they carry, streamed straight through.
// Started hidden and detached by the first server.mjs that needs it; exits after 2 h with no servers registered.
//   Control (127.0.0.1:47810, never reachable through the tunnel):
//     GET  /health                  -> { url, bornAt, backends }   url is null while the tunnel is (re)starting;
//                                      bornAt = when it became reachable (servers wait ~45 s after that before ringing)
//     POST /register {token, port}     servers repeat it every minute; entries lapse after 3 minutes
//   Public (127.0.0.1:47809, what the tunnel points at): everything else.
// Log: gateway.log
import http from 'node:http';
import { spawn } from 'node:child_process';
import { appendFileSync, writeFileSync } from 'node:fs';
import { fileURLToPath } from 'node:url';
import path from 'node:path';

const dir = path.dirname(fileURLToPath(import.meta.url));
const CONTROL_PORT = 47810, PUBLIC_PORT = 47809;
const IDLE_EXIT_MS = 2 * 60 * 60_000, BACKEND_TTL_MS = 3 * 60_000;
const logFile = path.join(dir, 'gateway.log');
const log = (...a) => appendFileSync(logFile, `${new Date().toISOString()} ${a.join(' ')}\n`);

const backends = new Map(); // token -> { port, seen }
let lastBackendAt = Date.now();
let tunnel = null, url = null, bornAt = 0;

const pub = http.createServer((req, res) => {
  const u = new URL(req.url, 'http://x');
  if (u.pathname === '/__gateway') { res.writeHead(200).end('ok'); return; } // reachability probe
  const t = u.searchParams.get('t');
  // Calls carry their token; untokened requests are the call page's public script, which any server can give.
  const b = t ? backends.get(t) : [...backends.values()].at(-1);
  if (!b) { res.writeHead(404).end(); return; }
  const out = http.request({ host: '127.0.0.1', port: b.port, method: req.method, path: req.url, headers: req.headers }, back => {
    res.writeHead(back.statusCode, back.headers);
    back.pipe(res); // chunk by chunk: the voice stream is never held back
  });
  out.on('error', () => { if (!res.headersSent) res.writeHead(502); res.end(); });
  res.on('close', () => out.destroy()); // the phone went away: drop the request (long polls, streams)
  req.pipe(out);
});

const control = http.createServer((req, res) => {
  if (req.method === 'GET' && req.url === '/health') {
    res.writeHead(200, { 'content-type': 'application/json' }).end(JSON.stringify({ url, bornAt, backends: backends.size }));
    return;
  }
  if (req.method === 'POST' && req.url === '/register') {
    let body = '';
    req.on('data', c => { body += c; if (body.length > 1000) req.destroy(); });
    req.on('end', () => {
      try {
        const { token, port } = JSON.parse(body);
        if (!/^[0-9a-f]{32}$/.test(token) || !Number.isInteger(port)) throw new Error('bad registration');
        if (!backends.has(token)) log('server registered on port', port);
        backends.set(token, { port, seen: Date.now() }); lastBackendAt = Date.now();
        res.writeHead(204).end();
      } catch { res.writeHead(400).end(); }
    });
    return;
  }
  res.writeHead(404).end();
});

// Both ports first: if another gateway already has them, this one just exits (no second tunnel).
const listen = (srv, port) => new Promise((resolve, reject) => { srv.once('error', reject); srv.listen(port, '127.0.0.1', resolve); });
try { await listen(control, CONTROL_PORT); await listen(pub, PUBLIC_PORT); } catch (e) { process.exit(0); }
writeFileSync(logFile, '');
log('gateway up, pid', process.pid);

// The tunnel: started once and kept; restarted if it dies. Backs off when trycloudflare says 429.
async function keepTunnel() {
  let wait = 0;
  for (;;) {
    if (wait) { log(`tunnel retry in ${wait / 1000}s`); await new Promise(r => setTimeout(r, wait)); }
    try {
      await openTunnel();
      wait = 0;
      if (tunnel.exitCode === null) await new Promise(r => tunnel.once('exit', r)); // runs until it dies
      log('tunnel died; restarting');
      url = null; bornAt = 0;
    } catch (e) {
      log('tunnel failed:', e.message);
      tunnel?.kill();
      wait = Math.min(Math.max(wait * 2, /429/.test(e.message) ? 30_000 : 5_000), 5 * 60_000);
    }
  }
}
async function openTunnel() {
  tunnel = spawn(path.join(dir, 'bin', 'cloudflared.exe'), ['tunnel', '--no-autoupdate', '--url', `http://127.0.0.1:${PUBLIC_PORT}`],
    { windowsHide: true, stdio: ['ignore', 'pipe', 'pipe'] });
  const found = await new Promise((resolve, reject) => {
    const timer = setTimeout(() => reject(new Error('tunnel did not start in 30s')), 30_000);
    const onData = d => {
      const s = String(d), m = s.match(/https:\/\/[a-z0-9-]+\.trycloudflare\.com/);
      if (m) { clearTimeout(timer); resolve(m[0]); }
      const fail = s.match(/failed with status \d+|provisioning failed[^\n]*/);
      if (fail) { clearTimeout(timer); reject(new Error(fail[0])); }
    };
    tunnel.stderr.on('data', onData); tunnel.stdout.on('data', onData);
    tunnel.once('exit', c => { clearTimeout(timer); reject(new Error('cloudflared exited ' + c)); });
  });
  // A new quick tunnel takes a moment before its name resolves publicly.
  for (let i = 0; i < 40; i++) {
    try { if ((await fetch(`${found}/__gateway`, { signal: AbortSignal.timeout(5000) })).ok) break; } catch {}
    await new Promise(r => setTimeout(r, 1500));
  }
  url = found; bornAt = Date.now();
  log('tunnel', url);
}
keepTunnel();

// Forget servers that stopped checking in; leave (and free the tunnel) after a long time with none.
setInterval(() => {
  for (const [t, b] of backends) if (Date.now() - b.seen > BACKEND_TTL_MS) { backends.delete(t); log('server gone, port', b.port); }
  if (backends.size) lastBackendAt = Date.now();
  else if (Date.now() - lastBackendAt > IDLE_EXIT_MS) { log('idle, exiting'); tunnel?.kill(); process.exit(0); }
}, 60_000);
process.on('exit', () => tunnel?.kill());
