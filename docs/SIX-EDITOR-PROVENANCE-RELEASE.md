# Six-editor provenance continuation — 6 October 2026

Later same-day [date-only follow-up](SPREADSHEET-DATE-SYNC-RELEASE.md) is now live
at `8c7461d`. The evidence below records the preceding six-editor rollout.

This release extends the primary external-editor workflow to Excel/Google Sheets
and PowerPoint/Google Slides alongside Word/Google Docs. The web editors stay;
building a competing Office suite is not the product priority.

## Implemented

- Shared field-level capture/retry/review/pull/recovery contract. Cells in one row
  can merge independently. Paragraph bundles remain compatible.
- Excel and PowerPoint development task panes and sideload manifests, requiring
  stable ExcelApi 1.4 / PowerPointApi 1.4. Scoped, opt-in capture persists locally;
  preview writes recheck the working copy and local values.
- Sheets and Slides Apps Script sidebar installations, sharing the generated
  protocol, private credentials and compare-and-swap queue storage. Sheets anchors
  rows with named ranges; Slides maps native IDs privately without changing alt text.
- All six key options, file-kind validation and migration 0024; existing scopes,
  retry receipts, revocation and append-only history stay in place.
- Existing Project creation, files/requests/own drafts, members, inherited review
  rules and watchers are included in the verified VPS rollout below.

## Verification and rollout

Implementation: `348bef9`; browser harness correction: `9cc782c`; pane framing
configuration: `0fb783f` / `f8fd6a0`. Deployed source: **f8fd6a00a92fb90bd1fb79b370f0fdd08ce526b7**.
Standalone engine code: `d6666a2`; capability/roadmap evidence: `4c067f3`.

- [Frontend CI 37430487141](https://github.com/ArefinAlter/dynodoc/actions/runs/37430487141)
  passed 253 unit tests, lint/format/build/artifact checks, 15 public browser tests
  (46 authenticated tests intentionally skipped there), and **all 61 signed-in
  browser tests without skips**. The first run exposed a test race in which a late
  Office mock could replace the Google bridge's edit hook; `9cc782c` isolates the
  Google tests. Twelve repeated local checks passed before the successful rerun.
- [Application Rust CI 37429675981](https://github.com/ArefinAlter/dynodoc/actions/runs/37429675981)
  passed formatting, Clippy, build and all 200 tests. Runtime Rust code is unchanged
  by the subsequent test/proxy commits.
- [Engine CI 37430229717](https://github.com/ArefinAlter/dynodoc-engine/actions/runs/37430229717)
  and [final engine CI 37430925686](https://github.com/ArefinAlter/dynodoc-engine/actions/runs/37430925686)
  passed all 200 tests, formatting, Clippy, build, docs and OpenAPI consistency.
  Local `task lint`, `task test`, `task docs`, `task openapi` also passed; existing
  Rustdoc link warnings remain. Generated OpenAPI is unchanged.
- All six connector browser checks passed locally against the production build
  with mocked host bridges. Native adapter unit tests also use synthetic services;
  neither establishes acceptance in an installed Microsoft/Google editor.

## VPS rollout and live verification

Activated 6 October 2026, verified at **07:49:20 UTC**, in `/opt/dynodoc-demo`.
Existing SSH key access was used. No supplied password, production environment,
user file or database content is included in source artifacts or these records.

- Validated backup: `backups/dynodoc-20261006T072921Z.dump`, pg_restore listing and
  SHA-256 check passed. Earlier backups were retained.
- Prior source/manifest and release logs: `releases/six-editors-348bef9-20261006`.
  Old API/web/admin images have `before-provenance-20261006` tags. Previous images:
  API `8e7b68708aab`, web `ef3d0e955e90`, admin `eca99bbb2e83`.
- Final archive contains 524 source files; SHA-256
  `a34a6661182fb27797147688f2c6d38da08f96b821253aa460c6ed2f635f703e`.
  Every manifest entry passed verification. Applied migration bytes and the private
  environment file were checked unchanged before activation.
- Recreated only API, web and admin. Migrations **0022, 0023, 0024** applied on API
  startup; latest schema is 24. Current images: API `f245f0870d96`, web
  `25f9c5dc1904`, admin `7bc50691cce9`.
- PostgreSQL, converter and all **13 unrelated containers** retained their IDs.
  All other Nginx site files retained their checksums. Only Dynodoc's app virtual
  host changed; Nginx syntax checks passed and reload was graceful.
- Database counts stayed **9 accounts, 2 documents, 24 events, 0 uploads, 0 drafts**.
  Whole event-chain, uploaded-file and access-list aggregate digests matched before
  and after. This is a preservation check, not public disclosure of document data.
- HTTPS workspace/admin health and Word/Excel/PowerPoint panes return 200.
  Signed-out Projects redirects to login (303); private Projects/provenance/key
  endpoints reject anonymous calls (401). Google/email sign-in remain enabled;
  development sign-in remains disabled. No production email, synthetic account or
  document was created. Signed-in workflow evidence is CI, not a live user session.
- Pane URLs alone use a Microsoft ancestor allowlist through CSP; login/workspace
  retain SAMEORIGIN framing protection. Live Chromium checks render Word, Excel
  and PowerPoint panes under mocked Office/Cloud/SharePoint parent origins and
  block an untrusted origin. These checks replace the Office bridge, not native
  acceptance. Microsoft documents the [web iframe runtime](https://learn.microsoft.com/en-us/office/dev/add-ins/concepts/privacy-and-security#web-clients).
- Vercel remains the separate homepage deployment; no VPS database or API is moved
  to Vercel. This rollout uses the existing workspace/admin HTTPS hosts.

Rollback images and the original app Nginx file (`nginx-app.before` in the release
folder) are retained. Prefer a forward fix: the older API/UI do not implement
Projects, inherited review rules or connector security behavior. Image rollback
alone is not functional compatibility with newly created Projects. Keep the new
schema/data; do not drop migrations or restore over subsequent writes without a
separate recovery decision.

## Boundaries and next work

These are development connectors, not marketplace publications. Automated native
adapter tests and browser bridges use mocks. Real Word/Docs, Excel/Sheets and
PowerPoint/Slides host acceptance remains open.

Supported exchange is existing paragraph text, spreadsheet cells/formula source,
and slide shape text. Structural/formatting sync, date cells, merged cells, slide
groups/tables/media/animations, durable closed-pane capture and complete cross-editor
formula/layout compatibility remain open. Remote structural or formatting changes
stop incremental pull and require file push. Queues observe snapshots; they do not
prove authorship or record every keystroke. Review/merge remains in Dynodoc Projects.

See [decision and priorities](decisions/011-six-editor-provenance.md),
[installation](../extensions/README.md) and [sync contract](PROVENANCE-SYNC.md).

Snapshot limitation: sorting or pasting complete spreadsheet rows can appear as
multiple cell-value changes when the host retains named range coordinates.
This increment does not reconstruct the original edit command or guarantee
recognition of every structural action; use file push for those operations.
