# Installing the Dynodoc editor add-ins directly

There is no Microsoft Marketplace or Google Workspace Marketplace listing for these
add-ins yet. Until one exists you can install them yourself, for your own account,
using the steps below. Both routes need only your own Office or Google sign-in; no
store account is involved. Deploy the matching Dynodoc API and web build first; the
default server is https://app.dynodoc.online and a source release alone does not
update it. Use a disposable copy of a small supported file added to a Dynodoc Project.
Use [the acceptance worksheet](ACCEPTANCE.md) to record real host results: installing a
package or passing browser tests alone is not host acceptance.

## Microsoft Word, Excel and PowerPoint

Download the matching XML manifest from the workspace's `/connectors` page or from
`extensions/<host>/manifest.xml`. These task-pane add-ins target compatible desktop
Office as well as Office on the web. Choose the instructions for your platform:

- **Windows desktop:** put the XML files in a shared folder, add its UNC path in
  File > Options > Trust Center > Trust Center Settings > Trusted Add-in Catalogs,
  enable Show in Menu and restart Office. Select Home > Add-ins > Advanced >
  Shared Folder, then add the matching Dynodoc manifest. Menu labels vary by Office
  version. [Microsoft's Windows guide](https://learn.microsoft.com/en-us/office/dev/add-ins/testing/create-a-network-shared-folder-catalog-for-task-pane-and-content-add-ins).
- **Mac desktop:** copy the manifest into the host's `wef` folder under
  `~/Library/Containers/com.microsoft.Word/Data/Documents/` (use
  `com.microsoft.Excel` or `com.microsoft.Powerpoint` for the other hosts), restart
  that application, and select Home > Add-ins.
  [Microsoft's Mac guide](https://learn.microsoft.com/en-us/office/dev/add-ins/testing/sideload-an-office-add-in-on-mac).
- **Office on the web:** open a file and upload the XML through the add-ins
  dialog. [Microsoft's web guide](https://learn.microsoft.com/en-us/office/dev/add-ins/testing/sideload-office-add-ins-for-testing).

Developers can alternatively use Microsoft's `office-addin-debugging` tooling for
an HTTPS-hosted pane; always run its matching stop command to remove test
registrations. [Developer workflow](https://learn.microsoft.com/en-us/office/dev/add-ins/testing/test-debug-non-local-server).

The add-ins are named **Dynodoc for Word**, **Dynodoc for Excel** and **Dynodoc for
PowerPoint**. The Home ribbon has a **Dynodoc** group with a **Dynodoc** button that
opens the task pane; the pane is also available through Office's add-in menu. Word
requires WordApi 1.3, Excel requires ExcelApi 1.13, and PowerPoint requires
PowerPointApi 1.4. Save the local file before connecting. Keep the pane open while
capturing changes.

**PowerPoint 2021 is not supported by the current shape-text connector.** It lacks
PowerPointApi 1.4. Use a supporting Microsoft 365 build or Office 2024 for that
connector; older installations can use edited-file push through Projects.
[Microsoft's requirement table](https://learn.microsoft.com/en-us/javascript/api/requirement-sets/powerpoint/powerpoint-api-requirement-sets).
Word/Excel eligibility also depends on their requirement sets and Office policy;
an installed executable or valid manifest does not prove a working add-in.

The file can live on your local disk. The panel is hosted over HTTPS inside Office;
loading it, signing in and syncing require connectivity. Enabled capture saves a
local queue while the pane is open. Complete offline commits/exchange and background
capture are not available. Originals must be at most 3,000,000 bytes.

## Google Docs, Sheets and Slides

Download and unzip the matching Google package. Create a **standalone** Apps Script
project for that host and name it **Dynodoc**, so the add-on menu reads Extensions >
Dynodoc. Add each `.gs` and `.html` file, preserving its name, and replace the
project's `appsscript.json` (enable manifest visibility in project settings).
Use **Deploy > Test deployments > Editor add-on**, choose a test file, save the test
and execute it. Open **Extensions > Dynodoc > Open Dynodoc**. Google may ask you to
authorize access to the current file and external requests.
This is Google's Editor add-on test workflow; it is not a published installation
link. See [Google's test-deployment guide](https://developers.google.com/workspace/add-ons/how-tos/testing-editor-addons).

Google keeps about 500 KB of saved add-on data per user and per Apps Script project,
shared by every file that user connects with it. Each connected file's capture queue
is capped at 100 KB. When Google's store is full the sidebar stops capture and says so;
your previous saved queue stays intact. Export your provenance when you see that
message.

## Sign in and connect

1. In the pane or sidebar select **Sign in to Dynodoc**, then open its browser link.
2. Sign in with your Dynodoc email. Check that the browser's comparison code
   matches the one in the editor. Choose the matching remote file and approve.
3. Return to the editor and select **I approved this connection**. Enable capture
   explicitly. The working copy must match the supported checkpoint structure/text.
4. Edit supported content, capture, push for review, then merge through the web
   review workflow with an authorized account. Preview and apply team changes in
   the other editor. Revoke the connection under **Sync & connections** to test
   that further access stops. Connection grants expire after seven days.

If browser sign-in is not available, open **Advanced: connect with a key** in the
pane or sidebar and paste a file-scoped key created under Sync & connections.

No account-wide token is given to the editor. The Office pane keeps its key in memory;
Google stores the approved key in that user's private properties for the local file.
Reopening Office requires sign-in again (or the existing manual connection key).
Capture queues remain saved. Existing queues and unrelated file bindings are not replaced.

## Limits and alternate servers

Current adapters capture existing paragraph text, cells/formula sources/date-only
values, and individual slide shape text. They do not synchronize arbitrary layout,
structural edits, embedded objects, animations or every keystroke. Real host acceptance
for this release remains required. Keep an original copy when testing pull/apply.

### Pointing an installation at a staging server

Use HTTPS with a certificate trusted by the editor. Do not disable TLS checks. Use a
matching API/web build, migrations through 0029 and your normal email sign-in
configuration. Public shared-checkpoint writes stay opt-in.

- **Office manifests.** In each copied `manifest.xml`, replace the production origin
  `https://app.dynodoc.online` everywhere it appears: `IconUrl`,
  `HighResolutionIconUrl`, the `AppDomain`, `DefaultSettings/SourceLocation`, and
  every `bt:Image`/`bt:Url` resource (icons, `commands.html`, the pane URL). Leave
  `SupportUrl` alone. If the staging copy will sit next to the production one, give it a
  different `<Id>` GUID and a distinct `DisplayName`. Raise `<Version>` each time you
  re-sideload so Office replaces its cached copy. Keep the origin identical in all of
  those places; a link to a domain missing from `AppDomains` opens in the user's
  browser instead of the pane.
- **Google add-ons.** In the host's `Code.gs` change `DYNODOC_ORIGIN`, and in
  `appsscript.json` change `urlFetchWhitelist` to the same origin followed by
  `/api/connectors/`. The two must match or `UrlFetchApp` refuses the request.
- The approval link shown during sign-in is generated from that same origin.

## Publication

Sideloading and Apps Script test deployments do not require a public marketplace
listing. Public Office installation goes through Microsoft Partner Center and
Microsoft Marketplace certification. Public Google installation uses a standard
Cloud project, OAuth configuration and the Google Workspace Marketplace SDK/listing;
scope-dependent OAuth verification and Marketplace review are separate steps.
Organization-only distribution has platform-specific admin options. See
[Microsoft publication](https://learn.microsoft.com/en-us/office/dev/add-ins/publish/publish)
and [Google publication](https://developers.google.com/workspace/add-ons/how-tos/publish-add-on-overview).
The listing copy, assets and the owner-only submission checklist are in
[`docs/listings/`](https://github.com/ArefinAlter/dynodoc/blob/main/docs/listings/SUBMISSION-CHECKLIST.md). No listing has been
submitted and none is approved.
