# Copy detection, roles, review rules, notifications and night mode — 30 September 2026

See [decision 009](decisions/009-copies-roles-and-notifications.md) for the design and
limits and the [plain-language guide](GIT-STYLE-WORKFLOW.md) for users, including a
test plan for 2–3 people. Publication and rollout are recorded at the end.

## Implemented

- **Copy detection** (`engine_core::similarity`, `engine_api::copies`): content
  fingerprints (exact hash, substantive-block containment, one-permutation densified
  MinHash with 32×4 LSH bands, normalised title keys); a background indexer outside
  the edit path; `document_match` pairs; owner notifications with the uploader and
  file name but never the content; `POST /similarity/check` for documents the uploader
  can open; owner actions invite / known / dismiss; **Share → Copies and similar
  files**; in-app copies recorded as linked; copies among documents that predate the
  release are listed but not announced.
- **Same name, different content**: server-decided `unrelated` change requests merge
  through `merge_strict`, where every change to an existing block is a `$block`
  choice; bulk **Keep all team paragraphs** / **Use all of theirs**; Home and the
  upload dialog warn first; pushes that match no saved revision start from the team
  version.
- **Roles**: Owner, Manager, Editor, Reviewer (new stored `approver`), Contributor,
  Viewer; people panel with capabilities, role guide, inline role changes and
  ownership transfer; workspace Reviewer role.
- **Review rules** (`document_policy`): protect the team version, 0–5 required
  approvals, who merges; approvals, requests for changes and per-change comments
  (`change_request_review`), stale approvals after the author's edits, audited owner
  override; protected-version banner and **Start a draft** for editors.
- **Notifications**: inbox with unread count on Home and in every file; nine kinds;
  deep links to requests, copies and **Send my copy as changes**.
- **Excel and PowerPoint push**: `structured-push.ts` planners, origin marker in
  downloads, upload menu items, Home recognition for all three kinds.
- **Day and night mode**: token palette with night values, pre-paint preference
  shared across hosts, switch on every surface, night-legible authored colours in
  documents and sheets, **View → White pages**, day colours for print and exports.
- **Administrator deletion**: one-step erase of active documents, batch erase with
  typed count, Documents-first admin list with Trash filter, per-row delete and
  multi-select.
- Migration `0021` (new file); no applied migration changed. OpenAPI regenerated.

## Verification (Windows, Rust 1.96, Node 24.12)

Isolated only: a disposable PostgreSQL 16 container on loopback 5549, an API on 8088
with throwaway keys and a production web build on 4188 with `DEV_AUTH=true`. No
hosted service or developer database was used.

| Check | Result |
| --- | --- |
| Rust | `cargo fmt --check` and `clippy -D warnings` clean; `cargo test --workspace` with `RUST_TEST_THREADS=1` (as CI): 194 passed, including the 100 ms SSE test |
| New Rust tests | 9 similarity unit tests (renamed/edited/unrelated, excerpts, bands, Bengali, stability, title keys, block extraction), strict-merge test, access tests, 7 API integration tests: renamed copy reaches the owner without revealing the original, original still being typed, widely copied content reaches every owner once, copies predating detection are listed not announced, roles/rules/approvals/override/transfer, unrelated same-name push, administrator one-step and batch deletion |
| Benchmark | 8,000 paragraphs / 192,000 words: 6.3 ms to fingerprint, 0.02 ms to compare ([raw](benchmarks/similarity-2026-09-30.json)) |
| Frontend | Prettier, ESLint, svelte-check 0 errors / 0 warnings; 186 unit tests in 23 files (16 night-colour helper tests, 24 Excel/PowerPoint push tests with real exceljs/pptxgenjs round trips, 4 upload-text/notification tests) |
| Browser, new | `e2e/copies.spec.ts`: renamed copy → owner notified → invite → **Send my copy as changes** → required approval → owner override merge → same-name unrelated file → **Replaces a team paragraph** → keep team → merge; protected team version sends an editor to a draft. Both pass, also on a database holding earlier runs' data |
| Browser, full suite | 52/54 on the first full run. The copy spec then failed because earlier runs' identical documents filled a five-notice cap (fixed: every pair recorded, each owner told once, with a regression test); a spreadsheet workflow test exceeded its 60 s limit under load and passes alone in 32 s |
| Visual | Day and night screenshots of Home, upload dialogs, notifications, people and rules, review, spreadsheet, slides, admin deletion and the public website |

Defects found and fixed during verification: an effect loop in the new night-ink
editor effect made documents read-only; detection could notify the uploader when the
original was still being typed; a push matching no revision started from the blank
first save instead of the team version.

## Known limits

- Detection needs at least 12 distinct three-word sequences; very short files are
  not compared. Pasting a newer document's text into an older document makes the
  older one look like the original.
- Heavily rewritten paragraphs, rows or slides can show as remove + add.
- Notifications are in-app; e-mail delivery is not implemented.
- The uploader's identity and file name are shown to owners of matching documents;
  the privacy page now says so, but the legal review in LEGAL-PAGES.md is still open.
- Not done: real-account acceptance with the pilot group, desktop Office opening of
  exported files.

## Publication and VPS rollout

Recorded below after the owner-authorized push and deployment.
