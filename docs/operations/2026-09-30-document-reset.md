# Production document reset — 30 September 2026

**Decision:** the product owner decided to permanently delete every existing document
created by all current users, keeping user accounts and sign-ins, to start the next
phase (projects, see [handoff](../proposals/PROJECTS-WORKSPACE-HANDOFF.md)) from a clean
workspace.

**Method:** the same audited path as the admin panel's permanent deletion
(`/admin/erase`, decision 009), run as one database transaction on the VPS at
~07:02 UTC:

1. Validated backup `backups/dynodoc-20260930T070143Z.dump` (checksum OK, 29 tables).
2. Locked all 24 documents and their drafts.
3. Moved the 14 documents outside Trash to Trash (audit `document.trashed_by_admin`)
   and revoked public links; 10 were already in Trash.
4. One erasure receipt per document (reason `policy`, with per-document counts of
   events, blocks, versions, snapshots, drafts, files, bytes and public links) and one
   audit entry `documents.erased` noting the owner's decision.
5. Deleted the documents; the erasure trigger removed their history, blocks,
   snapshots, versions, drafts, uploaded files and public copies, and foreign keys
   cascaded to fingerprints, copy matches, review rules, reviews, notifications and
   access rows.

**Result (verified after commit):** documents, events, blocks, snapshots, uploads,
versions, drafts, public links, fingerprints, notifications and access rows: 0.
Accounts: 7 (unchanged; one was already disabled — the 27 September synthetic test
account). Active sign-in sessions: 32 (unchanged). Workspaces: 0 before and after.
Erasure receipts: 26 (24 new, 2 from earlier admin deletions that morning). The
workspace stayed healthy (HTTP 200) throughout.

**Still retained:** pre-reset database backups on the VPS (`backups/`, including
`dynodoc-20260930T063321Z.dump`, `…T070104Z.dump` and `…T070143Z.dump`) contain the
deleted documents until the retention job removes dumps older than 14 days. They are
the only way to recover this content. Deleting them early is a separate owner
decision. Operation audit entries and erasure receipts remain by design; they hold
identifiers and counts, not content.
