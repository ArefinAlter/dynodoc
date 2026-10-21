// node --test scripts/check-apk-manifest.test.mjs
// The parsers are tested on sample output in the formats of `aapt2 dump xmltree` and
// `apkanalyzer manifest print`. Real tool output was NOT available when this was written.
import assert from 'node:assert/strict';
import { test } from 'node:test';
import { manifestProblems, newestBuildTool, parseAapt2, parseApkanalyzer, readValue } from './check-apk-manifest.mjs';

const NS = 'http://schemas.android.com/apk/res/android';
const aapt2 = ({ application = [], activity = [], extra = '' } = {}) => `N: android=${NS} (line=2)
  E: manifest (line=2)
    A: ${NS}:versionCode(0x0101021b)=1
    A: package="app.dynodoc.desktop" (Raw: "app.dynodoc.desktop")
    E: uses-permission (line=7)
      A: ${NS}:name(0x01010003)="android.permission.INTERNET" (Raw: "android.permission.INTERNET")
    E: application (line=12)
      A: ${NS}:label(0x01010001)=@0x7f0b001b
${application.map((a) => `      A: ${NS}:${a}`).join('\n')}
      E: activity (line=20)
        A: ${NS}:name(0x01010003)="app.dynodoc.desktop.MainActivity" (Raw: "app.dynodoc.desktop.MainActivity")
${activity.map((a) => `        A: ${NS}:${a}`).join('\n')}
      E: provider (line=40)
        A: ${NS}:allowBackup(0x01010280)=true
    ${extra}
`;

const SAFE = ['allowBackup(0x01010280)=false', 'usesCleartextTraffic(0x010104ec)=false'];

test('aapt2: reads the attributes of <application> only, not those of its children', () => {
  const attrs = parseAapt2(aapt2({ application: [...SAFE, 'debuggable(0x0101000f)=true'], activity: ['exported(0x01010010)=true'] }));
  assert.deepEqual(attrs, { label: '@0x7f0b001b', allowBackup: 'false', usesCleartextTraffic: 'false', debuggable: 'true' });
  assert.equal(attrs.exported, undefined);
  assert.equal(manifestProblems(attrs, { build: 'debug' }).length, 0);
});

test('aapt2: the older numeric boolean forms are understood', () => {
  const attrs = parseAapt2(aapt2({ application: ['allowBackup(0x01010280)=(type 0x12)0x0', 'usesCleartextTraffic(0x010104ec)=(type 0x12)0x0'] }));
  assert.equal(manifestProblems(attrs).length, 0);
  const bad = parseAapt2(aapt2({ application: ['allowBackup(0x01010280)=(type 0x12)0xffffffff', 'usesCleartextTraffic(0x010104ec)=(type 0x12)0xffffffff'] }));
  assert.equal(manifestProblems(bad).length, 2);
});

test('an unsafe or missing value fails, whatever form it is written in', () => {
  const cases = {
    'allowBackup true': [['allowBackup(0x01010280)=true', SAFE[1]], /allowBackup is true/],
    'cleartext true': [[SAFE[0], 'usesCleartextTraffic(0x010104ec)=true'], /usesCleartextTraffic is true/],
    'allowBackup missing': [[SAFE[1]], /allowBackup is not set/],
    'cleartext missing': [[SAFE[0]], /usesCleartextTraffic is not set/],
    'a resource reference, not a literal': [['allowBackup(0x01010280)=@0x7f050001', SAFE[1]], /allowBackup is @0x7f050001/],
    'something not understood': [['allowBackup(0x01010280)=maybe', SAFE[1]], /allowBackup is maybe/],
    'testOnly': [[...SAFE, 'testOnly(0x01010272)=true'], /testOnly is set/],
    'a network security config': [[...SAFE, 'networkSecurityConfig(0x01010527)=@0x7f0d0001'], /network security config/],
  };
  for (const [name, [application, expected]] of Object.entries(cases)) {
    const found = manifestProblems(parseAapt2(aapt2({ application })));
    assert.ok(found.some((f) => expected.test(f)), `${name}: ${found.join(' | ')}`);
  }
});

test('debuggable: allowed in a debug build, a failure in a release build, and unreadable values fail', () => {
  const debuggable = parseAapt2(aapt2({ application: [...SAFE, 'debuggable(0x0101000f)=true'] }));
  assert.equal(manifestProblems(debuggable, { build: 'debug' }).length, 0);
  assert.deepEqual(manifestProblems(debuggable, { build: 'release' }), ['debuggable is true in a release build']);
  assert.equal(manifestProblems(parseAapt2(aapt2({ application: [...SAFE, 'debuggable(0x0101000f)=false'] })), { build: 'release' }).length, 0);
  assert.equal(manifestProblems(parseAapt2(aapt2({ application: SAFE })), { build: 'release' }).length, 0, 'absent means not debuggable');
  assert.equal(manifestProblems(parseAapt2(aapt2({ application: [...SAFE, 'debuggable(0x0101000f)=@0x7f010002'] })), { build: 'release' }).length, 1);
});

test('an APK with no <application> at all, or garbage, fails', () => {
  assert.equal(manifestProblems(parseAapt2('')).length, 2);
  assert.equal(manifestProblems(parseAapt2('E: manifest\n  A: x\nnonsense')).length, 2);
  assert.equal(manifestProblems(parseApkanalyzer('<manifest></manifest>')).length, 2);
});

test('apkanalyzer: reads the XML attributes of <application>', () => {
  const xml = (app) => `<?xml version="1.0" encoding="utf-8"?>
<manifest xmlns:android="${NS}" package="app.dynodoc.desktop">
    <uses-permission android:name="android.permission.INTERNET" />
    <application
        android:label="Dynodoc"
        ${app}
        android:name="app.dynodoc.desktop.App">
        <activity android:name=".MainActivity" android:exported="true" android:allowBackup="true" />
    </application>
</manifest>`;
  const safe = parseApkanalyzer(xml('android:allowBackup="false" android:usesCleartextTraffic="false" android:debuggable="true"'));
  assert.equal(safe.allowBackup, 'false');
  assert.equal(safe.exported, undefined);
  assert.equal(manifestProblems(safe, { build: 'debug' }).length, 0);
  assert.equal(manifestProblems(safe, { build: 'release' }).length, 1);
  assert.ok(manifestProblems(parseApkanalyzer(xml('android:allowBackup="true" android:usesCleartextTraffic="true"'))).length === 2);
  assert.ok(manifestProblems(parseApkanalyzer(xml('android:usesCleartextTraffic="false"'))).some((f) => /allowBackup is not set/.test(f)));
});

test('readValue never turns something unclear into false', () => {
  for (const [raw, expected] of [['false', false], ['true', true], ['(type 0x12)0x0', false], ['(type 0x12)0xffffffff', true], ['0x0', false], ['1', true], ['@0x7f01', 'reference'], ['', undefined], ['False?', undefined], ['0x7', undefined]]) {
    assert.equal(readValue(raw), expected, raw);
  }
});

test('the newest build-tools folder holding aapt2 is chosen by version, not by text', () => {
  const have = new Set(['/sdk/build-tools', '/sdk/build-tools/9.0.0/aapt2', '/sdk/build-tools/34.0.0/aapt2.exe', '/sdk/build-tools/35.0.1/aapt2', '/sdk/build-tools/36.0.0-rc1']);
  const list = () => ['9.0.0', '34.0.0', '35.0.1', '36.0.0-rc1'];
  const exists = (p) => have.has(p.replaceAll('\\', '/'));
  assert.equal(newestBuildTool('/sdk', 'aapt2', list, exists).replaceAll('\\', '/'), '/sdk/build-tools/35.0.1/aapt2');
  assert.equal(newestBuildTool('/nowhere', 'aapt2', list, () => false), null);
});
