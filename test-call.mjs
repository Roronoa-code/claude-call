// Self-check: starts the server, places a call (private test topic unless CALLME_TOPIC is set), and drives it.
// Answer with: node phone-sim.mjs <call link>  (printed in the server log as "call link").
// Covers: call -> reply, hold (reply given while on hold is kept), continue -> gets the held reply at once, end.
import { Client } from '@modelcontextprotocol/sdk/client/index.js';
import { StdioClientTransport } from '@modelcontextprotocol/sdk/client/stdio.js';
import assert from 'node:assert';
import os from 'node:os';
import path from 'node:path';

const client = new Client({ name: 'test', version: '1' });
const statusFile = path.join(os.tmpdir(), `callme-status-${Date.now()}.json`); // never touches the real status
await client.connect(new StdioClientTransport({ command: 'node', args: ['server.mjs'], env: { ...process.env, CALLME_TOPIC: process.env.CALLME_TOPIC || 'claude-test-' + Date.now(), CALLME_STATUS: statusFile }, cwd: import.meta.dirname, stderr: 'inherit' }));
const tools = (await client.listTools()).tools.map(t => t.name);
assert.deepStrictEqual(tools.sort(), ['call_user', 'continue_call', 'end_call', 'get_contact_mode', 'hold_call', 'set_contact_mode']);
const call = (name, message, extra = {}) => client.callTool({ name, arguments: { message, ...extra } }, undefined, { timeout: 600_000 });
const tool = (name, args = {}) => client.callTool({ name, arguments: args });

// Contact mode gate: "text" must refuse calls, "both" must allow them.
await tool('set_contact_mode', { mode: 'text' });
assert.match((await tool('get_contact_mode')).content[0].text, /Contact mode: text/);
assert.match((await call('call_user', 'should not ring')).content[0].text, /Not called/);
await tool('set_contact_mode', { mode: 'both' });
console.log('contact mode gate OK');

const first = await call('call_user', 'Hi, it is Claude. This is a test call. Say anything back.', { reason: 'test call' });
console.log('REPLY 1:', first.content[0].text);
assert.match(first.content[0].text, /^User said: /);

const hold = await call('hold_call', 'One sec, let me check with the main Claude.');
assert.match(hold.content[0].text, /On hold/);
await new Promise(r => setTimeout(r, 8000)); // the phone replies while on hold

let t = Date.now();
const second = await call('continue_call', 'Okay, I am back. The main Claude says yes. Anything else?');
console.log('REPLY 2 (said while on hold):', second.content[0].text, `| returned in ${Date.now() - t}ms`);
assert.match(second.content[0].text, /^User said: /);
assert.ok(Date.now() - t < 5000, 'reply given while on hold should come back straight away');

await call('end_call', 'Thanks, bye!');
await new Promise(r => setTimeout(r, 3000));
await client.close();
console.log('PASS');
