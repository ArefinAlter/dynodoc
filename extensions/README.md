# Word and Google Docs provenance connectors

Both development connectors use the same protocol and capture queue. They are
not yet published or accepted in either host. Dynodoc is the review/sync platform;
Microsoft and Google remain the editors. [Contract and remaining work](../docs/PROVENANCE-SYNC.md).

## Prepare a test project

Run/deploy the matching engine (migrations 0022 and 0023) and web build. Open
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

1. Use [Microsoft's sideload instructions](https://learn.microsoft.com/office/dev/add-ins/testing/sideload-office-add-ins-for-testing)
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
