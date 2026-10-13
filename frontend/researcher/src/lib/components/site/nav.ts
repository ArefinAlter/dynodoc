import { CANONICAL_ORIGIN } from '$lib/seo/site';
import { ENGINE_REPO_URL } from '$lib/site/operator';

/**
 * Navigation data shared by SiteHeader and SiteFooter, and the helper that decides
 * whether a link may stay relative (page served from the marketing site) or must point
 * at the canonical origin (page served from an app/admin host such as /login or /view).
 */
export type NavLink = {
  label: string;
  /** Path on the marketing site (`/features`, `/#how-it-works`) or an absolute URL. */
  href: string;
  /** One line shown under the label in header dropdowns. */
  description?: string;
  external?: boolean;
};
export type NavGroup = { id: string; label: string; links: NavLink[] };

/** The public repository: the MIT engine, API and command-line tools. Nothing else is published. */
export const GITHUB_URL = ENGINE_REPO_URL;

export const NAV_GROUPS: NavGroup[] = [
  {
    id: 'product',
    label: 'Product',
    links: [
      {
        label: 'Features',
        href: '/features',
        description: 'What Dynodoc records, reviews and merges.'
      },
      {
        label: 'How it works',
        href: '/#how-it-works',
        description: 'From a changed file to a verifiable history.'
      },
      {
        label: 'Editors & add-ins',
        href: '/add-ins',
        description: 'Word, Excel, PowerPoint, Google editors and the web editors.'
      },
      {
        label: 'Security',
        href: '/security',
        description: 'How data is protected and where it lives.'
      }
    ]
  },
  {
    id: 'resources',
    label: 'Resources',
    links: [
      { label: 'Help', href: '/help', description: 'Guides for editing, drafts and review.' },
      {
        label: 'Why provenance',
        href: '/why-dynodoc',
        description: 'Why document history should be verifiable.'
      },
      { label: 'Roadmap', href: '/roadmap', description: 'What exists and what is planned.' },
      {
        label: 'Compatibility',
        href: '/compatibility',
        description: 'Supported formats and known limits.'
      },
      { label: 'Changelog', href: '/changelog', description: 'Release notes.' },
      { label: 'Support', href: '/support', description: 'Ask a question or report a problem.' }
    ]
  }
];

/** Top-level links that sit next to the dropdown groups. */
export const NAV_DIRECT: NavLink[] = [
  { label: 'Docs', href: '/help' },
  { label: 'GitHub', href: GITHUB_URL, external: true }
];

export type FooterColumn = { title: string; links: NavLink[] };

export const FOOTER_COLUMNS: FooterColumn[] = [
  {
    title: 'Product',
    links: [
      { label: 'Features', href: '/features' },
      { label: 'How it works', href: '/#how-it-works' },
      { label: 'Editors & add-ins', href: '/add-ins' },
      { label: 'Compatibility', href: '/compatibility' }
    ]
  },
  {
    title: 'Resources',
    links: [
      { label: 'Help', href: '/help' },
      { label: 'Why provenance', href: '/why-dynodoc' },
      { label: 'Roadmap', href: '/roadmap' },
      { label: 'Changelog', href: '/changelog' }
    ]
  },
  {
    title: 'Trust & legal',
    links: [
      { label: 'Privacy policy', href: '/privacy' },
      { label: 'Terms & conditions', href: '/terms' },
      { label: 'Cookies', href: '/cookies' },
      { label: 'Security', href: '/security' },
      { label: 'Subprocessors', href: '/subprocessors' },
      { label: 'Acceptable use', href: '/acceptable-use' },
      { label: 'Accessibility', href: '/accessibility' },
      { label: 'Licenses', href: '/licenses' },
      { label: 'DPA', href: '/dpa' }
    ]
  },
  {
    title: 'Company',
    links: [
      { label: 'Support', href: '/support' },
      { label: 'Brand', href: '/brand' },
      { label: 'Open-source engine', href: GITHUB_URL, external: true }
    ]
  }
];

/** True on the workspace and admin hosts, which serve the same app as the marketing site. */
export function isAppHost(hostname: string): boolean {
  return hostname.endsWith('.dynodoc.online') && hostname !== 'www.dynodoc.online';
}

/**
 * Link target for a marketing-site path: relative on the marketing site (and in local
 * development), the canonical origin when the page is served from an app host. Absolute
 * URLs are returned unchanged.
 */
export function siteHref(href: string, relative: boolean): string {
  if (/^https?:\/\//.test(href)) return href;
  return relative ? href : `${CANONICAL_ORIGIN}${href}`;
}

/** The workspace entry point: PUBLIC_WORKSPACE_URL/projects, or same-origin /projects. */
export function workspaceHref(workspaceUrl: string | undefined): string {
  return workspaceUrl ? `${workspaceUrl.replace(/\/$/, '')}/projects` : '/projects';
}
