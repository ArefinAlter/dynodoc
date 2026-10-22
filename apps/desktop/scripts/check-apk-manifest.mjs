#!/usr/bin/env node
// After the Android build: read the MERGED manifest out of the built APK and fail if it is
// unsafe or surprising. scripts/harden-android.mjs checks the text of the generated project (the
// first line of defence); this checks what actually shipped, after Gradle merged every library's
// manifest and applied the build type's placeholders.
//
//   node scripts/check-apk-manifest.mjs path/to/app.apk [--build debug|release]
//
// It uses `aapt2 dump xmltree` from the Android SDK build-tools (found through ANDROID_HOME or
// ANDROID_SDK_ROOT), or `apkanalyzer manifest print` if there is no aapt2. Both are parsed into
// the same element tree; ANYTHING the parser does not understand is a failure (exit 2), never a
// pass. Then `auditManifest` fails (exit 1) unless:
//   <application>: allowBackup is false, dataExtractionRules is set, usesCleartextTraffic is false,
//     no network security config, testOnly is not true, debuggable is not true in a release build;
//   permissions: only INTERNET and the signature permission AndroidX adds for its own receivers;
//   exported components: only MainActivity and AndroidX's ProfileInstallReceiver (guarded by the
//     DUMP permission); every other component is `exported="false"`;
//   intent filters of MainActivity: the launcher, and `dynodoc://open/...` links (VIEW, DEFAULT,
//     BROWSABLE; scheme dynodoc, host open, the three path shapes; no autoVerify);
//   minSdkVersion equals bundle.android.minSdkVersion in tauri.conf.json, targetSdkVersion >= 35.
// The signing certificate is read with `apksigner`: a debug build must carry the SDK's debug key,
// a release build must not. A debug build is debuggable by design; that is printed, not failed.
//
// The parsers are tested on REAL output of this tool chain (scripts/fixtures: aapt2 and
// apkanalyzer from build-tools 37.0.0 on the merged manifest of the Tauri 2.12 debug APK) and on
// edits of it.

import { spawnSync } from 'node:child_process';
import { existsSync, readdirSync, readFileSync } from 'node:fs';
import { dirname, join, resolve } from 'node:path';
import { fileURLToPath } from 'node:url';

const here = dirname(fileURLToPath(import.meta.url));

/** true, false, 'reference' (a resource such as @0x7f...), or undefined (not understood). */
export function readValue(raw) {
  const text = String(raw).trim();
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

/** A tool's output that cannot be turned into a manifest tree. */
export class ParseError extends Error {}

const node = (name, indent = -1) => ({ name, attrs: {}, children: [], indent });

/**
 * The element tree of `aapt2 dump xmltree`. Lines are `N: ns=uri (line=n)`, `E: element (line=n)`
 * and `A: [uri:]name[(0xid)]=value [(Raw: "...")]`; an attribute belongs to the nearest earlier
 * element, and an element nests by indentation. Attributes are keyed by local name.
 */
export function parseAapt2Tree(text) {
  const root = node('#document');
  const stack = [root];
  let sawElement = false;
  for (const line of String(text).split(/\r?\n/)) {
    if (line.trim() === '') continue;
    const m = line.match(/^(\s*)([NEA]): (.*)$/);
    if (!m) throw new ParseError(`aapt2: cannot read line: ${line.slice(0, 80)}`);
    const [, space, kind, rest] = m;
    const indent = space.length;
    if (kind === 'N') continue;
    if (kind === 'E') {
      const element = rest.match(/^([A-Za-z][\w.:-]*) \(line=\d+\)$/);
      if (!element) throw new ParseError(`aapt2: cannot read element: ${line.slice(0, 80)}`);
      while (stack.length > 1 && stack[stack.length - 1].indent >= indent) stack.pop();
      const parent = stack[stack.length - 1];
      const child = node(element[1], indent);
      parent.children.push(child);
      stack.push(child);
      sawElement = true;
      continue;
    }
    const owner = stack[stack.length - 1];
    if (owner === root || indent <= owner.indent) throw new ParseError(`aapt2: attribute outside an element: ${line.slice(0, 80)}`);
    if (owner.children.length > 0) throw new ParseError(`aapt2: attribute after a child element: ${line.slice(0, 80)}`);
    const eq = rest.indexOf('=');
    if (eq <= 0) throw new ParseError(`aapt2: attribute without a value: ${line.slice(0, 80)}`);
    const left = rest.slice(0, eq).replace(/\(0x[0-9a-f]+\)$/i, '');
    const name = left.slice(left.lastIndexOf(':') + 1);
    if (!/^[A-Za-z][\w.-]*$/.test(name)) throw new ParseError(`aapt2: odd attribute name: ${line.slice(0, 80)}`);
    let value = rest.slice(eq + 1);
    const quoted = value.match(/^"((?:[^"\\]|\\.)*)"(?: \(Raw: ".*"\))?$/);
    if (quoted) value = quoted[1];
    owner.attrs[name] = value.trim();
  }
  if (!sawElement) throw new ParseError('aapt2: no elements in the output');
  const top = root.children;
  if (top.length !== 1 || top[0].name !== 'manifest') throw new ParseError('aapt2: the root element is not <manifest>');
  return top[0];
}

const ENTITIES = { amp: '&', lt: '<', gt: '>', quot: '"', apos: "'" };
const decode = (s) => s.replace(/&(amp|lt|gt|quot|apos);/g, (_, e) => ENTITIES[e]);

/** The element tree of `apkanalyzer manifest print` (plain XML, attributes `android:name="..."`). */
export function parseApkanalyzerTree(xml) {
  const text = String(xml).replace(/<\?xml[\s\S]*?\?>/, '').replace(/<!--[\s\S]*?-->/g, '');
  const root = node('#document');
  const stack = [root];
  const tag = /<(\/?)([A-Za-z][\w.:-]*)((?:\s+[A-Za-z][\w.:-]*\s*=\s*"[^"]*")*)\s*(\/?)>/g;
  let at = 0;
  for (let m = tag.exec(text); m; m = tag.exec(text)) {
    if (text.slice(at, m.index).trim() !== '') throw new ParseError(`apkanalyzer: text outside a tag near: ${text.slice(at, at + 40).trim()}`);
    at = tag.lastIndex;
    const [, closing, name, attributes, selfClosing] = m;
    if (closing) {
      const top = stack.pop();
      if (!top || top.name !== name) throw new ParseError(`apkanalyzer: </${name}> does not match <${top?.name}>`);
      continue;
    }
    const element = node(name);
    for (const a of attributes.matchAll(/([A-Za-z][\w.:-]*)\s*=\s*"([^"]*)"/g)) {
      element.attrs[a[1].slice(a[1].lastIndexOf(':') + 1)] = decode(a[2]);
    }
    stack[stack.length - 1].children.push(element);
    if (!selfClosing) stack.push(element);
  }
  if (text.slice(at).trim() !== '') throw new ParseError('apkanalyzer: text after the last tag');
  if (stack.length !== 1) throw new ParseError('apkanalyzer: a tag is not closed');
  if (root.children.length !== 1 || root.children[0].name !== 'manifest') throw new ParseError('apkanalyzer: the root element is not <manifest>');
  return root.children[0];
}

const kids = (element, name) => element.children.filter((c) => c.name === name);

/** The attributes of <application> (what the first version of this script looked at). */
export function applicationAttrs(manifest) {
  const apps = kids(manifest, 'application');
  return apps.length === 1 ? apps[0].attrs : {};
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
  if (!('dataExtractionRules' in attrs) || readValue(attrs.dataExtractionRules) !== 'reference') {
    found.push('dataExtractionRules is not set to a resource in the merged manifest');
  }
  for (const name of ['testOnly', 'requestLegacyExternalStorage', 'hasFragileUserData']) {
    if (name in attrs && readValue(attrs[name]) !== false) found.push(`${name} is set`);
  }
  if ('fullBackupContent' in attrs && readValue(attrs.fullBackupContent) !== false) found.push('fullBackupContent is set');
  if ('networkSecurityConfig' in attrs) found.push('a network security config is present: it can allow cleartext, check it by hand');
  if ('debuggable' in attrs && readValue(attrs.debuggable) === undefined) found.push('debuggable has a value that is not understood');
  if ('debuggable' in attrs && readValue(attrs.debuggable) !== false && build === 'release') {
    found.push('debuggable is true in a release build');
  }
  return found;
}

const INTERNET = 'android.permission.INTERNET';
const COMPONENTS = ['activity', 'activity-alias', 'service', 'receiver', 'provider'];
const PROFILE_ACTIONS = [
  'androidx.profileinstaller.action.INSTALL_PROFILE',
  'androidx.profileinstaller.action.SKIP_FILE',
  'androidx.profileinstaller.action.SAVE_PROFILE',
  'androidx.profileinstaller.action.BENCHMARK_OPERATION',
];
/** The only deep-link paths the manifest may route to the app; the Rust parser still decides. */
const DEEP_LINK_PATHS = [
  ['path', '/workspace'],
  ['pathPrefix', '/projects/'],
  ['pathPrefix', '/documents/'],
];

const names = (parent, child) => kids(parent, child).map((c) => c.attrs.name);

/** Problems of one intent filter of MainActivity; the deep-link rules above. */
function filterProblems(filter, where) {
  const found = [];
  const actions = names(filter, 'action');
  const categories = names(filter, 'category');
  const data = kids(filter, 'data');
  for (const attr of Object.keys(filter.attrs)) {
    if (attr === 'autoVerify' && readValue(filter.attrs[attr]) === false) continue;
    found.push(`${where}: intent filter attribute ${attr}${attr === 'autoVerify' ? ' (app links are not set up)' : ''}`);
  }
  if (actions.length === 1 && actions[0] === 'android.intent.action.MAIN') {
    if (data.length > 0) found.push(`${where}: the launcher filter has data`);
    for (const c of categories) {
      if (!['android.intent.category.LAUNCHER', 'android.intent.category.LEANBACK_LAUNCHER'].includes(c)) found.push(`${where}: launcher filter category ${c}`);
    }
    return found;
  }
  const viewActions = ['android.intent.action.VIEW', 'org.chromium.arc.intent.action.VIEW'];
  if (actions.length === 0 || !actions.includes('android.intent.action.VIEW') || actions.some((a) => !viewActions.includes(a))) {
    found.push(`${where}: unexpected intent filter actions [${actions.join(', ')}]`);
  }
  if (categories.length !== 2 || !categories.includes('android.intent.category.DEFAULT') || !categories.includes('android.intent.category.BROWSABLE')) {
    found.push(`${where}: a link filter must have exactly DEFAULT and BROWSABLE, has [${categories.join(', ')}]`);
  }
  // Android merges every <data> element of a filter (the Tauri plugin writes one element per
  // attribute), so the rules apply to the filter as a whole: scheme dynodoc, host open, and
  // one of the three path shapes, nothing else.
  const seen = { scheme: [], host: [], path: [], pathPrefix: [] };
  for (const d of data) {
    for (const [attr, value] of Object.entries(d.attrs)) {
      const allowed =
        (attr === 'scheme' && value === 'dynodoc') ||
        (attr === 'host' && value === 'open') ||
        DEEP_LINK_PATHS.some(([a, v]) => a === attr && v === value);
      if (!allowed) found.push(`${where}: data ${attr}="${value}" is not part of dynodoc://open/...`);
      else seen[attr].push(value);
    }
  }
  if (data.length === 0) found.push(`${where}: a link filter without data matches every link`);
  if (seen.scheme.length === 0) found.push(`${where}: the link filter has no scheme dynodoc`);
  if (seen.host.length === 0) found.push(`${where}: the link filter has no host (any host would match)`);
  if (seen.path.length + seen.pathPrefix.length !== 1) found.push(`${where}: a link filter must name exactly one path shape`);
  return found;
}

/**
 * Everything about the merged manifest that has to hold; returns {problems, facts}. `facts` are
 * lines worth printing (components, permissions, link filters, SDK levels).
 */
export function auditManifest(manifest, { build = 'debug', appId, minSdk } = {}) {
  const problems = [];
  const facts = [];
  const pkg = manifest.attrs.package;
  if (appId && pkg !== appId) problems.push(`package is ${pkg}, expected ${appId}`);
  facts.push(`package ${pkg}, versionName ${manifest.attrs.versionName ?? '?'}, versionCode ${manifest.attrs.versionCode ?? '?'}`);

  const sdk = kids(manifest, 'uses-sdk');
  if (sdk.length !== 1) problems.push('uses-sdk is missing or repeated');
  else {
    const min = Number(sdk[0].attrs.minSdkVersion);
    const target = Number(sdk[0].attrs.targetSdkVersion);
    facts.push(`minSdkVersion ${min}, targetSdkVersion ${target}`);
    if (!Number.isInteger(min) || !Number.isInteger(target)) problems.push('minSdkVersion or targetSdkVersion is not a number');
    else {
      if (minSdk !== undefined && min !== minSdk) problems.push(`minSdkVersion is ${min}, tauri.conf.json says ${minSdk}`);
      if (target < 35) problems.push(`targetSdkVersion ${target} is below 35`);
    }
  }

  const permission = `${pkg}.DYNAMIC_RECEIVER_NOT_EXPORTED_PERMISSION`;
  const allowedUses = new Set([INTERNET, permission]);
  const uses = [];
  for (const kind of ['uses-permission', 'uses-permission-sdk-23', 'uses-permission-sdk-m']) uses.push(...names(manifest, kind));
  facts.push(`permissions: ${uses.join(', ') || '(none)'}`);
  for (const p of uses) if (!allowedUses.has(p)) problems.push(`permission ${p} is not on the allowed list (INTERNET only)`);
  if (!uses.includes(INTERNET)) problems.push('INTERNET is not requested (the app opens a web page)');
  for (const declared of kids(manifest, 'permission')) {
    if (declared.attrs.name !== permission) problems.push(`the app declares permission ${declared.attrs.name}`);
    else if (Number(declared.attrs.protectionLevel) !== 2) {
      problems.push(`${permission} is not a signature permission (protectionLevel ${declared.attrs.protectionLevel})`);
    }
  }
  for (const feature of kids(manifest, 'uses-feature')) {
    facts.push(`uses-feature ${feature.attrs.name ?? feature.attrs.glEsVersion} required=${feature.attrs.required ?? 'true'}`);
  }

  const apps = kids(manifest, 'application');
  if (apps.length !== 1) {
    problems.push('the manifest has no single <application>');
    return { problems, facts };
  }
  const application = apps[0];
  problems.push(...manifestProblems(application.attrs, { build }));
  facts.push(`application: allowBackup=${application.attrs.allowBackup}, dataExtractionRules=${application.attrs.dataExtractionRules}, usesCleartextTraffic=${application.attrs.usesCleartextTraffic}, debuggable=${application.attrs.debuggable ?? '(absent)'}`);

  for (const kind of COMPONENTS) {
    for (const c of kids(application, kind)) {
      const name = c.attrs.name ?? '(unnamed)';
      const filters = kids(c, 'intent-filter');
      const explicit = c.attrs.exported;
      const exported = explicit === undefined ? filters.length > 0 : readValue(explicit);
      if (explicit !== undefined && typeof readValue(explicit) !== 'boolean') problems.push(`${name}: exported has a value that is not understood`);
      if (explicit === undefined && filters.length > 0) problems.push(`${name}: has intent filters but no explicit exported`);
      facts.push(`${kind} ${name}: ${exported ? 'EXPORTED' : 'not exported'}${c.attrs.permission ? `, permission ${c.attrs.permission}` : ''}, ${filters.length} intent filter(s)`);
      if (!exported) continue;
      if (kind === 'activity' && name === `${pkg}.MainActivity`) {
        filters.forEach((f, i) => problems.push(...filterProblems(f, `MainActivity filter ${i + 1}`)));
        if (readValue(c.attrs.launchMode) === undefined && !/^\d+$/.test(String(c.attrs.launchMode))) problems.push('MainActivity launchMode is not understood');
        for (const f of filters) {
          for (const d of kids(f, 'data')) facts.push(`  link filter data: ${JSON.stringify(d.attrs)}`);
        }
      } else if (kind === 'receiver' && name === 'androidx.profileinstaller.ProfileInstallReceiver') {
        if (c.attrs.permission !== 'android.permission.DUMP') problems.push(`${name} is exported without the DUMP permission`);
        for (const f of filters) {
          const actions = names(f, 'action');
          if (actions.length !== 1 || !PROFILE_ACTIONS.includes(actions[0]) || kids(f, 'data').length > 0) problems.push(`${name}: unexpected intent filter [${actions.join(', ')}]`);
        }
      } else {
        problems.push(`${kind} ${name} is exported and is not on the allowed list`);
      }
    }
  }
  return { problems, facts };
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
    for (const name of [tool, `${tool}.exe`, `${tool}.bat`]) {
      const path = join(root, version, name);
      if (exists(path)) return path;
    }
  }
  return null;
}

/**
 * The signer lines of `apksigner verify --print-certs`: [{dn, sha256}] (one per distinct
 * certificate) or null if unparseable. build-tools 37 prints `V2 Signer: certificate DN: ...`
 * (one block per signature scheme); older tools print `Signer #1 certificate DN: ...`.
 */
export function parseApksignerCerts(text) {
  const line = (what) => new RegExp(`^(?:V\\d+(?:\\.\\d+)? )?Signer(?: #\\d+)?:? certificate ${what}: (.+)$`, 'gm');
  const dns = [...String(text).matchAll(line('DN'))].map((m) => m[1].trim());
  const hashes = [...String(text).matchAll(line('SHA-256 digest'))].map((m) => m[1].trim());
  if (dns.length === 0 || dns.length !== hashes.length || hashes.some((h) => !/^[0-9a-f]{64}$/.test(h))) return null;
  const seen = new Map();
  dns.forEach((dn, i) => seen.set(hashes[i], dn));
  return [...seen].map(([sha256, dn]) => ({ dn, sha256 }));
}

/** What is wrong with the signing certificates for this kind of build. */
export function signingProblems(certs, build) {
  if (!certs) return ['the signing certificate could not be read'];
  const debugKey = certs.some((c) => /\bCN=Android Debug\b/.test(c.dn));
  if (build === 'debug' && !debugKey) return ['a debug build is not signed with the SDK debug key'];
  if (build === 'release' && debugKey) return ['a release build is signed with the debug key'];
  return [];
}

function run(tool, args) {
  const options = { encoding: 'utf8', maxBuffer: 64 * 1024 * 1024 };
  // A .bat file (apksigner, apkanalyzer on Windows) needs a shell; one command string, every
  // piece quoted by us, rather than an argument list the shell would only concatenate.
  if (tool.endsWith('.bat')) return spawnSync(`"${tool}" ${args.map((a) => `"${a}"`).join(' ')}`, { ...options, shell: true });
  return spawnSync(tool, args, options);
}

function main() {
  const args = process.argv.slice(2);
  const apk = args.find((a) => !a.startsWith('--') && a !== args[args.indexOf('--build') + 1]);
  // usage: check-apk-manifest.mjs <apk> [--build debug|release] [--allow-unsigned]
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
  const config = JSON.parse(readFileSync(join(here, '..', 'src-tauri', 'tauri.conf.json'), 'utf8'));
  const options = { build, appId: config.identifier, minSdk: config.bundle?.android?.minSdkVersion };
  let manifest = null;
  try {
    const aapt2 = sdk ? newestBuildTool(sdk, 'aapt2') : null;
    if (aapt2) {
      const result = run(aapt2, ['dump', 'xmltree', resolve(apk), '--file', 'AndroidManifest.xml']);
      if (result.status !== 0) {
        console.error(`check-apk-manifest: aapt2 failed: ${result.stderr}`);
        process.exit(2);
      }
      manifest = parseAapt2Tree(result.stdout);
    } else if (sdk) {
      for (const rel of ['cmdline-tools/latest/bin/apkanalyzer', 'cmdline-tools/latest/bin/apkanalyzer.bat']) {
        const tool = join(sdk, rel);
        if (!existsSync(tool)) continue;
        const result = run(tool, ['manifest', 'print', resolve(apk)]);
        if (result.status === 0) manifest = parseApkanalyzerTree(result.stdout);
        break;
      }
    }
  } catch (error) {
    if (error instanceof ParseError) {
      console.error(`check-apk-manifest: cannot parse the tool output (failing closed): ${error.message}`);
      process.exit(2);
    }
    throw error;
  }
  if (!manifest) {
    console.error('check-apk-manifest: neither aapt2 nor apkanalyzer found under ANDROID_HOME / ANDROID_SDK_ROOT');
    process.exit(2);
  }
  const { problems, facts } = auditManifest(manifest, options);
  for (const fact of facts) console.log(`check-apk-manifest: ${fact}`);

  const signer = sdk ? newestBuildTool(sdk, 'apksigner') : null;
  let signing = ['apksigner not found: the signing certificate was not checked'];
  if (signer) {
    const result = run(signer, ['verify', '--print-certs', resolve(apk)]);
    const certs = result.status === 0 ? parseApksignerCerts(result.stdout) : null;
    signing = signingProblems(certs, build);
    if (certs) for (const c of certs) console.log(`check-apk-manifest: signed by ${c.dn} (SHA-256 ${c.sha256})`);
    // A release build made without the owner's keystore is unsigned: no phone installs it and no
    // store takes it. That is expected before the keystore exists, so it can be allowed, loudly.
    if (!certs && args.includes('--allow-unsigned') && /no signature|not signed|Missing META-INF|DOES NOT VERIFY/i.test(`${result.stdout}${result.stderr}`)) {
      console.log('check-apk-manifest: the APK is UNSIGNED (--allow-unsigned): it cannot be installed or uploaded until it is signed with the upload key');
      signing = [];
    }
  }
  problems.push(...signing);
  for (const problem of problems) console.error(`check-apk-manifest: UNSAFE: ${problem}`);
  if (problems.length > 0) process.exit(1);
  console.log(`check-apk-manifest: ${build} build: backup off, cleartext off, only the allowed permissions, components and links`);
}

if (process.argv[1] && resolve(process.argv[1]) === fileURLToPath(import.meta.url)) main();
