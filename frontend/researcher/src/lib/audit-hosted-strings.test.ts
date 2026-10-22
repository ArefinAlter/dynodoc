// The hosted-string audit (scripts/audit-hosted-strings.mjs), on a small build made here: it
// must tell a hosted string in code every page loads from one in the marketing pages, and
// must fail on anything the known list does not allow.
import { afterAll, beforeAll, describe, expect, it } from 'vitest';
import { mkdirSync, mkdtempSync, readFileSync, rmSync, writeFileSync } from 'node:fs';
import { tmpdir } from 'node:os';
import { dirname, join } from 'node:path';
import { strToU8, zipSync } from 'fflate';
// @ts-expect-error the audit is a plain script
import {
  PATTERNS,
  closure,
  importsOf,
  scanBuild,
  strictHits,
  violations
} from '../../scripts/audit-hosted-strings.mjs';

const known = JSON.parse(
  readFileSync(join(process.cwd(), 'scripts', 'hosted-strings-known.json'), 'utf8')
);
const root = mkdtempSync(join(tmpdir(), 'dynodoc-audit-'));
afterAll(() => rmSync(root, { recursive: true, force: true }));

function put(build: string, path: string, content: string | Uint8Array) {
  const full = join(build, path);
  mkdirSync(dirname(full), { recursive: true });
  writeFileSync(full, content);
}
const IMM = 'client/_app/immutable';

/** A build with a root layout (0), an error page (1), a marketing page (2) and the login page (3). */
function makeBuild(name: string, extra: (build: string) => void = () => {}) {
  const build = join(root, name);
  put(build, 'package.json', '{"type":"module"}');
  put(build, `${IMM}/entry/start.a1.js`, 'import"../chunks/shared.js";');
  put(
    build,
    `${IMM}/entry/app.a1.js`,
    'export const nodes=[()=>import("../nodes/0.n.js"),()=>import("../nodes/1.n.js"),()=>import("../nodes/2.n.js"),()=>import("../nodes/3.n.js")];'
  );
  put(build, `${IMM}/nodes/0.n.js`, 'import"../chunks/shared.js";');
  put(build, `${IMM}/nodes/1.n.js`, 'import"../chunks/shared.js";');
  put(build, `${IMM}/nodes/2.n.js`, 'import{a}from"../chunks/privacy.js";export{a};');
  put(
    build,
    `${IMM}/nodes/3.n.js`,
    'import{s}from"../chunks/shared.js";import("../chunks/lazy.js");export{s};'
  );
  put(build, `${IMM}/chunks/shared.js`, 'export const s="nothing hosted here";');
  put(build, `${IMM}/chunks/lazy.js`, 'export const l=1;');
  put(
    build,
    `${IMM}/chunks/privacy.js`,
    'export const a="Run by Verne Systems, Dhaka. See https://dynodoc.online/privacy";'
  );
  put(build, 'server/index.js', 'import"./chunks/hooks.js";');
  put(build, 'server/chunks/hooks.js', 'export const h="clean";');
  for (const [i, file] of ['0-a', '1-b', '2-c', '3-d'].entries())
    put(
      build,
      `server/chunks/${file}.js`,
      i === 2 ? 'import"./privacy-page.js";' : 'import"./hooks.js";'
    );
  put(build, 'server/chunks/privacy-page.js', 'export const p="mail privacy@dynodoc.online";');
  put(build, 'server/chunks/_server.ts-r.js', 'export const r="clean robots";');
  put(
    build,
    'server/manifest.js',
    `const __memo = (fn) => fn;
const manifest = { _: { client: {}, nodes: [
  __memo(() => import('./chunks/0-a.js')),
  __memo(() => import('./chunks/1-b.js')),
  __memo(() => import('./chunks/2-c.js')),
  __memo(() => import('./chunks/3-d.js'))
],
remotes: {},
routes: [
  { id: "/(site)/privacy", page: { layouts: [0], errors: [1], leaf: 2 }, endpoint: null },
  { id: "/login", page: { layouts: [0], errors: [1], leaf: 3 }, endpoint: null },
  { id: "/robots.txt", page: null, endpoint: __memo(() => import('./chunks/_server.ts-r.js')) }
] } };
export { manifest };
`
  );
  extra(build);
  return build;
}
const classes = (scan: { hits: { file: string; class: string }[] }) =>
  Object.fromEntries(scan.hits.map((hit) => [hit.file, hit.class]));

describe('the audit', () => {
  let scan: Awaited<ReturnType<typeof scanBuild>>;
  beforeAll(async () => {
    const build = makeBuild('basic', (b) => {
      put(b, 'client/fonts/geist-OFL.txt', 'Copyright dynodoc.online type foundry');
      put(b, 'client/.well-known/security.txt', 'Contact: mailto:info@vernesystems.com');
      put(b, 'client/connectors/packages/word.xml', '<Id>https://app.dynodoc.online/x</Id>');
      put(
        b,
        'client/connectors/packages/google-docs.zip',
        zipSync({ 'Code.gs': strToU8("const ORIGIN = 'https://app.dynodoc.online';") })
      );
      put(
        b,
        'client/connectors/commands.html',
        '<script src="https://appsforoffice.microsoft.com/x.js">'
      );
      put(b, 'server/chunks/hooks.js.map', '{"sourcesContent":["https://dynodoc.online"]}');
      put(b, 'client/_app/immutable/chunks/shared.js.br', 'dynodoc.online compressed copy');
    });
    scan = await scanBuild(build);
  });

  it('tells the marketing pages from code every page loads', () => {
    const where = classes(scan);
    expect(where[`${IMM}/chunks/privacy.js`]).toBe('marketing-client');
    expect(where['server/chunks/privacy-page.js']).toBe('marketing-server');
    expect(where[`${IMM}/chunks/shared.js`]).toBeUndefined();
    expect(scan.routeTable).toBe(true);
  });
  it('classifies static files, archives, notices and source maps; skips compressed copies', () => {
    const where = classes(scan);
    expect(where['client/fonts/geist-OFL.txt']).toBe('notice');
    expect(where['client/.well-known/security.txt']).toBe('static-asset');
    expect(where['client/connectors/packages/word.xml']).toBe('static-asset');
    expect(where['client/connectors/packages/google-docs.zip']).toBe('static-asset');
    expect(where['server/chunks/hooks.js.map']).toBe('sourcemap');
    expect(Object.keys(where).some((file) => file.endsWith('.br'))).toBe(false);
  });
  it('counts each pattern', () => {
    const privacy = scan.hits.find((h: { file: string }) => h.file === `${IMM}/chunks/privacy.js`);
    expect(privacy.found).toMatchObject({ 'hosted-domain': 1, operator: 2 });
    expect(PATTERNS.map((p: { id: string }) => p.id)).toEqual(
      expect.arrayContaining([
        'hosted-domain',
        'operator',
        'mail-service',
        'analytics',
        'office-cdn'
      ])
    );
  });
  it('fails the criterion as written and passes the known list', () => {
    // dynodoc.online outside third-party notices: strict fails.
    expect(strictHits(scan).length).toBeGreaterThan(0);
    expect(strictHits(scan).some((h: { class: string }) => h.class === 'notice')).toBe(false);
    // The known list allows exactly these classes of file.
    expect(violations(scan, known)).toEqual([]);
  });
  it('lets the Office.js CDN through as a documented exception', () => {
    const commands = scan.hits.find(
      (h: { file: string }) => h.file === 'client/connectors/commands.html'
    );
    expect(commands.found).toEqual({ 'office-cdn': 1 });
    expect(violations(scan, known).some((v: { file: string }) => v.file.includes('commands'))).toBe(
      false
    );
  });

  it('flags a hosted string in code every page loads', async () => {
    const leaky = makeBuild('leak', (b) =>
      put(b, `${IMM}/chunks/shared.js`, 'export const s="https://app.dynodoc.online/telemetry";')
    );
    const result = await scanBuild(leaky);
    expect(classes(result)[`${IMM}/chunks/shared.js`]).toBe('shared-client');
    const bad = violations(result, known);
    expect(bad).toHaveLength(1);
    expect(bad[0].why).toMatch(/hosted-domain is not allowed in shared-client/);
  });
  it('flags an operator fact in shared server code beyond the listed files', async () => {
    const leaky = makeBuild('operator', (b) =>
      put(b, 'server/chunks/hooks.js', 'export const h="Verne Systems";')
    );
    const bad = violations(await scanBuild(leaky), known);
    expect(bad.map((v: { why: string }) => v.why)).toEqual([
      'operator is not allowed in shared-server'
    ]);
  });
  it('flags more shared server files than the list allows, and an unlisted static file', async () => {
    const many = makeBuild('many', (b) => {
      for (const n of ['a', 'b', 'c', 'd'])
        put(b, `server/chunks/hosted-${n}.js`, 'export const x="https://dynodoc.online";');
      put(
        b,
        'server/chunks/hooks.js',
        'import"./hosted-a.js";import"./hosted-b.js";import"./hosted-c.js";import"./hosted-d.js";'
      );
      put(b, 'client/new-page.html', 'see https://dynodoc.online');
    });
    const why = violations(await scanBuild(many), known).map((v: { why: string }) => v.why);
    expect(why).toContain('4 files, at most 3 are listed');
    expect(
      why.some((w: string) => w.includes('client/new-page.html is not a listed static-asset'))
    ).toBe(true);
  });
  it('counts analytics and other third-party hosts in the shared code as violations', async () => {
    const tracked = makeBuild('analytics', (b) =>
      put(b, `${IMM}/chunks/shared.js`, 'fetch("https://plausible.io/api/event")')
    );
    const bad = violations(await scanBuild(tracked), known);
    expect(bad).toHaveLength(1);
    expect(bad[0].why).toContain('analytics is not allowed in shared-client');
  });
  it('treats every script as shared when the build has no route table', async () => {
    const bare = join(root, 'bare');
    put(bare, 'client/_app/immutable/chunks/x.js', 'const a="https://dynodoc.online";');
    const result = await scanBuild(bare);
    expect(result.routeTable).toBe(false);
    expect(result.hits[0].class).toBe('shared-client');
  });
});

describe('the import graph', () => {
  it('reads minified static, side-effect and dynamic imports', () => {
    const found = importsOf(
      'client/_app/immutable/nodes/3.n.js',
      'import{a as b}from"../chunks/x.js";import"../chunks/y.js";const z=()=>import("../chunks/z.js");import w from\'../chunks/w.js\';'
    );
    expect([...found].sort()).toEqual([
      'client/_app/immutable/chunks/w.js',
      'client/_app/immutable/chunks/x.js',
      'client/_app/immutable/chunks/y.js',
      'client/_app/immutable/chunks/z.js'
    ]);
  });
  it('follows edges but not through a blocked file', () => {
    const graph = new Map([
      ['a', new Set(['b'])],
      ['b', new Set(['c'])],
      ['c', new Set<string>()],
      ['x', new Set(['c'])]
    ]);
    expect([...closure(['a'], graph)].sort()).toEqual(['a', 'b', 'c']);
    expect([...closure(['a'], graph, new Set(['b']))]).toEqual(['a']);
  });
});
