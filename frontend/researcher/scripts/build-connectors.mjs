import { build } from 'vite';
import { readFile, writeFile, mkdir } from 'node:fs/promises';
import { fileURLToPath } from 'node:url';
const root = fileURLToPath(new URL('../', import.meta.url));
const out = fileURLToPath(new URL('../../../extensions/google-docs/', import.meta.url));
const result = await build({
  configFile: false,
  root,
  logLevel: 'warn',
  build: {
    write: false,
    minify: false,
    lib: {
      entry: root + 'src/lib/provenance/connector.ts',
      name: 'DynodocProvenance',
      formats: ['iife']
    }
  }
});
const output = Array.isArray(result) ? result[0] : result;
const source = output.output.find((x) => x.type === 'chunk').code;
await mkdir(out, { recursive: true });
const rendered = `<!-- Generated from the shared protocol. Run pnpm build:connectors. -->\n<script>\n${source.replaceAll('</script', '<\\/script')}\n</script>\n`;
const path = out + 'Protocol.html';
if (process.argv.includes('--check')) {
  if ((await readFile(path, 'utf8')) !== rendered)
    throw new Error('Google Docs protocol build is stale');
} else await writeFile(path, rendered);
