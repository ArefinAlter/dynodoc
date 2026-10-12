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
  'Version control and provenance for Word, Google Docs, Excel, Google Sheets, PowerPoint and Google Slides: every change is captured, reviewed as a change request, merged and recorded in an append-only, hash-chained history.';

const CAPABILITIES = [
  'Add-ins for Word, Excel and PowerPoint (task panes) and Google Docs, Sheets and Slides (Apps Script) that capture edits to existing paragraph text, cell values and formulas, and slide text while the pane is open.',
  'Projects that group related DOCX, XLSX, PPTX, CSV and Markdown files with members, roles and shared rules.',
  'Private drafts per file, change requests with word-level diffs, comments, required approvals and an audited override.',
  'Project policy floors: a file can tighten the protection rules of its project but never weaken them.',
  'Named versions, history browsing, and recovery by restoring, reverting or replaying a past change as a new commit.',
  'An append-only, hash-chained event log with stable node identities, so each paragraph, cell and slide keeps its identity across edits.',
  'Signed project commits and ref receipts in the open-source engine and CLI, verifiable with device keys.',
  'Basic web editors for documents and spreadsheets inside the workspace.',
  'Export of supported documents and spreadsheets, and retrieval of retained original uploads.'
];

const LIMITATIONS = [
  'No character-by-character co-editing and no live collaborator cursors. Collaboration works through drafts, change requests and deliberate merges.',
  'The add-ins capture only while their pane is open: there is no background agent. They do not capture every keystroke, layout change, embedded object or animation; use edited-file push for those.',
  'The add-ins are test installs. No Microsoft AppSource or Google Workspace Marketplace listing exists, and installed-editor acceptance is not complete.',
  'Tamper-evident, not tamper-proof: detecting a wholesale rewrite by a database administrator needs a hash or backup retained independently. External witnesses are planned, not shipped.',
  'Not a replacement for Word, Excel, PowerPoint or the Google editors, and no compliance certification or regulated-data hosting agreement is claimed.',
  'Original file uploads are limited to 3 MB (3,000,000 bytes) during testing, and large-file performance is not yet accepted.',
  'The published privacy policy and terms are drafts pending confirmed operator details, and are not yet effective.'
];

const FACTS = [
  `Product: ${SITE_NAME}, a document provenance, review and synchronization platform.`,
  'Status: early software under active development; hosted test deployment with email and Google sign-in, no paid plan.',
  'Hosting: a hosted application at dynodoc.online; the workspace runs at app.dynodoc.online.',
  'Licence: application code is open source under the AGPL-3.0-or-later; the standalone engine is MIT. Private document content placed in the app does not become open source.',
  'Self-hosting: PostgreSQL 16, Rust and Node; see SETUP.md. Commands: task setup, task migrate, task dev.',
  'Source code: the application source archive is at https://dynodoc.online/source; the standalone engine is https://github.com/ArefinAlter/dynodoc-engine'
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

${SITE_NAME} sits beside the editors a team already uses. It records what changed in each document, spreadsheet and presentation, who proposed it and who approved it, and keeps that record in a log that can be verified.

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
- [Source archive](https://dynodoc.online/source): the application source under AGPL-3.0-or-later.
- [Standalone engine](https://github.com/ArefinAlter/dynodoc-engine): the MIT engine repository.
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
