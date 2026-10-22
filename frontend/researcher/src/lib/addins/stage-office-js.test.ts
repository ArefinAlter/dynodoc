// @vitest-environment node
// scripts/stage-office-js.mjs: the operator's copy of Office.js goes into the served folder, and
// only the files the server would hand out. Nothing of Office.js is in this repository, so the
// source here is a stand-in made for the test.
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
import { join } from 'node:path';
import { resolveOfficeJsFile } from '../server/office-js';

const script = join(process.cwd(), 'scripts', 'stage-office-js.mjs');
const root = mkdtempSync(join(tmpdir(), 'dynodoc-stage-'));
afterAll(() => rmSync(root, { recursive: true, force: true }));
let counter = 0;
const folder = () => join(root, `f-${counter++}`);
const run = (...args: string[]) =>
  spawnSync(process.execPath, [script, ...args], { encoding: 'utf8' });

function source(extra: Record<string, string> = {}) {
  const dir = folder();
  const files: Record<string, string> = {
    'office.js': '/* stand-in */',
    'excel-15.debug.js': '/* excel */',
    'en-us/office_strings.js': 'var s;',
    'office.d.ts': 'declare namespace Office {}',
    'README.md': 'notes',
    'start.html': '<script>x</script>',
    '.hidden.js': 'x',
    ...extra
  };
  for (const [name, text] of Object.entries(files)) {
    const path = join(dir, ...name.split('/'));
    mkdirSync(join(path, '..'), { recursive: true });
    writeFileSync(path, text);
  }
  return dir;
}

describe('staging a copy of Office.js', () => {
  it('copies the files the server serves, skips the rest and says which', () => {
    const from = source();
    const to = folder();
    const result = run('--from', from, '--to', to, '--json');
    expect(result.status).toBe(0);
    const out = JSON.parse(result.stdout);
    expect(out.copied.map((f: { name: string }) => f.name).sort()).toEqual([
      'en-us/office_strings.js',
      'excel-15.debug.js',
      'office.js'
    ]);
    expect(
      out.skipped.sort((a: { name: string }, b: { name: string }) => (a.name < b.name ? -1 : 1))
    ).toEqual([
      { name: '.hidden.js', reason: 'hidden' },
      { name: 'README.md', reason: 'type not served' },
      { name: 'office.d.ts', reason: 'type not served' },
      { name: 'start.html', reason: 'type not served' }
    ]);
    expect(out.note).toMatch(/does not ship Office\.js/);
    expect(readFileSync(join(to, 'en-us', 'office_strings.js'), 'utf8')).toBe('var s;');
    expect(existsSync(join(to, 'start.html'))).toBe(false);
  });
  it('produces a folder the server accepts and serves', async () => {
    const to = folder();
    expect(run('--from', source(), '--to', to).status).toBe(0);
    expect(await resolveOfficeJsFile(to, 'office.js')).not.toBeNull();
    expect(await resolveOfficeJsFile(to, 'en-us/office_strings.js')).not.toBeNull();
  });
  it('exits 3 for a source without office.js, a missing source, and nested folders', () => {
    const none = folder();
    mkdirSync(none);
    writeFileSync(join(none, 'other.js'), 'x');
    expect(run('--from', none, '--to', folder()).status).toBe(3);
    expect(run('--from', join(root, 'nowhere'), '--to', folder()).status).toBe(3);
    const from = source();
    expect(run('--from', from, '--to', join(from, 'inside')).status).toBe(3);
    const empty = join(root, 'parent');
    mkdirSync(empty);
    expect(run('--from', join(from, '..'), '--to', from).status).toBe(3);
  });
  it('exits 4 when the target holds files, unless --force, and leaves them alone', () => {
    const to = folder();
    mkdirSync(to);
    writeFileSync(join(to, 'mine.txt'), 'mine');
    const from = source();
    const refused = run('--from', from, '--to', to);
    expect(refused.status).toBe(4);
    expect(refused.stderr).toMatch(/already holds files/);
    expect(readdirSync(to)).toEqual(['mine.txt']);
    expect(run('--from', from, '--to', to, '--force').status).toBe(0);
    expect(readFileSync(join(to, 'mine.txt'), 'utf8')).toBe('mine');
  });
  it('exits 2 for a wrong command line and 0 for --help', () => {
    expect(run().status).toBe(2);
    expect(run('--from', folder()).status).toBe(2);
    expect(run('--bogus').status).toBe(2);
    expect(run('--from').status).toBe(2);
    const help = run('--help');
    expect(help.status).toBe(0);
    expect(help.stdout).toContain('exit codes:');
  });
});
