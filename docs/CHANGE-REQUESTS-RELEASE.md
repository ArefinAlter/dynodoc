# File push, change requests, presentations and workspace fixes — 27 September 2026

Local, uncommitted work on top of `4db90f3`. Not pushed or deployed. See
[decision 008](decisions/008-change-requests-presentations-and-ux.md) for design and
boundaries and the [plain-language guide](GIT-STYLE-WORKFLOW.md) for users.

## Implemented

- **Push a local Word file:** downloads carry origin metadata, the `Title (vN).docx`
  name and hidden per-block bookmarks; uploads are aligned with the starting
  revision and become draft operations. Home → Open recognises an edited copy of an
  existing document and offers to upload it as changes.
- **Change requests:** submit, list, three-way review with per-overlap choices,
  selective merge, decline, withdraw and resubmit. Migration `0020` (new file) adds
  the review columns; routes are in `crates/engine-api/src/reviews.rs`.
- **Rebase:** `action: "rebase"` with optional dropped changes, plus a dialog and a
  "the team version has N newer changes" banner in drafts.
- **Presentations:** new `presentation` kind with slide editor, notes, themes,
  slide show, PPTX import/export (`pptxgenjs`, MIT, lazy-loaded) and public view.
- **Fixes:** sign-in before `/workspace` and `/documents/*` with a validated return
  path (also through Google and email links); File menu dismisses on outside
  click/focus/Escape with arrow-key navigation; grid view shows full names and is
  remembered; ribbon groups reworked so captions never wrap or overlap and large
  icons align; References ribbon rebuilt; spreadsheet tabs are an accessible tab
  list; position ordering uses code-unit comparison (exports/tabs could be out of
  order before); editor header `office-v5`.

## Verification (Windows, Node 24.12, pnpm 11.7, Rust 1.96)

Isolated stack only: disposable PostgreSQL 16 container on loopback 5549, test API
on 8088 with throwaway keys, production web build on 4188 with `DEV_AUTH=true`. No
hosted service or the developer database was used.

| Check | Result |
| --- | --- |
| Frontend unit tests | 142 passed (20 files), including 8 push-alignment tests with real DOCX round trips and 13 presentation tests |
| Svelte/TypeScript, ESLint, Prettier, production build | 0 errors/warnings; all pass |
| Rust | fmt check and strict clippy pass; workspace tests pass except the strict 100 ms SSE timing test, which also fails in isolation on this Windows/Docker machine (known from earlier releases; code path unchanged) |
| New Rust integration tests | 2 passed: full change-request lifecycle (stale base, overlap, partial merge, decline, rebase with drop, privacy, roles, chain verification) and viewer/outsider denial |
| OpenAPI | regenerated; `openapi-check` equivalent reports no drift |
| Full browser suite | 48/50 on the first full run; the two failures (a spreadsheet tab-list axe violation and a timing-sensitive step in the new workflow test) were fixed and all 8 affected specs then passed |
| New browser specs | two-person push → review → overlap → merge → draft rebase → review → merge with chain verification; marker-less upload; sign-in redirect and open-redirect refusal; File menu; grid names; ribbon captions; presentation save/reload/PPTX; typing across autosave in team version and draft |

## Known limits and observations

- Uploaded files change content only: page setup, headers/footers and embedded Word
  comments are not taken from them. Heavily rewritten paragraphs in marker-less
  files can appear as remove + add.
- Merged events are attributed to the merging editor; the contributor is recorded
  on the request and in the operations audit.
- Presentations: one style per text box, no rotation, tables, charts, SmartArt,
  animations or masters; the Suggestions panel still lists questionnaire fields for
  decks.
- In one browser run, text typed immediately after a reload and draft creation was
  inserted mid-paragraph. A dedicated test typing across autosave in both modes does
  not reproduce it; watch for it in manual testing.
- `pnpm install` under pnpm 11 reports `ERR_PNPM_IGNORED_BUILDS` because of the
  existing `allowBuilds` placeholder in `pnpm-workspace.yaml`; unchanged here.
- Not done: deployment, real-account acceptance, desktop Word/PowerPoint opening of
  exported files, and the owner's walkthrough session.
