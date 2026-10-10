# Dynodoc editor test installations

These are developer/test installations, not marketplace listings. Deploy the matching
Dynodoc API and web build before using them; the default server is
https://app.dynodoc.online. A source release alone does not update that server.
Make a disposable copy of a small supported file and add it to a Dynodoc Project.

## Microsoft Word, Excel and PowerPoint

Download the matching XML manifest from the workspace's `/connectors` page or
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
- **Office on the web:** open a test file and upload the XML through the add-ins
  dialog. [Microsoft's web guide](https://learn.microsoft.com/en-us/office/dev/add-ins/testing/sideload-office-add-ins-for-testing).

Developers can alternatively use Microsoft's `office-addin-debugging` tooling for
an HTTPS-hosted pane; always run its matching stop command to remove test
registrations. [Developer workflow](https://learn.microsoft.com/en-us/office/dev/add-ins/testing/test-debug-non-local-server).

The Home ribbon has a Dynodoc group and **Open provenance** button. The task pane
also remains available through Office's add-in menu. Word requires WordApi 1.3,
Excel requires ExcelApi 1.13, and PowerPoint requires PowerPointApi 1.4.
Save the local file before connecting. Keep the pane open while capturing changes.

**PowerPoint 2021 is not supported by the current shape-text connector.** It lacks
PowerPointApi 1.4. Use a supporting Microsoft 365 build or Office 2024 for that
connector; older installations can use edited-file push through Projects.
[Microsoft's requirement table](https://learn.microsoft.com/en-us/javascript/api/requirement-sets/powerpoint/powerpoint-api-requirement-sets).
Word/Excel eligibility also depends on their requirement sets and Office policy;
an installed executable or valid manifest does not prove a working add-in.

The file can live on your local disk. The panel is hosted over HTTPS inside Office;
loading it, signing in and syncing require connectivity. Enabled capture saves a
local queue while the pane is open. Complete offline commits/exchange and background
capture are still pending. Test originals must be at most 3,000,000 bytes.

## Google Docs, Sheets and Slides

Download and unzip the matching Google package. Create a separate Apps Script
project for that host. Add each `.gs` and `.html` file, preserving its name, and
replace the project's `appsscript.json` (enable manifest visibility in settings).
Use **Deploy > Test deployments > Editor add-on**, choose a test document,
save the test and execute it. Open the add-on's **Open provenance** menu item.
Google may ask you to authorize access to the current document and external requests.
This is Google's Editor add-on test workflow; it is not a published installation link.
See [Google's test-deployment guide](https://developers.google.com/workspace/add-ons/how-tos/testing-editor-addons).

## Sign in and connect

1. In the pane/sidebar select **Sign in to Dynodoc**, then open its browser link.
2. Sign in with your Dynodoc email. Check that the browser's comparison code
   matches the one in the editor. Choose the matching remote file and approve.
3. Return to the editor and select **I approved this connection**. Enable capture
   explicitly. The working copy must match the supported checkpoint structure/text.
4. Edit supported content, capture, push for review, then merge through the web
   review workflow with an authorized account. Preview and apply team changes in
   the other editor. Revoke the connection under **Sync & connections** to test
   that further access stops. Connection grants expire after seven days.

No account-wide token is given to the editor. The Office pane keeps its key in memory;
Google stores the approved key in that user's private properties for the local file.
Reopening Office requires sign-in again (or the existing manual connection key).
Capture queues remain saved. Existing queues and unrelated file bindings are not replaced.

## Limits and alternate servers

Current adapters capture existing paragraph text, cells/formula sources/date-only
values, and individual slide shape text. They do not synchronize arbitrary layout,
structural edits, embedded objects, animations or every keystroke. Real host acceptance
for this release remains required. Keep an original copy when testing pull/apply.

For a staging server, use HTTPS with a certificate trusted by the editor. Replace
the workspace origin in **all** URLs of each Office manifest; replace
`DYNODOC_ORIGIN` in the Google host's `Code.gs` and its `urlFetchWhitelist` in
`appsscript.json`. Do not disable TLS checks. The approval link is generated from
that same origin. Use a matching API/web build, migrations through 0026 and your
normal email sign-in configuration. Public shared-checkpoint writes stay opt-in.

## Publication

Sideloading and Apps Script test deployments do not require a public marketplace
listing. Public Office installation goes through Microsoft Partner Center and
Microsoft Marketplace certification. Public Google installation uses a standard
Cloud project, OAuth configuration and the Google Workspace Marketplace SDK/listing;
scope-dependent OAuth verification and Marketplace review are separate steps.
Organization-only distribution has platform-specific admin options. See
[Microsoft publication](https://learn.microsoft.com/en-us/office/dev/add-ins/publish/publish)
and [Google publication](https://developers.google.com/workspace/add-ons/how-tos/publish-add-on-overview).
Current downloads remain development packages, not approved marketplace listings.
