# 006 — Editable pages, Office workspace and standalone engine

Accepted direction: 22 September 2026, from the owner's current request and Word
screenshots. The owner separately confirmed control of the engine copyright and
explicitly authorized an MIT release named `ArefinAlter/dynodoc-engine`.

## Product priority

Editable pagination and broader Office editing are active product work. The earlier
handoff's P2 ranking and unresolved pagination component choice must not be treated
as reasons to keep deferring these features. Use the supplied Word screenshots as
layout references: a narrow navigation rail, templates, recent files, compact
ribbon groups, icon commands, a gray canvas, white pages and a small status bar.
Remove promotional copy from the workspace. Keep command names in accessible labels
and tooltips. Existing version-control tools remain reachable through header icons.

## Implementation

Keep one canonical ProseMirror document. `pagination.ts` measures browser line boxes
and table rows, then inserts noneditable view decorations to make page margins and
gaps. Automatic layout never inserts saved page nodes, splits stable paragraph IDs,
or adds history transactions. Explicit user page breaks remain saved content.

Recalculate after edits, page setup, font/image loads and resize, outside active IME
composition. A4/Letter/Legal, orientation, margins, plain repeating headers/footers,
page numbering, zoom and a continuous view are available. Rowspans stay together.
Indivisible oversized objects are reported and remain visible. Browser layout is
not Word's layout engine; print preview still uses Paged.js and can differ. Sections,
footnotes, floating objects, repeated table headers, widow/orphan controls and native
IME/device acceptance need further work. Large-document latency remains unproven.

XLSX archive parsing/conversion now runs in a terminable worker with the existing
input/expansion limits, a 45-second timeout and import cancellation. It returns
plain data, preserving empty row positions and remapping merge anchors to stable
IDs in the importing document. Numeric cell-value conditional rules are versioned
with cell formatting, respond to formula recalculation and are exported/imported
through the supported ExcelJS cell-is subset. Charts, pivots, protected ranges,
formula-based conditional rules, data bars and icon sets remain open.

## Open engine

The independent repository is https://github.com/ArefinAlter/dynodoc-engine and the
local checkout is `B:\dynodoc-engine`. Its initial commit is `6015014`. It contains
core/shared/API/CLI crates, all unchanged migrations, OpenAPI, local PostgreSQL
setup, its own rich-text fixtures and independent CI. It contains no frontend,
production configuration, user files, database dumps or credentials.

MIT applies to this extraction following the owner's explicit authorization.
The application repository's existing AGPL license is unchanged. Dependency licenses
remain independent. NOTICE records the origin and authorization. Community files
include conduct, contribution, security and governance policies, changelog, roadmap,
issue/PR templates, CODEOWNERS and Dependabot. Private vulnerability reporting is
enabled on GitHub.

The existing application retains its compatible engine copy. This release does not
silently change its production build/deployment layout. See ENGINE-EXTRACTION.md for
synchronization and the remaining dependency-boundary migration.

## Follow-through

Track Office work by executable acceptance criteria, not a complete-parity claim.
Next document work: section-local page setup and serialization; footnote/endnote
bodies; tracked formatting/structure; measured long-document reflow and native IME
acceptance. Next spreadsheet work: charts, pivots, formula-aware sorting and real
server-enforced protected ranges. Each requires saved/reloaded state, undo/review,
interchange rules and keyboard/accessibility checks.
