// @vitest-environment node
// The offline builder (scripts/build-addins.mjs): golden outputs, the checksum file, that it
// builds the same bytes as the web server, and its refusals and exit codes. It runs the real
// command, as dynodoctl will.
import { spawnSync } from 'node:child_process';
import { createHash } from 'node:crypto';
import { afterAll, describe, expect, it } from 'vitest';
import { mkdirSync, mkdtempSync, readFileSync, readdirSync, rmSync, writeFileSync } from 'node:fs';
import { tmpdir } from 'node:os';
import { join } from 'node:path';
import { unzipSync } from 'fflate';
import { buildAddinBundle } from './bundle.js';

const script = join(process.cwd(), 'scripts', 'build-addins.mjs');
const golden = join(process.cwd(), 'src', 'lib', 'addins', 'golden');
const root = mkdtempSync(join(tmpdir(), 'dynodoc-build-addins-'));
afterAll(() => rmSync(root, { recursive: true, force: true }));
let counter = 0;
const fresh = () => join(root, `out-${counter++}`);

const ARGS = [
  '--origin',
  'https://dynodoc.corp.example',
  '--brand',
  'Acme Docs',
  '--app-version',
  '1.0.0',
  '--support-url',
  'https://help.corp.example/desk',
  '--provider',
  'Acme Research Ltd.'
];
function build(args: string[], out = fresh()) {
  const result = spawnSync(process.execPath, [script, ...args, '--out', out], { encoding: 'utf8' });
  return { ...result, out };
}
const sha = (bytes: Uint8Array) => createHash('sha256').update(bytes).digest('hex');

describe('a build for an https origin', () => {
  const run = build([...ARGS, '--json']);
  const result = JSON.parse(run.stdout);

  it('exits 0 and prints one JSON document with the files and their ids', () => {
    expect(run.status).toBe(0);
    expect(run.stderr).toBe('');
    expect(result).toMatchObject({
      ok: true,
      origin: 'https://dynodoc.corp.example',
      brand: 'Acme Docs',
      appVersion: '1.0.0',
      manifestVersion: '1.1.0.0',
      supportUrl: 'https://help.corp.example/desk',
      google: { available: true }
    });
    expect(result.files.map((f: { name: string }) => f.name)).toEqual([
      'word.xml',
      'word-catalog.xml',
      'excel.xml',
      'excel-catalog.xml',
      'powerpoint.xml',
      'powerpoint-catalog.xml',
      'google-docs.zip',
      'google-sheets.zip',
      'google-slides.zip',
      'SHA256SUMS'
    ]);
    const office = result.files.filter((f: { group: string }) => f.group === 'office');
    expect(office.every((f: { id: string }) => /^[0-9a-f-]{36}$/.test(f.id))).toBe(true);
    expect(new Set(office.map((f: { id: string }) => f.id)).size).toBe(6);
  });
  it('writes exactly those files, and nothing else', () => {
    expect(readdirSync(run.out).sort()).toEqual(
      result.files.map((f: { name: string }) => f.name).sort()
    );
  });
  it('writes the six manifests the golden files hold, byte for byte', () => {
    for (const name of readdirSync(golden))
      expect(readFileSync(join(run.out, name), 'utf8'), name).toBe(
        readFileSync(join(golden, name), 'utf8')
      );
    expect(readdirSync(golden)).toHaveLength(6);
  });
  it('writes a checksum file that matches every file, in the form sha256sum -c reads', () => {
    const sums = readFileSync(join(run.out, 'SHA256SUMS'), 'utf8');
    const lines = sums.trimEnd().split('\n');
    expect(lines).toHaveLength(9);
    for (const line of lines) {
      const [hash, name] = [line.slice(0, 64), line.slice(66)];
      expect(line.slice(64, 66)).toBe('  ');
      expect(sha(readFileSync(join(run.out, name))), name).toBe(hash);
    }
    for (const file of result.files)
      expect(sha(readFileSync(join(run.out, file.name))), file.name).toBe(file.sha256);
  });
  it('builds what the web server serves for the same settings', () => {
    const served = buildAddinBundle({
      origin: 'https://dynodoc.corp.example',
      brand: 'Acme Docs',
      appVersion: '1.0.0',
      supportUrl: 'https://help.corp.example/desk',
      provider: 'Acme Research Ltd.'
    });
    for (const file of served.files)
      expect(sha(readFileSync(join(run.out, file.name))), file.name).toBe(file.sha256);
  });
  it('is identical on every run', () => {
    const again = build(ARGS);
    for (const name of readdirSync(run.out))
      expect(
        readFileSync(join(again.out, name)).equals(readFileSync(join(run.out, name))),
        name
      ).toBe(true);
  });
  it('writes Google packages that name the origin and nothing hosted', () => {
    const files = unzipSync(new Uint8Array(readFileSync(join(run.out, 'google-slides.zip'))));
    expect(new TextDecoder().decode(files['Code.gs'])).toContain(
      "DYNODOC_ORIGIN='https://dynodoc.corp.example'"
    );
    for (const name of readdirSync(run.out)) {
      const bytes = readFileSync(join(run.out, name));
      expect(bytes.toString('latin1'), name).not.toMatch(/dynodoc\.online/i);
    }
  });
  it('prints a plain summary without --json', () => {
    const plain = build(ARGS);
    expect(plain.status).toBe(0);
    expect(plain.stdout).toContain('Add-in files for https://dynodoc.corp.example written to');
    expect(plain.stdout).toMatch(/[0-9a-f]{12} {2}word\.xml/);
  });
});

describe('the refusals', () => {
  const failure = (args: string[], out?: string) => {
    const run = build([...args, '--json'], out);
    return { run, body: JSON.parse(run.stdout) };
  };
  it('exit 3 for an origin that is not safe, and write nothing', () => {
    for (const origin of [
      'http://dynodoc.corp.example',
      'http://localhost:4174',
      'https://dynodoc.corp.example/',
      'https://dynodoc.corp.example/app',
      'https://admin@dynodoc.corp.example',
      'https://dynodoc.corp.example:0',
      'https://dynodoc.corp.example:443',
      'https://app.dynodoc.online',
      'https://dynodoc.online',
      'javascript:alert(1)',
      ''
    ]) {
      const out = fresh();
      const { run, body } = failure(['--origin', origin], out);
      expect(run.status, origin).toBe(3);
      expect(body).toMatchObject({ ok: false, exit: 3, code: 'invalid-input' });
      expect(body.error, origin).toMatch(/^The origin/);
      expect(() => readdirSync(out), origin).toThrow();
    }
  });
  it('exit 3 for a bad name, version, link or suffix', () => {
    for (const args of [
      ['--brand', ''],
      ['--brand', 'x'.repeat(61)],
      ['--brand', 'a\nb'],
      ['--app-version', '1.0'],
      ['--app-version', 'latest'],
      ['--display-suffix', 'x'.repeat(31)]
    ]) {
      const { run, body } = failure(['--origin', 'https://dynodoc.corp.example', ...args]);
      expect(run.status, args.join(' ')).toBe(3);
      expect(body.code).toBe('invalid-input');
    }
  });
  it('accepts a support link that is not https only by replacing it with the origin', () => {
    const run = build([
      '--origin',
      'https://dynodoc.corp.example',
      '--support-url',
      'mailto:a@b.test',
      '--json'
    ]);
    expect(run.status).toBe(0);
    expect(JSON.parse(run.stdout).supportUrl).toBe('https://dynodoc.corp.example/');
  });
  it('accepts http://localhost only with --allow-loopback-http, and then makes no Google package', () => {
    const run = build(['--origin', 'http://localhost:4174', '--allow-loopback-http', '--json']);
    expect(run.status).toBe(0);
    const out = JSON.parse(run.stdout);
    expect(out.google.available).toBe(false);
    expect(out.files.map((f: { name: string }) => f.name)).not.toContain('google-docs.zip');
    expect(
      build(['--origin', 'http://intranet.example', '--allow-loopback-http', '--json']).status
    ).toBe(3);
  });
  it('exit 2 for a wrong command line', () => {
    const run = (...args: string[]) =>
      spawnSync(process.execPath, [script, ...args], { encoding: 'utf8' });
    expect(run().status).toBe(2);
    expect(run('--origin', 'https://dynodoc.corp.example').status).toBe(2);
    expect(run('--out', fresh()).status).toBe(2);
    expect(run('--bogus').status).toBe(2);
    expect(run('--origin').status).toBe(2);
    expect(
      run('--origin', 'https://a.example', '--origin', 'https://b.example', '--out', fresh()).status
    ).toBe(2);
    const json = run('--json', '--bogus');
    expect(JSON.parse(json.stdout)).toMatchObject({ ok: false, exit: 2, code: 'usage' });
    expect(run('--bogus').stderr).toMatch(/^build-addins: Unknown option --bogus/);
  });
  it('exit 0 with --help, and prints the interface', () => {
    const help = spawnSync(process.execPath, [script, '--help'], { encoding: 'utf8' });
    expect(help.status).toBe(0);
    expect(help.stdout).toContain('--origin');
    expect(help.stdout).toContain('exit codes:');
  });
  it('exit 4 when the output folder holds files, unless --force; a file is not a folder', () => {
    const out = fresh();
    mkdirSync(out);
    writeFileSync(join(out, 'keep.txt'), 'mine');
    const refused = build(ARGS, out);
    expect(refused.status).toBe(4);
    expect(refused.stderr).toContain('already holds files');
    expect(readdirSync(out)).toEqual(['keep.txt']);
    const forced = build([...ARGS, '--force'], out);
    expect(forced.status).toBe(0);
    expect(readFileSync(join(out, 'keep.txt'), 'utf8')).toBe('mine');
    const file = join(root, 'a-file');
    writeFileSync(file, 'x');
    expect(build(ARGS, file).status).toBe(4);
  });
  it('removes Google packages left by an earlier build that this one does not make', () => {
    const out = fresh();
    expect(build(ARGS, out).status).toBe(0);
    expect(readdirSync(out)).toContain('google-docs.zip');
    const rebuilt = build(
      ['--origin', 'http://localhost:4174', '--allow-loopback-http', '--force'],
      out
    );
    expect(rebuilt.status).toBe(0);
    expect(readdirSync(out).filter((name) => name.endsWith('.zip'))).toEqual([]);
  });
});
