# 008 — File push, change requests, rebase, presentations and workspace fixes

27 September 2026. The owner asked, in one pass, for: an end-to-end Git-like
workflow testable through the GUI with real local Word files from several people;
a document "rebase"; a PowerPoint-style editor for group presentations; fixes for
reported UX defects (File menu dismissal, grid view names, ribbon alignment,
signed-out workspace access); and a plain-language walkthrough. This record states
what was built and where its boundaries are. It does not claim Git compatibility
or PowerPoint parity.

## Pushing a local file

Word downloads of the team version now carry their origin in two ways:

- custom document properties `DynodocDocument`, `DynodocRevision` and
  `DynodocExported`, and a download name `Title (v42).docx` where `v42` is the
  event sequence the copy represents;
- a hidden bookmark `_dd<ULID>` in the first paragraph of every block, `_ddt<ULID>`
  on document-tab headings and `_ddcomments` on the exported comments appendix.
  Word keeps hidden bookmarks through ordinary editing.

"Upload edited Word file" reads a `.docx`/`.doc`, decides which revision it started
from (recorded metadata, then the file-name convention, then the saved revision —
original upload, named versions or latest — that already contains most of its
paragraphs) and aligns the file with that revision's stable block IDs:

1. bookmarks identify blocks exactly; out-of-order bookmarks become moves;
2. between those anchors, unchanged blocks are matched with a Myers diff over a
   canonical form that ignores run splitting, inherited defaults, renumbered notes
   and similar Word re-save noise; untouched blocks are also compared with the
   revision's own Word round trip;
3. remaining blocks are paired by word and character-pair similarity, including a
   single block replaced in place; the rest are additions or removals.

Edited blocks drop defaults Word re-applied unless the block already used that
formatting, keep attributes Word cannot carry and restore checklist state when the
list shape is intact. The result is ordinary draft operations. Files without any
markers still work but rely on wording, so heavy rewrites can appear as remove plus
add. Page setup, headers and footers are not taken from a pushed file.

## Change requests

A change request is a personal draft submitted for review (migration 0020 adds
submission, source, outcome and reviewer columns; no applied migration changed).
`POST /documents/{id}/change-requests` creates the draft from the starting revision
and the file's operations atomically, optionally submitted. Submitted requests are
visible to every member; unsubmitted drafts stay private. Editors review a live
three-way comparison (ancestor, current team version, contributor), choose each
overlap, select changes and merge through the existing governance and integrity
path; unselected changes stay open. Editors can decline; contributors can withdraw,
keep editing and resubmit. Reviewers ("can comment & suggest") can push and submit
but not merge. Merged events are appended by, and attributed to, the editor who
merged; the contributor and event range are recorded on the request and in the
operations audit. Git's separate author/committer identities are not modelled.

## Rebase

`/drafts/{id}/sync` gains `action: "rebase"` with optional `dropped_nodes`. The
draft moves its base to the current team version, replays its own changes on top
(the existing three-way merge), and discarded changes return to the team content
through appended draft edits. Draft history is not rewritten. This is the document
equivalent of rebasing a branch, including interactive "drop"; there is no commit
reordering or squashing because drafts do not have separate commits.

## Presentations

A fourth kind, `presentation`: slides are `section` nodes and shapes are `item`
nodes with text, geometry and a style field, so the existing per-node, per-field
merge applies (separate shapes and text-versus-position edits combine; the same
field conflicts). The editor offers a slide rail, canvas editing, notes, themes and
a full-screen slide show; PPTX export uses the MIT `pptxgenjs` library (lazy
chunk) and PPTX import uses the existing bounded ZIP/XML guards. Mixed formatting
within one text box, rotation, tables/charts/SmartArt, animations and masters are
not reproduced; imports report this.

## Compatibility and fixes

- The editor header advances to `office-v5` so older cached editors cannot open a
  presentation as a questionnaire.
- Sibling order now uses code-unit comparison everywhere. Several readers used
  `localeCompare`, which reordered keys such as `W` and `j`: exported DOCX, LaTeX,
  XLSX sheets, public views and document tabs could appear out of editor order.
- `/workspace` and `/documents/*` redirect signed-out visitors to sign-in with a
  validated return path; the public host forwards them to the workspace host.
- Menus close on outside click, focus change or Escape; ribbon groups place their
  caption below the commands so captions never wrap over controls.
