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
export const SITE_TAGLINE = 'Version control for documents, spreadsheets and presentations';
export const SITE_DESCRIPTION =
  'Capture, review and merge edits to text, cells and slides in Word, Docs, Excel, Sheets, PowerPoint and Slides, and keep them in a hash-chained history you can verify. Open-source engine.';
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
    path: '/try',
    title: `Try the pre-alpha · ${SITE_NAME}`,
    description:
      'Try Dynodoc 0.0.1b, a pre-alpha: the unsigned desktop app for Windows and Linux that keeps your workspace on your computer, or a sample workspace in the browser, removed after 24 hours, with file sizes and SHA-256 sums.',
    lastmod: '2026-10-27',
    changefreq: 'weekly',
    priority: '0.9'
  },
  {
    path: '/compatibility',
    title: 'Compatibility: editors, formats and limits · Dynodoc',
    description:
      'What Dynodoc captures, reviews and applies in Word, Excel, PowerPoint, Google Docs, Sheets and Slides today, with minimum host versions, file formats, supported browsers, small-screen limits and how to install the web app.',
    lastmod: '2026-10-26',
    changefreq: 'monthly',
    priority: '0.7'
  },
  {
    path: '/',
    title: `${SITE_NAME} — ${SITE_TAGLINE}`,
    description: SITE_DESCRIPTION,
    lastmod: '2026-10-26',
    changefreq: 'weekly',
    priority: '1.0'
  },
  {
    path: '/privacy',
    title: `Privacy policy · ${SITE_NAME}`,
    description:
      'How Dynodoc handles account information, documents, project history, signing keys, audit records and editor add-in data, and what can and cannot be erased. A draft.',
    lastmod: '2026-10-26',
    changefreq: 'monthly',
    priority: '0.5'
  },
  {
    path: '/terms',
    title: `Terms & conditions · ${SITE_NAME}`,
    description:
      'Proposed terms for using Dynodoc, including accounts, projects and history, editor add-ins, content ownership, responsible use and ending use. A draft.',
    lastmod: '2026-10-26',
    changefreq: 'monthly',
    priority: '0.5'
  },
  {
    path: '/features',
    title: 'Features · Dynodoc',
    description:
      'Stable identity for every paragraph, cell and shape; reviewed change requests under project rules; private drafts; recoverable history; tamper-evident records; local-first sync; connectors for six editors.',
    lastmod: '2026-10-26',
    changefreq: 'monthly',
    priority: '0.7'
  },
  {
    path: '/why-dynodoc',
    title: 'Why provenance matters · Dynodoc',
    description:
      'A document shows what it says now, not who changed it, who agreed or what it said last week. Why a verifiable history needs stable identity, an append-only log, deterministic merge and checkable evidence.',
    lastmod: '2026-10-12',
    changefreq: 'monthly',
    priority: '0.7'
  },
  {
    path: '/roadmap',
    title: 'Roadmap · Dynodoc',
    description:
      'What Dynodoc has built, what exists only in source, what is in progress and what comes later. Dated where a record gives a date, with no promised release dates.',
    lastmod: '2026-10-26',
    changefreq: 'monthly',
    priority: '0.7'
  },
  {
    path: '/changelog',
    title: 'Changelog · Dynodoc',
    description:
      'Plain-language release notes for Dynodoc, newest first: what changed for you, and whether it runs in the hosted pilot or only in source.',
    lastmod: '2026-10-26',
    changefreq: 'weekly',
    priority: '0.6'
  },
  {
    path: '/open-source',
    title: 'Open source: build from source and platforms · Dynodoc',
    description:
      'The engine, API and command-line tools are MIT licensed. How to build them from source, the platforms they are built and tested on, what that testing covers, and what is not published yet.',
    lastmod: '2026-10-26',
    changefreq: 'monthly',
    priority: '0.6'
  },
  {
    path: '/brand',
    title: 'Brand and press kit · Dynodoc',
    description:
      'Dynodoc logo and mark downloads, colour tokens, typography, usage rules, original dinosaur artwork and boilerplate text for press and partners.',
    lastmod: '2026-10-13',
    changefreq: 'monthly',
    priority: '0.4'
  },
  {
    path: '/help',
    title: 'Help: projects, review and history · Dynodoc',
    description:
      'Create a project, capture changes from an editor, review and merge a pull request, restore or revert earlier content, name versions, verify history and install the web app.',
    lastmod: '2026-10-26',
    changefreq: 'monthly',
    priority: '0.7'
  },
  {
    path: '/add-ins',
    title: 'Editors & add-ins · Dynodoc',
    description:
      'Add-ins for Word, Excel, PowerPoint, Google Docs, Sheets and Slides: what each captures, requirements, test installation, what is stored and sent, and marketplace status.',
    lastmod: '2026-10-26',
    changefreq: 'monthly',
    priority: '0.8'
  },
  {
    path: '/support',
    title: 'Support · Dynodoc',
    description:
      'How to get help with Dynodoc: guides, bug reports, security reports and add-in questions, with what to expect.',
    lastmod: '2026-10-26',
    changefreq: 'monthly',
    priority: '0.6'
  },
  {
    path: '/cookies',
    title: 'Cookies & local storage · Dynodoc',
    description:
      'Every cookie, local-storage item and cached file Dynodoc sets in your browser, why, and for how long, plus the control for optional performance measurements.',
    lastmod: '2026-10-26',
    changefreq: 'monthly',
    priority: '0.5'
  },
  {
    path: '/security',
    title: 'Security · Dynodoc',
    description:
      'What protects the history of your documents and access to it, what is live today, what exists only in source, what is not claimed, and how to report a vulnerability.',
    lastmod: '2026-10-26',
    changefreq: 'monthly',
    priority: '0.6'
  },
  {
    path: '/subprocessors',
    title: 'Subprocessors · Dynodoc',
    description:
      'The outside providers Dynodoc uses to host, deliver and sign in to the service, what each receives, and what is not used.',
    lastmod: '2026-10-26',
    changefreq: 'monthly',
    priority: '0.4'
  },
  {
    path: '/acceptable-use',
    title: 'Acceptable use · Dynodoc',
    description:
      'Proposed rules for using Dynodoc: what is not allowed, keeping provenance records honest, volume limits and good-faith security testing. A draft.',
    lastmod: '2026-10-26',
    changefreq: 'monthly',
    priority: '0.4'
  },
  {
    path: '/accessibility',
    title: 'Accessibility statement · Dynodoc',
    description:
      'Dynodoc aims for WCAG 2.2 level AA. What is done and tested, the known limitations, and how to report a barrier.',
    lastmod: '2026-10-13',
    changefreq: 'monthly',
    priority: '0.4'
  },
  {
    path: '/dpa',
    title: 'Data processing terms (draft) · Dynodoc',
    description:
      'The structure of data processing terms for Dynodoc: roles, subject matter, categories, security, subprocessors, deletion and audit. A draft, not an agreement.',
    lastmod: '2026-10-26',
    changefreq: 'monthly',
    priority: '0.3'
  },
  {
    path: '/licenses',
    title: 'Licenses · Dynodoc',
    description:
      'The engine, API and command-line tools are open source under the MIT licence; the workspace and website are not published as open source. Font licenses, direct dependencies and artwork.',
    lastmod: '2026-10-26',
    changefreq: 'monthly',
    priority: '0.4'
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
  '/healthz',
  // The native apps' sign-in pages belong to the workspace host and end in a session.
  '/app-signin'
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
