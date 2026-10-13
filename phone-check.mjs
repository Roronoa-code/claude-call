// Real-phone check of the call screen, without ringing: places a call on a private test topic (nobody listens to it),
// opens it on the phone through the app's CallCheck instrumentation, answers with typed replies, and saves
// screenshots, a screen recording and drawing/CPU stats.
//   node phone-check.mjs [device]          (device: adb serial; default the first one)
// Needs: the app and its check APK installed (gradlew installRelease installReleaseAndroidTest).
import { Client } from '@modelcontextprotocol/sdk/client/index.js';
import { StdioClientTransport } from '@modelcontextprotocol/sdk/client/stdio.js';
import { execFileSync, spawn } from 'node:child_process';
import { mkdirSync, writeFileSync } from 'node:fs';
import os from 'node:os';
import path from 'node:path';

const device = process.argv[2];
const adbArgs = a => (device ? ['-s', device, ...a] : a);
const adb = (...a) => execFileSync('adb', adbArgs(a), { maxBuffer: 64 << 20 });
const out = path.join('D:/Outputs/CallMe', `phone-check-${new Date().toISOString().slice(0, 16).replace(/[:T]/g, '')}`);
mkdirSync(out, { recursive: true });
const sleep = ms => new Promise(r => setTimeout(r, ms));
const t0 = Date.now();
const say = (...m) => console.log(`[${((Date.now() - t0) / 1000).toFixed(1)}s]`, ...m);
const shot = name => { writeFileSync(path.join(out, `${name}.png`), adb('exec-out', 'screencap', '-p')); say('shot', name); };

const transport = new StdioClientTransport({
  command: path.join(import.meta.dirname, 'target/release/callme.exe'), args: [], stderr: 'pipe',
  env: { ...process.env, CALLME_TOPIC: 'claude-check-' + Date.now(), CALLME_STATUS: path.join(os.tmpdir(), `callme-check-${Date.now()}.json`), CALLME_DEBUG: '1' },
});
let link = null, instrument = null;
transport.stderr.on('data', d => {
  const m = String(d).match(/call link (\S+)/);
  if (m && !link) {
    link = m[1];
    say('opening the call screen on the phone');
    instrument = spawn('adb', adbArgs(['shell', `am instrument -w -e link '${link}' fyi.mani.callme.test/fyi.mani.callme.CallCheck`]), { stdio: 'ignore' });
  }
});
const client = new Client({ name: 'phone-check', version: '1' });
await client.connect(transport);
const call = (name, message) => client.callTool({ name, arguments: { message } }, undefined, { timeout: 600_000 }).then(r => r.content[0].text);
const reply = async (text, after) => {
  await sleep(after);
  const [base, query] = link.split('/?');
  await fetch(`${base}/reply?${query.split('&')[0]}`, { method: 'POST', body: text });
  say('typed reply:', text);
};
const stats = label => {
  const g = String(adb('shell', 'dumpsys', 'gfxinfo', 'fyi.mani.callme'));
  const pick = k => (g.match(new RegExp(`${k}: ([^\\n]+)`)) || [])[1];
  const line = `${label}: frames ${pick('Total frames rendered')}, janky ${pick('Janky frames')}, p90 ${pick('90th percentile')}, p99 ${pick('99th percentile')}`;
  say(line);
  return line;
};
const cpu = () => {
  const pid = String(adb('shell', 'pidof', 'fyi.mani.callme')).trim();
  const top = String(adb('shell', 'top', '-b', '-n', '1', '-p', pid)).split('\n').find(l => l.includes(pid)) || '';
  return top.trim().split(/\s+/)[8] ?? '?';
};

const report = [];
const first = call('call_user', 'Hi, it is Claude. This is a quick test of the new call screen.');
while (!link) await sleep(200);
await sleep(9000);
shot('1-talking');
adb('shell', 'dumpsys', 'gfxinfo', 'fyi.mani.callme', 'reset');
const replied = reply('Looks good so far.', 3000);
say('REPLY 1', await first);
await replied;
await sleep(800);
shot('2-thinking');

// A long line: captions word by word, the glow and Clawd's dance; recorded and measured.
const rec = spawn('adb', adbArgs(['shell', 'screenrecord', '--time-limit', '20', '/sdcard/callme-check.mp4']), { stdio: 'ignore' });
adb('shell', 'dumpsys', 'gfxinfo', 'fyi.mani.callme', 'reset');
const long = call('continue_call', 'Great. So the plan is to move the settings into a side panel, change the colours to match the new theme, and rename a few of the buttons so they read more clearly. Then I will run the tests and push it.');
const cpus = [];
for (let i = 0; i < 6; i++) { await sleep(2500); cpus.push(cpu()); if (i === 2) shot('3-talking-long'); }
report.push(stats('talking'), `cpu while talking: ${cpus.join(', ')}%`);
const r2 = reply('Sounds right.', 0);
say('REPLY 2', await long);
await r2;
await new Promise(r => rec.on('exit', r));
adb('pull', '/sdcard/callme-check.mp4', path.join(out, 'talking.mp4'));

say(await call('hold_call', 'One sec, let me check with the main Claude.'));
await sleep(5000);
shot('4-hold');
adb('shell', 'dumpsys', 'gfxinfo', 'fyi.mani.callme', 'reset');
const idle = [];
for (let i = 0; i < 4; i++) { await sleep(2000); idle.push(cpu()); }
report.push(stats('on hold'), `cpu on hold: ${idle.join(', ')}%`);
const r3 = reply('Also make the button blue.', 0);
say('REPLY 3', await call('continue_call', 'Okay, I am back. The main Claude says yes.'));
await r3;
const bye = call('end_call', 'Okay, all done. Bye!');
await sleep(2500);
shot('5-ending');
await bye;
await new Promise(r => (instrument.exitCode !== null ? r() : instrument.on('exit', r)));
// The check ends by stopping the app (Android does that after instrumentation): start its call listener again.
adb('shell', 'am', 'start', '-n', 'fyi.mani.callme/.MainActivity');
await sleep(3000);
adb('shell', 'am', 'start', '-a', 'android.intent.action.MAIN', '-c', 'android.intent.category.HOME');
say('call listener restarted:', String(adb('shell', 'dumpsys', 'activity', 'services', 'fyi.mani.callme')).includes('ListenService'));
writeFileSync(path.join(out, 'report.txt'), report.join('\n') + '\n');
say('saved to', out);
await client.close();
