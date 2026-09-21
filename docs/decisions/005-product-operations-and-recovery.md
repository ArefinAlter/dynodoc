# Decision 005 — Web product operations, erasure and recovery

Accepted direction: 21 September 2026. The owner explicitly approved the deletion
policy below and deferred off-site backups. Dynodoc remains web-based; no desktop
or mobile-native client is part of this increment.

## Whole-resource erasure

Ordinary editing, restoration and Trash remain append-only. A separate,
allowlisted administrator operation can permanently erase an entire document from
the live database, including files/fonts, nodes, canonical events, snapshots, named
versions, personal drafts, public copies and related cached idempotency responses.
The document must already be in Trash. A current impact preview, exact title,
reason category and backup acknowledgement are mandatory. The server locks and
rechecks the resource, then writes a minimal receipt and erases it atomically.

Migration 0017 permits child-history deletion only inside the parent erasure
trigger and with a matching receipt created in that transaction. Event UPDATEs
remain prohibited. Direct event deletion fails even when a receipt has been
inserted. Runtime code never disables triggers. This is an explicit exception to
physical append-only retention, not an editing or history-rewriting feature.

Account erasure requires prior disabling and transfer/removal of owned documents
and workspace ownership. Configured administrators cannot be erased through the
UI. Sessions, pending email links, memberships and private drafts are removed;
email/display name become a deleted-account identity. The UUID remains so shared
contributions and their hashes remain verifiable. Reusing the original email
creates a fresh identity with no inherited access. Erased identities cannot be
restored. Historical documents may still contain user-authored personal data;
profile deletion is not content redaction across other people's documents.

Deletion receipts retain resource UUID, resource kind, administrator UUID, reason,
aggregate counts and timestamp. They do not copy a deleted title, email, file or
content. Existing administrative records remain under their retention policy.
Already downloaded/publicly copied data cannot be recalled. Existing backups
expire normally; this release does not promise immediate deletion from backups.
No live records are erased as part of deployment or verification.

## Administrative controls

The dashboard adds ownership transfer, forced sign-out, public-link revocation,
search/pagination, current-page CSV exports and deletion previews/receipts.
Infrastructure panels remain diagnostic; there is no arbitrary SQL, shell access,
Docker socket or database-drop button exposed to the browser. Operational actions
have a separate immutable audit, not the document content chain.

## Discussions

Migration 0018 adds `CommentReplied` and `CommentResolved`. Roots continue to be
`CommentAdded` events. Replies/resolution use canonical event IDs and optimistic
thread revisions, membership checks and the document write lock. Reviewers can
reply; authors resolve/reopen. Resolved threads must be reopened before a reply.
Bodies are limited to 10,000 UTF-8 bytes and threads to 100 replies. Pagination
returns 30 roots, optionally filtered to a stable node. Public copies exclude
review discussions. Mentions and notification delivery remain future work.

The structural materializer recognizes review events without altering nodes;
authorship and discussion status are projected from canonical events. Old event
bytes and hash framing are unchanged. Browsers now use `office-v3`. Deploy these
readers and the gateway before enabling the new backend writers. Rollback after
new events exist requires a backend that understands these variants; do not roll
back to an incompatible executable or rewrite history to accommodate it.

## Recovery and selected sharing

Browser recovery is opt-in on a trusted device and uses IndexedDB. It keeps the
original base/revision and local state, scoped to identity/document/draft. Restoring
requires current server access, explicit review and ordinary conflict checks.
Copies are capped at 10 MiB each and 30 total; copies older than seven days are
removed when recovery storage is accessed. Browser storage may be evicted. It is
not encrypted storage or a backup service. Signing out disables writes across tabs
and clears copies; the preference must be enabled again after sign-in.

Migration 0019 adds private per-draft excluded-node preferences. Sharing selected
changes can remember unchecked blocks, retaining prior exclusions for unchanged
blocks. Later default sharing excludes them until explicitly included again.
Preference changes are draft metadata; they do not rewrite canonical history or
implement a distributed Git repository.

## Scope and release evidence

Off-site backups are explicitly deferred by the owner; existing on-server backup
and isolated restore practices still apply. Legal operator/address/jurisdiction
were not supplied and must not be invented. Real OAuth/email delivery, provider
study acceptance and native IME/screen-reader checks cannot be inferred from
synthetic tests. See PRODUCT-OPERATIONS-RELEASE.md for actual verification results.
