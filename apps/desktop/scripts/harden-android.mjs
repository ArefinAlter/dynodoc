#!/usr/bin/env node
// After `tauri android init` (or in CI, on the committed project): make the Android project
// safe, then check it.
//
//   node scripts/harden-android.mjs [--check] [path/to/src-tauri/gen/android]
//
// What Tauri 2.12's template produces (checked by running `tauri android init`): a manifest
// with no `android:allowBackup` attribute (Android's default is TRUE, so the web view's
// cookies, including a signed-in session, would be eligible for cloud backup), and
// `android:usesCleartextTraffic="${usesCleartextTraffic}"` with the placeholder set to "false"
// in app/build.gradle.kts for release and to "true" for debug builds (for a dev server). This
// shell only ever opens https servers, so these are fixed here:
//   - android:allowBackup="false" is written into <application>;
//   - android:dataExtractionRules points at res/xml/data_extraction_rules.xml, which excludes
//     every domain from the cloud backup and the device transfer (Android 12+);
//   - cleartext traffic is "false" in every build type.
// Then the result is checked and the script exits 1 if anything unsafe is left, so a change
// in a later Tauri template cannot slip through. The generated project is committed
// (apps/desktop/src-tauri/gen/android), so CI runs it with --check: nothing is rewritten,
// and it fails if the committed project is not already in the hardened state.

import { existsSync, mkdirSync, readFileSync, writeFileSync } from 'node:fs';
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
  // Android 12+ reads the extraction rules instead of allowBackup for cloud backup and for
  // device-to-device transfer: say "nothing" there too (res/xml/data_extraction_rules.xml).
  if (!/android:dataExtractionRules\s*=/.test(m)) {
    m = m.replace(/<application(\s)/, '<application\n        android:dataExtractionRules="@xml/data_extraction_rules"$1');
  }
  m = m.replace(/android:usesCleartextTraffic\s*=\s*"true"/g, 'android:usesCleartextTraffic="false"');
  const g = gradle.replace(
    /(manifestPlaceholders\s*\[\s*"usesCleartextTraffic"\s*\]\s*=\s*)"true"/g,
    '$1"false"',
  );
  return { manifest: m, gradle: g };
}

const EXTRACTION_DOMAINS = ['root', 'file', 'database', 'sharedpref', 'external', 'device_root', 'device_file', 'device_database', 'device_sharedpref'];

/** The rules file this script writes when the project has none (and the one the project commits). */
export const EXTRACTION_RULES_XML = `<?xml version="1.0" encoding="utf-8"?>
<!-- Dynodoc keeps nothing worth backing up on the phone: the web view's cookies hold a signed-in
     session, so no domain is copied to the cloud or to a new phone. Checked by scripts/harden-android.mjs. -->
<data-extraction-rules>
    <cloud-backup>
${EXTRACTION_DOMAINS.map((d) => `        <exclude domain="${d}" />`).join('\n')}
    </cloud-backup>
    <device-transfer>
${EXTRACTION_DOMAINS.map((d) => `        <exclude domain="${d}" />`).join('\n')}
    </device-transfer>
</data-extraction-rules>
`;

/** What is wrong with a data-extraction-rules file; an empty list means it excludes everything. */
export function extractionRuleProblems(xml, name = 'data_extraction_rules') {
  const found = [];
  const body = xml.replace(/<!--[\s\S]*?-->/g, '');
  if (/<include\b/.test(body)) found.push(`@xml/${name} has an <include>`);
  for (const section of ['cloud-backup', 'device-transfer']) {
    const match = body.match(new RegExp(`<${section}\\b[^>]*>([\\s\\S]*?)</${section}>`));
    if (!match) {
      found.push(`@xml/${name} has no <${section}> section (the default would copy data)`);
      continue;
    }
    const excluded = [...match[1].matchAll(/<exclude\s+domain\s*=\s*"([^"]*)"[^>]*\/>/g)].map((x) => x[1]);
    for (const domain of EXTRACTION_DOMAINS) {
      if (!excluded.includes(domain)) found.push(`@xml/${name}: <${section}> does not exclude ${domain}`);
    }
    if (/<exclude\b[^>]*\bpath\s*=/.test(match[1])) found.push(`@xml/${name}: <${section}> excludes by path, not whole domains`);
  }
  return found;
}

/**
 * The problems left in the (patched) project; an empty list means it is safe. `readXml` reads
 * a resource of the app module (`xml/network_security_config.xml`) or returns null.
 */
export function problems(manifest, gradle, readXml = () => null) {
  const found = [];
  if (!/android:allowBackup\s*=\s*"false"/.test(manifest)) found.push('allowBackup is not "false"');
  if (/android:allowBackup\s*=\s*"true"/.test(manifest)) found.push('allowBackup="true"');
  if (/android:usesCleartextTraffic\s*=\s*"true"/.test(manifest)) found.push('usesCleartextTraffic="true" in the manifest');
  if (/android:debuggable\s*=\s*"true"/.test(manifest)) found.push('debuggable="true" in the manifest');
  if (/android:testOnly\s*=\s*"true"/.test(manifest)) found.push('testOnly="true" in the manifest');
  // The extraction rules must be the project's own file and must exclude every domain from
  // both the cloud backup and the device transfer (no <include>, nothing left out of the list).
  const rules = manifest.match(/android:dataExtractionRules\s*=\s*"@xml\/([A-Za-z0-9_]+)"/);
  if (!rules) {
    found.push('dataExtractionRules is not a plain @xml reference');
  } else {
    const xml = readXml(`xml/${rules[1]}.xml`);
    if (xml === null) found.push(`dataExtractionRules @xml/${rules[1]} is not a file in the project`);
    else found.push(...extractionRuleProblems(xml, rules[1]));
  }
  // A network security config can allow cleartext on its own: it must be a file in the
  // project that turns cleartext off everywhere, or there must be none.
  const config = manifest.match(/android:networkSecurityConfig\s*=\s*"@xml\/([A-Za-z0-9_]+)"/);
  if (/android:networkSecurityConfig/.test(manifest) && !config) {
    found.push('networkSecurityConfig is not a plain @xml reference');
  } else if (config) {
    const xml = readXml(`xml/${config[1]}.xml`);
    if (xml === null) found.push(`networkSecurityConfig @xml/${config[1]} is not a file in the project`);
    else if (/cleartextTrafficPermitted\s*=\s*"true"/.test(xml) || !/cleartextTrafficPermitted\s*=\s*"false"/.test(xml)) {
      found.push(`networkSecurityConfig @xml/${config[1]} does not turn cleartext off everywhere`);
    }
  }
  // Any other way the build file might hand the manifest a true value.
  for (const line of gradle.split('\n')) {
    if (/usesCleartextTraffic/i.test(line) && /\btrue\b/i.test(line)) {
      found.push(`the build file sets usesCleartextTraffic to true: ${line.trim()}`);
    }
  }
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
  // --check: change nothing; fail if the project is not already exactly what `harden` would make
  // it (the committed project is checked this way in CI).
  const check = process.argv.includes('--check');
  const root = resolve(process.argv.slice(2).find((a) => !a.startsWith('--')) ?? join(here, '..', 'src-tauri', 'gen', 'android'));
  const manifestPath = join(root, 'app', 'src', 'main', 'AndroidManifest.xml');
  const gradlePath = join(root, 'app', 'build.gradle.kts');
  const rulesPath = join(root, 'app', 'src', 'main', 'res', 'xml', 'data_extraction_rules.xml');
  for (const file of [manifestPath, gradlePath]) {
    if (!existsSync(file)) {
      console.error(`harden-android: ${file} not found (run: npm run tauri -- android init)`);
      process.exit(2);
    }
  }
  const before = { manifest: readFileSync(manifestPath, 'utf8'), gradle: readFileSync(gradlePath, 'utf8') };
  const after = harden(before.manifest, before.gradle);
  const changed = after.manifest !== before.manifest || after.gradle !== before.gradle;
  if (check) {
    if (changed) {
      console.error('harden-android: --check: the project is not hardened (run node scripts/harden-android.mjs and commit the result)');
      process.exit(1);
    }
  } else {
    if (after.manifest !== before.manifest) writeFileSync(manifestPath, after.manifest);
    if (after.gradle !== before.gradle) writeFileSync(gradlePath, after.gradle);
    if (!existsSync(rulesPath)) {
      mkdirSync(dirname(rulesPath), { recursive: true });
      writeFileSync(rulesPath, EXTRACTION_RULES_XML);
    }
    console.log(
      `harden-android: manifest ${after.manifest !== before.manifest ? 'patched' : 'already safe'}, build.gradle.kts ${after.gradle !== before.gradle ? 'patched' : 'already safe'}`,
    );
  }
  const readXml = (relative) => {
    const file = join(root, 'app', 'src', 'main', 'res', relative);
    return existsSync(file) ? readFileSync(file, 'utf8') : null;
  };
  const left = problems(after.manifest, after.gradle, readXml);
  if (left.length > 0) {
    for (const p of left) console.error(`harden-android: UNSAFE: ${p}`);
    process.exit(1);
  }
  console.log('harden-android: allowBackup=false, cleartext traffic off in every build type');
}
