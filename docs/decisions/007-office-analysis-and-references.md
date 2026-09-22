# 007 — Office analysis, references and main-branch release

22 September 2026. The owner requested broad Word/Excel implementation in one pass,
using the supplied Excel screenshots, and explicitly authorized pushing to `main`.
This supersedes earlier local-only release wording and the old P2 deferral. The
request establishes product direction; it does not establish that full Office
parity is achieved.

The spreadsheet now has Excel-style compact ribbon groups, a formula bar, thin
20 px rows, 64 px default columns, green selection, bottom sheet tabs and zoom.
Commands perform real edits. Analysis includes charts, refreshable pivot summaries,
workbook names, relative-formula sorting, text splitting, duplicate removal,
validation, formatting, print areas and whole-sheet protection.

Charts are versioned worksheet fields. Standard DrawingML parts provide supported
native XLSX chart exchange; the browser renderer uses computed worksheet values.
Pivot outputs are ordinary stable-ID rows plus a saved source definition; refresh
preserves surviving group IDs. XLSX exports pivot results as cells, not native pivot
caches. Structural changes rebase saved chart/print/table/pivot references. Deleting
a pivot source field invalidates its definition so refresh fails explicitly.

Sorting uses HyperFormula row permutation: moved relative formulas adjust with their
row. External formulas and named ranges continue to refer to their cell addresses,
as opposed to following a record identity through a sort. Insert/delete/rename
operations rebase ordinary formulas and workbook names. Merge/split and bulk edits
keep the existing operation limits.

Whole-sheet protection is a governance check at the existing locked write boundary,
including suggestion acceptance and structural operations. Authors may explicitly
unprotect; reviewers may still discuss and propose. This is an accidental-edit guard,
not passwords, protected cell ranges or an alternative access-control model. Replay
and event payloads are unchanged. The fix originates in the MIT standalone engine
and is ported to the application.

Word references use inline note nodes with stable note IDs and text bodies. The
browser provides numbered references and a notes panel; DOCX uses native footnote
and endnote parts. Browser print collects note bodies after the document. A table of
contents is a saved heading cache with manual refresh; DOCX exports a native TOC
field that Word can update. Note rich formatting, live per-page footnote placement,
section-local layout and full field fidelity remain separate engineering work.

The editor compatibility header advances to `office-v4`, preventing older browser
bundles from writing documents containing the new node types. No migration changes.
See [release evidence and limitations](../OFFICE-EXPANSION-RELEASE.md).
