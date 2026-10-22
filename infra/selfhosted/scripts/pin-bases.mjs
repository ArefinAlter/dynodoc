#!/usr/bin/env node
// Lists every base image the bundle pins by digest, asks the registry for the digest each tag
// points at today, and says which pins are out of date. Needs Docker with buildx and network
// access to the registries.
//
//   node infra/selfhosted/scripts/pin-bases.mjs            report only (exit 1 if a pin is stale)
//   node infra/selfhosted/scripts/pin-bases.mjs --update   rewrite the pinned digests in place
//
// A pin looks like `name:tag@sha256:<64 hex>` in a Dockerfile ARG, in compose.yml or in a test.
// The digest is that of the multi-platform index, so the same pin serves linux/amd64 and
// linux/arm64. After an update: rebuild, run scripts/smoke.mjs, test/proxy-tls.mjs and
// test/acme-pebble.mjs, then record the new digests in the release notes. Refresh at least with
// every release and whenever a base image has a security fix.
import { spawnSync } from 'node:child_process';
import { readFileSync, writeFileSync } from 'node:fs';
import path from 'node:path';
import { fileURLToPath } from 'node:url';

const root = path.resolve(path.dirname(fileURLToPath(import.meta.url)), '..');
const FILES = [
  'images/Dockerfile.api', 'images/Dockerfile.web', 'images/Dockerfile.proxy', 'images/Dockerfile.acme',
  'compose.yml', 'test/acme-pebble.mjs'
];
const PIN = /([a-z0-9][a-z0-9._/-]*(?::[A-Za-z0-9._-]+)?)@(sha256:[0-9a-f]{64})/g;
const update = process.argv.includes('--update');

const pins = new Map(); // ref -> { digest, files: Set }
for (const file of FILES) {
  const text = readFileSync(path.join(root, file), 'utf8');
  for (const [, ref, digest] of text.matchAll(PIN)) {
    if (!pins.has(ref)) pins.set(ref, { digest, files: new Set() });
    pins.get(ref).files.add(file);
  }
}

let stale = 0;
const current = new Map();
for (const [ref, pin] of pins) {
  const r = spawnSync('docker', ['buildx', 'imagetools', 'inspect', ref.replace(/@.*/, '')], { encoding: 'utf8' });
  const digest = /^Digest:\s+(sha256:[0-9a-f]{64})/m.exec(r.stdout ?? '')?.[1];
  if (!digest) { console.log(`?      ${ref}  (could not resolve: ${(r.stderr ?? '').trim().split('\n')[0]})`); stale++; continue; }
  current.set(ref, digest);
  const same = digest === pin.digest;
  if (!same) stale++;
  console.log(`${same ? 'ok    ' : 'STALE '} ${ref}\n         pinned  ${pin.digest}\n         current ${digest}${same ? '' : `\n         in ${[...pin.files].join(', ')}`}`);
}

if (update) {
  for (const file of FILES) {
    const p = path.join(root, file);
    const before = readFileSync(p, 'utf8');
    const after = before.replace(PIN, (whole, ref, digest) => `${ref}@${current.get(ref) ?? digest}`);
    if (after !== before) { writeFileSync(p, after); console.log(`updated ${file}`); }
  }
  process.exit(0);
}
console.log(stale ? `\n${stale} pin(s) out of date or unresolved. Run with --update, rebuild, and re-run the tests.` : '\nAll pins are current.');
process.exit(stale ? 1 : 0);
