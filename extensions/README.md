# Microsoft and Google provenance connectors

Current installation/sign-in instructions: [INSTALL.md](INSTALL.md). All six hosts
now offer browser sign-in and file approval without copying a connection key.
The manual workflow below remains supported. Microsoft manifests include ribbon
commands; `/connectors` offers generated XML/Google ZIP test downloads. Deploy the
matching API/web build first. This does not certify native acceptance or publication.

All six development connectors use the same protocol and capture queue. Native
acceptance and marketplace publication remain open. Dynodoc is the review/sync platform;
Microsoft and Google remain the editors. [Contract and remaining work](../docs/PROVENANCE-SYNC.md).

## Prepare a test project

Run/deploy the matching engine (migrations through 0026) and web build. Open
`/projects`, create an invite-only project, and add a plain-paragraph DOCX or move
an existing plain document here. Keep a local editor copy with exactly the same
paragraph text and order. In Files → Sync & connections, create a **Word** or
**Google Docs** key for each editor. Keys are different, expire after seven days,
are revocable, and authorize only read/propose for that file. Never paste a full
account token or the web service key into an add-in.

The first implementation captures changes to existing paragraph text only. It
does not synchronize formatting, new/deleted/moved paragraphs, lists, tables,
images, spreadsheets, slides, footnotes or all Google tabs. Use the existing
edited-file push workflow for unsupported changes. Text replacements may inherit
host formatting. Do not describe this as a complete edit recorder or AI/human
authorship attestation.

## Microsoft Word development installation

1. Use the platform-specific [Windows, Mac or browser instructions](INSTALL.md#microsoft-word-excel-and-powerpoint)
   with `word/manifest.xml`. Its pane URL is
   `https://app.dynodoc.online/connectors/word`. Before a source-only/local test,
   copy the manifest and set SourceLocation/AppDomain to your deployed test origin
   served over HTTPS. The URL must serve the matching new web build.
2. Open the same saved plain document in Word. The task pane requires stable
   WordApi 1.3 support. Enter the file ID and Word-scoped key; connect.
3. Enable capture explicitly. Edit existing paragraphs, capture, then push for
   review. Review/merge in Projects, preview team changes and explicitly apply.
4. Save the DOCX: `dynodoc.block.<ULID>` content-control tags retain block IDs.
   Removing/duplicating the tags makes structural syncing stop. Export the local
   ledger before clearing browser data or changing the binding.

The queue lives in Word's webview IndexedDB; the key stays in the current pane's
memory. Capture runs while the pane is open (two-second snapshots), with manual
capture available. This is not a background desktop agent. See
[Microsoft's content-control API](https://learn.microsoft.com/javascript/api/word/word.contentcontrol)
and [paragraph API](https://learn.microsoft.com/javascript/api/word/word.paragraph).

## Google Docs development installation

1. Run `pnpm build:connectors` in `frontend/researcher`. Copy `Code.gs`,
   `Sidebar.html`, generated `Protocol.html` and `appsscript.json` into a Docs
   Editor add-on Apps Script project. Enable the manifest in the Apps Script
   editor. For an HTTPS staging deployment, change the **code allowlist**
   `DYNODOC_ORIGIN` and manifest `urlFetchWhitelist` together.
2. Use [Google's Editor add-on test deployment](https://developers.google.com/workspace/add-ons/how-tos/testing-editor-addons)
   with your own document/account. Open **Dynodoc → Open provenance**. Choose the
   intended Google Docs tab and keep it selected throughout that sidebar session.
3. Enter the file ID and Google-Docs-scoped key; connect and opt into capture.
   Named ranges `dynodoc.block.<ULID>` anchor existing paragraphs in this tab.
4. Edit, capture and push. Review in Projects, then preview/apply the merged team
   checkpoint. Export the ledger before it reaches the 100 KB development cap.

Credentials and queues use **UserProperties**, scoped by Google document/tab ID;
no secret is stored in document properties or shared script properties. The
sidebar polls every five seconds while enabled. Google Docs has no universal
`onEdit` trigger: [official trigger support](https://developers.google.com/workspace/add-ons/concepts/editor-triggers).
This is an Editor add-on prototype, not a Workspace card-based extension; its
publication requirements differ. Named range anchors are documented by
[Google](https://developers.google.com/apps-script/reference/document/named-range).

## Recover a queue after a rename or device change

Use **Export local provenance** in either connector to save a versioned recovery
sidecar. Keep it with the document; it contains document text/history and no
connection key. In a fresh copy/tab with matching paragraph text, choose that
file before connecting. Use the same editor, Dynodoc account and remote file ID,
with a valid scoped key. The connector verifies the exact historical starting
checkpoint, binds paragraph identities and restores observations/receipts and
the frozen upload without changing its IDs or message. Enable capture explicitly
afterward. Paragraph text is not replaced by restoration.

Existing queues cannot be replaced. If storage fails, retain the export and retry;
Google's 100 KB ledger cap still applies. Previous unwrapped ledger exports are
evidence; recovery accepts the new `dynodoc.provenance-sidecar` format. To exchange
Word and Docs work, review/merge and pull a team checkpoint rather than importing
a queue into a different editor. This does not add background capture.

Word rechecks its current saved-file URL using the Office file-properties API.
After Save As/rename, the active connection stops and its queue remains exportable.
Reopen the task pane in the saved copy and select the sidecar before reconnecting.

## Acceptance before supported/public installation

- Word Desktop and Word Web: bind, save/reopen, edit, disconnect/retry, duplicate
  paragraphs, remove tags, pull preview, simultaneous typing and partial failures.
- Google Docs: bind active tab, edit/reopen, named-range behaviour after text
  replacement, duplicate/remove anchors, concurrent sidebars, expired/revoked keys
  and quota failure. Switching tabs must not apply a preview to a different tab.
- Word → review/merge → Docs → edit/review/merge → Word, preserving stable IDs and
  the uploader/merger distinction. Test renamed copies and unrelated same-name files.
- Complete legal/operator details, Microsoft manifest validation and supported
  host requirements, Google scopes/OAuth review, store listings and real user
  consent. Do not enable broad production scopes or publish based solely on unit tests.

Automated protocol/storage and signed-in platform tests cannot substitute for
these host tests. No production email is required for this development acceptance.

## Excel/Sheets and PowerPoint/Slides development installation

All six editors use the same bundle/review/merge/pull and recovery contract. This
increment supports existing cells, date-only values/formula source and existing slide shape text,
with a maximum of 2,000 projected cells/shapes. It does not capture structural or
formatting changes, timestamps, merged cells, charts, grouped/table slide shapes,
media or animations. Formula dialects/layout differences require real-host checks.
Remote structural/formatting changes stop incremental pull. Use file push for
unsupported edits; keep original files.

The development panes and scoped APIs are now deployed at `https://app.dynodoc.online`. Installation in the native hosts and marketplace publication remain open.

1. Import the XLSX/PPTX into a Project and keep a matching native working copy.
2. Create an **Excel**, **Google Sheets**, **PowerPoint** or **Google Slides** key
   under Sync & connections. The key must match both editor and file format.
3. For Microsoft hosts, sideload `excel/manifest.xml` or `powerpoint/manifest.xml`
   through the supported development/catalog workflow. The pane requires stable
   ExcelApi 1.13 or PowerPointApi 1.4 and a saved working-copy URL. Enter the file ID
   and key; connect. Excel named rows and PowerPoint slide/shape tags retain IDs.
4. For Google hosts, create a container-bound Apps Script project in the matching
   spreadsheet/presentation. Install that host directory's `Code.gs`, `Common.gs`,
   `Sidebar.html`, `Protocol.html` and `appsscript.json`.
   For Sheets, include the generated `Date.gs` as well. Authorize only the
   current-file scopes and reopen the file to use the **Open provenance** menu.
   Credentials/queues are private to your Google user and file. Slides mappings
   use native IDs in private properties and preserve accessibility descriptions.
5. Enable capture explicitly, edit supported values, push for review in Projects,
   review/merge, preview the latest team fields and apply explicitly. Preserve a
   sidecar before a rename, copy, device switch or failed upload. Restore only in
   the original editor with matching text/values and a fresh queue; original base
   checkpoints are verified and frozen upload IDs are retained.

Build/check all three Google protocol artifacts with `pnpm build:connectors` /
`pnpm check:connectors` from `frontend/researcher`. Those commands also synchronize
Sheets/Slides `Common.gs` from `google-common/Common.gs` and generate Sheets
`Date.gs` from the shared calendar module; edit the shared sources.

Date-only cells retain the calendar day across hosts. Excel evaluates native
calendar functions in the workbook; Sheets uses its spreadsheet timezone. The
queue stores ISO midnight UTC as a civil-day representation, not an instant to
shift into the user's timezone. Existing date formats remain in place. Invalid
dates, hidden times, type changes, unrepresentable local midnight and changes during
preflight stop the update. Blank dates stay blank; formula source stays a formula.
Changing a date format/type still requires file push. Excel requires API 1.13 to
check native merged cells before binding or writing. Native acceptance must check
both Excel date systems and Sheets timezone/DST boundaries.

Acceptance still required for each pair: desktop and cloud Microsoft hosts;
Google host reopen/permissions/private accounts; saved identity retention;
offline exact retry; late typing; explicit recovery into a fresh copy; independent
and competing cell/shape edits; review/merge/pull across Microsoft and Google;
and rejection of unsupported structures. Marketplace publication is separate.
See [release evidence](../docs/SIX-EDITOR-PROVENANCE-RELEASE.md) and the
[native acceptance checklist](../docs/NATIVE-CONNECTOR-ACCEPTANCE.md).

Snapshot limitation: sorting or pasting complete spreadsheet rows can appear as
multiple cell-value changes when the host retains named range coordinates.
This increment does not reconstruct the original edit command or guarantee
recognition of every structural action; use file push for those operations.
