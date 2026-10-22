// @vitest-environment node
// The manifest generator: the hosted manifests reproduced byte for byte, every kind and
// variant checked for structure, and the inputs a hostile or careless administrator could
// supply. There is no Office schema file in the repository and `office-addin-manifest` is not
// installed, so validity is checked structurally (the same invariants manifests.test.ts holds
// the committed manifests to) and is NOT a substitute for Microsoft's validator or a real host.
import { readFileSync } from 'node:fs';
import { resolve } from 'node:path';
import { DOMParser, type Document as XmlDocument, type Element } from '@xmldom/xmldom';
import { describe, expect, it } from 'vitest';
import {
  LIMITS,
  OFFICE_KINDS,
  OFFICE_VARIANTS,
  addinGuid,
  articleFor,
  manifestFileName,
  manifestVersion,
  renderOfficeManifest,
  type OfficeKind,
  type OfficeVariant
} from './manifest.js';
import { ADDIN_NAMESPACE, uuidV5 } from './uuid.js';
import { escapeXml, xml, raw } from './xml.js';

const root = resolve(process.cwd(), '../..');
const HOSTED = 'https://app.dynodoc.online';
const HOSTED_IDS: Record<OfficeKind, string> = {
  word: 'adbda646-1a13-4a70-baf5-c772e9cc90a6',
  excel: '794ef9dc-6d78-4b08-a7f3-2526d0476930',
  powerpoint: '28155d15-5a0e-4b94-b0ce-c448d684d125'
};
const ORIGIN = 'https://dynodoc.corp.example';
const base = { origin: ORIGIN, brand: 'Acme Docs', version: '1.0.1.0' } as const;

function parse(text: string) {
  const problems: string[] = [];
  const document = new DOMParser({
    onError: (level, message) => problems.push(`${level}: ${message}`)
  }).parseFromString(text, 'text/xml');
  return { document, problems };
}
const all = (node: XmlDocument | Element, tag: string) =>
  Array.from(node.getElementsByTagName(tag)) as Element[];
const value = (document: XmlDocument, tag: string) =>
  all(document, tag)[0]?.getAttribute('DefaultValue') ?? '';

describe('hosted values reproduce the committed manifests exactly', () => {
  for (const kind of OFFICE_KINDS) {
    it(`${kind}: extensions/ and the static download are byte for byte the generator's output`, () => {
      const generated = renderOfficeManifest({
        origin: HOSTED,
        brand: 'Dynodoc',
        kind,
        version: '1.1.0.0',
        guid: HOSTED_IDS[kind],
        supportUrl: 'https://dynodoc.online/support'
      });
      const source = readFileSync(resolve(root, 'extensions', kind, 'manifest.xml'));
      const served = readFileSync(
        resolve(process.cwd(), 'static/connectors/packages', `${kind}.xml`)
      );
      expect(Buffer.from(generated, 'utf8').equals(source)).toBe(true);
      expect(Buffer.from(generated, 'utf8').equals(served)).toBe(true);
    });
  }
});

describe('every kind and variant', () => {
  const cases = OFFICE_KINDS.flatMap((kind) =>
    OFFICE_VARIANTS.map((variant) => [kind, variant] as [OfficeKind, OfficeVariant])
  );
  it.each(cases)('%s (%s) is well-formed and names only the installation', (kind, variant) => {
    const text = renderOfficeManifest({ ...base, kind, variant });
    const { document, problems } = parse(text);
    expect(problems).toEqual([]);
    const rootElement = document.documentElement!;
    expect(rootElement.tagName).toBe('OfficeApp');
    expect(rootElement.getAttribute('xsi:type')).toBe('TaskPaneApp');
    expect(text.endsWith('</OfficeApp>\n')).toBe(true);
    expect(text).not.toMatch(/\r/);
    // Required elements, in the schema's order.
    const order = [
      'Id',
      'Version',
      'ProviderName',
      'DefaultLocale',
      'DisplayName',
      'Description',
      'IconUrl',
      'HighResolutionIconUrl',
      'SupportUrl',
      'AppDomains',
      'Hosts',
      'Requirements',
      'DefaultSettings',
      'Permissions'
    ];
    const children = Array.from(rootElement.childNodes)
      .filter((node) => node.nodeType === 1)
      .map((node) => (node as Element).tagName);
    expect(children.slice(0, order.length)).toEqual(order);
    expect(children.slice(order.length)).toEqual(variant === 'catalog' ? [] : ['VersionOverrides']);
    // Every address is HTTPS on the one origin; nothing carries credentials, a query or a hash.
    const urls = [
      ...all(document, 'IconUrl'),
      ...all(document, 'HighResolutionIconUrl'),
      ...all(document, 'SourceLocation'),
      ...all(document, 'bt:Url'),
      ...all(document, 'bt:Image')
    ]
      .filter((e) => e.hasAttribute('DefaultValue'))
      .map((e) => e.getAttribute('DefaultValue')!);
    expect(urls.length).toBeGreaterThanOrEqual(variant === 'catalog' ? 3 : 8);
    for (const url of urls) {
      const parsed = new URL(url);
      expect(parsed.protocol, url).toBe('https:');
      expect(parsed.origin, url).toBe(ORIGIN);
      expect(parsed.username + parsed.password + parsed.search + parsed.hash).toBe('');
    }
    expect(all(document, 'AppDomain').map((e) => e.textContent)).toEqual([ORIGIN]);
    expect(new URL(value(document, 'SourceLocation')).pathname).toBe(`/connectors/${kind}`);
    expect(value(document, 'SupportUrl')).toBe(`${ORIGIN}/`);
    // Nothing of the hosted service is left, in any spelling.
    expect(text).not.toMatch(/dynodoc\.online/i);
    expect(text).not.toContain('app.dynodoc');
    // The name and strings carry the configured product name.
    expect(value(document, 'DisplayName')).toBe(
      `Acme Docs for ${{ word: 'Word', excel: 'Excel', powerpoint: 'PowerPoint' }[kind]}`
    );
    expect(all(document, 'ProviderName')[0].textContent).toBe('Acme Docs');
    expect(value(document, 'Description')).toContain('Acme Docs project');
    expect(value(document, 'Description')).not.toContain('Dynodoc');
    expect(all(document, 'Permissions')[0].textContent).toBe('ReadWriteDocument');
    // Every resource id the ribbon refers to exists (standard); nothing is left over (catalog).
    const defined = new Set(
      [...all(document, 'bt:Image'), ...all(document, 'bt:Url'), ...all(document, 'bt:String')]
        .map((e) => e.getAttribute('id'))
        .filter(Boolean)
    );
    const used = all(document, 'VersionOverrides').length
      ? Array.from(document.getElementsByTagName('*'))
          .map((e) => (e as Element).getAttribute('resid'))
          .filter(Boolean)
      : [];
    for (const id of used) expect(defined.has(id), `${id}`).toBe(true);
    if (variant === 'catalog') {
      expect(text).not.toContain('VersionOverrides');
      expect(text).not.toContain('FunctionFile');
      expect(text).not.toContain('ExtensionPoint');
    } else {
      expect(all(document, 'FunctionFile')).toHaveLength(1);
      expect(all(document, 'ExtensionPoint')).toHaveLength(1);
    }
  });

  it('keeps each kind on its own host, requirement set and pane', () => {
    const expected = {
      word: ['Document', 'WordApi', '1.3'],
      excel: ['Workbook', 'ExcelApi', '1.13'],
      powerpoint: ['Presentation', 'PowerPointApi', '1.4']
    } as const;
    for (const kind of OFFICE_KINDS) {
      const { document } = parse(renderOfficeManifest({ ...base, kind }));
      const [host, set, min] = expected[kind];
      expect(all(document, 'Host')[0].getAttribute('Name')).toBe(host);
      expect(all(document, 'Set')[0].getAttribute('Name')).toBe(set);
      expect(all(document, 'Sets')[0].getAttribute('DefaultMinVersion')).toBe(min);
      const ribbonHost = all(document, 'VersionOverrides')[0].getElementsByTagName('Host')[0];
      expect(ribbonHost.getAttribute('xsi:type')).toBe(host);
    }
  });

  it('uses the version it is given, and the add-in id', () => {
    const { document } = parse(renderOfficeManifest({ ...base, kind: 'word' }));
    expect(all(document, 'Version')[0].textContent).toBe('1.0.1.0');
    expect(all(document, 'Id')[0].textContent).toBe(addinGuid(ORIGIN, 'word', 'standard'));
  });

  it('names the manifest file the way it is served', () => {
    expect(manifestFileName('word')).toBe('word.xml');
    expect(manifestFileName('excel', 'catalog')).toBe('excel-catalog.xml');
  });
});

describe('add-in ids', () => {
  it('follow RFC 9562 version 5: the published DNS-namespace test vector', () => {
    expect(uuidV5('python.org', '6ba7b810-9dad-11d1-80b4-00c04fd430c8')).toBe(
      '886313e1-3b8a-5372-9b90-0c9aee199e5d'
    );
  });
  it('are the same every time, for the same origin, kind and variant', () => {
    for (let i = 0; i < 3; i++)
      expect(addinGuid(ORIGIN, 'word', 'standard')).toBe(addinGuid(ORIGIN, 'word', 'standard'));
    // Pinned: a change here changes every installation's add-in id (Office would see a new add-in).
    expect(ADDIN_NAMESPACE).toBe('fd916f5f-7fd1-41c2-8727-62ba6c31caca');
    expect(addinGuid(ORIGIN, 'word', 'standard')).toBe(
      uuidV5(`dynodoc-office-addin\n${ORIGIN}\nword\nstandard`)
    );
    expect(addinGuid('https://dynodoc.corp.example', 'word', 'standard')).toMatch(
      /^[0-9a-f]{8}-[0-9a-f]{4}-5[0-9a-f]{3}-[89ab][0-9a-f]{3}-[0-9a-f]{12}$/
    );
  });
  it('differ by origin, by kind and by variant, and across regenerations of unrelated input', () => {
    const ids = new Set<string>();
    for (const origin of [
      ORIGIN,
      'https://other.corp.example',
      'https://dynodoc.corp.example:8443'
    ])
      for (const kind of OFFICE_KINDS)
        for (const variant of OFFICE_VARIANTS) ids.add(addinGuid(origin, kind, variant));
    expect(ids.size).toBe(3 * 3 * 2);
    // The brand, version and support link do not change the id.
    const one = renderOfficeManifest({ ...base, kind: 'word' });
    const two = renderOfficeManifest({
      ...base,
      kind: 'word',
      brand: 'Other',
      version: '1.5.0.0',
      supportUrl: 'https://help.example.org/'
    });
    const idOf = (text: string) => all(parse(text).document, 'Id')[0].textContent;
    expect(idOf(one)).toBe(idOf(two));
  });
  it('can be set by the caller, and must be a lower-case UUID', () => {
    const guid = 'aaaaaaaa-2222-5333-8444-bbbbbbbbbbbb';
    expect(renderOfficeManifest({ ...base, kind: 'word', guid })).toContain(`<Id>${guid}</Id>`);
    for (const bad of ['', 'not-a-uuid', guid.toUpperCase(), `${guid}<`, `{${guid}}`])
      expect(() => renderOfficeManifest({ ...base, kind: 'word', guid: bad }), bad).toThrow(
        /lower-case UUID/
      );
  });
});

describe('versions', () => {
  it('follow the application version', () => {
    expect(manifestVersion('0.1.0')).toBe('1.0.1.0');
    expect(manifestVersion('2.14.3')).toBe('1.2.14.3');
    expect(manifestVersion('1.0.0-rc.1')).toBe('1.1.0.0');
    // Raising the application version always raises the manifest version.
    const parts = (v: string) => v.split('.').map(Number);
    const less = (a: string, b: string) => {
      const [x, y] = [parts(a), parts(b)];
      for (let i = 0; i < 4; i++) if (x[i] !== y[i]) return x[i] < y[i];
      return false;
    };
    for (const [a, b] of [
      ['0.1.0', '0.1.1'],
      ['0.1.9', '0.2.0'],
      ['0.9.9', '1.0.0'],
      ['1.2.3', '1.2.10']
    ])
      expect(less(manifestVersion(a), manifestVersion(b)), `${a} < ${b}`).toBe(true);
  });
  it('refuses what is not major.minor.patch, or does not fit Office', () => {
    for (const bad of ['', '1', '1.2', '1.2.3.4', 'a.b.c', '1.2.65536', '-1.0.0', '1.0.0 '])
      expect(() => manifestVersion(bad), bad).toThrow(/major\.minor\.patch/);
  });
  it('are checked on the manifest too', () => {
    for (const bad of ['', '1.0', '1.0.0', '1.0.0.0.0', '1.0.0.x', '1.0.0.65536', '01234567.0.0.0'])
      expect(() => renderOfficeManifest({ ...base, kind: 'word', version: bad }), bad).toThrow(
        /version/i
      );
  });
});

describe('the product name and the other text', () => {
  const hostile = `Acme & <Sons> "Docs" 'x' ]]> &amp; <!-- -->`;
  it('is escaped in every place, and parses back to what was typed', () => {
    for (const kind of OFFICE_KINDS) {
      const text = renderOfficeManifest({
        ...base,
        kind,
        brand: hostile,
        displaySuffix: '(a & b)',
        provider: '<Provider> & "Co"'
      });
      const { document, problems } = parse(text);
      expect(problems).toEqual([]);
      // No raw markup from the name reached the document.
      expect(text).not.toContain('<Sons>');
      expect(text).not.toContain('<!--');
      expect(text).not.toContain('"Docs"');
      expect(value(document, 'DisplayName')).toBe(
        `${hostile} for ${{ word: 'Word', excel: 'Excel', powerpoint: 'PowerPoint' }[kind]} (a & b)`
      );
      expect(all(document, 'ProviderName')[0].textContent).toBe('<Provider> & "Co"');
      const labels = all(document, 'bt:String').map((e) => e.getAttribute('DefaultValue'));
      expect(labels).toContain(hostile);
      // Only the elements the template writes exist.
      expect(all(document, 'Sons')).toHaveLength(0);
    }
  });
  it('is refused when it is empty, padded, multi-line, too long, or holds controls', () => {
    const bad: [string, unknown][] = [
      ['empty', ''],
      ['blank', '   '],
      ['padded', ' Acme'],
      ['trailing space', 'Acme '],
      ['newline', 'Acme\nDocs'],
      ['tab', 'Acme\tDocs'],
      ['NUL', 'Acme\u0000Docs'],
      ['escape', 'Acme\u001bDocs'],
      ['line separator', `Acme${String.fromCharCode(0x2028)}Docs`],
      ['right-to-left override', `Acme${String.fromCharCode(0x202e)}Docs`],
      ['isolate', `Acme${String.fromCharCode(0x2066)}Docs`],
      ['too long', 'x'.repeat(61)],
      ['a number', 7],
      ['nothing', undefined]
    ];
    for (const [name, brand] of bad)
      expect(
        () => renderOfficeManifest({ ...base, kind: 'word', brand: brand as string }),
        name
      ).toThrow(/product name/);
    expect(() =>
      renderOfficeManifest({ ...base, kind: 'word', brand: 'x'.repeat(60) })
    ).not.toThrow();
  });
  it('keeps every Office length limit for names up to the longest allowed, in every kind', () => {
    for (const kind of OFFICE_KINDS)
      for (const length of [1, 10, 30, 45, 50, 55, 60]) {
        const brand = 'N'.repeat(length);
        const { document } = parse(
          renderOfficeManifest({ ...base, kind, brand, displaySuffix: 'staging' })
        );
        expect(value(document, 'DisplayName').length).toBeLessThanOrEqual(LIMITS.displayName);
        expect(value(document, 'Description').length).toBeLessThanOrEqual(LIMITS.description);
        for (const e of all(document, 'bt:String')) {
          const limit = ['Open.Description'].includes(e.getAttribute('id')!)
            ? LIMITS.longString
            : LIMITS.shortString;
          expect(e.getAttribute('DefaultValue')!.length).toBeLessThanOrEqual(limit);
        }
      }
  });
  it('is introduced with the right article', () => {
    for (const [brand, article] of [
      ['Dynodoc', 'a'],
      ['Acme Docs', 'an'],
      ['Unity Docs', 'a'],
      ['Euro Files', 'a'],
      ['Hour Records', 'an'],
      ['Orbit', 'an'],
      ['Records', 'a']
    ]) {
      expect(articleFor(brand), brand).toBe(article);
      const { document } = parse(renderOfficeManifest({ ...base, kind: 'word', brand }));
      expect(value(document, 'Description')).toContain(`to ${article} ${brand} project.`);
      expect(
        all(document, 'bt:String')
          .find((e) => e.getAttribute('id') === 'Open.Description')
          ?.getAttribute('DefaultValue')
      ).toContain(`to ${article} ${brand} project,`);
    }
  });
  it('adds the suffix that tells staging from production to the name only', () => {
    const { document } = parse(
      renderOfficeManifest({ ...base, kind: 'excel', displaySuffix: '(staging)' })
    );
    expect(value(document, 'DisplayName')).toBe('Acme Docs for Excel (staging)');
    expect(all(document, 'bt:String')[0].getAttribute('DefaultValue')).toBe('Acme Docs');
    expect(() =>
      renderOfficeManifest({ ...base, kind: 'excel', displaySuffix: 'x'.repeat(31) })
    ).toThrow(/suffix/);
    expect(() => renderOfficeManifest({ ...base, kind: 'excel', displaySuffix: 'a\nb' })).toThrow(
      /suffix/
    );
  });
  it('writes the support link only when it is a plain https address', () => {
    const support = (url: string | undefined) =>
      value(
        parse(renderOfficeManifest({ ...base, kind: 'word', supportUrl: url })).document,
        'SupportUrl'
      );
    expect(support('https://help.example.org/desk?a=1&b=2')).toBe(
      'https://help.example.org/desk?a=1&b=2'
    );
    for (const unusable of [
      undefined,
      '',
      'mailto:records@example.org',
      'http://help.example.org/',
      'https://user:pass@help.example.org/',
      'javascript:alert(1)',
      'not a url',
      `https://help.example.org/${'a'.repeat(400)}`
    ])
      expect(support(unusable), String(unusable)).toBe(`${ORIGIN}/`);
    // An ampersand in the link is escaped in the file, not copied raw.
    expect(
      renderOfficeManifest({ ...base, kind: 'word', supportUrl: 'https://h.example/?a=1&b=2' })
    ).toContain('SupportUrl DefaultValue="https://h.example/?a=1&amp;b=2"');
  });
});

describe('the origin', () => {
  it('is checked before anything is written', () => {
    for (const origin of [
      'http://dynodoc.corp.example',
      'https://dynodoc.corp.example/',
      'https://admin@dynodoc.corp.example',
      'https://dynodoc.corp.example/path',
      'https://dynodoc.corp.example:0',
      '',
      'javascript:alert(1)'
    ])
      expect(() => renderOfficeManifest({ ...base, origin, kind: 'word' }), origin).toThrow(
        /^The origin/
      );
  });
  it('allows http://localhost only when the caller says this is a test stack', () => {
    const origin = 'http://localhost:4174';
    expect(() => renderOfficeManifest({ ...base, origin, kind: 'word' })).toThrow(/https/);
    const text = renderOfficeManifest({ ...base, origin, kind: 'word', allowLoopbackHttp: true });
    expect(text).toContain(`<AppDomain>${origin}</AppDomain>`);
    expect(() =>
      renderOfficeManifest({
        ...base,
        origin: 'http://intranet.example',
        kind: 'word',
        allowLoopbackHttp: true
      })
    ).toThrow(/https/);
  });
  it('can refuse the hosted service, which a self-hosted package never names', () => {
    expect(() =>
      renderOfficeManifest({
        ...base,
        origin: HOSTED,
        kind: 'word',
        forbidDomain: 'dynodoc.online'
      })
    ).toThrow(/hosted service/);
  });
  it('refuses an unknown kind or variant', () => {
    expect(() => renderOfficeManifest({ ...base, kind: 'outlook' as OfficeKind })).toThrow(
      /Unknown/
    );
    expect(() =>
      renderOfficeManifest({ ...base, kind: 'word', variant: 'unified' as OfficeVariant })
    ).toThrow(/Unknown/);
  });
});

describe('the XML helpers', () => {
  it('escape what must be escaped and refuse what XML cannot hold', () => {
    expect(escapeXml(`<a href="x">&'</a>`)).toBe(
      '&lt;a href=&quot;x&quot;&gt;&amp;&apos;&lt;/a&gt;'
    );
    expect(escapeXml('tab\tline\ncr\r and 𝒳')).toBe('tab\tline\ncr\r and 𝒳');
    const bads = [
      '\u0000',
      '\u0008',
      '\u000b',
      '\u001f',
      '\ud800',
      '\udfff',
      String.fromCharCode(0xfffe),
      String.fromCharCode(0xffff)
    ];
    for (const bad of bads)
      expect(() => escapeXml(`a${bad}b`), JSON.stringify(bad)).toThrow(/cannot hold/);
  });
  it('escape every value placed in a template, unless it was marked as markup', () => {
    expect(xml`<a b="${'"><x/>'}">${'&'}</a>`).toBe('<a b="&quot;&gt;&lt;x/&gt;">&amp;</a>');
    expect(xml`${raw('<b/>')}${'<b/>'}`).toBe('<b/>&lt;b/&gt;');
  });
});
