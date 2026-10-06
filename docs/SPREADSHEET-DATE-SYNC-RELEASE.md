# Date-only provenance continuation — 6 October 2026

This increment extends the existing Excel/Google Sheets connectors with checked
date-only observations and pulls. Word/Docs and PowerPoint/Slides keep the same
shared queue, review, retry and recovery contract. Dynodoc remains the Projects
provenance platform around native editors.

## Implemented

- Canonical civil-day values use ISO midnight UTC; blanks and formula source stay
  distinct. Calendar validation rejects invalid leap days, locale strings and
  timestamps. Recovery checks every date observation, including superseded ones.
  Native General/text/time and ambiguous custom date masks require file push;
  changing between recognized date masks does not capture a formatting operation.
- Excel uses native YEAR/MONTH/DAY/DATE functions and verifies both conversions
  before writing. No epoch/timezone is guessed. Sheets uses its spreadsheet
  timezone and verifies local midnight before writing; timezone changes stop apply.
- Native values/identities are rechecked after date preflight. Concurrent typing,
  competing dates, invalid conversions and unsupported structure stop the update.
- Excel now checks native merged areas before binding or writing, requiring
  ExcelApi 1.13. PowerPoint remains 1.4. Blank sheets' bounds no longer treat an
  empty target set as negative infinity; new content still requires file push.
- Generated Google Sheets `Date.gs` uses the shared calendar validator. Existing
  installations must add it and update the other files. Development installation
  instructions and an explicit six-host acceptance matrix are provided.

## Verification

Implementation `7cc58e3`; final native-mask guard and deployed source
**8c7461d137f0cefbbe265d58a12d4078acb07212**. Standalone engine capability records:
**67060cf5950ba020d1d0a1665b0b0fcdf24b27a3**; service source remains `d6666a2`.

Local provenance regression tests pass all 69 cases, including 18 date cases with
synthetic native services. ESLint/Svelte, formatting, generated-artifact checks
and the production build pass. All six existing connector browser flows passed;
the added date retry/pull/apply-safety test passed three repetitions after fixing
the test's preview timing. A final local production run passed all seven connector
browser tests. This record does not claim installed-host acceptance.

- [Final frontend CI](https://github.com/ArefinAlter/dynodoc/actions/runs/37439700496)
  passes lint/types/format, production build, generated Google artifacts, **271
  unit tests**, 16 public browser tests (46 authenticated cases intentionally
  skipped there), and **all 62 signed-in browser tests without skips**.
- [Independent engine CI](https://github.com/ArefinAlter/dynodoc-engine/actions/runs/37439824200)
  passes formatting, Clippy, build, **all 200 tests**, docs and OpenAPI consistency.
  Existing Rustdoc warnings remain. All 21 committed migration files and the
  provenance service/test files match the application byte-for-byte as Git blobs.
- The same seven connector browser tests also passed against the deployed HTTPS
  panes. Host services and private API calls were intercepted with synthetic
  bridges/receipts; this is deployment/runtime evidence, not real native acceptance.

The service contract, Rust source, migrations through 0024, event serialization
and generated OpenAPI are unchanged. Standalone engine README/capability/roadmap/
contract/changelog records describe date observations as an application-client
enhancement. The standalone engine does not include native editor adapters.

## VPS rollout

Activated in `/opt/dynodoc-demo`, verified **6 October 2026, 09:09:11 UTC**.

- Fresh backup `backups/dynodoc-20261006T085018Z.dump` passed pg_restore listing
  and checksum checks; earlier backups remain. Release records/previous source
  are in `releases/date-sync-7cc58e3-20261006`.
- Previous web/admin images are retained as `before-dates-20261006`; their IDs
  are `25f9c5dc1904` and `7bc50691cce9`. Updated only web/admin, now `2719fbdb1b40`
  and `84ae2ed661f9`. All **17 preserved containers**, including the API/database/
  converter and unrelated services, retained their IDs/images; private environment and all Nginx site
  checksums are unchanged. No migration was applied; latest schema remains 24.
- Final archive has 529 verified source files, SHA-256
  `760e2d3db3a7c9ee642f24412742f34264cf6cb285bc4e22d91a5b58f4396df1`.
  Existing SQL and provenance service/test bytes were checked before extraction.
- Counts stayed 9 accounts, 2 documents, 24 events, 0 uploads and 0 drafts.
  Whole-event-chain, uploaded-file and access-list aggregate digests matched
  before and after. No production document, connector key, account or email was
  created during verification.
- Workspace/admin/API HTTPS health and homepage return 200. Projects redirects
  anonymous visits to login; private Projects/provenance/key/checkpoint endpoints
  return 401. Google/email sign-in remain enabled; development sign-in is disabled.
  Microsoft panes retain their scoped CSP policy; login retains SAMEORIGIN.
- Local previews are stopped; no local database was created for this increment.
  Owned temporary source archives, deployment scripts and downloaded CI logs are removed
  after recording evidence.

For a UI rollback, retain schema/data and use the previous web/admin images. No
API or database restore is needed for this client-only increment. Preserve new
date observations and original files; unsupported edits still use file push.

## Boundaries and next priorities

Native Word/Docs, Excel/Sheets and PowerPoint/Slides acceptance remains first;
[the checklist](NATIVE-CONNECTOR-ACCEPTANCE.md) includes both Excel date systems,
Sheets timezones/DST, closed/reopened panes, retry/recovery, review and cross-host
pull. Automated service/host mocks cannot sign that off.

Then extend structural/formatting block operations, formula compatibility and
slide notes/tables/media with stable identities and explicit conflict recovery.
Timestamps, changed date types/formats and unsupported edits still use file push.
Snapshot capture is pane-dependent and does not establish authorship or original
edit commands. Durable background capture, secure pairing and signed marketplace
distribution follow, before Project Compare/Activity/search/multi-file releases.

API basis: Microsoft's [stable worksheet functions](https://learn.microsoft.com/en-us/office/dev/add-ins/excel/excel-add-ins-worksheet-functions)
and [merged-area checks](https://learn.microsoft.com/en-us/javascript/api/excel/excel.range?view=excel-js-1.13),
Google's [spreadsheet timezone](https://developers.google.com/apps-script/reference/spreadsheet/spreadsheet#getSpreadsheetTimeZone())
and [calendar parsing/formatting](https://developers.google.com/apps-script/reference/utilities/utilities#parseDate(String,String,String)).
