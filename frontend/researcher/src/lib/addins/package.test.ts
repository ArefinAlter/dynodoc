// @vitest-environment node
// The Google packages, the ZIP writer and the whole bundle of files an installation
// distributes. The hosted packages are the reference: the generator, given the hosted origin,
// must rebuild every file of them except the installation guide.
import { readFileSync } from 'node:fs';
import { resolve } from 'node:path';
import { strFromU8, unzipSync } from 'fflate';
import { describe, expect, it } from 'vitest';
import { CHECKSUMS_NAME, buildAddinBundle, checksumText, sha256 } from './bundle.js';
import {
  GOOGLE_HOSTS,
  googleUnavailableReason,
  renderGooglePackage,
  renderGooglePackageFiles
} from './google.js';
import templates from './google-templates.generated.js';
import { crc32, zipStored } from './zip.js';

const HOSTED = 'https://app.dynodoc.online';
const ORIGIN = 'https://dynodoc.corp.example';
const staticPackages = resolve(process.cwd(), 'static/connectors/packages');

describe('the ZIP writer', () => {
  it('computes the standard CRC-32', () => {
    expect(crc32(new TextEncoder().encode('123456789'))).toBe(0xcbf43926);
    expect(crc32(new Uint8Array())).toBe(0);
  });
  it('writes an archive a standard reader opens, with the same names and bytes', () => {
    const bytes = zipStored([
      { name: 'a.txt', data: 'alpha' },
      { name: 'dir/b.bin', data: new Uint8Array([0, 1, 2, 255]) },
      { name: 'ü.txt', data: 'umlaut' }
    ]);
    const back = unzipSync(bytes);
    expect(Object.keys(back)).toEqual(['a.txt', 'dir/b.bin', 'ü.txt']);
    expect(strFromU8(back['a.txt'])).toBe('alpha');
    expect(Array.from(back['dir/b.bin'])).toEqual([0, 1, 2, 255]);
  });
  it('gives the same bytes every time, with the 1980 timestamp the hosted packages use', () => {
    const entries = [{ name: 'a.txt', data: 'alpha' }];
    expect(Buffer.from(zipStored(entries)).equals(Buffer.from(zipStored(entries)))).toBe(true);
    const view = new DataView(zipStored(entries).buffer);
    expect(view.getUint16(10, true)).toBe(0);
    expect(view.getUint16(12, true)).toBe(0x21);
  });
  it('refuses names that could write outside the folder it is unpacked into', () => {
    for (const name of [
      '',
      '/etc/passwd',
      '../x',
      'a/../x',
      'a\\b',
      './x',
      'a//b',
      'x/',
      'a'.repeat(300)
    ])
      expect(() => zipStored([{ name, data: 'x' }]), name).toThrow(/safe file name/);
    expect(() =>
      zipStored([
        { name: 'a', data: '1' },
        { name: 'a', data: '2' }
      ])
    ).toThrow(/twice/);
    expect(() =>
      zipStored(Array.from({ length: 101 }, (_, i) => ({ name: `f${i}`, data: '' })))
    ).toThrow(/Too many/);
  });
});

describe('the Google templates', () => {
  it('hold no trace of the hosted service, so no file an installation loads names it', () => {
    const texts = [
      ...Object.values(templates.common),
      ...Object.values(templates.hosts).flatMap((files) => Object.values(files))
    ];
    for (const text of texts) expect(text).not.toMatch(/dynodoc\.online/i);
    expect(templates.placeholder).toBe('{{DYNODOC_ORIGIN}}');
    // The placeholder is where the origin was: the code constant and the allow-list.
    for (const host of GOOGLE_HOSTS) {
      expect(templates.hosts[host]['Code.gs']).toMatch(
        /DYNODOC_ORIGIN ?= ?'\{\{DYNODOC_ORIGIN\}\}'/
      );
      expect(templates.hosts[host]['appsscript.json']).toContain(
        '"{{DYNODOC_ORIGIN}}/api/connectors/"'
      );
    }
  });
});

describe('a Google package', () => {
  it.each(GOOGLE_HOSTS)(
    '%s rebuilds the hosted package, file for file, from the hosted origin',
    (host) => {
      const hosted = unzipSync(
        new Uint8Array(readFileSync(resolve(staticPackages, `${host}.zip`)))
      );
      const made = renderGooglePackageFiles({ origin: HOSTED, brand: 'Dynodoc', host });
      // Every file of the hosted package is rebuilt exactly; only the guide is written afresh.
      expect(made.map((f) => f.name).sort()).toEqual(Object.keys(hosted).sort());
      for (const file of made.filter((f) => f.name !== 'INSTALL.md'))
        expect(file.data, file.name).toBe(strFromU8(hosted[file.name]));
    }
  );
  it.each(GOOGLE_HOSTS)('%s names the installation and nothing else', (host) => {
    const files = renderGooglePackageFiles({
      origin: ORIGIN,
      brand: 'Acme Docs',
      host,
      forbidDomain: 'dynodoc.online'
    });
    const by = Object.fromEntries(files.map((f) => [f.name, f.data]));
    expect(by['Code.gs']).toMatch(new RegExp(`DYNODOC_ORIGIN ?= ?'${ORIGIN}'`));
    const manifest = JSON.parse(by['appsscript.json']);
    expect(manifest.urlFetchWhitelist).toEqual([`${ORIGIN}/api/connectors/`]);
    expect(manifest.oauthScopes).toHaveLength(3);
    expect(manifest.exceptionLogging).toBe('STACKDRIVER');
    expect(manifest.runtimeVersion).toBe('V8');
    for (const file of files) {
      expect(file.data, file.name).not.toMatch(/dynodoc\.online/i);
      expect(file.data, file.name).not.toContain('{{');
    }
    expect(by['INSTALL.md']).toContain(ORIGIN);
    expect(by['INSTALL.md']).toContain('Acme Docs');
    expect(by['INSTALL.md']).toContain('outside your installation');
    expect(Object.keys(by)).toContain(host === 'google-sheets' ? 'Date.gs' : 'Code.gs');
  });
  it('is the same bytes every time', () => {
    const one = renderGooglePackage({ origin: ORIGIN, brand: 'Acme Docs', host: 'google-docs' });
    const two = renderGooglePackage({ origin: ORIGIN, brand: 'Acme Docs', host: 'google-docs' });
    expect(Buffer.from(one).equals(Buffer.from(two))).toBe(true);
    expect(Object.keys(unzipSync(one))).toContain('INSTALL.md');
  });
  it('is refused for an origin Google cannot call or that is not canonical', () => {
    expect(googleUnavailableReason('http://localhost:4174')).toMatch(/https/);
    expect(googleUnavailableReason(ORIGIN)).toBeUndefined();
    for (const origin of [
      'http://localhost:4174',
      `${ORIGIN}/`,
      `https://admin@dynodoc.corp.example`,
      'https://dynodoc.corp.example/x',
      'javascript:1'
    ])
      expect(
        () => renderGooglePackageFiles({ origin, brand: 'Acme', host: 'google-docs' }),
        origin
      ).toThrow();
    expect(() =>
      renderGooglePackageFiles({
        origin: HOSTED,
        brand: 'Acme',
        host: 'google-docs',
        forbidDomain: 'dynodoc.online'
      })
    ).toThrow(/hosted service/);
    expect(() =>
      renderGooglePackageFiles({ origin: ORIGIN, brand: 'Acme', host: 'google-forms' as never })
    ).toThrow(/Unknown/);
  });
  it('fails loudly if the templates lose the origin or keep a placeholder', () => {
    const broken = structuredClone(templates);
    broken.hosts['google-docs']['Code.gs'] = broken.hosts['google-docs']['Code.gs'].replace(
      "'{{DYNODOC_ORIGIN}}'",
      "'https://elsewhere.example'"
    );
    expect(() =>
      renderGooglePackageFiles({
        origin: ORIGIN,
        brand: 'A',
        host: 'google-docs',
        templates: broken
      })
    ).toThrow(/does not carry the origin/);
    const leftover = structuredClone(templates);
    leftover.hosts['google-docs']['Sidebar.html'] += '\n{{DYNODOC_BRAND}}';
    expect(() =>
      renderGooglePackageFiles({
        origin: ORIGIN,
        brand: 'A',
        host: 'google-docs',
        templates: leftover
      })
    ).toThrow(/placeholder/);
    const open = structuredClone(templates);
    open.hosts['google-docs']['appsscript.json'] = open.hosts['google-docs'][
      'appsscript.json'
    ].replace('"{{DYNODOC_ORIGIN}}/api/connectors/"', '"{{DYNODOC_ORIGIN}}/"');
    expect(() =>
      renderGooglePackageFiles({ origin: ORIGIN, brand: 'A', host: 'google-docs', templates: open })
    ).toThrow(/exactly the connector API/);
  });
});

describe('the bundle of files', () => {
  const input = {
    origin: ORIGIN,
    brand: 'Acme Docs',
    appVersion: '0.1.0',
    forbidDomain: 'dynodoc.online'
  };
  it('holds the six Office manifests, the three Google packages and the checksum list', () => {
    const bundle = buildAddinBundle(input);
    expect(bundle.files.map((f) => f.name)).toEqual([
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
    expect(bundle.manifestVersion).toBe('1.0.1.0');
    expect(bundle.google).toEqual({ available: true });
    expect(bundle.supportUrl).toBe(`${ORIGIN}/`);
    const ids = bundle.files.filter((f) => f.id).map((f) => f.id);
    expect(new Set(ids).size).toBe(6);
  });
  it('lists the checksum of every file but itself, in a form sha256sum reads', () => {
    const bundle = buildAddinBundle(input);
    const sums = bundle.files.find((f) => f.name === CHECKSUMS_NAME)!;
    const text = new TextDecoder().decode(sums.bytes);
    const lines = text.trimEnd().split('\n');
    expect(lines).toHaveLength(9);
    for (const line of lines) expect(line).toMatch(/^[0-9a-f]{64} {2}[A-Za-z0-9.-]+$/);
    const others = bundle.files.filter((f) => f.name !== CHECKSUMS_NAME);
    for (const file of others) {
      expect(file.sha256).toBe(sha256(file.bytes));
      expect(lines).toContain(`${file.sha256}  ${file.name}`);
    }
    expect(text).toBe(checksumText(others));
    const names = lines.map((line) => line.slice(66));
    expect(names).toEqual([...names].sort());
  });
  it('is identical on every run, including the add-in ids', () => {
    const one = buildAddinBundle(input);
    const two = buildAddinBundle({ ...input });
    expect(one.files.map((f) => f.sha256)).toEqual(two.files.map((f) => f.sha256));
  });
  it('changes when and only when the inputs that belong in the files change', () => {
    const base = buildAddinBundle(input);
    const hashes = (b: ReturnType<typeof buildAddinBundle>) => b.files.map((f) => f.sha256);
    expect(hashes(buildAddinBundle({ ...input, appVersion: '0.2.0' }))).not.toEqual(hashes(base));
    expect(hashes(buildAddinBundle({ ...input, brand: 'Other' }))).not.toEqual(hashes(base));
    const moved = buildAddinBundle({ ...input, origin: 'https://other.corp.example' });
    for (const file of moved.files) {
      const before = base.files.find((f) => f.name === file.name)!;
      expect(file.sha256, file.name).not.toBe(before.sha256);
    }
  });
  it('makes no Google package for a plain-http test stack, and says why', () => {
    const bundle = buildAddinBundle({
      ...input,
      origin: 'http://localhost:4174',
      allowLoopbackHttp: true
    });
    expect(bundle.files.map((f) => f.name)).toEqual([
      'word.xml',
      'word-catalog.xml',
      'excel.xml',
      'excel-catalog.xml',
      'powerpoint.xml',
      'powerpoint-catalog.xml',
      'SHA256SUMS'
    ]);
    expect(bundle.google.available).toBe(false);
    expect(bundle.google.reason).toMatch(/https/);
  });
  it('refuses an unsafe origin or the hosted service', () => {
    for (const origin of [
      'http://localhost:4174',
      'https://x.example/',
      HOSTED,
      'https://u@x.example'
    ])
      expect(() => buildAddinBundle({ ...input, origin }), origin).toThrow(/^The origin/);
  });
});
