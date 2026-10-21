#!/usr/bin/env node
// After the Android build: read the MERGED manifest out of the built APK and fail if it is
// unsafe. scripts/harden-android.mjs checks the text of the generated project (the first line
// of defence); this checks what actually shipped, after Gradle merged every library's manifest
// and applied the build type's placeholders.
//
//   node scripts/check-apk-manifest.mjs path/to/app.apk [--build debug|release]
//
// It uses `aapt2 dump xmltree` from the Android SDK build-tools (found through ANDROID_HOME
// or ANDROID_SDK_ROOT), or `apkanalyzer manifest print` if there is no aapt2. Fails (exit 1)
// if, on <application>: allowBackup is not explicitly false; usesCleartextTraffic is not
// explicitly false; testOnly is true; a network security config is present (it could allow
// cleartext: check it by hand); or, for a release build, debuggable is true. A debug build
// is debuggable by design, which is reported but allowed.
//
// NOT RUN against a real APK here (there is no Android SDK on this machine): the parsers are
// tested on sample output in the formats of aapt2 and apkanalyzer. First run in CI: read the
// printed attributes.

import { spawnSync } from 'node:child_process';
import { existsSync, readdirSync } from 'node:fs';
import { join, resolve } from 'node:path';
import { fileURLToPath } from 'node:url';

/** true, false, 'reference' (a resource such as @0x7f...), or undefined (not understood). */
export function readValue(raw) {
  const text = raw.trim();
  if (text === 'true' || text === '"true"') return true;
  if (text === 'false' || text === '"false"') return false;
  const number = text.match(/^(?:\(type 0x[0-9a-f]+\))?(0x[0-9a-f]+|-?\d+)$/i);
  if (number) {
    const n = Number(number[1]);
    if (n === 0) return false;
    if (n === 0xffffffff || n === 1 || n === -1) return true;
    return undefined;
  }
  if (/^@/.test(text) || /^\(type 0x1\)/i.test(text)) return 'reference';
  return undefined;
}

/**
 * The attributes of <application> from `aapt2 dump xmltree` text, as {name: value}.
 * Lines look like `A: http://schemas.android.com/apk/res/android:allowBackup(0x01010280)=false`
 * (older tools print `=(type 0x12)0x0`), nested under `E: application`.
 */
export function parseAapt2(text) {
  const attrs = {};
  // An element's attribute lines come right after its `E:` line and before its children, so the
  // latest `E:` line says whose attribute this is (an <activity> inside <application> is not).
  let inApplication = false;
  for (const line of text.split(/\r?\n/)) {
    const element = line.match(/^\s*E: ([\w.:-]+)/);
    if (element) {
      inApplication = element[1] === 'application';
      continue;
    }
    const attribute = line.match(/^\s*A: [^\s(]*?([A-Za-z]+)\((0x[0-9a-f]+)\)=(.*)$/);
    if (attribute && inApplication) attrs[attribute[1]] = attribute[3].replace(/\s*\(Raw:.*$/, '');
  }
  return attrs;
}

/** The same from `apkanalyzer manifest print` (XML text). */
export function parseApkanalyzer(xml) {
  const application = xml.match(/<application\b([^>]*)>/s);
  const attrs = {};
  if (!application) return attrs;
  for (const m of application[1].matchAll(/android:([A-Za-z]+)\s*=\s*"([^"]*)"/g)) attrs[m[1]] = m[2];
  return attrs;
}

/** What is wrong with the merged <application>; an empty list means it is safe. */
export function manifestProblems(attrs, { build = 'debug' } = {}) {
  const found = [];
  const need = (name) => {
    if (!(name in attrs)) {
      found.push(`${name} is not set in the merged manifest`);
      return;
    }
    const value = readValue(attrs[name]);
    if (value !== false) found.push(`${name} is ${String(attrs[name]).trim() || 'empty'}, not false`);
  };
  need('allowBackup');
  need('usesCleartextTraffic');
  if ('testOnly' in attrs && readValue(attrs.testOnly) !== false) found.push('testOnly is set');
  if ('networkSecurityConfig' in attrs) found.push('a network security config is present: it can allow cleartext, check it by hand');
  if ('debuggable' in attrs && readValue(attrs.debuggable) !== false && build === 'release') {
    found.push('debuggable is true in a release build');
  }
  if ('debuggable' in attrs && readValue(attrs.debuggable) === undefined) found.push('debuggable has a value that is not understood');
  return found;
}

/** Version-aware newest `build-tools/<x.y.z>` folder holding `tool`. */
export function newestBuildTool(sdk, tool, list = (d) => readdirSync(d), exists = existsSync) {
  const root = join(sdk, 'build-tools');
  if (!exists(root)) return null;
  const key = (v) => v.split(/[.-]/).map((p) => (Number.isFinite(Number(p)) ? Number(p) : -1));
  const compare = (a, b) => {
    const [x, y] = [key(a), key(b)];
    for (let i = 0; i < Math.max(x.length, y.length); i += 1) {
      const d = (y[i] ?? 0) - (x[i] ?? 0);
      if (d !== 0) return d;
    }
    return 0;
  };
  for (const version of list(root).sort(compare)) {
    for (const name of [tool, `${tool}.exe`]) {
      const path = join(root, version, name);
      if (exists(path)) return path;
    }
  }
  return null;
}

function main() {
  const args = process.argv.slice(2);
  const apk = args.find((a) => !a.startsWith('--'));
  const buildAt = args.indexOf('--build');
  const build = buildAt >= 0 ? args[buildAt + 1] : 'debug';
  if (!apk || !['debug', 'release'].includes(build)) {
    console.error('usage: check-apk-manifest.mjs <apk> [--build debug|release]');
    process.exit(2);
  }
  const sdk = process.env.ANDROID_HOME || process.env.ANDROID_SDK_ROOT;
  if (!existsSync(apk)) {
    console.error(`check-apk-manifest: ${apk} not found`);
    process.exit(2);
  }
  let attrs = null;
  const aapt2 = sdk ? newestBuildTool(sdk, 'aapt2') : null;
  if (aapt2) {
    const run = spawnSync(aapt2, ['dump', 'xmltree', resolve(apk), '--file', 'AndroidManifest.xml'], { encoding: 'utf8' });
    if (run.status !== 0) {
      console.error(`check-apk-manifest: aapt2 failed: ${run.stderr}`);
      process.exit(2);
    }
    attrs = parseAapt2(run.stdout);
  } else if (sdk) {
    for (const rel of ['cmdline-tools/latest/bin/apkanalyzer', 'cmdline-tools/latest/bin/apkanalyzer.bat']) {
      const tool = join(sdk, rel);
      if (!existsSync(tool)) continue;
      const run = spawnSync(tool, ['manifest', 'print', resolve(apk)], { encoding: 'utf8', shell: tool.endsWith('.bat') });
      if (run.status === 0) attrs = parseApkanalyzer(run.stdout);
      break;
    }
  }
  if (!attrs) {
    console.error('check-apk-manifest: neither aapt2 nor apkanalyzer found under ANDROID_HOME / ANDROID_SDK_ROOT');
    process.exit(2);
  }
  console.log(`check-apk-manifest: merged <application> attributes: ${JSON.stringify(attrs)}`);
  const left = manifestProblems(attrs, { build });
  for (const problem of left) console.error(`check-apk-manifest: UNSAFE: ${problem}`);
  if (left.length > 0) process.exit(1);
  console.log(`check-apk-manifest: ${build} build: allowBackup=false, cleartext off${'debuggable' in attrs ? `, debuggable=${attrs.debuggable}` : ''}`);
}

if (process.argv[1] && resolve(process.argv[1]) === fileURLToPath(import.meta.url)) main();
