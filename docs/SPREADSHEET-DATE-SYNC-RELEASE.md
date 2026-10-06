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

## Verification status

Local provenance regression tests pass all 69 cases, including 18 date cases with
synthetic native services. ESLint/Svelte, formatting, generated-artifact checks
and the production build pass. All six existing connector browser flows passed;
the added date retry/pull/apply-safety test passed three repetitions after fixing
the test's preview timing. Final full-suite CI and deployment evidence will be
recorded after those checks complete. This record does not claim installed-host acceptance.

The service contract, Rust source, migrations through 0024, event serialization
and generated OpenAPI are unchanged. Standalone engine capability documentation
will describe date observations as an application-client enhancement, rather
than claiming the engine includes native editor adapters.

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
