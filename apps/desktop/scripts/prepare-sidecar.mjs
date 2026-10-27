#!/usr/bin/env node
// Builds the two engine programs the app starts, `engine-api` (the engine's HTTP API, on loopback) and
// `engine-cli` (migrations and the key tools), from the repository root, and copies them to
// src-tauri/binaries/<name>-<target-triple>[.exe], the names Tauri's `externalBin` needs. Run it before
// building the desktop app (tauri-build refuses to start without the files). The binaries folder is
// git-ignored.
//
//   node scripts/prepare-sidecar.mjs                         release build for this machine
//   node scripts/prepare-sidecar.mjs --debug                 faster, larger, for development
//   node scripts/prepare-sidecar.mjs --target aarch64-apple-darwin
//   node scripts/prepare-sidecar.mjs --skip-build            only copy programs you built
//
// CARGO_TARGET_DIR is honoured (the location comes from `cargo metadata`).

import { spawnSync } from 'node:child_process';
import { createHash } from 'node:crypto';
import { copyFileSync, existsSync, mkdirSync, readFileSync, statSync } from 'node:fs';
import { dirname, join, resolve } from 'node:path';
import { fileURLToPath } from 'node:url';

const here = dirname(fileURLToPath(import.meta.url));
const repoRoot = resolve(here, '..', '..', '..');
const binaries = resolve(here, '..', 'src-tauri', 'binaries');
const PROGRAMS = ['engine-cli', 'engine-api'];

function fail(message) {
  console.error(`prepare-sidecar: ${message}`);
  process.exit(1);
}

function run(command, args, options = {}) {
  const result = spawnSync(command, args, { cwd: repoRoot, encoding: 'utf8', ...options });
  if (result.error) fail(`could not run ${command}: ${result.error.message}`);
  return result;
}

const args = process.argv.slice(2);
let target = null;
let release = true;
let skipBuild = false;
for (let i = 0; i < args.length; i += 1) {
  const arg = args[i];
  if (arg === '--target') {
    target = args[(i += 1)];
    if (!target || !/^[A-Za-z0-9_.-]+$/.test(target)) fail('--target needs a target triple');
  } else if (arg === '--debug') release = false;
  else if (arg === '--release') release = true;
  else if (arg === '--skip-build') skipBuild = true;
  else fail(`unknown argument: ${arg}`);
}

const hostLine = run('rustc', ['-vV']).stdout.split('\n').find((l) => l.startsWith('host: '));
if (!hostLine) fail('could not read the host target from `rustc -vV`');
const host = hostLine.slice('host: '.length).trim();
const triple = target ?? host;
if (/android|ios/.test(triple)) {
  fail(`${triple}: phone apps have no bundled engine programs (the sidecars are desktop only)`);
}
const exe = triple.includes('windows') ? '.exe' : '';
const profile = release ? 'release' : 'debug';

if (!skipBuild) {
  const build = ['build', ...PROGRAMS.flatMap((p) => ['-p', p]), '--locked'];
  if (release) build.push('--release');
  if (triple !== host) build.push('--target', triple);
  console.log(`prepare-sidecar: cargo ${build.join(' ')}  (in ${repoRoot})`);
  const result = run('cargo', build, { stdio: 'inherit' });
  if (result.status !== 0) fail('cargo build failed');
}

const meta = run('cargo', ['metadata', '--no-deps', '--format-version', '1', '--locked']);
if (meta.status !== 0) fail(`cargo metadata failed: ${meta.stderr}`);
const targetDir = JSON.parse(meta.stdout).target_directory;
mkdirSync(binaries, { recursive: true });
for (const name of PROGRAMS) {
  const built = triple === host ? join(targetDir, profile, `${name}${exe}`) : join(targetDir, triple, profile, `${name}${exe}`);
  if (!existsSync(built)) fail(`expected ${built} after the build; it does not exist`);
  const destination = join(binaries, `${name}-${triple}${exe}`);
  copyFileSync(built, destination);
  const digest = createHash('sha256').update(readFileSync(destination)).digest('hex');
  console.log(`prepare-sidecar: ${destination}`);
  console.log(`prepare-sidecar: ${statSync(destination).size} bytes, sha256 ${digest}`);
}
