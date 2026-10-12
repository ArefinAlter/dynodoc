// JSON-LD for the public pages. Everything here is static, first-party product
// metadata: no visitor, session or document value is ever interpolated.
//
// Answer engines lean on this graph more heavily than classic search does, so
// it describes what Dynodoc is, rather than only carrying keywords.

import { FAQ } from './faq';
import {
  CANONICAL_ORIGIN,
  SITE_DESCRIPTION,
  SITE_IMAGE,
  SITE_NAME,
  canonicalUrl,
  pageMeta
} from './site';

const REPOSITORY = 'https://github.com/ArefinAlter/dynodoc-engine';

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
    'Version control and provenance for Word, Google Docs, Excel, Google Sheets, PowerPoint and Google Slides. Changes are captured by editor add-ins, reviewed as change requests, merged under project rules and recorded in an append-only, hash-chained history.',
  image: SITE_IMAGE,
  license: 'https://www.gnu.org/licenses/agpl-3.0.html',
  isAccessibleForFree: true,
  featureList: [
    'Add-ins that capture edits in Word, Excel, PowerPoint, Google Docs, Google Sheets and Google Slides',
    'Projects that group DOCX, XLSX, PPTX, CSV and Markdown files',
    'Change requests with word-level diffs, approvals and project policy floors',
    'Private drafts, named versions and recovery by restore, revert or replay',
    'Append-only, hash-chained event log with stable node identities',
    'Signed project commits and ref receipts in the open-source engine and CLI',
    'Self-hosting with PostgreSQL under AGPL-3.0'
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
export function scriptTag(data: unknown): string {
  const json = JSON.stringify(data).replace(/</g, '\\u003c');
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
