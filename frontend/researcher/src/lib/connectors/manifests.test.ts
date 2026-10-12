// @vitest-environment node
// Offline invariants for the six editor add-ins. These read extensions/* and
// static/connectors/* straight from the repository: no network, no host.
// They do not replace `pnpm validate:manifests` (Microsoft's online schema check)
// or the real-host acceptance in extensions/ACCEPTANCE.md.
import { readFileSync, existsSync } from 'node:fs';
import { resolve } from 'node:path';
import {
  DOMParser,
  type Document as XmlDocument,
  type Element as XmlElement
} from '@xmldom/xmldom';
import { describe, expect, it } from 'vitest';

const root = resolve(process.cwd(), '../..');
const extensions = resolve(root, 'extensions');
const staticDir = resolve(process.cwd(), 'static');
const routes = resolve(process.cwd(), 'src/routes');

const APP_ORIGIN = 'https://app.dynodoc.online';
const SUPPORT_URL = 'https://dynodoc.online/support';
const GUID = /^[0-9a-f]{8}-[0-9a-f]{4}-[0-9a-f]{4}-[0-9a-f]{4}-[0-9a-f]{12}$/;
const UNFINISHED = /\b(development|dev|test|testing|prototype|beta|sandbox|todo|lorem)\b/i;

const office = [
  {
    host: 'word',
    id: 'adbda646-1a13-4a70-baf5-c772e9cc90a6',
    name: 'Dynodoc for Word',
    manifestHost: 'Document',
    requirement: ['WordApi', '1.3']
  },
  {
    host: 'excel',
    id: '794ef9dc-6d78-4b08-a7f3-2526d0476930',
    name: 'Dynodoc for Excel',
    manifestHost: 'Workbook',
    requirement: ['ExcelApi', '1.13']
  },
  {
    host: 'powerpoint',
    id: '28155d15-5a0e-4b94-b0ce-c448d684d125',
    name: 'Dynodoc for PowerPoint',
    manifestHost: 'Presentation',
    requirement: ['PowerPointApi', '1.4']
  }
] as const;

const google = [
  {
    host: 'google-docs',
    scope: 'https://www.googleapis.com/auth/documents.currentonly',
    menu: 'DocumentApp'
  },
  {
    host: 'google-sheets',
    scope: 'https://www.googleapis.com/auth/spreadsheets.currentonly',
    menu: 'SpreadsheetApp'
  },
  {
    host: 'google-slides',
    scope: 'https://www.googleapis.com/auth/presentations.currentonly',
    menu: 'SlidesApp'
  }
] as const;

function read(path: string) {
  return readFileSync(path, 'utf8');
}
function parse(xml: string) {
  const problems: string[] = [];
  const document = new DOMParser({
    onError: (level, message) => problems.push(`${level}: ${message}`)
  }).parseFromString(xml, 'text/xml');
  return { document, problems };
}
function all(node: XmlElement | XmlDocument, tag: string): XmlElement[] {
  return Array.from(node.getElementsByTagName(tag)) as XmlElement[];
}
function defaultValue(document: XmlDocument, tag: string) {
  const element = all(document, tag)[0];
  if (!element) throw new Error(`Missing <${tag}>`);
  return element.getAttribute('DefaultValue') ?? '';
}
function pngSize(path: string) {
  const bytes = readFileSync(path);
  expect(bytes.subarray(0, 8).toString('hex')).toBe('89504e470d0a1a0a');
  expect(bytes.subarray(12, 16).toString('ascii')).toBe('IHDR');
  return { width: bytes.readUInt32BE(16), height: bytes.readUInt32BE(20) };
}
/** Maps a manifest URL on the app origin to the file the web build serves for it. */
function servedFile(url: string) {
  const { pathname } = new URL(url);
  return resolve(staticDir, '.' + pathname);
}

describe('Office manifests', () => {
  const manifests = office.map((app) => {
    const text = read(resolve(extensions, app.host, 'manifest.xml'));
    return { ...app, text, ...parse(text) };
  });

  it('are well-formed XML task-pane add-ins', () => {
    for (const m of manifests) {
      expect(m.problems, m.host).toEqual([]);
      const root = m.document.documentElement!;
      expect(root.tagName).toBe('OfficeApp');
      expect(root.getAttribute('xsi:type')).toBe('TaskPaneApp');
    }
  });

  it('keep stable, unique, well-formed GUIDs and a four-part version', () => {
    const ids = manifests.map((m) => all(m.document, 'Id')[0].textContent!.trim());
    for (const [index, id] of ids.entries()) {
      expect(id).toMatch(GUID);
      // Changing an Id forks the listing and orphans installs; update this pin deliberately.
      expect(id).toBe(manifests[index].id);
    }
    expect(new Set(ids).size).toBe(ids.length);
    for (const m of manifests) {
      const version = all(m.document, 'Version')[0].textContent!.trim();
      expect(version).toMatch(/^\d+\.\d+\.\d+\.\d+$/);
      expect(version.split('.').every((part) => Number(part) <= 65535)).toBe(true);
      expect(all(m.document, 'ProviderName')[0].textContent).toBe('Dynodoc');
      expect(all(m.document, 'DefaultLocale')[0].textContent).toBe('en-US');
    }
  });

  it('use unique production names and plain-language descriptions within store limits', () => {
    const names = manifests.map((m) => defaultValue(m.document, 'DisplayName'));
    expect(names).toEqual(manifests.map((m) => m.name));
    expect(new Set(names).size).toBe(names.length);
    for (const m of manifests) {
      const name = defaultValue(m.document, 'DisplayName');
      const description = defaultValue(m.document, 'Description');
      expect(name.length).toBeGreaterThan(0);
      expect(name.length).toBeLessThanOrEqual(125);
      expect(description.length).toBeGreaterThan(40);
      expect(description.length).toBeLessThanOrEqual(250);
      expect(description).not.toMatch(/\bblocks?\b/i);
      expect(description).not.toMatch(/provenance|ULID|checkpoint|hash/i);
    }
    expect(new Set(manifests.map((m) => defaultValue(m.document, 'Description'))).size).toBe(3);
  });

  it('show no development or test wording in any user-visible string', () => {
    for (const m of manifests) {
      const strings = all(m.document, 'bt:String')
        .map((e) => e.getAttribute('DefaultValue') ?? '')
        .concat(defaultValue(m.document, 'DisplayName'), defaultValue(m.document, 'Description'));
      for (const value of strings) {
        expect(value, `${m.host}: ${value}`).not.toMatch(UNFINISHED);
        expect(value).not.toMatch(/open provenance/i);
      }
      expect(m.text).not.toMatch(/\(development\)/i);
    }
  });

  it('use HTTPS on the single app origin, with support on the marketing domain', () => {
    for (const m of manifests) {
      const urls = [
        ...all(m.document, 'IconUrl'),
        ...all(m.document, 'HighResolutionIconUrl'),
        ...all(m.document, 'SourceLocation'),
        ...all(m.document, 'bt:Url'),
        ...all(m.document, 'bt:Image')
      ]
        .filter((e) => e.hasAttribute('DefaultValue'))
        .map((e) => e.getAttribute('DefaultValue')!);
      expect(urls.length).toBeGreaterThanOrEqual(8);
      for (const url of urls) {
        const parsed = new URL(url);
        expect(parsed.protocol, url).toBe('https:');
        expect(parsed.origin, url).toBe(APP_ORIGIN);
        expect(parsed.username + parsed.password + parsed.search + parsed.hash).toBe('');
      }
      const domains = all(m.document, 'AppDomain').map((e) => e.textContent!.trim());
      expect(domains).toEqual([APP_ORIGIN]);
      expect(defaultValue(m.document, 'SupportUrl')).toBe(SUPPORT_URL);
    }
  });

  it('serve every referenced icon, page and command file from this checkout', () => {
    for (const m of manifests) {
      const sizes = new Map<string, number>([
        [defaultValue(m.document, 'IconUrl'), 32],
        [defaultValue(m.document, 'HighResolutionIconUrl'), 64]
      ]);
      const resources = new Map(
        all(m.document, 'bt:Image')
          .filter((e) => e.hasAttribute('DefaultValue'))
          .map((e) => [e.getAttribute('id')!, e.getAttribute('DefaultValue')!])
      );
      for (const image of all(m.document, 'bt:Image').filter((e) => e.hasAttribute('size'))) {
        const url = resources.get(image.getAttribute('resid')!);
        expect(url, image.getAttribute('resid')!).toBeTruthy();
        sizes.set(url!, Number(image.getAttribute('size')));
      }
      for (const [url, size] of sizes) {
        const file = servedFile(url);
        expect(existsSync(file), url).toBe(true);
        expect(pngSize(file), url).toEqual({ width: size, height: size });
      }
      const ribbon = [...sizes.values()];
      for (const required of [16, 32, 64, 80]) expect(ribbon).toContain(required);
      const pane = new URL(all(m.document, 'SourceLocation')[0].getAttribute('DefaultValue')!);
      expect(pane.pathname).toBe(`/connectors/${m.host}`);
      expect(existsSync(resolve(routes, `connectors/${m.host}/+page.svelte`))).toBe(true);
      expect(existsSync(servedFile(`${APP_ORIGIN}/connectors/commands.html`))).toBe(true);
    }
  });

  it('request only ReadWriteDocument and the host API set the panes check at runtime', () => {
    for (const m of manifests) {
      expect(all(m.document, 'Permissions')[0].textContent).toBe('ReadWriteDocument');
      expect(all(m.document, 'Hosts')[0].getElementsByTagName('Host')[0].getAttribute('Name')).toBe(
        m.manifestHost
      );
      const sets = all(m.document, 'Set');
      expect(sets).toHaveLength(1);
      expect(sets[0].getAttribute('Name')).toBe(m.requirement[0]);
      expect(all(m.document, 'Sets')[0].getAttribute('DefaultMinVersion')).toBe(m.requirement[1]);
      const pane = read(
        m.host === 'word'
          ? resolve(routes, 'connectors/word/+page.svelte')
          : resolve(process.cwd(), 'src/lib/components/product/OfficeStructuredPane.svelte')
      );
      expect(pane).toContain(`'${m.requirement[1]}'`);
    }
  });

  it('label the ribbon command with the product name', () => {
    for (const m of manifests) {
      const labels = Object.fromEntries(
        all(m.document, 'bt:String').map((e) => [
          e.getAttribute('id'),
          e.getAttribute('DefaultValue')
        ])
      );
      expect(labels['Group.Label']).toBe('Dynodoc');
      expect(labels['Open.Label']).toBe('Dynodoc');
      expect(String(labels['Open.Description']).length).toBeLessThanOrEqual(250);
    }
  });

  it('match the downloadable copies served by the web app', () => {
    for (const m of manifests)
      expect(read(resolve(staticDir, `connectors/packages/${m.host}.xml`))).toBe(m.text);
  });
});

describe('marketplace image assets', () => {
  const assets = resolve(extensions, 'marketplace/assets');
  const expected: Record<string, [number, number]> = {
    'office-icon-16.png': [16, 16],
    'office-icon-32.png': [32, 32],
    'office-icon-64.png': [64, 64],
    'office-icon-80.png': [80, 80],
    'office-store-logo-300.png': [300, 300],
    'google-icon-32.png': [32, 32],
    'google-icon-96.png': [96, 96],
    'google-icon-128.png': [128, 128],
    'google-card-banner-220x140.png': [220, 140],
    'google-card-banner-440x280.png': [440, 280]
  };
  it('have the exact pixel sizes the stores ask for', () => {
    for (const [file, [width, height]] of Object.entries(expected))
      expect(pngSize(resolve(assets, file)), file).toEqual({ width, height });
  });
  it('are the same bytes the manifests link to', () => {
    for (const size of [16, 32, 64, 80])
      expect(readFileSync(resolve(assets, `office-icon-${size}.png`))).toEqual(
        readFileSync(resolve(staticDir, `connectors/assets/icon-${size}.png`))
      );
  });
});

describe('Google editor add-ons', () => {
  for (const app of google) {
    describe(app.host, () => {
      const dir = resolve(extensions, app.host);
      const manifest = JSON.parse(read(resolve(dir, 'appsscript.json')));
      const code = read(resolve(dir, 'Code.gs'));
      const sidebar = read(resolve(dir, 'Sidebar.html'));

      it('has a minimal V8 manifest with exception logging', () => {
        expect(manifest.runtimeVersion).toBe('V8');
        expect(manifest.exceptionLogging).toBe('STACKDRIVER');
        expect([...manifest.oauthScopes].sort()).toEqual(
          [
            app.scope,
            'https://www.googleapis.com/auth/script.container.ui',
            'https://www.googleapis.com/auth/script.external_request'
          ].sort()
        );
        expect(
          manifest.oauthScopes.every((s: string) =>
            s.startsWith('https://www.googleapis.com/auth/')
          )
        ).toBe(true);
      });

      it('may fetch only the Dynodoc connector API over HTTPS', () => {
        expect(manifest.urlFetchWhitelist).toEqual([`${APP_ORIGIN}/api/connectors/`]);
        expect(code).toMatch(new RegExp(`DYNODOC_ORIGIN\\s*=\\s*'${APP_ORIGIN}'`));
        expect(code).toMatch(new RegExp(`DYNODOC_HOST\\s*=\\s*'${app.host}'`));
      });

      it('uses the product name in menus and sidebar, with no development wording', () => {
        expect(code).toContain("addItem('Open Dynodoc'");
        expect(code).toContain(".setTitle('Dynodoc')");
        expect(code).toContain(app.menu);
        for (const text of [code, sidebar]) {
          expect(text).not.toMatch(/development (connector|binding|queue)/i);
          expect(text).not.toMatch(/Open provenance/);
          expect(text).not.toMatch(/\ba editor\b/);
        }
        expect(sidebar).toMatch(/<html lang="en">/);
        expect(sidebar).toMatch(/<title>Dynodoc<\/title>/);
        expect(sidebar).not.toMatch(/Development/);
      });

      it('leads with browser sign-in and keeps manual key entry under a disclosure', () => {
        const signIn = sidebar.indexOf('id="signin"');
        const advanced = sidebar.indexOf('<summary>Advanced: connect with a key</summary>');
        const documentId = sidebar.indexOf('id="documentId"');
        const token = sidebar.indexOf('id="token"');
        const connect = sidebar.indexOf('id="connect"');
        expect(signIn).toBeGreaterThan(-1);
        expect(advanced).toBeGreaterThan(signIn);
        for (const control of [documentId, token, connect])
          expect(control).toBeGreaterThan(advanced);
        const closing = sidebar.indexOf('</details>', advanced);
        for (const control of [documentId, token, connect]) expect(control).toBeLessThan(closing);
      });

      it('receives the shared Common.gs and Pairing.gs byte-for-byte', () => {
        for (const file of ['Common.gs', 'Pairing.gs'])
          expect(read(resolve(dir, file))).toBe(read(resolve(extensions, 'google-common', file)));
      });
    });
  }
});
