# Editable Office workspace and standalone engine — 22 September 2026

This records local implementation and verification. The application changes have
not been deployed; they are on local branch `feat/editable-office-workspace`. The standalone engine is publicly published under MIT at
https://github.com/ArefinAlter/dynodoc-engine; the application remains AGPL.

## Implemented

- Word-style home: narrow navigation rail, working blank/template actions,
  recent-file rows, type filters, search, sorting, Trash and expandable team/import
  options. Promotional workspace copy and the unsolicited tour popup are removed.
- Compact icon ribbon: actual formatting, paragraph, style, clipboard, insertion,
  review, page setup and view actions, with accessible names and tooltips. Help is
  integrated in the header. Editor tabs and page/word/zoom status sit below content.
- Live editable pages: browser-measured paragraph lines and table row groups reflow
  inside one canonical document. Display decorations add page gaps without changing
  saved blocks, IDs, undo or event history. Resize, edits, font/image loading and
  page setup trigger reflow outside active composition. Continuous view remains.
- XLSX conversion runs in a terminable worker, with bounded archive expansion,
  45-second timeout and cancellation. Empty row positions and merge anchors survive
  import. Numeric conditional formatting supports greater-than, less-than, equality
  and between rules, formula results, saved formatting events and XLSX exchange.
- Independent engine: four Rust crates, unchanged migrations, OpenAPI, standalone
  replay fixtures, MIT license, origin notice, contribution/conduct/security and
  governance policies, issue/PR templates, CI, Dependabot and local setup.

## Visual review

The supplied Word screenshots were read from the mounted Windows Downloads folder.
These are actual application captures at 1920×1080, using synthetic local test data:

- [Workspace](review/word-workspace-2026-09-22.png)
- [Document editor](review/word-editor-2026-09-22.png)

## Verification

The tests use a disposable PostgreSQL 16 container on loopback port 5549, test-only
authentication, the independent engine on 8088 and a production app build on 4188.
They do not access the hosted service or user data. Windows Node 22.22.2 and the
pinned Rust 1.96 toolchain were used because WSL does not have native Node/Cargo/Task.
Equivalent project script commands were run directly. Temporary test servers and
the disposable database container were removed after verification; the existing
local development database was not changed.

- Frontend unit suite: 108 tests pass, including pagination planning and XLSX
  formatting/merge/blank-row round trips.
- ESLint: passes. Svelte/TypeScript: 0 errors and 0 warnings. Production build: passes. Full frontend Prettier check: passes.
- Authenticated browser acceptance: all 30 scenarios pass across the stable-build
  run (16 passes) and focused rerun (14 passes). Coverage includes live page geometry,
  stable IDs/undo/reload, tables, mixed-script composition and concurrent edits,
  formula-driven conditional formatting, DOCX/XLSX/XLSForm workers and round trips,
  sharing/drafts, frozen public copies, admin erasure, cross-tab recovery/sign-out,
  mobile workspace containment and automated accessibility checks.
- Standalone engine: all 171 Rust tests pass on Linux, including the unchanged
  100 ms SSE assertion with serial database tests. Formatting, strict Clippy, build
  and documentation pass. OpenAPI was regenerated to reflect MIT metadata after
  the initial consistency check identified the inherited AGPL fields. The final
  generated OpenAPI is byte-identical to the checked-in specification.
- Published [v0.1.0 source prerelease](https://github.com/ArefinAlter/dynodoc-engine/releases/tag/v0.1.0) at `5cd3a1b`. [Passing independent CI run](https://github.com/ArefinAlter/dynodoc-engine/actions/runs/35691910746).

The first parallel database test run missed the strict SSE timing threshold on both
Windows/Docker and Linux. Serializing database tests avoids concurrent database
creation during that check; it does not establish a production latency guarantee.
Local engine tests also passed by suite, with an isolated retry for the timing test.
A later Windows full-suite retry encountered a locked running API executable; the
complete Linux run supplies the consolidated engine verification.

Earlier development-server runs included hot-reload navigation failures. Stable
production-build reruns are the acceptance evidence. Browser fixtures were updated
for the new controls and use generated ULIDs, so repeated runs do not collide with
nodes from a previous run.

## Remaining acceptance and features

This is not complete Word/Excel parity. Sections, footnotes/endnotes, floating
objects, repeated table headers, widow/orphan control, structural/formatting
redlines and precise Word layout interchange remain. Oversized indivisible objects
stay visible and are reported; print layout uses a separate Paged.js path and may
differ. Real native IME/device, screen-reader and 300–400-page performance acceptance
remain unproven.

Spreadsheet charts, pivots, protected ranges, formula-aware sorting, advanced
conditional rules/priority composition and full formula compatibility remain.
Imported unsupported Office features still require reviewing the conversion report.

The application retains a compatible engine source copy. Extraction is independently
buildable and published; replacing the application build dependency is separate
work described in [ENGINE-EXTRACTION.md](ENGINE-EXTRACTION.md). No production deployment,
crates.io package, signed binary or external security audit is implied by this record.
