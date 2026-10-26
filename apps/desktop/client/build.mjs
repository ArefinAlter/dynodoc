// Builds the Dynodoc client.
//
//   node client/build.mjs            -> ui/launcher.js and ui/launcher.css (the desktop app's page)
//   node client/build.mjs --check    -> fails if ui/launcher.js or ui/launcher.css differ from a fresh build
//   node client/build.mjs --web DIR [--base /desktop/]  -> a hosted copy in DIR: index.html (noindex), launcher.js, launcher.css, mark.svg
//
// ui/launcher.js and ui/launcher.css are committed: Tauri ships ui/ as it is, and the policy tests in
// src-tauri read them. They are generated from client/src; edit the sources and rebuild. The bundle is
// one classic script (no modules, no imports at run time, no eval), as the launcher's
// Content-Security-Policy and policy tests require.
import { build } from 'esbuild';
import { readFileSync, writeFileSync, mkdirSync, copyFileSync, readdirSync, rmSync, existsSync } from 'node:fs';
import { dirname, join, resolve } from 'node:path';
import { fileURLToPath } from 'node:url';

const here = dirname(fileURLToPath(import.meta.url));
const app = resolve(here, '..');
const ui = join(app, 'ui');
// One version for the app: the one in src-tauri/Cargo.toml.
const VERSION = /^version\s*=\s*"([^"]+)"/m.exec(readFileSync(join(app, 'src-tauri', 'Cargo.toml'), 'utf8'))[1];

const HEADER = '// GENERATED from apps/desktop/client/src by client/build.mjs. Do not edit; edit the sources and rebuild.\n';
const CSS_ORDER = ['tokens.css', 'app.css'];

async function bundle(web, base = '') {
  const out = await build({
    entryPoints: [join(here, 'src', 'main.ts')],
    bundle: true,
    write: false,
    format: 'iife',
    target: ['es2020', 'chrome105', 'safari15'],
    platform: 'browser',
    charset: 'utf8',
    legalComments: 'none',
    minifySyntax: true,
    define: { __WEB__: String(web), __ASSET_BASE__: JSON.stringify(base), __CLIENT_VERSION__: JSON.stringify(VERSION) },
    logLevel: 'warning',
  });
  return HEADER + out.outputFiles[0].text;
}

function css() {
  return (
    '/* GENERATED from apps/desktop/client/src/styles by client/build.mjs. Do not edit. */\n' +
    CSS_ORDER.map((f) => readFileSync(join(here, 'src', 'styles', f), 'utf8')).join('\n')
  );
}

/** Words the launcher's policy test (src-tauri/src/policy_tests.rs) refuses in launcher.js. */
const BANNED = ['eval(', 'new Function', 'innerHTML', 'outerHTML', 'insertAdjacentHTML', 'document.write', 'fetch(', 'XMLHttpRequest', 'WebSocket', 'import(', 'importScripts', 'postMessage', 'localStorage', 'sessionStorage', 'setTimeout("', 'javascript:', '__TAURI__.'];

function audit(js, styles) {
  const bad = BANNED.filter((w) => js.includes(w));
  if (bad.length) throw new Error(`the bundle contains ${bad.join(', ')}; the launcher's policy refuses it`);
  if (styles.includes('@import') || styles.includes('url(')) throw new Error('the stylesheet must load nothing');
}

// ---- the Office codec (office-host.js on the page, office-worker.js in a worker) -------------------
// The worker bundles the web workspace's own importers, exporters and push planner. Their
// dependencies (exceljs, docx, pptxgenjs, jszip, @xmldom/xmldom, ulid, ...) come from
// frontend/researcher/node_modules, so the worker is rebuilt only where that is installed; the
// built files are committed, like launcher.js.
const frontend = resolve(app, '..', '..', 'frontend', 'researcher');
const haveFrontend = existsSync(join(frontend, 'node_modules'));

const STUBS = ['latex', 'research-files', 'office-import'];
const stubs = {
  name: 'office-stubs',
  setup(b) {
    b.onResolve({ filter: new RegExp(`^\\./(${STUBS.join('|')})$`) }, (a) =>
      a.importer.includes(join('lib', 'workspace')) ? { path: join(here, 'office', 'stubs', `${a.path.slice(2)}.ts`) } : undefined,
    );
    // The web workspace sends a legacy .doc to its converter service; the desktop app has no such
    // service, so that one request becomes a plain refusal (the worker refuses .doc before this).
    b.onLoad({ filter: /workspace[\\/]files\.ts$/ }, (a) => {
      const text = readFileSync(a.path, 'utf8').replace(
        /await fetch\('\/api\/convert-doc', \{ method: 'POST', body: data \}\);/,
        "await Promise.reject(new Error('Legacy .doc files are not supported in the desktop app. Save the file as .docx first.'));",
      );
      return { contents: text, loader: 'ts', resolveDir: dirname(a.path) };
    });
  },
};

async function bundleOffice(entry, extra = {}) {
  const out = await build({
    entryPoints: [join(here, 'office', entry)],
    bundle: true,
    write: false,
    format: 'iife',
    target: ['es2020', 'chrome105', 'safari15'],
    platform: 'browser',
    charset: 'utf8',
    legalComments: 'none',
    logLevel: 'error',
    minify: true,
    tsconfigRaw: '{}',
    alias: { $lib: join(frontend, 'src', 'lib') },
    plugins: [stubs],
    ...extra,
  });
  return HEADER + out.outputFiles[0].text;
}

/** Words the codec files must not contain: any network or cross-origin reach of their own. */
const OFFICE_BANNED = ['fetch(', 'WebSocket', 'sendBeacon', 'EventSource'];

async function officeFiles() {
  const host = await bundleOffice('host.ts');
  const worker = haveFrontend ? await bundleOffice('worker.ts') : null;
  for (const [name, text] of [['office-host.js', host], ['office-worker.js', worker]]) {
    if (!text) continue;
    const bad = OFFICE_BANNED.filter((w) => text.includes(w)); if (bad.length && process.env.OFFICE_DEBUG) { const at = text.indexOf(bad[0]); console.error(text.slice(Math.max(0, at - 200), at + 120)); }
    if (bad.length) throw new Error(`${name} contains ${bad.join(', ')}; the codec must have no network of its own`);
  }
  return { host, worker };
}

const args = process.argv.slice(2);
if (args[0] === '--web') {
  const dir = resolve(args[1] ?? join(here, 'dist-web'));
  // --base /desktop/: the path the copy is served under, so /desktop (no slash) still finds its files.
  const bi = args.indexOf('--base');
  const base = bi > 0 ? args[bi + 1] : '';
  if (base && !/^\/[a-z0-9/-]*\/$/.test(base)) throw new Error('--base is a path such as /desktop/');
  const js = await bundle(true, base);
  const styles = css();
  audit(js, styles);
  if (existsSync(dir)) for (const f of readdirSync(dir)) rmSync(join(dir, f), { recursive: true, force: true });
  mkdirSync(dir, { recursive: true });
  writeFileSync(join(dir, 'launcher.js'), js);
  writeFileSync(join(dir, 'launcher.css'), styles);
  writeFileSync(join(dir, 'index.html'), readFileSync(join(here, 'web', 'index.html'), 'utf8').replaceAll('{{BASE}}', base));
  copyFileSync(join(ui, 'mark.svg'), join(dir, 'mark.svg'));
  console.log(`web copy written to ${dir}`);
} else {
  const js = await bundle(false);
  const styles = css();
  audit(js, styles);
  if (args[0] === '--check') {
    const same = readFileSync(join(ui, 'launcher.js'), 'utf8') === js && readFileSync(join(ui, 'launcher.css'), 'utf8') === styles;
    if (!same) {
      console.error('ui/launcher.js or ui/launcher.css is out of date: run `npm run build:client` in apps/desktop.');
      process.exit(1);
    }
    const office = await officeFiles();
    if (readFileSync(join(ui, 'office-host.js'), 'utf8') !== office.host || (office.worker && readFileSync(join(ui, 'office-worker.js'), 'utf8') !== office.worker)) {
      console.error('ui/office-host.js or ui/office-worker.js is out of date: run `npm run build:client` in apps/desktop (with frontend/researcher installed).');
      process.exit(1);
    }
    console.log(`ui/launcher.js, launcher.css and the Office codec match the sources${office.worker ? '' : ' (the worker was not rebuilt: frontend/researcher is not installed here)'}`);
  } else {
    writeFileSync(join(ui, 'launcher.js'), js);
    writeFileSync(join(ui, 'launcher.css'), styles);
    const office = await officeFiles();
    writeFileSync(join(ui, 'office-host.js'), office.host);
    if (office.worker) writeFileSync(join(ui, 'office-worker.js'), office.worker);
    console.log(`ui/launcher.js (${(js.length / 1024).toFixed(0)} KiB), launcher.css, office-host.js${office.worker ? ` and office-worker.js (${(office.worker.length / 1024).toFixed(0)} KiB)` : ' written (the worker needs frontend/researcher installed)'}`);
  }
}
