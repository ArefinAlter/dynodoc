// JSON-LD for the public pages. Everything here is static, first-party product
// metadata: no visitor, session or document value is ever interpolated.
//
// Answer engines lean on this graph more heavily than classic search does, so
// it describes what Dynodoc is and what the pilot does *not* do, rather than
// only carrying keywords.

import { FAQ } from './faq';
import {
  CANONICAL_ORIGIN,
  SITE_DESCRIPTION,
  SITE_IMAGE,
  SITE_NAME,
  canonicalUrl,
  pageMeta
} from './site';

const REPOSITORY = 'https://github.com/ArefinAlter/dynodoc';

const organization = {
  '@type': 'Organization',
  '@id': `${CANONICAL_ORIGIN}/#organization`,
  name: SITE_NAME,
  url: `${CANONICAL_ORIGIN}/`,
  logo: `${CANONICAL_ORIGIN}/brand/logo.svg`,
  sameAs: [REPOSITORY]
};

const website = {
  '@type': 'WebSite',
  '@id': `${CANONICAL_ORIGIN}/#website`,
  name: SITE_NAME,
  url: `${CANONICAL_ORIGIN}/`,
  description: SITE_DESCRIPTION,
  inLanguage: 'en',
  publisher: { '@id': `${CANONICAL_ORIGIN}/#organization` }
};

const application = {
  '@type': 'SoftwareApplication',
  '@id': `${CANONICAL_ORIGIN}/#application`,
  name: SITE_NAME,
  url: `${CANONICAL_ORIGIN}/`,
  applicationCategory: 'ProductivityApplication',
  operatingSystem: 'Web browser',
  description:
    'A shared workspace for research documents, spreadsheets and questionnaires with block-based version history. Teams draft in private, compare revisions, resolve overlapping edits and share selected changes into the team version.',
  image: SITE_IMAGE,
  license: 'https://www.gnu.org/licenses/agpl-3.0.html',
  isAccessibleForFree: true,
  featureList: [
    'Document editor with headings, lists, tables, images and comments',
    'Spreadsheet editor with formulas, multiple sheets and cell formatting',
    'Questionnaire and XLSForm editing',
    'Private drafts with deliberate sharing into the team version',
    'Block-level version history and revision comparison',
    'Guided resolution of overlapping edits',
    'Imports from DOCX, legacy DOC, XLSX and CSV',
    'Snapshot imports from ODK Central and KoboToolbox'
  ],
  publisher: { '@id': `${CANONICAL_ORIGIN}/#organization` }
};

const faqPage = {
  '@type': 'FAQPage',
  '@id': `${CANONICAL_ORIGIN}/#faq`,
  mainEntity: FAQ.map((entry) => ({
    '@type': 'Question',
    name: entry.question,
    acceptedAnswer: { '@type': 'Answer', text: entry.answer }
  }))
};

/**
 * `{@html}` writes this straight into the document, so close any sequence that
 * could end the script element early. The data is first-party and static, but
 * escaping keeps that guarantee local to this function.
 */
function scriptTag(data: unknown): string {
  const json = JSON.stringify(data).replace(/</g, '\u003c');
  return `<script type="application/ld+json">${json}</` + `script>`;
}

/** Graph for the homepage: site identity, the product, and the visible FAQ. */
export const structuredDataScript = scriptTag({
  '@context': 'https://schema.org',
  '@graph': [organization, website, application, faqPage]
});

/** Graph for a legal page, tied back to the same site and publisher. */
export function legalPageStructuredData(path: string): string {
  const meta = pageMeta(path);
  return scriptTag({
    '@context': 'https://schema.org',
    '@graph': [
      {
        '@type': 'WebPage',
        '@id': `${canonicalUrl(path)}#webpage`,
        url: canonicalUrl(path),
        name: meta?.title ?? SITE_NAME,
        description: meta?.description ?? SITE_DESCRIPTION,
        inLanguage: 'en',
        isPartOf: { '@id': `${CANONICAL_ORIGIN}/#website` },
        publisher: { '@id': `${CANONICAL_ORIGIN}/#organization` },
        dateModified: meta?.lastmod
      }
    ]
  });
}
