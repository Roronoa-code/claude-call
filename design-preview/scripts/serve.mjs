// Builds the bundle and serves the preview at http://127.0.0.1:5178 (rebuilds on change).
import { createServer } from 'node:http';
import { readFile } from 'node:fs/promises';
import { extname, join, normalize } from 'node:path';
import * as esbuild from 'esbuild';

const root = join(import.meta.dirname, '..');
// The D design's source lives in ../call-ui (it is the real call screen); make every import of these
// packages use the preview's copy, so the page never carries two Reacts.
const pkgs = ['react', 'react-dom', 'voice-glow', 'thinking-orbs'];
export const alias = Object.fromEntries(pkgs.map(n => [n, join(root, 'node_modules', n)]));
const ctx = await esbuild.context({ entryPoints: [join(root, 'src/main.jsx')], bundle: true, outfile: join(root, 'dist/app.js'), jsx: 'automatic', logLevel: 'warning', alias });
if (process.argv.includes('--build')) { await ctx.rebuild(); await ctx.dispose(); process.exit(0); }
await ctx.watch();
const types = { '.html': 'text/html', '.js': 'text/javascript', '.css': 'text/css', '.png': 'image/png', '.svg': 'image/svg+xml' };
createServer(async (req, res) => {
  const path = normalize(decodeURIComponent(new URL(req.url, 'http://x').pathname)).replace(/^[\\/]+/, '') || 'index.html';
  if (path.startsWith('..')) return res.writeHead(403).end();
  try {
    const body = await readFile(join(root, path));
    res.writeHead(200, { 'content-type': types[extname(path)] || 'application/octet-stream', 'cache-control': 'no-store' }).end(body);
  } catch { res.writeHead(404).end('Not found'); }
}).listen(5178, '127.0.0.1', () => console.log('Preview: http://127.0.0.1:5178'));
