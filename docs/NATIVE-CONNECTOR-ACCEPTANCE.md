# Native connector acceptance

Status: **not yet performed in installed Microsoft/Google editors**. Automated
adapters/browser bridges are development evidence. Run this checklist with test
accounts and disposable files before claiming native compatibility or publishing
an add-in. No production email is needed. Installation instructions are in
[extensions](../extensions/README.md).

The matching server is now deployed at application `536a448`, schema 25, with
[all six test packages available](https://app.dynodoc.online/connectors).
[Delivery and rollback checks passed](operations/2026-10-10-history-connector-release.md).
This makes the matrix below ready to run; it does not mark any host accepted.

## Record the environment

Record the application commit, host/platform and version, available API sets,
account type, file ID, working-copy URL, timezone/date system and test result.
Exclude connection keys, credentials and private file contents from the record.
Microsoft targets: Word desktop/web (WordApi 1.3), Excel desktop/web (ExcelApi 1.13),
PowerPoint desktop/web (PowerPointApi 1.4). Google targets: Docs, Sheets and Slides,
including a second user to verify private credential/queue isolation.

## Common workflow for every pair

For the current release first install from `/connectors`: Office manifests must
show the Home ribbon command/icons, and each Google test deployment must show the
Open provenance menu/sidebar. Use **Sign in to Dynodoc**, compare the code in the
browser, sign in, approve one file and return to finish. Check expiry, cancel/retry,
wrong-file/kind, two accounts, lost membership and revocation. Check browser launch
behavior on each desktop/web host; the sign-in page must not replace the working
copy. Also exercise the manual-key fallback below. No marketplace acceptance is
implied by Microsoft's XML validation.

1. Create an invite-only Project with an owner and contributor. Import a simple
   DOCX, XLSX or PPTX and open matching Microsoft and Google working copies. Create
   a separate file/host key for each. Confirm a wrong-file/wrong-host key fails.
2. Connect explicitly. Capture starts disabled. Enable it, change supported text
   or cells, wait for the saved-observation notice, then close/reopen the pane.
   Reconnect and confirm the local queue survives and native stable IDs remain.
3. Push a request while transport is interrupted, export the sidecar, then retry
   after reconnecting. One immutable request/receipt should result. Changing the
   request name after the first attempt must not change the frozen upload intent.
4. Keep typing after the upload attempt. Confirm later observations survive retry.
   The contributor cannot merge or manage members through the connector key.
5. Have the owner review and merge in Projects. Preview and apply in the other
   editor. Check stable IDs, text/formula/date values and preserved local content.
   Receipt alone must not advance the canonical checkpoint or overwrite the file.
6. Make independent edits in separate blocks/cells: both should survive pull.
   Make competing edits in the same block/cell: pull should report a conflict.
   Type after preview: apply must stop, retaining the queue and previous base.
7. Rename/Save As and copy the native file. The original connection must stop
   or demand explicit recovery. Restore a sidecar into a fresh matching copy in
   the same host; verify the original historical base and exact retry IDs. An
   existing queue, foreign IDs or different content must not be overwritten.
8. Revoke the key, remove access and force sign-out in separate runs. Subsequent
   requests must fail while locally exportable observations remain available.
   A similarly named unrelated file must never attach to provenance automatically.

## Format-specific checks

| Pair | Fixture and checks |
| --- | --- |
| Word/Docs | Three plain body paragraphs. Change each editor in both directions; retain content-control/named-range IDs. Lists, tables, new/moved/deleted paragraphs and unsupported Google tabs require file push. Check text-replacement formatting behavior. |
| Excel/Sheets | Two sheets with scalar types, blank cells, `=A1*2` and date-only cells. Change different cells in one named row. Add a merged range locally: binding/capture/apply must stop before writes. Missing/moved/foreign row names, new rows/columns and sheet changes require file push. Sorting/pasting may appear as value observations; do not claim reconstruction of the original command. |
| PowerPoint/Slides | Two slides with text shapes and an image, plus a blank slide. Keep native slide/shape tags or private native-ID mappings, including accessibility descriptions. Competing shape text must conflict. New/moved/grouped/table shapes, notes/media/style changes require file push. Check native text replacement and layout separately. |

## Date-only acceptance matrix

Import an XLSX containing real dates, not date-looking text: `2024-02-29`,
`2026-10-06`, a blank date-formatted cell, and a formula cell. Retain date formats.
Run Excel's 1900 and 1904 workbook settings separately; compare the **calendar day**
after Excel → review/merge → Sheets → review/merge → Excel. Include 1904-01-01,
whose serial is zero in a 1904 workbook, so it cannot be confused with a blank.

For Sheets, run spreadsheet timezones Asia/Dhaka, America/New_York and
Pacific/Kiritimati with a different Apps Script timezone. Include the New York DST
transition dates. Date-only values must not shift to the previous/next day. Test
a timezone whose midnight/day is skipped: the connector must stop if local
midnight cannot round-trip, rather than silently change the day.

Invalid dates, Excel's fictional 1900-02-29, hidden hours/minutes/milliseconds,
date-to-text/type changes and locale strings such as `06/10/2026` must stop sync.
Change native date formatting to General, text, a time mask or an ambiguous custom
mask: sync must stop. Common date-only masks are supported; changing between them
does not record or synchronize formatting.
Original files and file push retain unsupported timestamps/format changes.
Formula source must survive without claiming matching calculation results across
the two formula engines. Change a cell or spreadsheet timezone during date
preflight: apply must stop before its first write. Repeat offline retry, sidecar
recovery, separate-cell merge and same-date conflict with date observations.

## Evidence and publication

Record pass/fail per host and pair with reproducible steps and redacted results.
Do not mark this checklist accepted based on mocks. Microsoft manifest/store
validation, Google OAuth/scopes and Marketplace review, operator/legal details,
signed distribution and durable background capture remain separate work.
