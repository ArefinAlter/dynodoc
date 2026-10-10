# Dynodoc editor test installations

These are developer/test installations, not marketplace listings. Deploy the matching
Dynodoc API and web build before using them; the default server is
https://app.dynodoc.online. A source release alone does not update that server.
Make a disposable copy of a small supported file and add it to a Dynodoc Project.

## Microsoft Word, Excel and PowerPoint

Download the matching XML manifest from the workspace's `/connectors` page or
`extensions/<host>/manifest.xml`. Sideload it using the method for your Office
platform in [Microsoft's installation guide](https://learn.microsoft.com/en-us/office/dev/add-ins/testing/sideload-office-add-ins-for-testing).
The Home ribbon has a Dynodoc group and **Open provenance** button. The task pane
also remains available through Office's add-in menu. Word requires WordApi 1.3,
Excel requires ExcelApi 1.13, and PowerPoint requires PowerPointApi 1.4.
Save the local file before connecting. Keep the pane open while capturing changes.

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
that same origin. Use a matching API/web build, migrations through 0025 and your
normal email sign-in configuration. Public shared-checkpoint writes stay opt-in.
