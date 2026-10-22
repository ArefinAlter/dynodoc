#!/usr/bin/env node
// Writes a software bill of materials (SPDX JSON) for each image of the bundle, using the Docker
// Scout CLI plugin that ships with Docker Desktop (`docker scout sbom`; it indexes the local image
// and needs no account). Where that plugin is missing, build with BuildKit attestations instead:
//   docker buildx build --sbom=true --provenance=mode=max ...   (the release pipeline's job, P0-6)
//
//   node infra/selfhosted/scripts/sbom.mjs [--registry dynodoc-local] [--version dev] [--out sbom]
//
// The SBOM lists operating-system packages and application dependencies found in the image
// layers. It is evidence for the release record, not a vulnerability verdict.
import { spawnSync } from 'node:child_process';
import { mkdirSync, readFileSync, statSync } from 'node:fs';
import path from 'node:path';
import { fileURLToPath } from 'node:url';

const args = process.argv.slice(2);
const opt = (name, fallback) => { const i = args.indexOf(name); return i >= 0 && i + 1 < args.length ? args[i + 1] : fallback; };
const registry = opt('--registry', 'dynodoc-local');
const version = opt('--version', 'dev');
const out = path.resolve(opt('--out', path.join(path.dirname(fileURLToPath(import.meta.url)), '..', 'sbom')));
mkdirSync(out, { recursive: true });

if (spawnSync('docker', ['scout', 'version'], { encoding: 'utf8' }).status !== 0) {
  console.error('docker scout is not installed. Use BuildKit attestations (docker buildx build --sbom=true) instead.');
  process.exit(2);
}
let failed = 0;
for (const name of ['api', 'web', 'tools', 'proxy', 'acme']) {
  const ref = `${registry}/${name}:${version}`;
  const file = path.join(out, `${name}.spdx.json`);
  const r = spawnSync('docker', ['scout', 'sbom', '--format', 'spdx', '--output', file, ref], { encoding: 'utf8' });
  if (r.status !== 0) { console.log(`FAIL ${ref}: ${(r.stderr ?? '').split('\n').filter((l) => !l.startsWith('{')).join(' ').slice(0, 200)}`); failed++; continue; }
  const doc = JSON.parse(readFileSync(file, 'utf8'));
  console.log(`${ref.padEnd(34)} ${String((doc.packages ?? []).length).padStart(5)} packages  ${(statSync(file).size / 1024).toFixed(0).padStart(6)} KiB  ${file}`);
}
process.exit(failed ? 1 : 0);
