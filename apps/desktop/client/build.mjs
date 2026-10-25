// Builds the Dynodoc client.
//
//   node client/build.mjs            -> ui/launcher.js and ui/launcher.css (the desktop app's page)
//   node client/build.mjs --check    -> fails if ui/launcher.js or ui/launcher.css differ from a fresh build
//   node client/build.mjs --web DIR  -> a hosted copy in DIR: index.html (noindex), launcher.js, launcher.css, mark.svg
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

async function bundle(web) {
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
    define: { __WEB__: String(web), __CLIENT_VERSION__: JSON.stringify(VERSION) },
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

const args = process.argv.slice(2);
if (args[0] === '--web') {
  const dir = resolve(args[1] ?? join(here, 'dist-web'));
  const js = await bundle(true);
  const styles = css();
  audit(js, styles);
  if (existsSync(dir)) for (const f of readdirSync(dir)) rmSync(join(dir, f), { recursive: true, force: true });
  mkdirSync(dir, { recursive: true });
  writeFileSync(join(dir, 'launcher.js'), js);
  writeFileSync(join(dir, 'launcher.css'), styles);
  copyFileSync(join(here, 'web', 'index.html'), join(dir, 'index.html'));
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
    console.log('ui/launcher.js and ui/launcher.css match the sources');
  } else {
    writeFileSync(join(ui, 'launcher.js'), js);
    writeFileSync(join(ui, 'launcher.css'), styles);
    console.log(`ui/launcher.js (${(js.length / 1024).toFixed(0)} KiB) and ui/launcher.css written`);
  }
}
