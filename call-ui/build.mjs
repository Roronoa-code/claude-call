// Builds the call screen into one self-contained page (dist/page.html) that server.mjs can serve as is.
import * as esbuild from 'esbuild';
import { readFileSync, writeFileSync, mkdirSync } from 'node:fs';
import { join } from 'node:path';

const root = import.meta.dirname;
const r = await esbuild.build({
  entryPoints: [join(root, 'src/main.jsx')], bundle: true, minify: true, write: false, outdir: join(root, 'dist'),
  jsx: 'automatic', target: ['chrome110'], define: { 'process.env.NODE_ENV': '"production"' }, legalComments: 'none', logLevel: 'warning',
});
const pick = ext => r.outputFiles.find(f => f.path.endsWith(ext))?.text ?? '';
// Keep "</script>" inside the bundle from closing the tag early.
const js = pick('.js').replaceAll('</script', '<\/script');
const html = readFileSync(join(root, 'src/page.html'), 'utf8').replace('/*CSS*/', () => pick('.css')).replace('/*JS*/', () => js);
mkdirSync(join(root, 'dist'), { recursive: true });
writeFileSync(join(root, 'dist/page.html'), html);
console.log(`dist/page.html ${(html.length / 1024).toFixed(0)} KB`);
