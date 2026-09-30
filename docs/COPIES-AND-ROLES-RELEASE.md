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
- Notifications are in-app; e-mail delivery is not implemented. (Invitations are
  emailed since `b70f048`; see below.)
- The uploader's identity and file name are shown to owners of matching documents;
  the privacy page now says so, but the legal review in LEGAL-PAGES.md is still open.
- Not done: real-account acceptance with the pilot group, desktop Office opening of
  exported files.

## Publication and VPS rollout

The owner authorized the push to `main` and deployment to Vercel and the VPS on
30 September 2026.

- `main` at `b0b9abb`, then `377152e` (adds the detection baseline). [Rust
  CI](https://github.com/ArefinAlter/dynodoc/actions/runs/36679131659) and [frontend
  CI](https://github.com/ArefinAlter/dynodoc/actions/runs/36679131728) pass for
  `377152e`, including the authenticated browser job with the new copy and protected
  draft specs. The earlier frontend run for `b0b9abb` was cancelled by the newer push.
- Vercel deployed the homepage automatically; `https://dynodoc.online` serves the
  theme script and the night logo.
- VPS (`/opt/dynodoc-demo`, SSH key access): validated backup
  `backups/dynodoc-20260930T063321Z.dump` (checksum OK, 21 tables listed). Prior source
  and manifest saved in `releases/copies-20260930-before`; prior images tagged
  `before-copies-20260930` (api `ed47a2101fc5`, web `e781abaa5970`, admin
  `d20bd725ce52`). Reviewed source archive SHA-256
  `645cd09c5e0292f451bd51c425d2311685fb87f86c265d271680093b4d346088`; all 455 files
  passed manifest verification; private environment file checksum unchanged. New
  images: api `8e7b68708aab`, web `9aabf2301797`, admin `1740b1cdcf2c`. The API was
  recreated first (migration `0021` applied on start; latest migration 21), then
  `web` and `admin` after frontend CI passed. PostgreSQL and the converter were not
  recreated.
- Data preserved: 1,258 events, 26 documents, 8 uploads, 7 accounts and 3 drafts,
  with identical event-chain (`13e56f7a…`), uploaded-file (`fd67b539…`) and access-list
  (`1477d285…`) digests before and after. The indexer fingerprinted all 13 live
  documents with content within seconds; 0 matches and 0 notifications, as the
  baseline intends for existing documents. All 13 unrelated containers kept their IDs.
- Live checks: workspace and admin health 200; provider flags unchanged (Google and
  email on, development sign-in off); signed-out `/workspace` → 303 to
  `/login?next=/workspace`; signed-out admin → 303 to login; anonymous API and
  similarity check → 401; the new routes (`/notifications/count`,
  `/similarity/check`, `/documents/{id}/people`, `/admin/erase-batch`) answer 401 inside
  the private network while an unknown route answers 404; the workspace serves the
  theme script. No synthetic account or document was created in production this time,
  so signed-in workflows were verified on the isolated stack and in CI, not live.
- Rollback: recreate `api`, `web` and `admin` from the `before-copies-20260930` tags.
  The older API ignores the new tables; the migrated `effective_document_role` still
  maps the new Reviewer (`approver`) rows to the engine's reviewer role, so access
  keeps working, but the older sharing panel would label those people "Viewer" and
  review rules would stop being enforced. Prefer a forward fix.

## Follow-up releases

Both on 30 September 2026, web and admin only: the API, database, migrations and
converter were not touched, so each rolls back by recreating `web` and `admin` from
its tag.

**Upload is visible on Home (`eed4cd5`).** The owner reported not finding an upload
option. Home now has an **Upload** tile first under New, an **Upload** label on the
side rail, an **Upload a file** button when the list is empty, and drag-and-drop of a
file anywhere on Home. Each upload runs the similarity check first and offers **Upload
as changes** for a copy of an existing file. Frontend CI passed
([run](https://github.com/ArefinAlter/dynodoc/actions/runs/36685178822)). VPS: backup
`backups/dynodoc-20260930T074227Z.dump`; prior images tagged `before-upload-20260930`
(web `9aabf2301797`, admin `1740b1cdcf2c`); new web `da006398f20a`, admin
`4e425fbbd277`.

**Invitation emails (`b70f048`).** The owner reported that people added by email
received nothing: sharing only created an in-app notice. The web server now has
`POST /api/invite`. It checks the origin and the session, asks the engine to grant
access (the engine alone decides who may share), and only if that succeeded sends an
invitation through Resend. The invitation says who shared which document or
workspace, the role in plain words, an optional note of up to 500 characters, the
link and how to sign in; reply-to is the person who shared. Names and notes are
stripped of control characters and escaped in HTML. No email is sent to yourself or
for removals, the same address is emailed at most once per item every 10 minutes, and
one person can send at most 30 an hour (in memory, so a restart resets the counts).
The sharing panel and the workspace access page have a **Send an email invitation
with a link** checkbox (on by default) and say afterwards whether the email was sent,
skipped, rate-limited, failed or not configured. A failed send never undoes the
access. Changing a role from the list does not email.

- Tests: 5 unit tests for the email content, escaping, cleaning and limits; 6 route
  tests (share-then-send with reply-to and link, nothing sent when sharing is
  refused, opt-out/removal/self, delivery failure keeps access, not configured,
  cross-site and bad input). Frontend CI passed, including the authenticated browser
  job ([run](https://github.com/ArefinAlter/dynodoc/actions/runs/36691331772)).
- VPS: backup `backups/dynodoc-20260930T084232Z.dump` (checksum OK); reviewed source
  archive SHA-256 `3215bc28a601c9b25c6f650c9653a4df5426bde6f4b8861fa9768c8d7fc91490`,
  all 468 files passed manifest verification; private environment file unchanged;
  prior images tagged `before-invite-20260930` (web `da006398f20a`, admin
  `4e425fbbd277`); new web `ef3d0e955e90`, admin `eca99bbb2e83`. Health 200 for both;
  signed-out `/workspace` → 303 to login; the built server contains the invite route
  and Resend settings are present in the web container. No invitation was sent from
  production during rollout; the first real delivery is the owner's own test.
- Limits: other notices (copies, reviews, merges) are still in-app only. Delivery
  depends on the Resend sending domain; invitations may land in spam until the
  domain has a sending history.
