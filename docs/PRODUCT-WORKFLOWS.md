# Product workflows and boundaries

This release extends the hosted workspace contract in decision 002. All mutations
below preserve the append-only semantic event log. Existing documents are neither
published nor moved to Trash by deployment.

## Access and recoverable removal

Create an organization from **Manage team workspaces**, select it to create a team,
then select a team to create a folder. Select a workspace before creating/importing
a document, or move an existing document through **Share → Folder and inherited access**.
Organizations, teams and folders support owners, managers, editors, reviewers and
viewers. Grants inherit downward; the strongest direct/inherited role wins. A direct
viewer grant cannot restrict someone who inherits editor access. Remove the grant
at its source. Direct document invitations remain when a document moves.

Document creators and inherited owners/managers can manage sharing, public copies,
location and Trash. Editors can change content but cannot grant access or remove
the document. The creator's direct ownership remains after a move. A manager cannot
promote someone to owner or remove an owner. The last direct workspace owner is
protected. Workspace nesting is limited to twelve levels.

**Move to Trash** revokes public links and blocks ordinary reads, writes, search and
future event delivery. **Trash → Restore document** restores content/history, but
revoked links stay revoked. This is recoverable removal, not permanent erasure.

At `admin.dynodoc.online`, an allowlisted administrator can search accounts,
documents and the operations log. Disable an account or move a document to Trash
by typing its exact displayed email/title. Administrators cannot disable themselves
or another configured administrator. Restored accounts need fresh sign-in: previous
access tokens, refresh tokens and pending email links remain invalid. Content,
contribution attribution and backups are retained. A permanent-erasure workflow
requires a separately designed retention process; the UI does not claim to provide one.

## Public copies

Use **Share → Publish a read-only copy** after saving the team version. Personal
drafts must be shared first. Choose 7, 30, 90 days or explicit revocation. The full
random link is shown once; only its hash is stored. Anyone holding it can read the
captured title/content without an account. Later edits require a new published copy.
Comments, suggestions, deleted content, source-connection metadata, event history
and original uploads are excluded. Public pages are noindex, no-store and no-referrer;
VPS access logs redact the token path. Revocation cannot recall previously downloaded
copies. Public links may expose research data: the owner must choose appropriate content.

## Editors and onboarding

- Documents: tabs, rich formatting, checklists, superscript/subscript, table actions,
  inline/display LaTeX and context-menu comments. Word export follows tab order and
  retains equations as LaTeX text, not native Word equation objects.
- Spreadsheets: row/column insertion and deletion adjust formulas across sheets;
  stable row identities survive moves. Rename, duplicate, reorder and delete sheets
  using tab context menus or **Sheet actions**. Deleted references remain `#REF!`.
  Sorting remains values-only; charts, pivots and full Excel fidelity are not implemented.
- Questionnaires: broad XLSForm type catalog, groups/repeats, external-choice file
  names, appearances, parameters, validation and custom translation columns. This
  authors definitions for destination collection tools; native preview executes only
  its documented subset. Attach media/datasets and validate in ODK/Kobo before fieldwork.
- LaTeX: import `.tex` or a source ZIP; export `.tex` or an Overleaf project ZIP.
  Unknown commands remain editable source blocks. No server-side TeX execution,
  Overleaf account sync, bibliography engine or full project round trip is provided.
- **Help & tutorials** reopens the welcome walkthrough. `/help` explains how personal
  drafts, team versions, comparisons and restoration map to version-control concepts.

## Operational API additions

Authenticated routes use the existing session gateway at `/api/engine`:

| Route | Purpose |
| --- | --- |
| GET/POST `/spaces` | List accessible hierarchy / create a workspace |
| GET/POST `/spaces/:id/members` | Inherited/direct membership / update a direct grant |
| GET `/workspace/index?trash=&space_id=&offset=` | Accessible documents, 100 per page |
| GET `/documents/:id/access` | Effective role, management capability and location |
| POST `/documents/:id/location` | Move using `{space_id}`; null means personal |
| POST `/documents/:id/trash` | `{deleted: true/false}` |
| GET/POST `/documents/:id/public-links` | List / publish with `{base_seq, expires_days}` |
| POST `/documents/:id/public-links/:link/revoke` | Revoke a published copy |
| GET `/public/documents/:token` | Anonymous frozen content, no private history |
| GET `/admin/records?kind=&q=&offset=` | Accounts/documents/audit, 50 per page |
| POST `/admin/actions` | `{kind,id,removed,confirmation}` with admin checks |

These additions are documented here; the generated OpenAPI currently covers the
previous engine/workspace routes and does not yet describe these additional routes.
Operations audit rows are append-only, but are not part of the document's cryptographic
content chain. Database/backup access remains an infrastructure-operator capability.
