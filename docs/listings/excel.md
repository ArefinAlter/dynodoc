# Dynodoc for Excel — listing kit

Draft copy prepared from this repository for the owner to paste into **Partner Center**. No listing exists, none has been submitted and none is approved. Re-check every claim against the exact build you submit; the limits below are enforced by the code in `extensions/` and `frontend/researcher/src/lib/provenance/`. Owner-only steps are in [SUBMISSION-CHECKLIST.md](SUBMISSION-CHECKLIST.md); what the add-in stores and sends is in [addin-data-handling.md](addin-data-handling.md).

## Listing fields

| Field | Value | Limit |
| --- | --- | --- |
| Name (manifest `DisplayName` and reserved offer name; keep identical) | Dynodoc for Excel | 50 in Partner Center, 125 in the manifest |
| Publisher | Dynodoc (owner to confirm the legal publisher name) | |
| Summary / tagline | Send your Excel cell edits to Dynodoc for review and bring approved team changes back. | 100 |
| Manifest description | Connect this workbook to a Dynodoc project. Dynodoc records edits to existing cell values and formulas while this pane is open, sends them for review, and brings approved changes from your team back into the workbook. | 250 |
| Category | Collaboration (secondary: Productivity). Choose from the list Partner Center shows. | |
| Search keywords | spreadsheet review, approvals, version history, change tracking, audit trail, collaboration, provenance, workbook history, team review | |
| Language | English (United States) | |
| Manifest | `extensions/excel/manifest.xml` (Id `794ef9dc-6d78-4b08-a7f3-2526d0476930`; keep it for the life of the listing) | |
| Support URL | https://dynodoc.online/support | public page, no sign-in |
| Privacy policy / terms | https://dynodoc.online/privacy and https://dynodoc.online/terms (owner must confirm the operator details are final before submitting) | |
| Icons and logo | `extensions/marketplace/assets/office-icon-16.png`, `-32`, `-64`, `-80` and `office-store-logo-300.png` | manifest uses 32 and 64 px; Partner Center logo is 300 x 300 |

Microsoft's listing guidance recommends not repeating the host product name in an add-in name. The name is kept as Dynodoc for Excel because it tells users which app it is for; if certification objects, rename the offer and the manifest `DisplayName` together and keep the same Id.

## Long description

Paste as the offer description (under 4,000 characters).

```
Dynodoc for Excel connects the workbook you are editing in Excel to a Dynodoc project, where changes are reviewed before they become the team's version.

Work in Excel as usual. While the Dynodoc pane is open and capture is switched on, the add-on notices edits to existing cell values, formula text and date-only values and keeps them in a queue. When you are ready, send them for review. Teammates compare the change in Dynodoc, approve or reject it and merge it. Back in Excel, preview the approved team changes and apply them with one click. Nothing is written to your workbook until you choose Apply.

What you get
- Connect the open workbook to a Dynodoc project with browser sign-in and a one-time code check
- Record edits to existing cell values, formulas and date-only cells while the pane is open
- Send your changes for review as a change request that teammates can compare, approve and merge
- Preview approved team changes, then apply them to the workbook in one step
- Stops safely on merged cells, type changes or a changed workbook instead of overwriting your work

Good to know
- Covers existing cell values, formula text and date-only values. Formatting, new, deleted or moved content, comments and embedded objects are not synchronized; use Dynodoc's file upload for those changes.
- Records changes only while the pane is open. There is no background capture.
- Needs an internet connection to load the pane, sign in and send changes. Captured changes wait in a queue until you send them.
- Sign-in happens in your browser with your Dynodoc account; the add-on never sees your password. Each connection reaches one file, expires after seven days and can be revoked in Dynodoc.
- Requires a Dynodoc account and a Dynodoc project that contains the file.
```

## Feature bullets

- Connect the open workbook to a Dynodoc project with browser sign-in and a one-time code check
- Record edits to existing cell values, formulas and date-only cells while the pane is open
- Send your changes for review as a change request that teammates can compare, approve and merge
- Preview approved team changes, then apply them to the workbook in one step
- Stops safely on merged cells, type changes or a changed workbook instead of overwriting your work

## Platforms (eligible by manifest; list only what passes acceptance)

- Excel on the web
- Excel on Windows (Microsoft 365)
- Excel on Mac (Microsoft 365) and Excel 2019 or later on Mac
- Excel on iPad (eligible by requirement set; see the note below)

Not supported:

- Excel on iPhone and Android phones (task pane only, desktop form factor declared)
- Builds without ExcelApi 1.13

Requirement set declared in the manifest: **ExcelApi 1.13**. The pane checks it at runtime and shows a plain message in an unsupported host. The platform list above is what Microsoft's `office-addin-manifest validate` reported on 12 October 2026 from the requirement set alone: it is **eligibility, not a test result**, and the validator output adds that Microsoft will test the add-in on those platforms at certification. List only platforms that have passed [extensions/ACCEPTANCE.md](../../extensions/ACCEPTANCE.md); at the time of writing no real-host run has been recorded. If a platform cannot be tested before submission (iPad in particular), expect certification to exercise it anyway.

## Honest limitations (keep these visible in the listing)

What it covers:

- Edits to existing cell values, formula source text and date-only values, up to 2,000 projected cells.
- Date-only cells keep their calendar day; the date is evaluated with the workbook's own calendar functions.

What it does not do:

- Structural changes (inserting, deleting or sorting rows or columns), formatting, merged cells, charts, pivot tables, timestamps and changing a cell's type or date format.
- Sorting or pasting whole rows can appear as several cell-value changes; use Dynodoc's file upload for those operations.
- It is not a background recorder, not a keystroke log and not proof of who typed a change.
- Loading the pane, signing in and sending changes need an internet connection. Offline commits and background capture are not available.
- Files imported into a Dynodoc project for sync must be 3 MB or smaller.
- Sign-in uses a Dynodoc browser approval for one file. It does not use Microsoft sign-in or Microsoft Graph.

Changes made to the file itself: Dynodoc adds one workbook-scoped named range per synced row, named `dynodoc_<ID>`, so rows keep their identity in the saved file. Deleting those names stops syncing for that file.

## Permissions and why

| Manifest permission | Why |
| --- | --- |
| `ReadWriteDocument` | Reads cell values and formulas to detect your edits and, only when you choose Apply, writes approved team values back. Also adds the named ranges described above. |

No other permission, no Microsoft Graph, no single sign-on and no access to other files. Network access is to `https://app.dynodoc.online` (pane, icons and API) and Microsoft's own `https://appsforoffice.microsoft.com` Office.js library.

## Reviewer test instructions

Paste into **Notes for certification**. The reviewer needs the sample workbook, a Dynodoc reviewer account they can actually sign in to, and a Dynodoc project that already contains the matching file.

1. Open the supplied sample (Q3 budget.xlsx) in Excel. Save it first.
2. Choose the Dynodoc entry point (below). The pane opens with **Sign in to Dynodoc** as the first action.
3. Choose **Sign in to Dynodoc**, then **Open browser to sign in and choose a file**. Sign in with the reviewer account supplied below, check the code matches the one in the pane, choose the sample file and choose **Approve connection**.
4. Return to Excel and choose **I approved this connection**. The pane shows the connected controls.
5. Tick the capture box. Change B2 from `10` to `11`, then choose **Push changes for review**. Expected: the pane reports the changes were sent for review.
6. In the browser open https://app.dynodoc.online/projects, open the project's change request and merge it (the reviewer account is an owner).
7. Back in Excel choose **Preview team changes**, then **Apply preview to Excel**. Expected: the merged value appears in the file.
8. Optional: in Dynodoc open Sync & connections for the file and revoke the connection; a further push from the pane is refused.

Entry point: on the Home tab choose **Dynodoc**.

## What the owner must provision for review

- A Dynodoc account for the reviewer that can be signed into without access to the owner's inbox. Dynodoc email sign-in sends a link to the account's mailbox, so give the reviewer a mailbox they can read, or an account that can use Google sign-in. Put the details only in the private certification notes.
- A project that contains the sample file, imported from the same document the reviewer opens. Sample content: One sheet named `Acceptance`: A1 `Label`, B1 `Value`, A2 `Alpha`, B2 `10`, A3 `Beta`, B3 `20`, B4 `=SUM(B2:B3)`.
- The sample file itself, attached or downloadable without sign-in, with text identical to the imported file.
- The production web and API deployed at https://app.dynodoc.online at the schema level this build needs, and `python infra/demo/check-connectors.py https://app.dynodoc.online` passing.
- A support mailbox that is read during review.

## Screenshots to capture

Real captures from the installed add-in only (see [extensions/marketplace/README.md](../../extensions/marketplace/README.md)). Use synthetic names and content. Partner Center screenshots: PNG, 1366 x 768 is the commonly required size and up to five are allowed. Confirm the current rule on the Marketplace listings page before capturing.

| # | Shot | What it shows | Caption |
| --- | --- | --- | --- |
| 1 | Pane open next to a workbook | Excel with the Dynodoc button on the Home ribbon and the pane showing Sign in to Dynodoc, over a sample file called Q3 budget.xlsx. | Open Dynodoc from the Home ribbon |
| 2 | Browser approval | The Dynodoc approval page in a browser with the comparison code and the file chooser. Crop to the page; use a synthetic user such as Maya Chen. | Approve one file, nothing else |
| 3 | Capture on | Pane after connecting, Enable capture ticked and a status line such as “1 observed change saved locally”. | Edits are queued on your device |
| 4 | Review in Dynodoc | Dynodoc change request showing the cell before/after diff, reviewer Sam Okoye. | Teammates review before it is merged |
| 5 | Preview and apply | Pane with Preview team changes done and the Apply preview to Excel button visible, one cell changed. | You decide when team changes land |
