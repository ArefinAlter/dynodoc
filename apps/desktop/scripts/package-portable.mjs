#!/usr/bin/env node
// Assembles the portable Windows zip from a release build: the program, the two engine programs, the
// PostgreSQL tree and the notices in one folder, with a `dynodoc-data` folder beside the program (its
// presence is what makes the app keep everything in that folder). Needs the release build, the files
// `prepare-sidecar.mjs` and `prepare-pg.mjs` made, and Windows 10's tar (it writes zip).
//
//   node scripts/package-portable.mjs <out folder>           (CARGO_TARGET_DIR is honoured)
import { spawnSync } from 'node:child_process';
import { cpSync, existsSync, mkdirSync, readFileSync, rmSync, statSync, writeFileSync } from 'node:fs';
import { createHash } from 'node:crypto';
import { dirname, join, resolve } from 'node:path';
import { fileURLToPath } from 'node:url';

const here = dirname(fileURLToPath(import.meta.url));
const app = resolve(here, '..');
const tauri = join(app, 'src-tauri');
const out = resolve(process.argv[2] ?? join(app, 'dist'));
const version = /^version\s*=\s*"([^"]+)"/m.exec(readFileSync(join(tauri, 'Cargo.toml'), 'utf8'))[1];
const targetDir = process.env.CARGO_TARGET_DIR ?? join(tauri, 'target');
const triple = 'x86_64-pc-windows-msvc';
const exe = join(targetDir, 'release', 'dynodoc-desktop.exe');
const parts = {
  exe,
  api: join(tauri, 'binaries', `engine-api-${triple}.exe`),
  cli: join(tauri, 'binaries', `engine-cli-${triple}.exe`),
  pg: join(tauri, 'resources', 'pg'),
  notices: join(tauri, 'resources', 'notices'),
};
for (const [name, path] of Object.entries(parts)) if (!existsSync(path)) { console.error(`package-portable: ${name} is missing: ${path}`); process.exit(1); }

const label = `Dynodoc_${version.replace('-beta', 'b').replace('-alpha', 'a')}_windows-x64-portable`;
const work = join(out, label);
rmSync(work, { recursive: true, force: true });
mkdirSync(join(work, 'dynodoc-data'), { recursive: true });
cpSync(parts.exe, join(work, 'Dynodoc.exe'));
cpSync(parts.api, join(work, 'engine-api.exe'));
cpSync(parts.cli, join(work, 'engine-cli.exe'));
cpSync(parts.pg, join(work, 'pg'), { recursive: true });
cpSync(parts.notices, join(work, 'notices'), { recursive: true });
writeFileSync(join(work, 'dynodoc-data', 'README.txt'), 'Everything this copy of Dynodoc keeps (settings, the workspace with its database, the web view\'s data) is in this folder.\r\nDelete the folder to make the app use the per-user folders instead; copy the whole Dynodoc folder to move it.\r\n');
writeFileSync(join(work, 'README-portable.txt'), [
  'Dynodoc portable (pre-alpha)',
  '',
  'Run Dynodoc.exe. Keep the other files beside it (engine-api.exe, engine-cli.exe, pg and notices).',
  'Windows 10 or 11, x64. It needs WebView2, which Windows 11 has.',
  '',
  'The folder named dynodoc-data holds your workspace and settings; the database passwords are in a',
  'file inside it, so the whole folder opens wherever you copy it (anyone who can read the folder',
  'can read that file). Provider keys and sign-in sessions are kept in the Windows Credential Manager',
  'under a name made from this folder\'s path: after moving the folder, enter a key or sign in again.',
  'The program is not signed: SmartScreen may warn on first run (More info, then Run anyway).',
  '',
].join('\r\n'));

const zip = join(out, `${label}.zip`);
rmSync(zip, { force: true });
const tar = process.env.SystemRoot ? join(process.env.SystemRoot, 'System32', 'tar.exe') : 'tar';
const r = spawnSync(tar, ['-a', '-c', '-f', zip, '-C', out, label], { stdio: 'inherit' });
if (r.status !== 0) { console.error('package-portable: tar failed'); process.exit(1); }
const digest = createHash('sha256').update(readFileSync(zip)).digest('hex');
console.log(`package-portable: ${zip}`);
console.log(`package-portable: ${statSync(zip).size} bytes, sha256 ${digest}`);
