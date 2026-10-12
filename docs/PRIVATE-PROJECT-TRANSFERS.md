# Private project history transfers

12 October 2026. PostgreSQL schema 35; portable commit encodings and SQLite schema
2 are unchanged. This is an authenticated private staging protocol. Uploading a
commit does not change a project branch, create a review or update any team file.
Protected shared refs, publication receipts and native connector adoption remain
the next dependencies. These sources are not deployed to the schema-29 pilot.

A signed-in account can upload typed objects, resume by asking which supplied
addresses are missing, stage signed multi-file commits and retrieve those exact
commits on another device. All dependencies must already have been staged by the
same account. The server verifies signatures, dependency scope, recovery semantics
and complete typed file graphs before acknowledging a new commit. Accepted history
survives signing-key revocation; new commits need an active enrolled key belonging
to the current account. A self-declared signing time is not a trusted timestamp.

## Access and privacy

The endpoints accept ordinary account bearer sessions, not file connector grants.
Every transaction locks and rechecks the current account/session, document rows
and containing project/folder membership. Uploads and new commits require project
and file contribution rights. Downloads and exact accepted retries require current
read access. Trashed files, removed membership and files moved out of the project
make their staged history unavailable. A file's direct owner still needs project
membership. Project review protection is untouched because nothing is published.

Bytes are deduplicated in the existing document-scoped shared store. A separate
immutable acknowledgement records which account supplied each exact object in
each project. Knowing a hash or having file access does not expose another
person's drafts, cached snapshots or unsubmitted uploads. The upload response does
not disclose global reuse. Missing-object queries and inventory consult only the
current account's acknowledgements. There is no global commit/object directory.
The authenticated private inventory includes that account's uncommitted uploads;
it must never become a project-member or public discovery endpoint.

Commit bodies contain paths, messages and document IDs. A stored commit's access
scope therefore includes every file from its parents and recovery dependencies,
even if it deletes those files. Reading the entire signed envelope requires current
access to that whole scope. This conservative rule avoids leaking old metadata via
descendants, but moving/removing a historical file can block an otherwise readable
later commit. Partial redacted commit envelopes would not preserve the signature.

Audited whole-document erasure physically removes all staged envelopes whose
transitive scope contains the document, and its upload acknowledgements, before
the shared bytes are erased. Even empty-tree descendants disappear. Audited account
erasure removes its private envelopes/acknowledgements; shared document objects
remain governed by the document's erasure policy. Audit records retain commit
addresses, without filenames or messages. Arbitrary copies of sensitive text into
another file's content/message cannot be automatically detected or purged.

## Bounded storage and work

| Boundary | Limit |
|---|---|
| Upload body | 1,500,000 encoded HTTP bytes; 64 objects and 1 MiB decoded per atomic batch |
| Negotiation/inventory | 256 addresses; inventory uses an exclusive address cursor |
| Download | One verified object, at most 1 MiB before base64 encoding |
| Signed envelope | Existing commit limit plus 1024 bytes (1,049,600 bytes) |
| Transitive file scope | 256 distinct historical documents per staged commit |
| New commit graph verification | 65,536 object reads / 64 MiB encoded reads across all files, including reuse |
| Height | 100,000 parent edges |
| Persistent account staging | 4 GiB logical bytes, 1,000,000 object acknowledgements, 100,000 commits across projects |

Logical staging bytes charge every acknowledged object once per account/project,
plus each encoded signed envelope and 16 bytes per scope UUID. This bounds grant
metadata and commit retention even when physical objects already exist. Physical
shared objects retain schema-34 creator/document quotas. Repeated acknowledged
uploads and accepted commits cost zero additional quota. Quota errors return
HTTP 507 / `storage_limit`; the failed batch and its counters roll back together.
Previously successful transfer batches remain available for resume.

The runtime cannot modify limits, counters or retained staging rows. Only the
operator can deliberately change capacity; audit-controlled erasure releases it.
There is no automatic expiry, eviction or garbage collection yet. These budgets
do not cover indexes, WAL, backups, original files, events or legacy JSON states.
The 3,000,000-byte native pilot upload cap remains unchanged.

Commit validation holds locks and reconstructs one file state at a time. It still
scans complete graphs, with bounded semantic decoding; this is not incremental or
constant-memory validation. Existing process admission, 30-second request deadline,
10-second SQL statement timeout and 3-second lock timeout apply. Per-account
serialization and bounded 16-object prefetch are baseline choices, not a
multi-instance service-capacity claim.

## HTTP contract and developer workflow

Let `P` be the project UUID and `D` an existing document currently in that project.
Use TLS and an account `Authorization: Bearer ...` session. Never place session or
private-key material in URLs, committed scripts or logs.

1. Enroll a device using [the existing possession-proof flow](DEVICE-SIGNING-KEYS.md).
2. `POST /projects/P/staging/objects/missing` with
   `{"document":"D","addresses":["64-hex-address"]}`. It returns `missing`.
3. `POST /projects/P/staging/objects` with
   `{"document":"D","objects":[{"address":"64-hex-address","bytes":"standard-base64"}]}`.
   Send only needed packets in bounded batches. Order does not matter. A successful
   response says `published:false`; interrupted responses may be retried.
4. `POST /projects/P/staging/commits` with an existing `SignedCommit` JSON envelope.
   Stage parents and recovery dependencies first. A missing dependency is 404;
   an unavailable graph, invalid signature or incorrect recovery is 400.
5. On another device, `GET /projects/P/staging/commits/ADDRESS` retrieves the
   original signed envelope for the same account. Retain its address locally:
   branch-based discovery and shared publication are not implemented here.
6. `GET /projects/P/staging/objects/D?limit=64&after=ADDRESS` lists that account's
   acknowledged keys. Omit `after` initially; use `next` as the next exclusive
   cursor and stop at an empty page. This inventory is not a graph-closure listing.
7. `GET /projects/P/staging/objects/D/ADDRESS` returns a verified base64 packet.
   Import the required graphs and then the signed dependencies/commit into the
   local repository. Updating its local ref is a separate explicit action.
8. `GET /projects/P/staging/storage` reports the current account's usage and limits
   across all projects; it requires access to project P.

The CLI now exposes the matching packet primitives:

```text
engine-cli repository DB PROJECT objects --limit 64
engine-cli repository DB PROJECT objects --after-document UUID --after-address HASH
engine-cli repository DB PROJECT export-object DOCUMENT HASH
engine-cli repository DB PROJECT import-objects batch.json
engine-cli repository DB PROJECT show COMMIT
engine-cli repository DB PROJECT import-commit signed.json
```

`batch.json` uses the upload JSON shape above. Local imports verify addresses,
enforce persistent capacity and never move a branch. These CLI commands do not
contact a remote or provide a new login flow. Do not blindly publish local
inventory: it can contain unrelated staged/private objects. Automatic selected-root
push/fetch, durable remote tracking refs and shared review/publication remain work.

## Evidence and next integration

The router tests use a restricted runtime login and an isolated PostgreSQL instance.
They exercise multi-file download into a second SQLite repository, non-ASCII text
and formatting metadata, reorder/retry, foreign-account privacy, guessed private
roots, missing dependencies, permanent key revocation, current membership/Trash/
move checks, contributor/viewer roles, recovery validation, immutable tables,
quota rollback, concurrent quota spending and audited document/account erasure.
A lock-wait test removes membership while an upload waits and requires rejection.
Exact executed results are recorded in [the handoff](ENGINE-EVOLUTION-HANDOFF.md).

Next, define explicit shared candidate publication and protected project refs,
including reader-visible reachability, collaborator ancestry and reviewer rights.
Enforce current key/session/project/file authorization in that publication
transaction and issue the server ref receipt atomically. Shared discovery must
follow accepted reachability, never private acknowledgements. Then adopt the
contract in Projects and the six native connectors. These tests do not substitute
for the owner's installed-editor worksheet or certify native-format fidelity.
