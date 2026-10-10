# Office editing and the document-version-control pivot

Updated 22 September 2026. This records implemented behavior and explicit limits;
it is not a Microsoft Office compatibility certification. The public equivalent
is `/compatibility`. Decisions 004, 006 and 007 record the owner's expanded scope.
See [the latest release evidence](OFFICE-EXPANSION-RELEASE.md) for verification and runtime status.

## Document editor

- Home/Insert/Layout/References/Review/View ribbon; headings 1–6, font family/size, emphasis,
  colors/highlighting, lists/checklists, links, alignment, paragraph indentation,
  spacing, keep-with-next and direction. Existing table commands, equations,
  comments, document tabs, search/replace and context menus remain available.
- Page size (A4/Letter/Legal), orientation, uniform margins, plain headers/footers,
  page numbers, document language and explicit breaks are versioned root fields.
  The editable canvas now reflows across visible pages using browser-measured
  ProseMirror decorations; automatic layout preserves content IDs, undo and history.
  A continuous view remains available. An isolated Paged.js iframe produces
  preview/print/PDF independently of the live layout. Oversized tables/blocks and browser print settings can
  still affect output; this is not Word's layout engine.
- Text insertions/deletions inside existing paragraphs can be tracked, inspected,
  accepted and rejected. Deleting a pending insertion cancels that insertion.
  Structural edits are refused while tracking; disable tracking to change
  structure, which remains recorded in version history. Format revisions/moves
  and cross-paragraph redlines are not implemented. Review author labels are
  metadata; authenticated event actors remain the provenance authority.
- Composition-aware input defers saving and remote state replacement until IME
  completion, and avoids treating IME Enter as spreadsheet navigation. Pending spreadsheet cell edits also retain their original save base, so concurrent changes require review. Explicit
  LTR/RTL/automatic direction is supported. Self-hosted SIL-OFL Noto fonts cover
  Bengali, Arabic, Devanagari and Hebrew; shaping still uses the browser/platform.
  Native IME/device and assistive-technology acceptance remains necessary.
- Shared WOFF/WOFF2/TTF/OTF uploads are parsed by FontFace before saving; maximum
  2 MB/font, 16 families/document. Bytes use the existing authenticated PostgreSQL
  upload store, descriptors use versioned fields. Copies clone referenced font
  uploads. Word receives family names, not embedded binaries. Public copies strip
  private font descriptors. Microsoft fonts are not bundled.
- Labelled controls, keyboard ribbon navigation, document language and basic checks
  for heading order, image descriptions, table headers and empty links improve
  authoring accessibility. These checks do not certify accessible exported files.
- Editable footnote/endnote text with sequential references and a notes panel.
  Heading-based contents insertion and manual refresh. Native DOCX notes/TOC fields
  export; browser print places note bodies after the text.

## DOCX conversion

`docx-import.ts` parses native WordprocessingML in `docx-import.worker.ts`, with
45-second cancellation and transferable input. `bounded-zip.ts` enforces 3 MB (3,000,000 bytes)
input, 10,000 entries, 32 MiB per selected expanded part, 64 MiB selected total,
XML depth 128, no DTD/entity declarations, and duplicate-path rejection. Unselected
ZIP entries are not decompressed. Image import is limited to supported embedded
raster types (1 MB/image, 5 MB combined). File limits can reject otherwise valid
large Office documents; originals are retained on successful workspace import.

Resolve defaults → named/base styles → direct formatting. Preserve common fonts,
sizes/colors/emphasis, headings, direction/spacing/indent, list structure, table
spans/header/fill/alignment, links, raster images, text revision and comment marks.
Read the final section's page settings and plain header/footer text. Native export
writes supported content, text revisions and source comments using `docx`.

Explicit limits: exact page layout, floating objects/wrap, multi-section settings,
rich note formatting, per-page note layout, bibliography/citation fields, tracked formatting/moves,
native equation round trips, numbering restart/style fidelity and complete theme
resolution. Floating images are placed after their paragraph; complex layout and
unsupported constructs require original-file review. Source comments are imported
as marks, not full collaborative threads. Native block comments export in a notes
section rather than pretending to have precise character anchors.

## Spreadsheet editor

Preserve number/date/boolean/text types, formulas, common fonts/fills/alignment/
number formats, cell notes, validations, widths/heights, hidden rows/columns,
freeze panes and merged ranges through ExcelJS. HyperFormula recalculates changed
cells in a retained engine; style-only changes do not rebuild its graph. Relative
fill uses formula-aware references. Row/column changes rebase references; copying
sheets/documents remaps stable merge anchors. Merge refuses to discard nonempty
cells. Range navigation, selection summaries, inline list/number/text-length
validation, find/replace, resize/hide/freeze and full-visible-sheet print preview
are available. Printing derives all rows from state, not virtualized DOM rows.

Sorting adjusts relative formulas as rows move; external references stay attached
to cell addresses. Merged sheets must be unmerged before sorting. Named ranges,
text splitting, duplicate removal, chart/print/pivot source rebasing and data dialogs
are supported. Unsupported imported validation rules are retained
for export but not evaluated. Limits: 3,000 imported rows/workbook, 100 columns,
3 MB (3,000,000 bytes) source, 5,000 ordinary batch operations; UI operations use smaller guards.
XLSX ZIP expansion and ExcelJS conversion run in a terminable worker with a
45-second timeout and upload cancellation. Empty rows and merge anchors survive
conversion. Numeric greater-than, less-than, equal-to and between conditional
formatting rules are saved, recalculated and exchanged through XLSX. Rule output
is limited to RGB fill/text colors; formula-based rules, data bars, icon sets and
complex priority/style composition remain unsupported. Column/bar/line/pie chart
creation, resizing and supported rectangular same-sheet native XLSX exchange are
implemented. Pivot summaries support grouping, aggregates and explicit refresh;
their XLSX output is cells rather than native pivot caches. Whole-sheet protection
runs through engine governance; authors may unprotect. Protected cell ranges,
passwords, native pivot definitions, macros, external connections and full Excel
function/format parity remain incomplete.

## Presentation editor

Slides are `section` nodes and shapes are `item` nodes (text, geometry in percent,
one style field, optional image data up to 1 MB), so separate slides and shapes
merge independently. The editor has a slide rail with live thumbnails, canvas
select/drag/resize/in-place text editing, layouts, themes, backgrounds, speaker
notes, hidden slides and a full-screen slide show. PPTX export uses `pptxgenjs`;
import reads slide order and size, placeholders, text with basic formatting and
bullets, rectangles, pictures, backgrounds, notes and hidden slides. Not reproduced:
mixed formatting within one text box, rotation, tables, charts, SmartArt,
connectors, animations, transitions and custom masters.

## Word file push

Downloads write `DynodocDocument`/`DynodocRevision` custom properties, a
`Title (vN).docx` name and hidden `_dd…` block bookmarks. Uploading an edited copy
aligns it with the starting revision and creates reviewable draft changes; see
[decision 008](decisions/008-change-requests-presentations-and-ux.md).

## Versioning, performance and privacy

The semantic event schema is unchanged. Supported formatting and review metadata
participate in ordinary block history, drafts, three-way comparison and restoration.
Accept/reject appends changes; it never rewrites prior events. Snapshot sharing,
offline replicas, portable histories/remotes, persistent exclusion rules and live
CRDT collaboration remain outstanding, as described in VERSIONING-CONFORMANCE.md.

Immutable ProseMirror nodes reuse serialized block JSON; changes use copy-on-write
local state. This removes repeated serialization/cloning of unchanged block content
on ordinary typing. Save still diffs/materializes complete states; initial import
and snapshots remain full-state operations. No end-to-end 400-page latency guarantee
is inferred from classifier microbenchmarks.

Publishing refuses unresolved text revisions. Public projections remove private
inline Word comments, spreadsheet cell notes and shared-font upload descriptors,
in addition to existing private discussions/suggestions. Legacy published snapshots
are projected on read as well. Review metadata in private history remains intact.
New clients use `x-dynodoc-editor: office-v2`: earlier cached editors must copy any
unsaved work and reload so they cannot strip marks/settings they do not understand.

## Why these components

- [ProseMirror](https://prosemirror.net/docs/) provides the document model,
  transactions, composition handling and schema extensions already used in Dynodoc.
- [Tiptap Pages](https://tiptap.dev/docs/pages/getting-started/overview) is a separate
  commercial product. No license has been assumed. [Paged.js](https://github.com/pagedjs/pagedjs)
  supplies an open-source pagination preview. The live editable layout is now an
  in-repository ProseMirror decoration plugin; no commercial Pages dependency was
  adopted. See [decision 006](decisions/006-editable-pages-and-open-engine.md).
- [Tiptap's DOCX matrix](https://tiptap.dev/docs/conversion/getting-started/feature-support-matrix)
  illustrates that conversion requires feature-specific fidelity claims. Dynodoc
  implements its own documented OOXML subset rather than claiming converter parity.
- [ExcelJS](https://github.com/exceljs/exceljs) handles workbook interchange and
  [HyperFormula](https://hyperformula.handsontable.com/guide/supported-features.html)
  provides calculation; neither makes this UI a complete Excel replacement.
- [xmldom](https://github.com/xmldom/xmldom) 0.9.12 parses XML and
  [fflate](https://github.com/101arrowz/fflate) 0.8.3 enables bounded streaming unzip.
  Paged.js is pinned at 0.4.3. Dependency licenses remain applicable.

## Verification

Automated coverage includes native style inheritance with Bengali/Arabic text,
DOCX export/reimport for supported formatting/tables/lists/revisions/page settings,
ZIP expansion/depth/entity guards, text tracking acceptance/rejection/IME completion,
Excel typed/style/layout round trips, relative formula fill, safe merges and
incremental calculation. API tests cover removing private embedded review metadata.
Browser workflows exercise saved page setup, tracked changes/undo, real two-page
preview, worker DOCX import, formula fill, merge/freeze persistence and keyboard
navigation. Automated WCAG A/AA checks on tested views supplement, not replace,
manual keyboard/screen-reader/IME and real Office-file acceptance.

### Synthetic parser measurement

`pnpm bench:office` generates 8,000 paragraphs (two formatted runs each, Bengali/
Arabic text, a page-start flag every 20 paragraphs). A five-run local Node test
produced 8,000 blocks / 3,336,023 JSON bytes from a repetitive 26,520-byte ZIP.
Parse times were 716, 616, 642, 588 and 611 ms (median 616 ms). Process RSS after
five conversions was 607,129,600 bytes, including the Vite/Node process and unreclaimed
prior allocations; this is neither browser-worker peak memory nor a per-document
storage cost. Memory profiling remains necessary before claiming lightweight large-file
support. The fixture has no images or complex tables; 400 page-start flags do not
validate the pagination or end-to-end import of a real 400-page file.

### Historical release verification — 21 September 2026

For the editable-page increment, see [22 September verification](EDITABLE-OFFICE-RELEASE.md).

- 101 frontend unit tests passed; ESLint, formatting, Svelte/TypeScript checks and
  the production Node build passed.
- All 34 browser scenarios passed across the full run and focused reruns. The
  final seven-scenario run passed Word review/pages, spreadsheet formula/layout,
  native DOCX import, Bengali IME/concurrent-edit protection, offscreen-row printing,
  automated editor accessibility and reviewer suggestion acceptance. Additional checks passed merged-cell keyboard navigation and concurrent spreadsheet cell editing.
- 33 of 34 database-backed API tests passed, including the new publication guard
  and private review/font/note projection, hierarchy, restoration, compact replay
  and ten simultaneous independent edits. The existing test requiring an event
  write plus SSE delivery within 100 ms timed out on Windows/Docker, including
  its isolated rerun. Its threshold was not weakened. Functional SSE delivery is
  exercised by the passing concurrent-edit browser test; a 100 ms end-to-end SLO
  is not established by this release.
- Full Linux Rust CI passed for release commit `470abbc`, including all workspace
  tests and the strict 100 ms SSE check. This does not erase the Windows/Docker
  timing failures above or establish a production latency guarantee.
- Strict Rust clippy passed. No migrations or historical-event rewrites are added.

Native desktop/mobile IMEs, screen readers, real Google/Resend/provider accounts,
representative complex Office documents and production load/recovery acceptance
remain separate validation work. Automated browser composition events do not
simulate every operating-system input method.

## Editable-layout limits and acceptance

Long paragraphs can cross pages without splitting their saved structural node.
Tables paginate between rows; rows connected by rowspans are kept together.
Oversized images, equations and indivisible table-row groups remain visible and
show an oversized-content status. There is no repeated table-header, widow/orphan,
section-local margin, footnote-layout or Word-identical line-breaking guarantee.
Browser shaping and fonts affect layout. Native desktop/mobile IME, screen-reader
and representative long-document performance acceptance remain open.

The current tests exercise mixed Bengali/Arabic/Latin text across page boundaries,
undo, margins, reload and stable block IDs, table rows across pages without saved
layout nodes, and formula-driven conditional formatting across save/reload/undo.
See [the release record](EDITABLE-OFFICE-RELEASE.md) for the verification status.
