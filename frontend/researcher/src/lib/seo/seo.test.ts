import { describe, expect, it } from 'vitest';
import { readFileSync } from 'node:fs';
import { resolve } from 'node:path';
import { GET as robots } from '../../routes/robots.txt/+server';
import { GET as sitemap } from '../../routes/sitemap.xml/+server';
import { GET as llms } from '../../routes/llms.txt/+server';
import { GET as llmsFull } from '../../routes/llms-full.txt/+server';
import { buildPayload, describeStatus, readKey } from '../../../scripts/indexnow.mjs';
import { FOOTER_COLUMNS, NAV_GROUPS } from '../components/site/nav';
import { FAQ } from './faq';
import { INDEXNOW_KEY, INDEXNOW_KEY_PATH } from './indexnow';
import { structuredDataScript } from './structured-data';
import {
  CANONICAL_ORIGIN,
  DISALLOWED_PATHS,
  INDEXABLE_PAGES,
  SITE_DESCRIPTION,
  SITE_TAGLINE,
  canonicalUrl
} from './site';

// Vitest runs from the package root, so resolve the published assets from there.
const staticDir = resolve(process.cwd(), 'static');
// Each route's generated RequestHandler is typed to its own path, so call them
// through the one thing they share: an event carrying a URL.
type UrlHandler = (event: { url: URL }) => Response | Promise<Response>;
const get = (handler: unknown, href: string) => (handler as UrlHandler)({ url: new URL(href) });
const body = async (handler: unknown, href: string) => (await get(handler, href)).text();

const PUBLIC = 'https://dynodoc.online';
const PRIVATE_HOSTS = ['https://app.dynodoc.online', 'https://admin.dynodoc.online'];

describe('robots.txt', () => {
  it('opens the public host to crawlers but holds back private routes', async () => {
    const text = await body(robots, `${PUBLIC}/robots.txt`);
    expect(text).toMatch(/^User-agent: \*\nAllow: \/$/m);
    for (const path of DISALLOWED_PATHS) expect(text).toContain(`Disallow: ${path}`);
    expect(text).toContain(`Sitemap: ${CANONICAL_ORIGIN}/sitemap.xml`);
  });

  it('allows answer-engine crawlers, for retrieval and for training', async () => {
    const text = await body(robots, `${PUBLIC}/robots.txt`);
    for (const agent of [
      'OAI-SearchBot',
      'GPTBot',
      'ClaudeBot',
      'PerplexityBot',
      'Google-Extended'
    ])
      expect(text).toContain(`User-agent: ${agent}\nAllow: /`);
  });

  it('holds back the native-app sign-in pages, which end in a session', async () => {
    const text = await body(robots, `${PUBLIC}/robots.txt`);
    expect(text).toMatch(/^Disallow: \/app-signin$/m);
    // Disallow rules match by prefix, so one entry covers the finish page too.
    for (const path of ['/app-signin', '/app-signin/finish'])
      expect(
        DISALLOWED_PATHS.some((prefix) => path.startsWith(prefix)),
        path
      ).toBe(true);
    const xml = await body(sitemap, `${PUBLIC}/sitemap.xml`);
    expect(xml).not.toContain('app-signin');
  });

  it('closes the workspace and admin hosts entirely', async () => {
    for (const host of PRIVATE_HOSTS)
      expect(await body(robots, `${host}/robots.txt`)).toBe('User-agent: *\nDisallow: /\n');
  });
});

describe('sitemap.xml', () => {
  it('publishes every indexable page as a canonical URL with a lastmod', async () => {
    const xml = await body(sitemap, `${PUBLIC}/sitemap.xml`);
    for (const page of INDEXABLE_PAGES) {
      expect(xml).toContain(`<loc>${canonicalUrl(page.path)}</loc>`);
      expect(xml).toContain(`<lastmod>${page.lastmod}</lastmod>`);
    }
    // Private areas must never be advertised.
    for (const path of DISALLOWED_PATHS) expect(xml).not.toContain(`<loc>${PUBLIC}${path}`);
  });

  it('advertises canonical URLs even when served from another host, and 404s there', async () => {
    for (const host of PRIVATE_HOSTS)
      expect((await get(sitemap, `${host}/sitemap.xml`)).status).toBe(404);
  });
});

describe('llms.txt', () => {
  it('states what the product is, and what it does not do', async () => {
    const text = await body(llms, `${PUBLIC}/llms.txt`);
    expect(text.startsWith('# Dynodoc')).toBe(true);
    expect(text).toContain('Word, Google Docs, Excel, Google Sheets, PowerPoint and Google Slides');
    expect(text).toContain('## What it does not do');
    expect(text).toContain('no live collaborator cursors');
    for (const page of INDEXABLE_PAGES) expect(text).toContain(canonicalUrl(page.path));
  });

  it('serves the full variant with every FAQ answer', async () => {
    const text = await body(llmsFull, `${PUBLIC}/llms-full.txt`);
    for (const entry of FAQ) {
      expect(text).toContain(`### ${entry.question}`);
      expect(text).toContain(entry.answer);
    }
  });

  it('is not served from the private hosts', async () => {
    for (const host of PRIVATE_HOSTS) {
      expect((await get(llms, `${host}/llms.txt`)).status).toBe(404);
      expect((await get(llmsFull, `${host}/llms-full.txt`)).status).toBe(404);
    }
  });
});

describe('structured data', () => {
  const json = JSON.parse(
    structuredDataScript
      .replace(/^<script type="application\/ld\+json">/, '')
      .replace(/<\/?script>$/, '')
  );
  const graph: Array<Record<string, unknown>> = json['@graph'];

  it('describes the site, the organization and the application', () => {
    expect(graph.map((node) => node['@type'])).toEqual([
      'Organization',
      'WebSite',
      'SoftwareApplication',
      'FAQPage'
    ]);
  });

  it('marks up exactly the questions and answers the page renders', () => {
    const faq = graph.find((node) => node['@type'] === 'FAQPage') as {
      mainEntity: Array<{ name: string; acceptedAnswer: { text: string } }>;
    };
    expect(faq.mainEntity.map((q) => [q.name, q.acceptedAnswer.text])).toEqual(
      FAQ.map((entry) => [entry.question, entry.answer])
    );
  });

  it('escapes any sequence that could close the script element early', () => {
    expect(structuredDataScript.slice(30, -9)).not.toContain('<');
  });
});

describe('homepage copy', () => {
  // The research/questionnaire framing is retired: no crawler-facing text may bring it back.
  const stale = /questionnaire|xlsform|\bodk\b|kobo|redcap|survey|research|respondent|pilot/i;

  it('describes document provenance for the six editors in the tagline and description', () => {
    expect(SITE_TAGLINE).toBe('Version control for documents, spreadsheets and presentations');
    for (const editor of ['Word', 'Docs', 'Excel', 'Sheets', 'PowerPoint', 'Slides'])
      expect(SITE_DESCRIPTION).toContain(editor);
  });

  it('keeps retired framing out of the FAQ, llms.txt and structured data', async () => {
    // The page list is each page's own description; this test covers the homepage copy.
    const withoutPages = (text: string) => text.replace(/## Pages[\s\S]*?(?=## What it does)/, '');
    const texts = [
      SITE_TAGLINE,
      SITE_DESCRIPTION,
      structuredDataScript,
      withoutPages(await body(llms, `${PUBLIC}/llms.txt`)),
      withoutPages(await body(llmsFull, `${PUBLIC}/llms-full.txt`)),
      ...FAQ.flatMap((entry) => [entry.question, entry.answer])
    ];
    for (const text of texts) expect(text).not.toMatch(stale);
  });

  it('answers the questions the page promises, at a length search engines can quote', () => {
    expect(FAQ).toHaveLength(8);
    for (const entry of FAQ) {
      expect(entry.question.endsWith('?')).toBe(true);
      expect(entry.answer.length).toBeGreaterThan(80);
    }
    expect(FAQ.map((entry) => entry.question)).toContain('What does it not do?');
  });
});

describe('licence and operator statements', () => {
  it('claims open source only for the engine, API and command-line tools', async () => {
    const texts = [
      SITE_DESCRIPTION,
      structuredDataScript,
      await body(llms, `${PUBLIC}/llms.txt`),
      await body(llmsFull, `${PUBLIC}/llms-full.txt`),
      ...FAQ.flatMap((entry) => [entry.question, entry.answer])
    ];
    for (const text of texts)
      expect(text).not.toMatch(/AGPL|source archive|dynodoc\.online\/source/i);
    const text = await body(llms, `${PUBLIC}/llms.txt`);
    expect(text).toContain(
      'The engine, API and command-line tools are open source under the MIT licence. The workspace and website are not published as open source.'
    );
    expect(structuredDataScript).toContain(
      '"codeRepository":"https://github.com/ArefinAlter/dynodoc-engine"'
    );
    expect(structuredDataScript).toContain('https://opensource.org/license/mit');
  });

  it('names the operator and its contact, and never a tax or commercial identifier', async () => {
    const text = await body(llms, `${PUBLIC}/llms.txt`);
    expect(text).toContain('Verne Systems');
    expect(text).toContain('info@vernesystems.com');
    expect(structuredDataScript).toContain('"legalName":"Verne Systems"');
    expect(structuredDataScript).toContain('"telephone":"+880 1870 034384"');
    expect(`${structuredDataScript}${text}`).not.toMatch(/\bTIN\b|D-U-N-S|DUNS/);
  });
});

describe('the open-source page', () => {
  const source = readFileSync(
    resolve(process.cwd(), 'src/routes/(site)/open-source/+page.svelte'),
    'utf8'
  );

  it('is indexable, listed in the sitemap, llms.txt and the navigation', async () => {
    expect(INDEXABLE_PAGES.map((page) => page.path)).toContain('/open-source');
    expect(INDEXABLE_PAGES).toHaveLength(20);
    expect(await body(sitemap, `${PUBLIC}/sitemap.xml`)).toContain(
      `<loc>${canonicalUrl('/open-source')}</loc>`
    );
    expect(await body(llms, `${PUBLIC}/llms.txt`)).toContain(canonicalUrl('/open-source'));
    const links = [
      ...NAV_GROUPS.flatMap((group) => group.links),
      ...FOOTER_COLUMNS.flatMap((c) => c.links)
    ];
    expect(links.map((link) => link.href)).toContain('/open-source');
    expect(
      NAV_GROUPS.find((group) => group.id === 'resources')?.links.map((l) => l.href)
    ).toContain('/open-source');
  });

  it('offers no download and does not say a release exists', () => {
    expect(source).toContain('none is published yet');
    // No link to a release, an archive or a package: the publish path has never run.
    expect(source).not.toMatch(
      /href=["'{][^>]*(releases|download|\.zip|\.tar\.gz|crates\.io|ghcr\.io)/i
    );
    // Unsigned binaries are stated, never the opposite.
    expect(source).toMatch(/will not be signed or notarised/);
    expect(source).not.toMatch(/\bsigned and notarised\b|\bnotarized\b/i);
  });

  it('keeps to the public engine repository: no desktop, phone-app or self-hosted work', () => {
    expect(source).not.toMatch(/self-host|desktop|android|iphone|tauri|marketplace/i);
  });
});

describe('IndexNow', () => {
  it('publishes a key file whose name and contents match the key', async () => {
    const onDisk = readFileSync(`${staticDir}${INDEXNOW_KEY_PATH}`, 'utf8');
    expect(onDisk).toBe(INDEXNOW_KEY);
    // No BOM and no trailing newline: the file must contain the key, exactly.
    expect(onDisk).toMatch(/^[0-9a-f]{32}$/);
  });

  it('reads the same key from disk as the app publishes', async () => {
    const { key, fileName } = await readKey(staticDir);
    expect(key).toBe(INDEXNOW_KEY);
    expect(`/${fileName}`).toBe(INDEXNOW_KEY_PATH);
  });

  it('builds a payload pointing at the published key location', () => {
    const payload = buildPayload(CANONICAL_ORIGIN, INDEXNOW_KEY, ['/privacy']);
    expect(payload).toEqual({
      host: 'dynodoc.online',
      key: INDEXNOW_KEY,
      keyLocation: `${CANONICAL_ORIGIN}${INDEXNOW_KEY_PATH}`,
      urlList: [`${CANONICAL_ORIGIN}/privacy`]
    });
  });

  it('refuses URLs on another host, which IndexNow would reject as 422', () => {
    expect(() =>
      buildPayload(CANONICAL_ORIGIN, INDEXNOW_KEY, ['https://app.dynodoc.online/workspace'])
    ).toThrow(/outside dynodoc\.online/);
    expect(() => buildPayload(CANONICAL_ORIGIN, INDEXNOW_KEY, [])).toThrow(/at least one URL/);
  });

  it('explains the response codes the endpoint documents', () => {
    expect(describeStatus(200)).toMatch(/successfully/);
    expect(describeStatus(403)).toMatch(/key not valid/);
    expect(describeStatus(422)).toMatch(/not on this host/);
    expect(describeStatus(418)).toMatch(/Unexpected status 418/);
  });
});
