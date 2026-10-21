// node --test scripts/harden-android.test.mjs
import assert from 'node:assert/strict';
import test from 'node:test';
import { harden, problems } from './harden-android.mjs';

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

test('the unpatched template has two problems this script exists for', () => {
  const found = problems(templateManifest, templateGradle);
  assert.ok(found.some((p) => p.includes('allowBackup')), found.join('; '));
  assert.ok(found.some((p) => p.includes('"true"')), found.join('; '));
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
  assert.deepEqual(problems(withConfig, gradle, (f) => (f === 'xml/nsc.xml' ? off : null)), []);
  for (const xml of [on, silent, null, off + on]) {
    assert.ok(problems(withConfig, gradle, () => xml).length > 0, String(xml));
  }
  const odd = manifest.replace('<application', '<application android:networkSecurityConfig="@xml/../x"');
  assert.ok(problems(odd, gradle, () => off).length > 0);
  const raw = manifest.replace('<application', '<application android:networkSecurityConfig="${nsc}"');
  assert.ok(problems(raw, gradle, () => off).length > 0);
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
