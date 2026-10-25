# Dynodoc for Docs — listing kit

Draft copy prepared from this repository for the owner to paste into the **Google Workspace Marketplace SDK** configuration. No listing exists, none has been submitted and nothing has passed OAuth verification. Re-check every claim against the exact build you submit; the limits below are enforced by the code in `extensions/google-docs/` and `frontend/researcher/src/lib/provenance/`. Owner-only steps are in [SUBMISSION-CHECKLIST.md](SUBMISSION-CHECKLIST.md); what the add-on stores and sends is in [addin-data-handling.md](addin-data-handling.md).

## Listing fields

| Field | Value | Limit |
| --- | --- | --- |
| Application name (must match the OAuth consent screen name) | Dynodoc for Docs; fallback `Dynodoc` | 50 |
| Short description / tagline | Send your Google Docs edits to Dynodoc for review and bring approved team changes back. | 100 here (Google allows 200) |
| Detailed description | see below | 16,000 |
| Category | Productivity (alternative: Business Tools). Choose from the list the Marketplace SDK shows. | |
| Search keywords | document review, approvals, version history, change tracking, audit trail, collaboration, provenance, document history, team review | |
| Language | English | |
| Editor | Google Docs (Editor add-on, standalone Apps Script project; source in `extensions/google-docs/`) | |
| Developer name / contact | Verne Ltd.; info@vernesystems.com, +880 1870 034384 (owner to confirm the mailbox is monitored) | developer contact email required |
| Support / privacy / terms links | https://dynodoc.online/support, https://dynodoc.online/privacy, https://dynodoc.online/terms (the privacy and terms pages are drafts and not yet effective; the owner must adopt them first) | all required |
| Icons | `extensions/marketplace/assets/google-icon-128.png` and `google-icon-32.png` (`google-icon-96.png` only for a listing that includes a web app) | 128 and 32 required |
| Card banner | `extensions/marketplace/assets/google-card-banner-220x140.png` (a 440 x 280 version is alongside) | 220 x 140 |
| Screenshots | real captures, see below | 1280 x 800 recommended |

Naming: Google's Marketplace branding rules allow a Google product name after “for” with a trademark attribution, but the add-on style guide discourages product names in add-on names and prefers short names. Submit `Dynodoc for Docs`; if review objects, use `Dynodoc` and let the description and icon say which editor it serves. Do not put the word “Google” in the name.

## Long description

```
Dynodoc for Docs connects the document you are editing in Google Docs to a Dynodoc project, where changes are reviewed before they become the team's version.

Work in Google Docs as usual. While the Dynodoc sidebar is open and capture is switched on, the add-on notices edits to existing paragraph text in the current tab and keeps them in a queue. When you are ready, send them for review. Teammates compare the change in Dynodoc, approve or reject it and merge it. Back in Google Docs, preview the approved team changes and apply them with one click. Nothing is written to your document until you choose Apply.

What you get
- Connect the open document to a Dynodoc project with browser sign-in and a one-time code check
- Record edits to existing paragraph text while the sidebar is open
- Send your changes for review as a change request that teammates can compare, approve and merge
- Preview approved team changes, then apply them to the document in one step
- Stops safely if you switch tabs or the document changed after the preview

Good to know
- Covers existing paragraph text in the current tab. Formatting, new, deleted or moved content, comments and embedded objects are not synchronized; use Dynodoc's file upload for those changes.
- Records changes only while the sidebar is open. There is no background capture.
- Needs an internet connection to load the sidebar, sign in and send changes. Captured changes wait in a queue until you send them.
- Sign-in happens in your browser with your Dynodoc account; the add-on never sees your password. Each connection reaches one file, expires after seven days and can be revoked in Dynodoc.
- Requires a Dynodoc account and a Dynodoc project that contains the file.

Google Docs is a trademark of Google LLC. Dynodoc is not affiliated with or endorsed by Google.
```

## Feature bullets

- Connect the open document to a Dynodoc project with browser sign-in and a one-time code check
- Record edits to existing paragraph text while the sidebar is open
- Send your changes for review as a change request that teammates can compare, approve and merge
- Preview approved team changes, then apply them to the document in one step
- Stops safely if you switch tabs or the document changed after the preview

## Supported platforms

- Google Docs in a desktop browser (Editor add-on sidebar)

Not supported:

- Google Docs mobile apps (Editor add-on sidebars are not available there)
- Documents with tables, lists or images in the synced tab

List only what has passed [extensions/ACCEPTANCE.md](../../extensions/ACCEPTANCE.md) inside a real Editor add-on host; at the time of writing no such run has been recorded.

## Honest limitations (keep these visible in the listing)

What it covers:

- Edits to the text of existing plain body paragraphs in the tab that was active when you connected, up to 2,000 paragraphs.

What it does not do:

- Formatting, new, deleted or moved paragraphs, tables, lists, images, footnotes, comments and other tabs.
- Any edit made while the sidebar is closed (the sidebar compares snapshots every five seconds while capture is on; Google Docs has no general edit trigger).
- It is not a background recorder, not a keystroke log and not proof of who typed a change.
- Loading the sidebar, signing in and sending changes need an internet connection. Offline commits and background capture are not available.
- Files imported into a Dynodoc project for sync must be 3 MB or smaller.
- Google keeps about 500 KB of saved add-on data per user and script, shared by every file. A single file's queue is capped at 100 KB; when Google's store is full capture stops with a message and the previous saved queue stays intact. Export your provenance when you see it.
- Sign-in uses a Dynodoc browser approval for one file. Google sign-in is not the identity Dynodoc uses.

Changes made to the file itself: Dynodoc adds one named range per paragraph, named `dynodoc.block.<ID>`, so paragraphs keep their identity. Named ranges are not shown in the normal Docs interface.

## OAuth scopes and why

The manifest (`extensions/google-docs/appsscript.json`) requests exactly these scopes. The consent screen and the Marketplace SDK scope list must match it.

| Scope | Why |
| --- | --- |
| `https://www.googleapis.com/auth/documents.currentonly` | Lets the add-on read and edit only the document it is opened in: reads paragraph text to detect edits, adds paragraph identity named ranges, and writes approved team text when the user chooses Apply. |
| `https://www.googleapis.com/auth/script.container.ui` | Shows the Extensions > Dynodoc menu item and the Dynodoc sidebar. |
| `https://www.googleapis.com/auth/script.external_request` | Sends requests to Dynodoc and nowhere else (the manifest allow-list is `https://app.dynodoc.online/api/connectors/`). |

Runtime: V8, `exceptionLogging: STACKDRIVER`, `urlFetchWhitelist` limited to `https://app.dynodoc.online/api/connectors/`. The Cloud console shows each scope's sensitivity category when you add it; treat `script.external_request` as needing OAuth verification until the console says otherwise. Do not broaden scopes to simplify distribution.

## Reviewer test instructions

Paste into the review notes. The reviewer needs a copy of the sample document, a Dynodoc reviewer account they can actually sign in to, and a Dynodoc project that already contains the matching file.

1. Open the supplied sample (Board update (Google Doc)) in Google Docs. Use a copy; do not convert from an Office format.
2. Choose the Dynodoc entry point (below). The sidebar opens with **Sign in to Dynodoc** as the first action.
3. Choose **Sign in to Dynodoc**, then **Open browser to sign in and choose a file**. Sign in with the reviewer account supplied below, check the code matches the one in the sidebar, choose the sample file and choose **Approve connection**.
4. Return to Google Docs and choose **I approved this connection**. The sidebar shows the connected controls.
5. Tick the capture box. Change `Acceptance alpha` to `Acceptance alpha revised`, then choose **Push for review**. Expected: the sidebar reports the changes were sent for review.
6. In the browser open https://app.dynodoc.online/projects, open the project's change request and merge it (the reviewer account is an owner).
7. Back in Google Docs choose **Preview team changes**, then **Apply preview**. Expected: the merged value appears in the file.
8. Optional: in Dynodoc open Sync & connections for the file and revoke the connection; a further push from the sidebar is refused.

Entry point: choose **Extensions > Dynodoc > Open Dynodoc** and authorize the add-on when Google asks.

## What the owner must provision for review

- A Dynodoc account for the reviewer that can be signed into without access to the owner's inbox. Dynodoc email sign-in sends a link to the account's mailbox, so give the reviewer a mailbox they can read, or an account that can use Google sign-in. Put the details only in private review notes.
- A project that contains the sample file, imported from the same content the reviewer opens. Sample content: Three plain body paragraphs, for example: `Acceptance alpha`, `Acceptance beta` and a third in another script. Start from a Google-native document, not a converted .docx, so identity matches.
- A shareable “make a copy” link to the Google-native sample, and a short screen recording of the full flow if Google's OAuth verification asks for one.
- The production web and API deployed at https://app.dynodoc.online at the schema level this build needs, and `python infra/demo/check-connectors.py https://app.dynodoc.online` passing.
- A support mailbox that is read during review.

## Screenshots to capture

Real captures from the installed add-on only (see [extensions/marketplace/README.md](../../extensions/marketplace/README.md)). Use synthetic names and content. Recommended 1280 x 800 (640 x 400 and 2560 x 1600 are also accepted), square corners, full bleed, at least one.

| # | Shot | What it shows | Caption |
| --- | --- | --- | --- |
| 1 | Sidebar open next to a document | Google Docs with Extensions > Dynodoc > Open Dynodoc and the sidebar showing Sign in to Dynodoc, over a sample document called Board update. | Open Dynodoc from the Extensions menu |
| 2 | Browser approval | The Dynodoc approval page in a browser with the comparison code and the file chooser. Crop to the page; use a synthetic user such as Maya Chen. | Approve one file, nothing else |
| 3 | Capture on | Sidebar after connecting, Enable paragraph capture ticked and a status line showing saved observations. | Edits are queued privately |
| 4 | Review in Dynodoc | Dynodoc change request showing the paragraph before/after diff, reviewer Sam Okoye. | Teammates review before it is merged |
| 5 | Preview and apply | Sidebar with a preview ready and the Apply preview button enabled, one paragraph changed. | You decide when team changes land |
