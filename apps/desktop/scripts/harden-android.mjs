#!/usr/bin/env node
// After `tauri android init`: make the generated Android project safe, then check it.
//
//   node scripts/harden-android.mjs [path/to/src-tauri/gen/android]
//
// What Tauri 2.12's template produces (checked by running `tauri android init`): a manifest
// with no `android:allowBackup` attribute (Android's default is TRUE, so the web view's
// cookies, including a signed-in session, would be eligible for cloud backup), and
// `android:usesCleartextTraffic="${usesCleartextTraffic}"` with the placeholder set to "false"
// in app/build.gradle.kts for release and to "true" for debug builds (for a dev server). This
// shell only ever opens https servers, so both are fixed here:
//   - android:allowBackup="false" is written into <application>;
//   - cleartext traffic is "false" in every build type.
// Then the result is checked and the script exits 1 if anything unsafe is left, so a change
// in a later Tauri template cannot slip through. Run it before `tauri android build`.

import { existsSync, readFileSync, writeFileSync } from 'node:fs';
import { dirname, join, resolve } from 'node:path';
import { fileURLToPath } from 'node:url';

const here = dirname(fileURLToPath(import.meta.url));

/** Returns the patched text of the manifest and of app/build.gradle.kts. */
export function harden(manifest, gradle) {
  let m = manifest;
  if (/android:allowBackup\s*=/.test(m)) {
    m = m.replace(/android:allowBackup\s*=\s*"[^"]*"/g, 'android:allowBackup="false"');
  } else {
    m = m.replace(/<application(\s)/, '<application\n        android:allowBackup="false"$1');
  }
  m = m.replace(/android:usesCleartextTraffic\s*=\s*"true"/g, 'android:usesCleartextTraffic="false"');
  const g = gradle.replace(
    /(manifestPlaceholders\s*\[\s*"usesCleartextTraffic"\s*\]\s*=\s*)"true"/g,
    '$1"false"',
  );
  return { manifest: m, gradle: g };
}

/** The problems left in the (patched) project; an empty list means it is safe. */
export function problems(manifest, gradle) {
  const found = [];
  if (!/android:allowBackup\s*=\s*"false"/.test(manifest)) found.push('allowBackup is not "false"');
  if (/android:allowBackup\s*=\s*"true"/.test(manifest)) found.push('allowBackup="true"');
  if (/android:usesCleartextTraffic\s*=\s*"true"/.test(manifest)) found.push('usesCleartextTraffic="true" in the manifest');
  if (/android:debuggable\s*=\s*"true"/.test(manifest)) found.push('debuggable="true" in the manifest');
  if (/manifestPlaceholders\s*\[\s*"usesCleartextTraffic"\s*\]\s*=\s*"true"/.test(gradle)) {
    found.push('a build type sets the usesCleartextTraffic placeholder to "true"');
  }
  // With a placeholder in the manifest, every value the build file gives it must be "false".
  if (/usesCleartextTraffic\s*=\s*"\$\{usesCleartextTraffic\}"/.test(manifest)) {
    const values = [...gradle.matchAll(/manifestPlaceholders\s*\[\s*"usesCleartextTraffic"\s*\]\s*=\s*"([^"]*)"/g)].map((x) => x[1]);
    if (values.length === 0 || values.some((v) => v !== 'false')) {
      found.push(`the usesCleartextTraffic placeholder has values [${values.join(', ')}], not only "false"`);
    }
  }
  if (!/android:usesCleartextTraffic\s*=/.test(manifest)) found.push('usesCleartextTraffic is not set (the default depends on the target SDK)');
  return found;
}

if (process.argv[1] && resolve(process.argv[1]) === fileURLToPath(import.meta.url)) {
  const root = resolve(process.argv[2] ?? join(here, '..', 'src-tauri', 'gen', 'android'));
  const manifestPath = join(root, 'app', 'src', 'main', 'AndroidManifest.xml');
  const gradlePath = join(root, 'app', 'build.gradle.kts');
  for (const file of [manifestPath, gradlePath]) {
    if (!existsSync(file)) {
      console.error(`harden-android: ${file} not found (run: npm run tauri -- android init)`);
      process.exit(2);
    }
  }
  const before = { manifest: readFileSync(manifestPath, 'utf8'), gradle: readFileSync(gradlePath, 'utf8') };
  const after = harden(before.manifest, before.gradle);
  if (after.manifest !== before.manifest) writeFileSync(manifestPath, after.manifest);
  if (after.gradle !== before.gradle) writeFileSync(gradlePath, after.gradle);
  console.log(
    `harden-android: manifest ${after.manifest !== before.manifest ? 'patched' : 'already safe'}, build.gradle.kts ${after.gradle !== before.gradle ? 'patched' : 'already safe'}`,
  );
  const left = problems(after.manifest, after.gradle);
  if (left.length > 0) {
    for (const p of left) console.error(`harden-android: UNSAFE: ${p}`);
    process.exit(1);
  }
  console.log('harden-android: allowBackup=false, cleartext traffic off in every build type');
}
