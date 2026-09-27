# File push, change requests, presentations and workspace fixes — 27 September 2026

Released as [`9c1ec9d`](https://github.com/ArefinAlter/dynodoc/commit/9c1ec9d) on `main`
(on top of the owner's `90a759e`) and deployed to the VPS workspace on 27 September
2026; see [publication and rollout](#publication-and-vps-rollout). See
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
- Not done: real-account acceptance, desktop Word/PowerPoint opening of exported
  files, and the owner's walkthrough session.

## Publication and VPS rollout

The owner authorized commit, push and deployment on 27 September 2026.

- [Rust CI](https://github.com/ArefinAlter/dynodoc/actions/runs/36310645115) and
  [frontend CI](https://github.com/ArefinAlter/dynodoc/actions/runs/36310645101)
  pass for `9c1ec9d`, including the authenticated browser job with the new
  change-request, UX, presentation and typing specs, and the strict SSE test on Linux.
  The lockfile was also checked with pnpm 10 `--frozen-lockfile` before pushing.
- Vercel deployed the homepage automatically; `https://dynodoc.online/workspace` now
  redirects to the workspace host.
- VPS (`/opt/dynodoc-demo`): reviewed source archive SHA-256
  `a5844df978407ec2366b109aec7405283597cd0d429ed41e4c38066e38927178`; all 431 files
  passed manifest verification; private environment file checksum unchanged.
  Validated backup `backups/dynodoc-20260927T095509Z.dump`. Prior source and manifest
  are in `releases/changes-20260927-before`; prior images are tagged
  `before-changes-20260927` (api `5ab6fd9fc7e2`, web `fa534f6a7bce`, admin
  `de88ea54725e`). New images: api `ed47a2101fc5`, web `e781abaa5970`, admin
  `d20bd725ce52`. Only `api`, then `web` and `admin`, were recreated; PostgreSQL and the
  converter were not.
- Migration `0020` applied on API start (17 migrations, latest 20). All 1,029
  pre-release events and chain hashes, 20 documents, 7 uploads with content hashes and
  6 accounts matched their pre-release fingerprints. All 13 unrelated containers
  (`server-*`, `verne-marketing-*`, `chakma-*`) kept their IDs. Two new documents
  created by an existing account at 10:03 UTC, after the web rollout, account for the
  only other new events.
- Live checks: workspace/admin health 200; provider flags unchanged (Google and email
  enabled, development sign-in disabled); signed-out `/workspace` and document URLs
  return 303 to `/login?next=…`; anonymous API access returns 401; admin sign-in is
  unaffected. A synthetic account ran 14 API checks inside the private network
  (history points, push from an older revision, listing, comparison, merge that keeps
  the newer team edit, rebase with a dropped change, submit/withdraw, chain
  verification) on one synthetic document, which was then moved to Trash; the
  synthetic account was then disabled with its sessions revoked. No existing document
  was edited.
- Rollback: recreate `api`, `web` and `admin` from the `before-changes-20260927` tags.
  The older API ignores the new nullable/defaulted review columns, but old editors
  would open presentations as questionnaires, so prefer a forward fix. Open tabs
  must reload for `office-v5`.
