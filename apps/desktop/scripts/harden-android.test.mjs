// node --test scripts/harden-android.test.mjs
import assert from 'node:assert/strict';
import test from 'node:test';
import { EXTRACTION_RULES_XML, extractionRuleProblems, harden, problems as rawProblems } from './harden-android.mjs';

// The shape of Tauri 2.12's generated files (the parts that matter).
const templateManifest = `<?xml version="1.0" encoding="utf-8"?>
<manifest xmlns:android="http://schemas.android.com/apk/res/android">
    <uses-permission android:name="android.permission.INTERNET" />
    <application
        android:icon="@mipmap/ic_launcher"
        android:label="@string/app_name"
        android:theme="@style/Theme.app"
        android:usesCleartextTraffic="\${usesCleartextTraffic}">
        <activity android:name=".MainActivity" android:exported="true" />
    </application>
</manifest>`;
const templateGradle = `android {
    defaultConfig {
        manifestPlaceholders["usesCleartextTraffic"] = "false"
    }
    buildTypes {
        getByName("debug") {
            manifestPlaceholders["usesCleartextTraffic"] = "true"
            isDebuggable = true
        }
        getByName("release") {
            isMinifyEnabled = true
        }
    }
}`;

// The project's own res/xml files, as the script reads them.
const readRules = (f) => (f === 'xml/data_extraction_rules.xml' ? EXTRACTION_RULES_XML : null);
const problems = (manifest, gradle, read = readRules) => rawProblems(manifest, gradle, read);

test('the unpatched template has the problems this script exists for', () => {
  const found = problems(templateManifest, templateGradle);
  assert.ok(found.some((p) => p.includes('allowBackup')), found.join('; '));
  assert.ok(found.some((p) => p.includes('"true"')), found.join('; '));
  assert.ok(found.some((p) => p.includes('dataExtractionRules')), found.join('; '));
});

test('the extraction rules must exclude every domain from the cloud backup and the device transfer', () => {
  assert.deepEqual(extractionRuleProblems(EXTRACTION_RULES_XML), []);
  const noTransfer = EXTRACTION_RULES_XML.replace(/<device-transfer>[\s\S]*<\/device-transfer>/, '');
  assert.ok(extractionRuleProblems(noTransfer).some((p) => p.includes('device-transfer')));
  const noCloud = EXTRACTION_RULES_XML.replace(/<cloud-backup>[\s\S]*?<\/cloud-backup>/, '<cloud-backup />');
  assert.ok(extractionRuleProblems(noCloud).some((p) => p.includes('cloud-backup')));
  const missingDomain = EXTRACTION_RULES_XML.replace('<exclude domain="sharedpref" />', '');
  assert.ok(extractionRuleProblems(missingDomain).some((p) => p.includes('sharedpref')));
  const included = EXTRACTION_RULES_XML.replace('</data-extraction-rules>', '<cloud-backup><include domain="file" path="x" /></cloud-backup></data-extraction-rules>');
  assert.ok(extractionRuleProblems(included).some((p) => p.includes('<include>')));
  const byPath = EXTRACTION_RULES_XML.replace('<exclude domain="file" />', '<exclude domain="file" path="a" />');
  assert.ok(extractionRuleProblems(byPath).length > 0);
  // A commented-out exclude does not count.
  const commented = EXTRACTION_RULES_XML.replace('<exclude domain="root" />', '<!-- <exclude domain="root" /> -->');
  assert.ok(extractionRuleProblems(commented).some((p) => p.includes('root')));
});

test('a manifest whose extraction rules file is missing, or not a plain reference, fails', () => {
  const { manifest, gradle } = harden(templateManifest, templateGradle);
  assert.deepEqual(problems(manifest, gradle), []);
  assert.ok(problems(manifest, gradle, () => null).some((p) => p.includes('not a file in the project')));
  const raw = manifest.replace('@xml/data_extraction_rules', '${rules}');
  assert.ok(problems(raw, gradle).some((p) => p.includes('plain @xml reference')));
});

test('the template is made safe', () => {
  const { manifest, gradle } = harden(templateManifest, templateGradle);
  assert.deepEqual(problems(manifest, gradle), []);
  assert.match(manifest, /android:allowBackup="false"/);
  assert.doesNotMatch(gradle, /usesCleartextTraffic"\]\s*=\s*"true"/);
});

test('hardening is idempotent', () => {
  const once = harden(templateManifest, templateGradle);
  const twice = harden(once.manifest, once.gradle);
  assert.deepEqual(twice, once);
});

test('literal true values are turned off and a changed template is still checked', () => {
  const hostile = `<manifest><application android:allowBackup="true" android:usesCleartextTraffic="true" android:debuggable="true"></application></manifest>`;
  const { manifest } = harden(hostile, '');
  assert.match(manifest, /allowBackup="false"/);
  assert.match(manifest, /usesCleartextTraffic="false"/);
  assert.doesNotMatch(manifest, /allowBackup="true"|usesCleartextTraffic="true"/);
  // debuggable is not ours to rewrite: the check keeps failing until a person looks.
  assert.ok(problems(manifest, '').some((p) => p.includes('debuggable')));
});

test('a build file that sets the placeholder to anything but false fails the check', () => {
  const { manifest } = harden(templateManifest, templateGradle);
  for (const bad of ['"true"', '"${something}"', '"TRUE"']) {
    const gradle = `manifestPlaceholders["usesCleartextTraffic"] = ${bad}`;
    assert.ok(problems(manifest, gradle).length > 0, bad);
  }
  assert.ok(problems(manifest, 'no placeholder here').length > 0);
  assert.ok(problems('<manifest><application android:allowBackup="false"></application></manifest>', '').length > 0, 'cleartext not set at all');
});

test('a network security config is allowed only if a file in the project turns cleartext off', () => {
  const { manifest, gradle } = harden(templateManifest, templateGradle);
  const withConfig = manifest.replace('<application', '<application android:networkSecurityConfig="@xml/nsc"');
  const off = '<network-security-config><base-config cleartextTrafficPermitted="false" /></network-security-config>';
  const on = '<network-security-config><base-config cleartextTrafficPermitted="true" /></network-security-config>';
  const silent = '<network-security-config><base-config /></network-security-config>';
  assert.deepEqual(problems(withConfig, gradle, (f) => (f === 'xml/nsc.xml' ? off : readRules(f))), []);
  for (const xml of [on, silent, null, off + on]) {
    assert.ok(problems(withConfig, gradle, (f) => (f === 'xml/nsc.xml' ? xml : readRules(f))).length > 0, String(xml));
  }
  const odd = manifest.replace('<application', '<application android:networkSecurityConfig="@xml/../x"');
  assert.ok(problems(odd, gradle, (f) => (f === 'xml/nsc.xml' ? off : readRules(f))).length > 0);
  const raw = manifest.replace('<application', '<application android:networkSecurityConfig="${nsc}"');
  assert.ok(problems(raw, gradle, (f) => (f === 'xml/nsc.xml' ? off : readRules(f))).length > 0);
});

test('every spelling of a true cleartext placeholder in the build file is caught', () => {
  const { manifest } = harden(templateManifest, templateGradle);
  for (const line of [
    'manifestPlaceholders["usesCleartextTraffic"] = "true"',
    'manifestPlaceholders += mapOf("usesCleartextTraffic" to "true")',
    "manifestPlaceholders.put('usesCleartextTraffic', 'true')",
    'manifestPlaceholders = [usesCleartextTraffic: "true"]',
  ]) {
    const gradle = `manifestPlaceholders["usesCleartextTraffic"] = "false"\n${line}`;
    assert.ok(problems(manifest, gradle).length > 0, line);
  }
});

test('testOnly in the manifest is a problem', () => {
  const { manifest, gradle } = harden(templateManifest, templateGradle);
  assert.ok(problems(manifest.replace('<application', '<application android:testOnly="true"'), gradle).some((p) => p.includes('testOnly')));
});
