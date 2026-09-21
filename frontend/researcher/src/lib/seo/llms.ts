// llms.txt and llms-full.txt, per the llmstxt.org convention: a short, factual
// description of the product in Markdown that an answer engine can read without
// executing the marketing page's JavaScript.
//
// These are generated from the same config and FAQ as the rendered site, so
// they stay true as the site changes. Accuracy matters more than persuasion
// here: the limitations section exists so an engine answering "can Dynodoc do
// live co-editing?" gets the real answer rather than inferring one.

import { FAQ } from './faq';
import { CANONICAL_ORIGIN, INDEXABLE_PAGES, SITE_NAME, canonicalUrl } from './site';

const SUMMARY =
  'Version control for rich-text documents and spreadsheets, with familiar editing, personal drafts, reviewed merges and formatting-aware history.';

const CAPABILITIES = [
  'Document editing with ribbon tabs, headings, lists, links, tables, comments, page setup, print preview, direction controls and text revisions.',
  'Spreadsheet editing with formulas, multiple sheets, formatting, merged ranges, frozen panes, validation, fill operations and row and column tools.',
  'Questionnaire and XLSForm editing alongside documents and data.',
  'Personal drafts that can be compared against the team version before anything is shared.',
  'Block-level version history: paragraphs, questions and cells each carry their own record.',
  'Guided resolution when two people edit the same content.',
  'Imports from DOCX, legacy DOC, XLSX and CSV.',
  'Snapshot imports from ODK Central and KoboToolbox, including XLSForms and response snapshots.',
  'Export of supported documents and spreadsheets, and retrieval of retained original uploads.'
];

const LIMITATIONS = [
  'No live character-by-character co-editing and no live collaborator cursors. Collaboration works through personal drafts and deliberate sharing.',
  'Not a feature-for-feature replacement for Google Docs, Google Sheets, Word or Excel. Advanced charts, pivot tables and citation tools are not included.',
  'Imports are one-time snapshots. There is no scheduled sync and no writeback to the source system.',
  'An evolving document platform; no institutional compliance or regulated-data hosting agreement is claimed.',
  'The published privacy policy and terms are drafts pending confirmed operator details, and are not yet effective.'
];

const FACTS = [
  `Product: ${SITE_NAME}, a document version-control workspace.`,
  'Status: hosted application under active development, with documented compatibility limits.',
  'Hosting: a hosted application at dynodoc.online; the workspace runs at app.dynodoc.online.',
  'Licence: application code is open source under the AGPL-3.0-or-later. Private document content placed in the app does not become open source.',
  'Source code: https://github.com/ArefinAlter/dynodoc'
];

function bullets(lines: string[]): string {
  return lines.map((line) => `- ${line}`).join('\n');
}

function pageLinks(): string {
  return INDEXABLE_PAGES.map(
    (page) => `- [${page.title}](${canonicalUrl(page.path)}): ${page.description}`
  ).join('\n');
}

/** The short index: identity, links, and the shape of the product. */
export function llmsTxt(): string {
  return `# ${SITE_NAME}

> ${SUMMARY}

${SITE_NAME} gives a team one place to revise documents, maintain spreadsheets and compare alternatives, while keeping a readable record of how each piece of content changed and who proposed it.

## Pages

${pageLinks()}

## What it does

${bullets(CAPABILITIES)}

## What it does not do

${bullets(LIMITATIONS)}

## Facts

${bullets(FACTS)}

## Optional

- [Full description](${CANONICAL_ORIGIN}/llms-full.txt): the same summary with the complete FAQ.
- [Source code](https://github.com/ArefinAlter/dynodoc): the application repository.
`;
}

/** The expanded version: everything above plus the FAQ in full. */
export function llmsFullTxt(): string {
  const faq = FAQ.map((entry) => `### ${entry.question}\n\n${entry.answer}`).join('\n\n');
  return `${llmsTxt()}
## Frequently asked questions

${faq}
`;
}
