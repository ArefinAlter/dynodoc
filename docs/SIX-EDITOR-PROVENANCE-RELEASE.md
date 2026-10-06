# Six-editor provenance continuation — 6 October 2026

This release extends the primary external-editor workflow to Excel/Google Sheets
and PowerPoint/Google Slides alongside Word/Google Docs. The web editors stay;
building a competing Office suite is not the product priority.

## Implemented

- Shared field-level capture/retry/review/pull/recovery contract. Cells in one row
  can merge independently. Paragraph bundles remain compatible.
- Excel and PowerPoint development task panes and sideload manifests, requiring
  stable ExcelApi 1.4 / PowerPointApi 1.4. Scoped, opt-in capture persists locally;
  preview writes recheck the working copy and local values.
- Sheets and Slides Apps Script sidebar installations, sharing the generated
  protocol, private credentials and compare-and-swap queue storage. Sheets anchors
  rows with named ranges; Slides maps native IDs privately without changing alt text.
- All six key options, file-kind validation and migration 0024; existing scopes,
  retry receipts, revocation and append-only history stay in place.
- Existing Project creation, files/requests/own drafts, members, inherited review
  rules and watchers are included in the pending VPS rollout.

## Verification and rollout

Local checks passed: 253 frontend unit tests, all six connector browser checks
against the production build (mock native bridges), all five Projects/provenance
engine integration tests, formatting, Clippy, ESLint, zero Svelte errors/warnings,
production build and shared Google artifact consistency. CI and VPS rollout
evidence will be recorded below once completed.

## Boundaries and next work

These are development connectors, not marketplace publications. Automated native
adapter tests and browser bridges use mocks. Real Word/Docs, Excel/Sheets and
PowerPoint/Slides host acceptance remains open.

Supported exchange is existing paragraph text, spreadsheet cells/formula source,
and slide shape text. Structural/formatting sync, date cells, merged cells, slide
groups/tables/media/animations, durable closed-pane capture and complete cross-editor
formula/layout compatibility remain open. Remote structural or formatting changes
stop incremental pull and require file push. Queues observe snapshots; they do not
prove authorship or record every keystroke. Review/merge remains in Dynodoc Projects.

See [decision and priorities](decisions/011-six-editor-provenance.md),
[installation](../extensions/README.md) and [sync contract](PROVENANCE-SYNC.md).
