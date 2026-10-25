# Editor add-ins: what is stored, sent and kept

Written for whoever maintains the privacy policy and the marketplace data-handling
forms. It describes what the **add-in code in this repository** does, taken from
`extensions/` and `frontend/researcher/src/lib/provenance/`. It is not legal advice and
it does not describe the rest of the Dynodoc service (see `docs/LEGAL-PAGES.md` and
the live `/privacy` page for that). Re-verify against the build you submit: if the
code changes, this file must change with it.

Applies to six add-ins: Dynodoc for Word, Excel and PowerPoint (Office task panes
hosted at `https://app.dynodoc.online/connectors/<host>`), and Dynodoc for Docs, Sheets
and Slides (Google Apps Script editor add-ons with a sidebar).

## 1. Summary

| Question | Office add-ins (Word, Excel, PowerPoint) | Google add-ons (Docs, Sheets, Slides) |
| --- | --- | --- |
| Where does the code run? | In Office's embedded web view, loaded from `app.dynodoc.online` | Apps Script runtime (server side) plus a sidebar page served by Google |
| Where is the capture queue kept? | Browser database (IndexedDB) of the Office web view, on the user's device | Apps Script **UserProperties** of the user who installed the add-on |
| Where is the connection key kept? | In the pane's memory only; gone when the pane closes | In UserProperties of that user, per file (per tab for Docs), until replaced |
| What leaves the device? | Changed text and metadata, to `https://app.dynodoc.online/api/connectors/` only | The same, sent by the Apps Script runtime to the same origin only |
| Other network calls | Loads Microsoft's Office.js library from `appsforoffice.microsoft.com` | None by the add-on; Google's own services as the host |
| Analytics, ads, third-party SDKs | None in the add-in code | None in the add-on code |
| Cookies / local storage written by the add-in | None | None |
| Change to the user's file | Identity markers only (section 5) plus text the user chooses to Apply | Identity markers only (section 5) plus text the user chooses to Apply |

## 2. What the add-in reads from the open file

Only what its adapter covers, and only while the pane or sidebar is open:

- **Word / Docs:** the text of body paragraphs (Docs: of the active tab). Up to 2,000
  paragraphs; any table, list, image or embedded object makes the adapter stop with a
  message instead of reading further.
- **Excel / Sheets:** values, formula text and date-only values of the cells in
  Dynodoc-mapped rows, up to 2,000 projected cells; sheet names; whether cells are merged;
  number formats. Sheets also reads the spreadsheet's time zone.
- **PowerPoint / Slides:** the text of existing text shapes and the identity of slides
  and shapes, up to 2,000 shapes.
- File name or title: the Office pane reads the saved file's URL to detect Save As
  (it is hashed, never sent). The title of the Dynodoc file (not the local file name) is
  copied into change bundles as `document_name`.

It does not read other files, other tabs (Docs), the user's contacts, mail, calendar,
Drive listing or account profile, and it requests no scope that could.

## 3. What is stored locally

### Office (IndexedDB)

- Database `dynodoc-provenance`, object store `ledgers`, in the origin
  `https://app.dynodoc.online` inside the Office web view. One record per
  Dynodoc file and local working copy, keyed
  `dynodoc.<host>.ledger.<origin>.<dynodoc file id>.<SHA-256 of the saved file URL>`.
- A record holds: the Dynodoc checkpoint the file was bound to (the retained text
  of that file's synced fields, as served by Dynodoc), the last observed text of every
  covered paragraph/cell/shape, the list of observed changes with timestamps and
  stable ids, any frozen upload awaiting delivery, the last upload receipt, and
  archives of earlier base checkpoints. It contains no connection key.
- Written only while capture is enabled or when the user chooses Capture, Push, Preview
  or Apply. Kept until the user clears the Office web view's storage, removes the
  add-in, or the host discards it. There is no automatic expiry and no in-product
  delete button; **Export local provenance** produces a JSON copy.
- The `/connectors/*` pages are sent with `Cache-Control: private, no-store` and
  `Referrer-Policy: no-referrer`.

### Google (Apps Script UserProperties)

- Stored with `PropertiesService.getUserProperties()`: private to the Google user,
  never DocumentProperties or ScriptProperties, so collaborators on the file cannot read
  them.
- Keys: `dynodoc.<file id>[.<tab id for Docs>].credential` (Dynodoc file id and
  connection key), `.ledger` (manifest) and `.queue.<generation>.<n>` (queue chunks of
  at most 2,000 characters); Slides also keeps `.binding...` (mapping from Google
  slide/shape ids to Dynodoc ids). Queue content is the same as for Office, above.
- The per-file queue is capped at 100 KB. Google caps the whole property store at about
  500 KB per user and script and applies daily read/write quotas; when the store is full
  capture stops with a message and the previous saved queue stays intact.
- Kept until removed. The add-on has no automatic expiry and no delete action;
  **Export local provenance** produces a JSON copy. What Google does with a user's
  script properties when the add-on is uninstalled or its access revoked is Google's
  platform behaviour: confirm it before the policy states anything about it.

## 4. The connection key

- Created in the pane or sidebar as 32 random bytes. Only its SHA-256 hash is placed in the
  approval URL that opens in the user's browser; the key itself is sent only in an
  `Authorization: Bearer` header from the editor to Dynodoc.
- The user approves in the browser as a signed-in Dynodoc user, chooses one file and
  confirms a code shown in both places. Dynodoc stores a grant (file, host, account,
  hash, expiry, revocation), not the key.
- The key authorizes read and propose for that one file and host only. It cannot merge,
  grant access or act as an account session. It expires seven days after approval and can
  be revoked in **Sync & connections**; revocation stops access immediately.
- Office: key held in memory; reopening the pane needs sign-in again. Google: key stored
  as above and replaced on re-connection. The manual **Advanced: connect with a key** path
  uses a key created in the same Sync & connections screen with the same scope and expiry.
- No Dynodoc password, session cookie or account-wide token is ever given to the add-in.
  API calls from the pane use `credentials: 'omit'`.

## 5. What is written into the user's file

- **Word:** each covered paragraph is wrapped in a content control titled
  `Dynodoc paragraph` with tag `dynodoc.block.<ULID>`.
- **Excel:** workbook-scoped named ranges `dynodoc_<ULID>`, one per mapped row.
- **PowerPoint:** a tag `DYNODOC_ID` on each slide and mapped text shape.
- **Docs:** named ranges `dynodoc.block.<ULID>` in the active tab.
- **Sheets:** named ranges `dynodoc_<ULID>`, one per mapped row.
- **Slides:** nothing visible; the mapping is in the user's own properties.
- **Text changes:** only when the user chooses *Apply preview*, after a compare-before-write
  check, and only for text that has been reviewed and merged in Dynodoc.

These markers travel with the file when it is shared or exported; they contain
identifiers, not document text.

## 6. What is transmitted, to whom, and when

All Dynodoc requests go to `https://app.dynodoc.online/api/connectors/` over HTTPS.
The Google manifests enforce this with `urlFetchWhitelist`.

| Request | When | Contents |
| --- | --- | --- |
| `GET /api/connectors/connection` | After the user says they approved | Bearer secret; response: Dynodoc file id, grant id, host |
| `GET /api/connectors/documents/<id>/checkpoint` (optionally `?through_seq=n`) | On connect, preview and recovery | Bearer key; response: the file's retained synced text and history position |
| `POST /api/connectors/documents/<id>/bundles` | When the user chooses Push for review | Bundle id, client id (random per queue), file id, base revision and hash, capture host and mode, the Dynodoc file title, change request name and note typed by the user, and for each change: node id, field, new text/value, observed time |

Nothing is sent while the user only edits, and nothing is sent in the background. A push
is retried with identical contents; it is not split or enriched. The add-in sends no
device identifiers, no user agent beyond what the host's network stack adds, no
telemetry and no document text outside a push the user started.

Third parties touched by the add-in itself:

- **Microsoft** (Office add-ins): the pane page loads `https://appsforoffice.microsoft.com/lib/1/hosted/office.js`
  and runs inside Office, which has its own data practices. The pane's Content Security
  Policy adds only Microsoft's Office.js hosts (`appsforoffice.microsoft.com`,
  `officeapis.public.onecdn.static.microsoft`, `ajax.aspnetcdn.com`) to the app's own
  origin for scripts and connections; there is no other third-party origin by default.
- **Google** (Google add-ons): Apps Script, PropertiesService, LockService, UrlFetch and
  the sidebar are Google's platform. Google's logs may record execution errors because
  the manifests set `exceptionLogging: STACKDRIVER` (Cloud Logging for the owner's
  Cloud project).
- The user's **browser** carries the approval step; sign-in itself is Dynodoc's
  existing email or Google sign-in, described in the main privacy policy.

## 7. What the server keeps as a result (pointer only)

A pushed bundle becomes a proposal in the project's append-only, hash-chained history
and is visible to project members with the relevant role. A grant record is kept
with its hash, expiry and revocation. Retention, erasure and backup behaviour of that
server-side data is governed by the main privacy policy and `docs/LEGAL-PAGES.md`; this
document does not restate or extend it. Note for the policy author: text that was pushed
cannot be silently removed from retained history, and capture queues on the user's
device or in their Google properties are outside Dynodoc's control.

## 8. Things the add-in does not do

- No analytics, advertising, fingerprinting, A/B testing or error-reporting service.
- No reading of files other than the one open, and no content from other apps.
- No background capture: nothing is recorded while the pane or sidebar is closed.
  Edits made in that interval are not reported as observed changes.
- No Microsoft Graph, Microsoft single sign-on, Google Drive, Gmail or Calendar access.
- No sale or sharing of data. (The commercial and processor position of the service
  as a whole is the privacy policy's to state.)
- It does not prove who typed a change; it records observations at snapshot times.

## 9. Open points for the legal operator

- Retention wording for local queues (no expiry today) and whether to add an in-add-in
  clear/disconnect action: not built.
- Whether Google's OAuth consent screen and the Microsoft certification form need
  additional data-use declarations beyond sections 2 to 6.
- The operator identity, address and contact (Verne Ltd., info@vernesystems.com,
  +880 1870 034384) are published on the legal pages as of 13 October 2026; the pages
  remain unadopted drafts without an effective date, and the hosting provider and
  location are still unconfirmed (see `docs/LEGAL-PAGES.md`).
