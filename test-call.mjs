// Self-check: starts the server (private test topic, no tunnel, its own status file: never the real phone or status),
// places a call and answers it with phone-sim.mjs (the real call page in headless Edge with a fake mic).
// Covers: contact-mode gate, streamed voice, live captions, early end of turn, talking over Claude (quietly and
// loudly), hold (a reply given while on hold is kept), continue -> gets the held reply at once, end.
import { Client } from '@modelcontextprotocol/sdk/client/index.js';
import { StdioClientTransport } from '@modelcontextprotocol/sdk/client/stdio.js';
import { spawn } from 'node:child_process';
import assert from 'node:assert';
import os from 'node:os';
import path from 'node:path';

const client = new Client({ name: 'test', version: '1' });
const statusFile = path.join(os.tmpdir(), `callme-status-${Date.now()}.json`); // never touches the real status
const transport = new StdioClientTransport({ command: path.join(import.meta.dirname, 'target/release/callme.exe'), args: [], cwd: import.meta.dirname, stderr: 'pipe',
  env: { ...process.env, CALLME_TOPIC: 'claude-test-' + Date.now(), CALLME_STATUS: statusFile, CALLME_LOCAL: '1', CALLME_DEBUG: '1' } });
let sim = null;
transport.stderr.on('data', d => {
  process.stderr.write(d);
  const m = String(d).match(/call link (\S+)/);
  if (m && !sim) sim = spawn('node', ['phone-sim.mjs', m[1]], { cwd: import.meta.dirname, stdio: 'inherit' });
});
await client.connect(transport);
const tools = (await client.listTools()).tools.map(t => t.name);
assert.deepStrictEqual(tools.sort(), ['call_user', 'continue_call', 'end_call', 'get_contact_mode', 'hold_call', 'set_contact_mode']);
const call = (name, message, extra = {}) => client.callTool({ name, arguments: { message, ...extra } }, undefined, { timeout: 600_000 });
const tool = (name, args = {}) => client.callTool({ name, arguments: args });
const said = r => r.content[0].text;

// Contact mode gate: "text" must refuse calls, "both" must allow them.
await tool('set_contact_mode', { mode: 'text' });
assert.match(said(await tool('get_contact_mode')), /Contact mode: text/);
assert.match(said(await call('call_user', 'should not ring')), /Not called/);
await tool('set_contact_mode', { mode: 'both' });
console.log('contact mode gate OK');

const r1 = await call('call_user', 'Hi, it is Claude. This is a test call. Say anything back.', { reason: 'test call' });
console.log('REPLY 1:', said(r1));
assert.match(said(r1), /^User said: .*hear you/i);

const r2 = await call('continue_call', 'Great. So the plan is to move the settings into a side panel, change the colours to match the new theme, and rename a few of the buttons so they read more clearly. Then I will run the tests and push it.');
console.log('REPLY 2 (talked over quietly):', said(r2));
assert.match(said(r2), /^User said: \(interrupting you\) .*layout/i);

assert.match(said(await call('hold_call', 'One sec, let me check with the main Claude.')), /On hold/);
await new Promise(r => setTimeout(r, 9000)); // the phone replies while on hold

let t = Date.now();
const r4 = await call('continue_call', 'Okay, I am back. The main Claude says yes.');
console.log('REPLY 4 (said while on hold):', said(r4), `| returned in ${Date.now() - t}ms`);
assert.match(said(r4), /^User said: .*blue/i);
assert.ok(Date.now() - t < 5000, 'reply given while on hold should come back straight away');

const r5 = await call('continue_call', 'Right, next I am going to delete the old screens, clear out the unused pictures, and tidy every file in the project so the whole thing is smaller and quicker to build.');
console.log('REPLY 5 (talked over loudly):', said(r5));
assert.match(said(r5), /^User said: \(interrupting you\) .*stop/i);

await call('end_call', 'Okay, I will leave it. Bye!');
await new Promise(r => sim.on('exit', r));
assert.equal(sim.exitCode, 0, 'phone sim failed');
await client.close();
console.log('PASS');
