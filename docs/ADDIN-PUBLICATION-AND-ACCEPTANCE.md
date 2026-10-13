# Editor installation, acceptance and publication

10 October 2026, updated 12 October 2026. Distribution is separate from Dynodoc
sign-in and file permissions. The manifests and add-on sources are now shaped for
store submission (production names, store icons, plain-language copy), but none is an
approved marketplace listing: no publisher account was registered and nothing was
submitted. The submission kit is in [`docs/listings/`](listings/SUBMISSION-CHECKLIST.md).

## Where registration is needed

| Audience | Microsoft Word/Excel/PowerPoint | Google Docs/Sheets/Slides |
| --- | --- | --- |
| Developer testing | Sideload the XML in a compatible desktop/web host; no public listing required. | Use an Apps Script Editor add-on test deployment; no public listing required. |
| One organization | A Microsoft 365 administrator can deploy a custom add-in through Integrated apps, subject to tenant/client eligibility. | A Workspace organization can use a private Marketplace listing; domain/admin restrictions apply. |
| Public self-service installation | Register/use a Partner Center publisher account, submit the Office add-in and pass Marketplace certification. | Use a standard Google Cloud project, configure OAuth and Marketplace SDK/listing, and submit for public review. OAuth verification depends on the scopes/audience and is a separate process. |

Microsoft's [deployment options](https://learn.microsoft.com/en-us/office/dev/add-ins/publish/publish)
and [Partner Center publication process](https://learn.microsoft.com/en-us/office/dev/add-ins/publish/publish-office-add-ins-to-appsource)
describe the Office paths. Google's [publication overview](https://developers.google.com/workspace/add-ons/how-tos/publish-add-on-overview),
[OAuth configuration](https://developers.google.com/workspace/marketplace/configure-oauth-consent-screen)
and [Marketplace SDK configuration](https://developers.google.com/workspace/marketplace/enable-configure-sdk)
describe the Google path. References checked on the date above; verify again before
submission. Do not promise approval, completion dates or fees from these notes.

## Current package inventory

Office uses three XML task-pane manifests, version `1.1.0.0`, named **Dynodoc for
Word**, **Dynodoc for Excel** and **Dynodoc for PowerPoint** (the earlier
`1.0.0.0` development manifests were all called Dynodoc Provenance (Development)).
Each has a plain-language description under 250 characters, `SupportUrl`
`https://dynodoc.online/support` (a page the site work provides; confirm it is live
before submitting), a 32 px `IconUrl`, a 64 px `HighResolutionIconUrl`, and 16/32/80 px
ribbon icons, all served from `https://app.dynodoc.online/connectors/assets/`. The
Home ribbon shows a Dynodoc group with a Dynodoc button. Word requires WordApi 1.3,
Excel requires ExcelApi 1.13, and PowerPoint requires PowerPointApi 1.4. Keep IDs
stable when updating the same listing and increment manifest versions for manifest
changes:

- Word: `adbda646-1a13-4a70-baf5-c772e9cc90a6`.
- Excel: `794ef9dc-6d78-4b08-a7f3-2526d0476930`.
- PowerPoint: `28155d15-5a0e-4b94-b0ce-c448d684d125`.

Office manifests request ReadWriteDocument and point to the hosted Dynodoc pane.
The current authentication is browser approval of a file-scoped connector key;
it does not use Microsoft SSO/Graph. Marketplace publication alone does not require
adding those unrelated permissions or replacing the existing approval flow.

Google has three separate Apps Script source packages. Their scopes are the host's
`documents.currentonly`, `spreadsheets.currentonly` or `presentations.currentonly`,
plus `script.container.ui` and `script.external_request`. Their external-request
allowlist is the Dynodoc connector API. Each manifest sets `runtimeVersion: V8` and
`exceptionLogging: STACKDRIVER`, and each add-on appears under Extensions > Dynodoc >
Open Dynodoc. Select the listing/script/project mapping before submission: one
standalone script and one listing per editor, so three ZIP downloads are three
listings, not one multi-host add-on. Do not broaden scopes merely to simplify
distribution.

Saved queues use Apps Script UserProperties: about 500 KB per user and script, shared by
every file, against a 100 KB per-file queue cap and daily property quotas. A full store
stops capture with a message and keeps the previous queue; this is documented in the
listing copy rather than hidden. There is no in-add-on clear action.

## Store assets and listing copy

| Item | Where |
| --- | --- |
| Per-host listing fields, long description, scopes/permissions with reasons, reviewer steps, screenshot shot-list | [`docs/listings/<host>.md`](listings/) |
| Owner-only account, verification, form and versioning steps | [`docs/listings/SUBMISSION-CHECKLIST.md`](listings/SUBMISSION-CHECKLIST.md) |
| What each add-in stores, sends and keeps (privacy policy input) | [`docs/listings/addin-data-handling.md`](listings/addin-data-handling.md) |
| Icons, 300 px logo, 128/96/32 px Google icons, 220 x 140 and 440 x 280 banners | `extensions/marketplace/assets/`, built by `pnpm build:marketplace-assets` |
| Offline manifest/scope/asset invariants | `frontend/researcher/src/lib/connectors/*.test.ts` |
| Microsoft schema validation (online; not in CI) | `pnpm validate:manifests` |

Store screenshots are not generated: they must be real captures from installed hosts.

Microsoft's `office-addin-manifest validate` reported all three manifests valid on
12 October 2026 and listed eligible platforms from the requirement sets alone (for
example Word and Excel on iPad, Mac 2016/2019 builds); that is eligibility, not a
runtime result. Validation also confirmed the support URL, HTTPS icon URLs and file
extensions. The offline checks in `src/lib/connectors/` re-verify names, limits, GUIDs,
URL origins, icon pixel sizes, scopes and Google manifest settings on every test run.

## Concrete publication work remaining

1. Owner supplies the appropriate account access: Microsoft Partner Center, Google
   Cloud/Workspace organization and responsible support contact. The publishing identity
   is Verne Systems (trading as Verne), with support at info@vernesystems.com and
   +880 1870 034384 (supplied 13 October 2026; the policies are still unadopted drafts).
   Existing OAuth web sign-in credentials are not proof of publisher setup.
2. Complete the host acceptance matrix below and retain exact app builds, manifest/
   script versions and evidence. Correct unsupported-platform declarations first.
3. Capture host-specific screenshots from installed hosts and provision reviewer test
   access. Display names, descriptions, icons and support instructions are drafted in
   `docs/listings/` and `extensions/marketplace/`. Review existing `/privacy` and
   `/terms` against [addin-data-handling.md](listings/addin-data-handling.md) and actual
   remote storage, retention, account erasure and third-party processing; their
   existence is not legal approval.
4. Configure Google standard projects, consent branding/scopes, script versions and
   SDK integration/listing. Complete any required OAuth verification separately
   from Marketplace review. Choose organization-only/public visibility deliberately.
5. Prepare Microsoft submissions in Partner Center and Google draft listings.
   Record application IDs, scope sets, version mapping and review feedback.
   Public submission/organization-wide rollout requires the owner's concrete
   publication decision; this document is preparation, not an instruction to publish.

## Desktop preflight on this workstation

Read-only inspection on 10 October found Word, Excel and PowerPoint executables at
`16.0.14326.20348`, x64, product `ProPlus2021Volume`. This is a local installation
inventory, not a successful sideload or runtime test. The connected-document tool
reported no document sessions; no Office UI was controlled or real account used.

The current PowerPoint connector cannot be accepted on this installation:
PowerPointApi 1.4 requires a supporting newer build, with volume-licensed support
listed for Office 2024. Office 2021 has only the earlier PowerPoint sets. See
[Microsoft's PowerPoint table](https://learn.microsoft.com/en-us/javascript/api/requirement-sets/powerpoint/powerpoint-api-requirement-sets).
Word/Excel appear eligible for the declared API baselines; installation policy,
runtime API detection, task-pane loading and authentication still need testing.
Do not reduce the PowerPoint requirement to force installation: the adapter uses
the newer shape APIs. The existing edited-file workflow is the older-host fallback.

## Acceptance matrix and procedure

The [native acceptance worksheet](https://github.com/ArefinAlter/dynodoc/blob/main/extensions/ACCEPTANCE.md) provides a run record,
synthetic test contents and thirteen reproducible cases for all three editor pairs.
It is included in each Google test ZIP. On 11 October the document-control connector
again reported no connected document sessions; runtime acceptance remains pending.

Use disposable files no larger than 3,000,000 bytes and two permitted test identities.
Keep credentials and document contents out of committed evidence. Record status as
not run, failed (with reason), or passed; manifest validation is a separate result.

| Host | Current runtime acceptance |
| --- | --- |
| Word desktop Windows/Mac and web | Not run; Windows executable inventory only. |
| Excel desktop Windows/Mac and web | Not run; Windows executable inventory only. |
| PowerPoint desktop Windows/Mac and web | Not run; installed Windows 2021 fails the declared API prerequisite. |
| Google Docs / Sheets / Slides | Not run inside actual Editor add-on hosts. |

For each supported host/version:

1. Install with [the platform-specific instructions](https://github.com/ArefinAlter/dynodoc/blob/main/extensions/INSTALL.md),
   verify the native ribbon/menu and task pane, and check required API support.
2. Use editor sign-in, verify the comparison code in the browser, approve the right
   Project file, return to the host and enable capture. Test cancellation/expiry.
3. Edit supported content; capture and inspect its local queue. Push from two users,
   compare/review/merge in Projects, then preview and apply changes in the other host.
   Verify unsupported structural/format changes stop or use the file workflow.
4. Save, close and reopen a disposable file/pane. Check stable binding and queued
   data; Office requires sign-in again because its secret is held in pane memory.
   Disconnect networking to document actual queue/retry behavior. Do not claim a
   full offline commit graph or capture while the pane is closed.
5. Exercise conflicts, rejected/stale changes, access revocation and grant expiry.
   Confirm pending local work remains recoverable without widening access.
6. Remove only the test installation/registration and owned fixtures. Leave the
   user's existing documents, add-ins, trusted catalogs and Office policies intact.

Public review should follow successful host tests and completed listing materials.
Bounded storage, portable commits and offline convergence can continue independently
while interactive host/account setup is completed.
