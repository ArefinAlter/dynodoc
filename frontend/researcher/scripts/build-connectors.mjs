import { build } from 'vite';
import { readFile, writeFile, mkdir } from 'node:fs/promises';
import { fileURLToPath } from 'node:url';
import { zipSync, strToU8 } from 'fflate';
const root = fileURLToPath(new URL('../', import.meta.url));
const extensions = fileURLToPath(new URL('../../../extensions/', import.meta.url));
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
const rendered = `<!-- Generated from the shared protocol. Run pnpm build:connectors. -->\n<script>\n${source.replaceAll('</script', '<\\/script')}\n</script>\n`;
async function artifact(path, content) {
  if (process.argv.includes('--check')) {
    if ((await readFile(path, 'utf8')) !== content)
      throw new Error(`Connector build is stale: ${path}`);
  } else await writeFile(path, content);
}
for (const host of ['google-docs', 'google-sheets', 'google-slides']) {
  const out = extensions + host + '/';
  await mkdir(out, { recursive: true });
  await artifact(out + 'Protocol.html', rendered);
  await artifact(
    out + 'Pairing.gs',
    await readFile(extensions + 'google-common/Pairing.gs', 'utf8')
  );
  if (host !== 'google-docs')
    await artifact(
      out + 'Common.gs',
      await readFile(extensions + 'google-common/Common.gs', 'utf8')
    );
}
// Apps Script's native Sheets adapter uses the same strict calendar validator.
const calendar = await build({
  configFile: false,
  root,
  logLevel: 'warn',
  build: {
    write: false,
    minify: false,
    lib: { entry: root + 'src/lib/provenance/date.ts', name: 'DynodocCalendar', formats: ['iife'] }
  }
});
const calendarOutput = Array.isArray(calendar) ? calendar[0] : calendar;
await artifact(
  extensions + 'google-sheets/Date.gs',
  '// Generated from the shared calendar validator. Run pnpm build:connectors.\n' +
    calendarOutput.output.find((x) => x.type === 'chunk').code
);

// Reproducible developer installation downloads, built from these exact sources.
const packages = root + 'static/connectors/packages/';
await mkdir(packages, { recursive: true });
for (const host of ['word', 'excel', 'powerpoint'])
  await artifact(
    packages + host + '.xml',
    await readFile(extensions + host + '/manifest.xml', 'utf8')
  );
for (const host of ['google-docs', 'google-sheets', 'google-slides']) {
  const names = ['Code.gs', 'Sidebar.html', 'Protocol.html', 'Pairing.gs', 'appsscript.json'];
  if (host !== 'google-docs') names.push('Common.gs');
  if (host === 'google-sheets') names.push('Date.gs');
  const entries = {};
  for (const name of names)
    entries[name] = [
      strToU8((await readFile(extensions + host + '/' + name, 'utf8')).replaceAll('\r\n', '\n')),
      { mtime: new Date(1980, 0, 1) }
    ];
  entries['INSTALL.md'] = [
    strToU8((await readFile(extensions + 'INSTALL.md', 'utf8')).replaceAll('\r\n', '\n')),
    { mtime: new Date(1980, 0, 1) }
  ];
  const bytes = zipSync(entries, { level: 9 });
  const path = packages + host + '.zip';
  if (process.argv.includes('--check')) {
    if (!Buffer.from(bytes).equals(await readFile(path)))
      throw new Error(`Connector package is stale: ${path}`);
  } else await writeFile(path, bytes);
}
