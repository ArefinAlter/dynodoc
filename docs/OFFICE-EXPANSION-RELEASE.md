# Office editing expansion — 22 September 2026

The owner authorized this implementation and pushing it to `main`. It includes the
previous editable-page/Word-workspace commit and this Excel/reference increment.
Full Microsoft Office parity is not complete.

## Implemented

- Excel-style compact Home, Insert, Page Layout, Formulas, Data, Review and View
  ribbons; icon commands, formula/name box, thin grid, green selection, sheet tabs,
  selection statistics and zoom. Blank workbooks start with 100 rows and 30 columns.
  The last active worksheet is remembered in this browser.
- Fonts/emphasis/colors/borders, horizontal/vertical alignment, wrap, merge,
  decimal/percent/currency/date/scientific formats, decimal precision, cell styles,
  format painter, notes/hyperlinks, fill, find/replace, freeze, hide/unhide and dimensions.
- Live column/bar/line/pie charts, editing, dragging, resizing, deletion and print.
  Up to 12 charts per sheet, 200 categories and 8 series. Supported rectangular,
  same-sheet charts use native XLSX chart/drawing parts with references and caches.
- Pivot summaries with row and optional column grouping; sum, count, average, min,
  max and explicit refresh. Surviving groups retain node IDs. Native XLSX pivot
  definitions/caches are not implemented; export contains the result cells.
- Workbook names, formula-aware row sorting, custom header handling, Text to Columns,
  duplicate row removal and validation dialogs. Structural changes also rebase names
  and chart/print/table/pivot metadata. External references remain address-based when
  sorting; they are not record-ID lookups.
- Sheet page setup, print areas, XLSX print/protection metadata, and server-checked
  whole-sheet editing protection. Authors can unprotect; this is not password security.
- Word References ribbon: editable footnote/endnote text, sequential numbering,
  notes panel, heading-based contents insertion/update, native DOCX note parts and TOC
  field. Browser print includes note bodies after the text. Existing live editable
  pagination and stable paragraph IDs remain in place.
- `office-v4` compatibility gate for the new document content types. No schema or
  historical-event migration. Standalone engine governance fix is synchronized into
  the application; see [extraction notes](ENGINE-EXTRACTION.md).

## Verification

- Frontend: 120 unique unit tests pass (119-test full suite, followed by the additional note identity regression). ESLint and full Prettier checks pass; Svelte/TypeScript reports zero errors and warnings. Production build passes.
- Browser: all 33 scenarios pass across the broad production-build run (31 passes) and the focused final rerun (3 passes, one overlapping). The final rerun verifies the relocated print command, duplicate removal on Svelte state, named formulas, charts, actual pivot refresh, protection and reload. Other coverage includes concurrent edits, live page geometry, undo/identity, imports, permissions, drafts, recovery and accessibility.
- Standalone engine: all 173 tests pass locally. [Independent CI](https://github.com/ArefinAlter/dynodoc-engine/actions/runs/35698213107) also passes Rust formatting, strict Clippy, build, tests, documentation and OpenAPI consistency for engine commit `02a7aa7`. The app contains byte-identical ports of the two changed Rust files. All 16 SQL migration files remain byte-identical between repositories.
- Verification uses Windows Node 22.22.2, pinned Rust 1.96, a disposable PostgreSQL 16 container on loopback 5549, test-only auth/API on 8088 and a production web build on 4188. No hosted data is used. The existing development database is unchanged.
- Synthetic 1920-pixel captures: [Excel Home ribbon](review/excel-editor-2026-09-22.png), [workbook home](review/excel-home-2026-09-22.png), [chart](review/excel-chart-2026-09-22.png). Supplied screenshots are reference material, not repository instructions or app assets.

The first broad browser run exposed an obsolete print selector and a Proxy cloning
error in duplicate removal; both were corrected and the affected workflows rerun.
The runtime on the VPS has not been replaced by this Git publication. It needs both
the new web bundle and API governance change. Vercel and application CI status are
recorded after the main push; publishing source does not certify a VPS rollout.

## Remaining gaps

This is not a complete Office implementation or a certification of exact file fidelity.
Remaining document work includes section-local page layout, live footnotes at the
bottom of each page, floating objects, rich notes, citations/mail merge, native Word
equation exchange, tracked formatting/moves, and exact line/page matching with Word.
The table of contents needs manual refresh; Word can compute page numbers on export.
Browser print still uses Paged.js and can differ from editing layout.

Remaining workbook work includes native pivot caches, structured table references,
advanced charts, VBA/macros, external connections, protected cell ranges/passwords,
advanced conditional formatting, full Excel formula/date/locale semantics, rich
filter menus and Excel-scale workbooks. Format as Table currently applies a banded
range style and XLSX auto-filter; it does not implement structured table formulas.
Existing limits remain: 10 MB uploads, 3,000 rows per workbook import, 100 columns per
sheet, 5,000 edit operations per normal batch, with smaller fill/format limits.

Automated interchange checks do not substitute for opening exported files in desktop
Office. Native IME/device testing and representative 300–400-page performance acceptance
remain open. Real external-provider acceptance, backup and legal-operator requirements
from the handoff are also not resolved by this editor release.
