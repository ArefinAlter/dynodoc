// @vitest-environment node
// scripts/prune-hosted-static.mjs and the audit profile for the self-hosted image: the hosted
// add-in packages are removed from the image, a package left in it is a violation, and the
// hosted build is audited exactly as before.
import { spawnSync } from 'node:child_process';
import { afterAll, describe, expect, it } from 'vitest';
import {
  existsSync,
  mkdirSync,
  mkdtempSync,
  readFileSync,
  readdirSync,
  rmSync,
  writeFileSync
} from 'node:fs';
import { tmpdir } from 'node:os';
import { dirname, join } from 'node:path';
import { strToU8, zipSync } from 'fflate';
import { HOSTED_ADDIN_FILES, prune } from '../../scripts/prune-hosted-static.mjs';
import { knownFor, scanBuild, violations } from '../../scripts/audit-hosted-strings.mjs';

const known = JSON.parse(
  readFileSync(join(process.cwd(), 'scripts', 'hosted-strings-known.json'), 'utf8')
);
const root = mkdtempSync(join(tmpdir(), 'dynodoc-prune-'));
afterAll(() => rmSync(root, { recursive: true, force: true }));
const script = join(process.cwd(), 'scripts', 'prune-hosted-static.mjs');

function put(build: string, path: string, content: string | Uint8Array) {
  const full = join(build, path);
  mkdirSync(dirname(full), { recursive: true });
  writeFileSync(full, content);
}
let counter = 0;
/** A build that holds the hosted packages, their compressed copies, and files that must stay. */
function makeBuild() {
  const build = join(root, `build-${counter++}`);
  for (const name of HOSTED_ADDIN_FILES) {
    put(
      build,
      `client/${name}`,
      name.endsWith('.zip')
        ? zipSync({ 'Code.gs': strToU8("const ORIGIN = 'https://app.dynodoc.online';") })
        : '<Id>https://app.dynodoc.online/x</Id>'
    );
    if (name.endsWith('.xml')) {
      put(build, `client/${name}.br`, 'compressed');
      put(build, `client/${name}.gz`, 'compressed');
    }
  }
  put(build, 'client/connectors/commands.html', '<script src="/connectors/commands.js">');
  put(build, 'client/connectors/assets/icon-16.png', 'png');
  put(build, 'client/.well-known/security.txt', 'Contact: info@dynodoc.online');
  put(build, 'client/brand/social.svg', '<svg>dynodoc.online</svg>');
  put(build, 'package.json', '{"type":"module"}');
  return build;
}
const walk = (dir: string): string[] =>
  readdirSync(dir, { withFileTypes: true }).flatMap((entry) =>
    entry.isDirectory() ? walk(join(dir, entry.name)) : [join(dir, entry.name)]
  );

describe('the prune step', () => {
  it('lists exactly the files in static/connectors/packages, so a new hosted package cannot stay', () => {
    const inSource = readdirSync(join(process.cwd(), 'static/connectors/packages'))
      .map((name) => `connectors/packages/${name}`)
      .sort();
    expect([...HOSTED_ADDIN_FILES].sort()).toEqual(inSource);
  });
  it('removes the six packages and their compressed copies and nothing else', () => {
    const build = makeBuild();
    const before = walk(build).length;
    const removed = prune(build);
    expect(removed).toHaveLength(12);
    expect(walk(build)).toHaveLength(before - 12);
    expect(existsSync(join(build, 'client/connectors/packages'))).toBe(false);
    for (const kept of [
      'client/connectors/commands.html',
      'client/connectors/assets/icon-16.png',
      'client/.well-known/security.txt',
      'client/brand/social.svg',
      'package.json'
    ])
      expect(existsSync(join(build, kept)), kept).toBe(true);
  });
  it('is safe to run twice and on a build without them', () => {
    const build = makeBuild();
    prune(build);
    expect(prune(build)).toEqual([]);
    const bare = join(root, 'bare');
    mkdirSync(bare);
    expect(prune(bare)).toEqual([]);
  });
  it('changes nothing in a dry run', () => {
    const build = makeBuild();
    const before = walk(build).length;
    expect(prune(build, { dryRun: true })).toHaveLength(12);
    expect(walk(build)).toHaveLength(before);
  });
  it('does not touch the folder of a package that is not a hosted one', () => {
    const build = makeBuild();
    put(build, 'client/connectors/packages/custom.xml', 'x');
    prune(build);
    expect(existsSync(join(build, 'client/connectors/packages/custom.xml'))).toBe(true);
  });
});

describe('the command line', () => {
  const run = (...args: string[]) =>
    spawnSync(process.execPath, [script, ...args], { encoding: 'utf8' });
  it('prints what it removed as JSON, and exits 0', () => {
    const build = makeBuild();
    const result = run('--build', build, '--json');
    expect(result.status).toBe(0);
    const out = JSON.parse(result.stdout);
    expect(out.removed).toHaveLength(12);
    expect(out.dryRun).toBe(false);
  });
  it('exits 2 for a missing build, an unknown option and a missing value', () => {
    expect(run('--build', join(root, 'nowhere')).status).toBe(2);
    expect(run('--bogus').status).toBe(2);
    expect(run('--build').status).toBe(2);
  });
});

describe('the audit of the self-hosted image', () => {
  it('lets the hosted build keep the packages, as listed', async () => {
    const scan = await scanBuild(makeBuild());
    expect(violations(scan, known)).toEqual([]);
  });
  it('fails a package left in the image', async () => {
    const scan = await scanBuild(makeBuild());
    const bad = violations(scan, knownFor(known, 'selfhosted-image'));
    expect(bad.map((row: { file: string }) => row.file).sort()).toEqual(
      HOSTED_ADDIN_FILES.map((name: string) => `client/${name}`).sort()
    );
  });
  it('passes the image after the prune step, and counts fewer hosted-domain files', async () => {
    const build = makeBuild();
    const before = await scanBuild(build);
    prune(build);
    const after = await scanBuild(build);
    expect(violations(after, knownFor(known, 'selfhosted-image'))).toEqual([]);
    const hosted = (scan: typeof before) =>
      scan.hits.filter((hit: { found: Record<string, number> }) => 'hosted-domain' in hit.found)
        .length;
    expect(hosted(before) - hosted(after)).toBe(6);
  });
  it('knows the profile and refuses an unknown one', () => {
    expect(knownFor(known, undefined)).toBe(known);
    const profile = knownFor(known, 'selfhosted-image');
    expect(profile.classes['static-asset'].paths.join('|')).not.toContain('connectors/packages');
    expect(profile.classes['marketing-client']).toEqual(known.classes['marketing-client']);
    expect(() => knownFor(known, 'nonsense')).toThrow(/Unknown profile/);
  });
});
