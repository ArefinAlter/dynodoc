# Native editor acceptance worksheet

Use this with INSTALL.md on disposable files and two test identities. Record an
independent result for Word, Excel, PowerPoint, Docs, Sheets and Slides, and for
each desktop/web version you intend to support. No real-host pass is implied by
this worksheet. Test packages are not approved Marketplace installations.

## Record the environment

Copy this record for each run. Use pass, fail, blocked or not run for each case.
Store only synthetic content and sanitized evidence; omit credentials, connector
keys, pairing URLs/codes, email addresses and private files.

```
Date/time with timezone:
Host / platform / exact version / license channel:
Manifest or Apps Script source commit and package hash:
Workspace/API build and schema:
Required API set and detected support:
Test file type / size / synthetic identifier:
Tester roles: owner and editor (no personal identifiers):
Case / result / expected and observed behavior / evidence / follow-up:
Cleanup completed:
```

## Make the test files

Keep each original below 3,000,000 bytes. Save an untouched original, import it to
a private test Project, then give the second identity a separate identical copy.
Install the matching connector and bind both copies to that remote file using the
normal checked binding flow. Stop and record a mismatch rather than forcing a bind.

| Pair | Synthetic contents |
| --- | --- |
| Word / Docs | Three separate paragraphs: `Acceptance alpha`, `Acceptance beta`, `বাংলা sample`. |
| Excel / Sheets | One sheet named `Acceptance`: A1 `Label`, B1 `Value`, A2 `Alpha`, B2 `10`, A3 `Beta`, B3 `20`, B4 formula `=SUM(B2:B3)`. Also enter a date-only cell using the host's native date control. Record formula source and displayed result separately. |
| PowerPoint / Slides | Two slides with ordinary editable text shapes: `Acceptance alpha` and `Acceptance beta`. Record which existing shape IDs the connector recognizes before editing. |

Google conversion may change identity/structure. Test a Google-native baseline
first; cross-format binding is an additional case and must not be assumed from
matching filenames. Preserve formulas/date-only semantics rather than claiming
equal calculation/rendering behavior across products.

## Run the cases

1. **Install and load.** Confirm the native ribbon/menu, HTTPS pane/sidebar, declared
   API support and a useful unsupported-host response. Current requirements are
   WordApi 1.3, ExcelApi 1.13 and PowerPointApi 1.4. The recorded local PowerPoint
   2021 inventory is below its requirement; use a supporting host for a pass.
   Also check what a store reviewer will see: Office shows a **Dynodoc** group with a
   **Dynodoc** button and the add-in name reads "Dynodoc for Word/Excel/PowerPoint";
   Google shows **Extensions > Dynodoc > Open Dynodoc** and a sidebar titled Dynodoc.
   No "development" or "test" wording is visible, **Sign in to Dynodoc** is the first
   action and the manual key fields sit collapsed under **Advanced: connect with a
   key**. Record the manifest `Version` or Apps Script version that was tested.
2. **Authorize.** Pair through the normal browser approval flow, compare the code
   and choose the intended Project file. Check cancellation and expiry. No access
   should be granted before explicit approval; unrelated projects stay inaccessible.
3. **Capture supported work.** Enable capture with the pane open. Change `alpha`
   to `alpha revised`, B2 to `11`, or the first slide's existing shape text. Inspect
   the queued change and push. Verify the remote diff contains the intended change
   and preserves the other paragraph/cells/shape and Unicode text.
4. **Review and pull.** Use the configured project approval rules to review/merge.
   In the second copy, preview and apply the accepted change. Save, reopen and
   compare the native content with the reviewed result. Formula sources and date
   values need their own spreadsheet checks.
5. **Independent edits.** Edit different existing paragraphs/cells/shapes in the
   two copies. Push both for review and inspect the resulting comparison. Do not
   infer independence from different IDs if the edits share structural constraints.
6. **Conflicting edits.** Change the same existing value differently in both copies.
   A conflict must remain visible for resolution; verify the chosen result and
   attribution after merge and pull. Neither local version should disappear.
7. **Stale application.** Preview a remote pull, then edit the same local content
   before applying it. Verify that stale application stops and preserves local work.
8. **Retry and reconnect.** With the pane already loaded, disconnect networking,
   edit supported content, and inspect the captured queue. Reconnect and retry a
   push; check for duplicate canonical changes and lost pending work. Record exact
   host behavior. Hosted load/sign-in/sync still require connectivity; this is not
   a complete offline-commit or background-capture acceptance claim.
9. **Restart.** Save and close/reopen the file and pane. Verify binding and queue
   recovery. Office currently requires reauthorization because its secret is held
   in pane memory. Edits made while the pane is closed must not be represented as
   fully observed capture. Record any rescan and unsupported interval honestly.
10. **Revoke access.** Revoke the file connection through the owner account, then
    attempt preview/push from the old pane. Access must stop without silently
    deleting recoverable local edits. Also test an editor trying an owner-only merge.
11. **Unsupported change.** In another disposable copy, insert/delete a structural
    element or change unsupported formatting. Verify a clear unsupported outcome
    or the documented edited-file workflow. A text-only successful push does not
    certify complete layout, style, object, animation or formula-engine fidelity.
12. **Historical recovery.** Download or propose recovery of an earlier retained
    supported revision through Projects, review it and apply it to a disposable
    copy. Verify it creates new reviewed work and preserves newer history. Current
    operations are per file; do not report project-wide branch acceptance.
13. **Clean up.** Remove only this test deployment/sideload registration and these
    owned fixtures/connections. Keep existing trusted catalogs, add-ins, native
    documents and unrelated projects intact.

A failed or blocked case remains open. Record exact symptoms and a reproducible
sequence; simulated Office APIs, manifest checks, package checks and web browser
tests cannot substitute for these real-host results. Keep public listing submission
separate until all supported-host results and publisher/account requirements are met.
