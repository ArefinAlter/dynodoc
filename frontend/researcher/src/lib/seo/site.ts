// Single source of truth for the public site's identity: canonical origin,
// which hosts and paths may be indexed, and the page copy that search and
// answer engines read. robots.txt, sitemap.xml, llms.txt, the head tags and
// hooks.server.ts all derive from this, so the signals cannot drift apart.

import { INDEXNOW_KEY_PATH } from './indexnow';

export const CANONICAL_ORIGIN = 'https://dynodoc.online';

/**
 * Only the public homepage host is indexable. `app.` and `admin.` serve the
 * same SvelteKit app but hold private document content, so they are excluded
 * everywhere: robots.txt, the head tags and the x-robots-tag header.
 */
export const INDEXABLE_HOSTS = ['dynodoc.online', 'www.dynodoc.online'];

export const SITE_NAME = 'Dynodoc';
export const SITE_TAGLINE = 'Version control for documents';
export const SITE_DESCRIPTION =
  'Version control for rich-text documents and spreadsheets. Write in a familiar editor, explore private drafts, review wording and formatting, and share selected changes.';
export const SITE_IMAGE = `${CANONICAL_ORIGIN}/brand/social.png`;

export type IndexablePage = {
  path: string;
  title: string;
  description: string;
  /** Date the page's content last changed; kept by hand, not at build time,
   *  so a redeploy that changes nothing does not claim a fresh lastmod. */
  lastmod: string;
  changefreq: 'daily' | 'weekly' | 'monthly' | 'yearly';
  priority: string;
};

export const INDEXABLE_PAGES: IndexablePage[] = [
  {
    path: '/compatibility',
    title: 'Document & spreadsheet compatibility · Dynodoc',
    description:
      'Current Word, Excel, DOCX, XLSX, pagination, review and accessibility support, with explicit limits.',
    lastmod: '2026-09-21',
    changefreq: 'monthly',
    priority: '0.7'
  },
  {
    path: '/',
    title: `${SITE_NAME} — ${SITE_TAGLINE}`,
    description: SITE_DESCRIPTION,
    lastmod: '2026-09-21',
    changefreq: 'weekly',
    priority: '1.0'
  },
  {
    path: '/privacy',
    title: `Privacy policy · ${SITE_NAME}`,
    description:
      'How Dynodoc handles account information, uploaded documents, collaboration history and connected services.',
    lastmod: '2026-09-21',
    changefreq: 'monthly',
    priority: '0.5'
  },
  {
    path: '/terms',
    title: `Terms & conditions · ${SITE_NAME}`,
    description:
      'Proposed terms for using Dynodoc, including accounts, collaboration, content ownership and responsible use.',
    lastmod: '2026-09-21',
    changefreq: 'monthly',
    priority: '0.5'
  },
  {
    path: '/features',
    title: 'Features · Dynodoc',
    description:
      'Current document, spreadsheet, questionnaire, integration and review capabilities.',
    lastmod: '2026-09-21',
    changefreq: 'monthly',
    priority: '0.7'
  },
  {
    path: '/why-dynodoc',
    title: 'Why Dynodoc · Dynodoc',
    description:
      'Document version control, transparent revisions and the evidence behind the design.',
    lastmod: '2026-09-21',
    changefreq: 'monthly',
    priority: '0.7'
  },
  {
    path: '/roadmap',
    title: 'Product roadmap · Dynodoc',
    description:
      'The roadmap for document fidelity, efficient history, attribution, offline collaboration and additional use cases.',
    lastmod: '2026-09-21',
    changefreq: 'monthly',
    priority: '0.7'
  },
  {
    path: '/help',
    title: 'Help & tutorials · Dynodoc',
    description: 'Learn editing, team access, personal drafts, public sharing and version history.',
    lastmod: '2026-09-21',
    changefreq: 'monthly',
    priority: '0.7'
  },
  {
    path: '/cookies',
    title: 'Cookies & local storage · Dynodoc',
    description:
      'Sign-in cookies, local preferences and optional PostgreSQL performance measurements.',
    lastmod: '2026-09-21',
    changefreq: 'monthly',
    priority: '0.7'
  }
];

/** Plain-text and XML assets that must stay fetchable and free of noindex. */
export const PUBLIC_SEO_FILES = [
  '/robots.txt',
  '/sitemap.xml',
  '/llms.txt',
  '/llms-full.txt',
  INDEXNOW_KEY_PATH
];

/** Routes that must never be crawled, even on the public host. */
export const DISALLOWED_PATHS = [
  '/print',
  '/api/',
  '/auth/',
  '/admin',
  '/login',
  '/workspace',
  '/documents/',
  '/view/',
  '/integrations/',
  '/source',
  '/telemetry',
  '/healthz'
];

export function canonicalUrl(path: string): string {
  return `${CANONICAL_ORIGIN}${path === '/' ? '/' : path}`;
}

export function isIndexableHost(hostname: string): boolean {
  return INDEXABLE_HOSTS.includes(hostname);
}

export function isIndexablePath(pathname: string): boolean {
  return INDEXABLE_PAGES.some((page) => page.path === pathname);
}

/** True for pages and SEO assets that may be served without a noindex header. */
export function isPublicPath(pathname: string): boolean {
  return isIndexablePath(pathname) || PUBLIC_SEO_FILES.includes(pathname);
}

export function pageMeta(pathname: string): IndexablePage | undefined {
  return INDEXABLE_PAGES.find((page) => page.path === pathname);
}

/** Every canonical URL the sitemap publishes; also what IndexNow submits. */
export function allCanonicalUrls(): string[] {
  return INDEXABLE_PAGES.map((page) => canonicalUrl(page.path));
}
