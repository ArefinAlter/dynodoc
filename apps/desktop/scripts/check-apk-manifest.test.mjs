// node --test scripts/check-apk-manifest.test.mjs
//
// The fixtures are REAL tool output: the merged manifest of the Tauri 2.12 debug APK built on
// Windows 11 with Android build-tools 37.0.0 (`aapt2 dump xmltree`, `apkanalyzer manifest print`,
// `apksigner verify --print-certs`). The failing cases are edits of that output, so a change in
// the tools' format breaks the parse tests rather than silently passing.
import assert from 'node:assert/strict';
import { readFileSync } from 'node:fs';
import { dirname, join } from 'node:path';
import { test } from 'node:test';
import { fileURLToPath } from 'node:url';
import {
  ParseError,
  applicationAttrs,
  auditManifest,
  manifestProblems,
  newestBuildTool,
  parseAapt2Tree,
  parseApkanalyzerTree,
  parseApksignerCerts,
  readValue,
  signingProblems,
} from './check-apk-manifest.mjs';

const fixtures = join(dirname(fileURLToPath(import.meta.url)), 'fixtures');
const aapt2Text = readFileSync(join(fixtures, 'aapt2-xmltree-debug.txt'), 'utf8');
const analyzerText = readFileSync(join(fixtures, 'apkanalyzer-manifest-debug.xml'), 'utf8');
const signerText = readFileSync(join(fixtures, 'apksigner-certs-debug.txt'), 'utf8');
const options = { appId: 'online.dynodoc.client', minSdk: 24 };

const ANDROID = 'http://schemas.android.com/apk/res/android';
const audit = (text, build = 'debug') => auditManifest(parseAapt2Tree(text), { ...options, build }).problems;

/** Replace exactly one occurrence, so a changed fixture cannot make a mutation a no-op. */
function swap(text, from, to) {
  assert.equal(text.split(from).length, 2, `expected exactly one: ${from}`);
  return text.replace(from, to);
}
/** Add an attribute line to the element whose `E:` line contains `element` (first match). */
function addAttribute(text, element, attribute) {
  const lines = text.split(/\r?\n/);
  const at = lines.findIndex((l) => l.includes(`E: ${element} (`));
  assert.ok(at >= 0, element);
  const indent = lines[at].match(/^\s*/)[0].length + 2;
  lines.splice(at + 1, 0, `${' '.repeat(indent)}A: ${ANDROID}:${attribute}`);
  return lines.join('\n');
}
/** Set the `exported` attribute of the component whose name line contains `name`. */
function setExported(text, name, value) {
  const lines = text.split(/\r?\n/);
  const at = lines.findIndex((l) => l.includes(`"${name}" (Raw:`));
  assert.ok(at >= 0, name);
  const flag = lines.findIndex((l, i) => i > at && l.includes('exported(0x01010010)='));
  assert.ok(flag > at && flag < at + 6, `no exported flag near ${name}`);
  if (value === null) lines.splice(flag, 1);
  else lines[flag] = lines[flag].replace(/=(true|false)$/, `=${value}`);
  return lines.join('\n');
}
/**
 * Add an element (with attributes) next to the `E:` line `after` (first match): as its last
 * sibling-to-be after its whole subtree, or, with `asChild`, as its first child.
 */
function addElement(text, after, element, attributes = [], asChild = false) {
  const lines = text.split(/\r?\n/);
  const at = lines.findIndex((l) => l.includes(after));
  assert.ok(at >= 0, after);
  const indentOf = (l) => l.match(/^\s*/)[0].length;
  const base = indentOf(lines[at]);
  let j = at + 1;
  if (asChild) {
    while (j < lines.length && lines[j].trim().startsWith('A:') && indentOf(lines[j]) === base + 2) j += 1;
  } else {
    while (j < lines.length && indentOf(lines[j]) > base) j += 1;
  }
  const indent = asChild ? base + 4 : base;
  const add = [`${' '.repeat(indent)}E: ${element} (line=999)`, ...attributes.map((a) => `${' '.repeat(indent + 2)}A: ${ANDROID}:${a}`)];
  lines.splice(j, 0, ...add);
  return lines.join('\n');
}
/** The first occurrence only. */
function swapFirst(text, from, to) {
  assert.ok(text.includes(from), from);
  return text.replace(from, to);
}

test('real aapt2 output: the tree, the application attributes and a clean audit', () => {
  const manifest = parseAapt2Tree(aapt2Text);
  assert.equal(manifest.attrs.package, 'online.dynodoc.client');
  const app = applicationAttrs(manifest);
  assert.equal(app.allowBackup, 'false');
  assert.equal(app.usesCleartextTraffic, 'false');
  assert.match(app.dataExtractionRules, /^@0x[0-9a-f]+$/);
  assert.equal(app.debuggable, 'true');
  assert.equal(app.appComponentFactory, 'androidx.core.app.CoreComponentFactory', 'quoted values lose their quotes');
  const { problems, facts } = auditManifest(manifest, { ...options, build: 'debug' });
  assert.deepEqual(problems, []);
  assert.ok(facts.some((f) => f.includes('minSdkVersion 24, targetSdkVersion 37')), facts.join('\n'));
  assert.ok(facts.some((f) => f.includes('MainActivity: EXPORTED, 4 intent filter(s)')));
  const activity = manifest.children.find((c) => c.name === 'application').children.find((c) => c.name === 'activity');
  assert.equal(activity.children.filter((c) => c.name === 'intent-filter').length, 4);
});

test('real apkanalyzer output gives the same audit as aapt2', () => {
  const manifest = parseApkanalyzerTree(analyzerText);
  assert.equal(manifest.attrs.package, 'online.dynodoc.client');
  assert.equal(applicationAttrs(manifest).allowBackup, 'false');
  assert.deepEqual(auditManifest(manifest, { ...options, build: 'debug' }).problems, []);
  const a = auditManifest(parseAapt2Tree(aapt2Text), { ...options }).facts.filter((f) => !f.startsWith('application:') && !f.startsWith('package'));
  const b = auditManifest(manifest, { ...options }).facts.filter((f) => !f.startsWith('application:') && !f.startsWith('package'));
  assert.deepEqual(b, a);
  // The same edits fail in this format too.
  const bad = analyzerText.replace('android:allowBackup="false"', 'android:allowBackup="true"');
  assert.notEqual(bad, analyzerText);
  assert.ok(auditManifest(parseApkanalyzerTree(bad), options).problems.some((p) => p.includes('allowBackup is true')));
  const exported = analyzerText.replace(/(<provider\s+android:name="androidx\.core\.content\.FileProvider"\s+android:exported=)"false"/, '$1"true"');
  assert.notEqual(exported, analyzerText);
  assert.ok(auditManifest(parseApkanalyzerTree(exported), options).problems.some((p) => p.includes('FileProvider is exported')));
});

test('application flags: an unsafe or missing value fails, whatever form it is written in', () => {
  const cases = {
    'allowBackup true': [swap(aapt2Text, 'allowBackup(0x01010280)=false', 'allowBackup(0x01010280)=true'), /allowBackup is true, not false/],
    'allowBackup numeric true': [swap(aapt2Text, 'allowBackup(0x01010280)=false', 'allowBackup(0x01010280)=(type 0x12)0xffffffff'), /allowBackup is/],
    'cleartext true': [swap(aapt2Text, 'usesCleartextTraffic(0x010104ec)=false', 'usesCleartextTraffic(0x010104ec)=true'), /usesCleartextTraffic is true/],
    'allowBackup missing': [swap(aapt2Text, /.*allowBackup\(0x01010280\)=false\r?\n/.exec(aapt2Text)[0], ''), /allowBackup is not set/],
    'cleartext missing': [swap(aapt2Text, /.*usesCleartextTraffic\(0x010104ec\)=false\r?\n/.exec(aapt2Text)[0], ''), /usesCleartextTraffic is not set/],
    'extraction rules missing': [swap(aapt2Text, /.*dataExtractionRules\(0x0101063e\)=@0x7f120000\r?\n/.exec(aapt2Text)[0], ''), /dataExtractionRules is not set/],
    'extraction rules a literal': [swap(aapt2Text, 'dataExtractionRules(0x0101063e)=@0x7f120000', 'dataExtractionRules(0x0101063e)="x"'), /dataExtractionRules is not set/],
    'something not understood': [swap(aapt2Text, 'allowBackup(0x01010280)=false', 'allowBackup(0x01010280)=maybe'), /allowBackup is maybe/],
    testOnly: [addAttribute(aapt2Text, 'application', 'testOnly(0x01010272)=true'), /testOnly is set/],
    'a network security config': [addAttribute(aapt2Text, 'application', 'networkSecurityConfig(0x01010527)=@0x7f130001'), /network security config/],
    'legacy external storage': [addAttribute(aapt2Text, 'application', 'requestLegacyExternalStorage(0x01010603)=true'), /requestLegacyExternalStorage/],
  };
  for (const [name, [text, expected]] of Object.entries(cases)) {
    const found = audit(text);
    assert.ok(found.some((f) => expected.test(f)), `${name}: ${found.join(' | ')}`);
  }
});

test('debuggable: allowed in a debug build, a failure in a release build, and unreadable values fail', () => {
  assert.deepEqual(audit(aapt2Text, 'debug'), []);
  assert.deepEqual(audit(aapt2Text, 'release'), ['debuggable is true in a release build']);
  const off = swap(aapt2Text, 'debuggable(0x0101000f)=true', 'debuggable(0x0101000f)=false');
  assert.deepEqual(audit(off, 'release'), []);
  const absent = swap(aapt2Text, /.*debuggable\(0x0101000f\)=true\r?\n/.exec(aapt2Text)[0], '');
  assert.deepEqual(audit(absent, 'release'), [], 'absent means not debuggable');
  assert.equal(audit(swap(aapt2Text, 'debuggable(0x0101000f)=true', 'debuggable(0x0101000f)=@0x7f010002'), 'release').length, 1);
});

test('permissions: only INTERNET and the AndroidX signature permission', () => {
  const extra = addElement(aapt2Text, 'E: uses-permission (line=11)', 'uses-permission', ['name(0x01010003)="android.permission.CAMERA" (Raw: "android.permission.CAMERA")']);
  assert.ok(audit(extra).some((p) => p.includes('android.permission.CAMERA')));
  const noInternet = swap(aapt2Text, '"android.permission.INTERNET" (Raw: "android.permission.INTERNET")', '"android.permission.ACCESS_NETWORK_STATE" (Raw: "x")');
  const found = audit(noInternet);
  assert.ok(found.some((p) => p.includes('INTERNET is not requested')), found.join('|'));
  assert.ok(found.some((p) => p.includes('ACCESS_NETWORK_STATE')));
  const weak = swap(aapt2Text, 'protectionLevel(0x01010009)=0x00000002', 'protectionLevel(0x01010009)=0x00000000');
  assert.ok(audit(weak).some((p) => p.includes('not a signature permission')));
  const declared = addElement(aapt2Text, 'E: permission (line=18)', 'permission', ['name(0x01010003)="online.dynodoc.client.OTHER" (Raw: "x")']);
  assert.ok(audit(declared).some((p) => p.includes('declares permission online.dynodoc.client.OTHER')));
});

test('components: anything exported beyond MainActivity and the DUMP-guarded profile receiver fails', () => {
  assert.ok(audit(setExported(aapt2Text, 'androidx.core.content.FileProvider', 'true')).some((p) => p.includes('FileProvider is exported')));
  const service = addElement(aapt2Text, 'E: application (line=24)', 'service', [], true);
  // a service with no exported attribute and no filter is not exported, but one with an exported flag is not allowed
  const exportedService = addAttribute(service, 'service', 'exported(0x01010010)=true');
  assert.ok(audit(exportedService).some((p) => p.includes('service') && p.includes('exported and is not on the allowed list')));
  const noPermission = swap(aapt2Text, /.*permission\(0x01010006\)="android.permission.DUMP"[^\r\n]*\r?\n/.exec(aapt2Text)[0], '');
  assert.ok(audit(noPermission).some((p) => p.includes('without the DUMP permission')));
  const implicit = setExported(aapt2Text, 'online.dynodoc.client.MainActivity', null);
  assert.ok(audit(implicit).some((p) => p.includes('no explicit exported')));
});

test('link filters: only dynodoc://open/<three shapes>, VIEW + DEFAULT + BROWSABLE, no autoVerify', () => {
  const cases = {
    'another scheme': [swapFirst(aapt2Text, '"dynodoc" (Raw: "dynodoc")', '"https" (Raw: "https")'),/scheme="https"|no scheme dynodoc/],
    'another host': [aapt2Text.replaceAll('="open" (Raw: "open")', '="evil" (Raw: "evil")'), /host="evil"/],
    'a path outside the three': [swap(aapt2Text, '="/workspace" (Raw: "/workspace")', '="/admin" (Raw: "/admin")'), /path="\/admin"/],
    'a broader prefix': [swap(aapt2Text, '="/projects/" (Raw: "/projects/")', '="/" (Raw: "/")'), /pathPrefix="\/"/],
    'autoVerify (app links)': [addAttribute(aapt2Text, 'intent-filter', 'autoVerify(0x010104ee)=true'), /autoVerify/],
  };
  for (const [name, [text, expected]] of Object.entries(cases)) {
    const found = audit(text);
    assert.ok(found.some((f) => expected.test(f)), `${name}: ${found.join(' | ')}`);
  }
  // Missing BROWSABLE, an extra category, a host-less filter, a path-less filter.
  assert.ok(audit(swapFirst(aapt2Text, 'android.intent.category.BROWSABLE" (Raw: "android.intent.category.BROWSABLE")', 'android.intent.category.INFO" (Raw: "x")')).some((p) => p.includes('exactly DEFAULT and BROWSABLE')));
  const lines = aapt2Text.split(/\r?\n/);
  const hostAt = lines.findIndex((l) => l.includes('="open" (Raw: "open")'));
  const noHost = [...lines.slice(0, hostAt - 1), ...lines.slice(hostAt + 1)].join('\n');
  assert.ok(audit(noHost).some((p) => p.includes('no host')), audit(noHost).join('|'));
  const pathAt = lines.findIndex((l) => l.includes('="/workspace" (Raw: "/workspace")'));
  const noPath = [...lines.slice(0, pathAt - 1), ...lines.slice(pathAt + 1)].join('\n');
  assert.ok(audit(noPath).some((p) => p.includes('exactly one path shape')), audit(noPath).join('|'));
});

test('SDK levels: minSdk must equal tauri.conf.json, target must be recent', () => {
  assert.ok(audit(swap(aapt2Text, 'minSdkVersion(0x0101020c)=24', 'minSdkVersion(0x0101020c)=21')).some((p) => p.includes('tauri.conf.json says 24')));
  assert.ok(audit(swap(aapt2Text, 'targetSdkVersion(0x01010270)=37', 'targetSdkVersion(0x01010270)=30')).some((p) => p.includes('below 35')));
  assert.ok(audit(swap(aapt2Text, 'package="online.dynodoc.client" (Raw: "online.dynodoc.client")', 'package="x.y" (Raw: "x.y")')).some((p) => p.includes('expected online.dynodoc.client')));
});

test('output that cannot be parsed is a ParseError, never an empty pass', () => {
  for (const bad of [
    '',
    'nonsense',
    'E: manifest\n  A: x',
    `N: android=${ANDROID} (line=2)\n  E: application (line=2)`,
    `N: android=${ANDROID} (line=2)\n  E: manifest (line=2)\n    A: ${ANDROID}:x(0x1)=1\n      E: application (line=3)\n    A: ${ANDROID}:late(0x2)=1`,
    `  E: manifest (line=2)\n    A: no-equals-here`,
    `  E: manifest (line=2)\n    A: ${ANDROID}:bad name(0x1)=1`,
    aapt2Text.replace('E: application (line=24)', 'E: application'),
  ]) {
    assert.throws(() => parseAapt2Tree(bad), ParseError, bad.slice(0, 40));
  }
  for (const bad of ['', '<manifest>', '<manifest></application></manifest>', '<manifest/> trailing', 'text <manifest/>', '<application/>', '<manifest><a b=c/></manifest>']) {
    assert.throws(() => parseApkanalyzerTree(bad), ParseError, bad);
  }
  assert.equal(auditManifest(parseAapt2Tree(`N: android=${ANDROID} (line=2)\n  E: manifest (line=2)\n    A: package="x" (Raw: "x")`)).problems.length >= 3, true);
});

test('signing: the debug key for a debug build, anything but it for a release build', () => {
  const certs = parseApksignerCerts(signerText);
  assert.equal(certs.length, 1, 'one distinct certificate for the V2 block');
  assert.match(certs[0].dn, /CN=Android Debug/);
  assert.match(certs[0].sha256, /^[0-9a-f]{64}$/);
  assert.deepEqual(signingProblems(certs, 'debug'), []);
  assert.deepEqual(signingProblems(certs, 'release'), ['a release build is signed with the debug key']);
  assert.deepEqual(signingProblems([{ dn: 'CN=Dynodoc upload', sha256: 'a'.repeat(64) }], 'release'), []);
  assert.ok(signingProblems([{ dn: 'CN=Dynodoc upload', sha256: 'a'.repeat(64) }], 'debug').length > 0);
  assert.ok(signingProblems(null, 'debug').length > 0);
  // The older tool format.
  const old = 'Signer #1 certificate DN: CN=Android Debug, O=Android, C=US\nSigner #1 certificate SHA-256 digest: ' + 'b'.repeat(64);
  assert.equal(parseApksignerCerts(old).length, 1);
  assert.equal(parseApksignerCerts('garbage'), null);
  assert.equal(parseApksignerCerts('V2 Signer: certificate DN: CN=x'), null, 'a DN without a digest is unreadable');
});

test('readValue never turns something unclear into false', () => {
  for (const [raw, expected] of [['false', false], ['true', true], ['(type 0x12)0x0', false], ['(type 0x12)0xffffffff', true], ['0x0', false], ['1', true], ['@0x7f01', 'reference'], ['', undefined], ['False?', undefined], ['0x7', undefined]]) {
    assert.equal(readValue(raw), expected, raw);
  }
});

test('manifestProblems on its own: the older (application attributes only) interface still fails closed', () => {
  assert.equal(manifestProblems({}).length, 3);
  assert.equal(manifestProblems(applicationAttrs(parseAapt2Tree(aapt2Text))).length, 0);
});

test('the newest build-tools folder holding a tool is chosen by version, not by text', () => {
  const have = new Set(['/sdk/build-tools', '/sdk/build-tools/9.0.0/aapt2', '/sdk/build-tools/34.0.0/aapt2.exe', '/sdk/build-tools/35.0.1/aapt2', '/sdk/build-tools/36.0.0-rc1']);
  const list = () => ['9.0.0', '34.0.0', '35.0.1', '36.0.0-rc1'];
  const exists = (p) => have.has(p.replaceAll('\\', '/'));
  assert.equal(newestBuildTool('/sdk', 'aapt2', list, exists).replaceAll('\\', '/'), '/sdk/build-tools/35.0.1/aapt2');
  assert.equal(newestBuildTool('/nowhere', 'aapt2', list, () => false), null);
});
