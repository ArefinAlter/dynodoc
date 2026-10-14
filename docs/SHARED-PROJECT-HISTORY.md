# Shared project history and branch receipts

12 October 2026. Source schema 36 extends the account-private transfer protocol
with explicit publication, protected `main` updates, reviews and atomic signed
branch receipts. Portable commit/receipt encodings and local SQLite schema 2 do
not change. These APIs are separate from centralized document events and the
current per-file web drafts. The schema-29 pilot has not received this release.

## Publication and access

Upload objects and stage a signed commit using [private transfers](PRIVATE-PROJECT-TRANSFERS.md).
Then explicitly publish its address. Its author must be the signed-in account
with an active enrolled device key; project and every historical file must still
allow contribution. Publish dependencies first, including recovery sources.
Staging can now use another author's already published dependency, but cannot use
another account's private staging. A contributor can privately acknowledge up to 256 reached objects from an
authorized published file state without uploading those bytes again. The whole
batch fails if any address is not reached or the private quota is exceeded; exact
retries are free. Only genuinely new/private objects need full-byte uploads. Automatic selected-root push/fetch and remote tracking remain open.

Publication revalidates all typed file graphs and records only reached object
addresses. Unrelated uploads, private drafts and guessed hashes confer no shared
access. Physical bytes remain document-scoped and deduplicated. Reachability
indexes are reused for the same project/document/state root across commits.
Publication alone does not move a branch. It deliberately shares the candidate
with currently authorized project members, including before review.

Reads require current project membership and access to the entire transitive
file scope, including removed files and recovery dependencies. Trashing, moving
or revoking access to a historical file can block the complete signed envelope.
A download addresses a published commit and one of its file states, never an
unscoped object inventory. Published history remains readable after the author's
device revocation or account erasure; new publications need an active own key.

Fetching an older commit by its address does not move a branch or lower a client
trust pin. Removing a file path from a portable manifest does not itself trash or
erase the document; earlier authorized revisions remain downloadable. Application
Trash and audited erasure are separate access/retention operations.

## Branches and review

Every update names a branch, candidate and expected `{commit,generation}`; use
`null` only for a branch that has never existed. A project advisory transaction
lock serializes publication/reviews/ref updates after account, document and space
locks. Only one competing compare-and-swap succeeds. Stale retries return 409;
fetch the head/receipt to learn whether a lost response represented success.

Branches only fast-forward through commit **parents**, not recovery references.
No branch deletion, rename, force push or implicit reset exists. A merge commit
can preserve both parents; revert/cherry-pick creates new signed recovery history.
Contributors can advance feature branches. Literal `main` is the team branch:
current project/file merge-role floors and the maximum required approvals apply
even to owners and its first publication. Other protected branch patterns remain
future work. A review merge is an explicit operation, so project protection of
direct team editing does not forbid eligible editors from merging reviewed work.

Approvals and requests for changes bind the exact candidate, branch and expected
base generation. The publisher cannot approve their own candidate. The latest
decision per reviewer applies; only currently active project reviewers with the
necessary rights on every historical file count. An eligible request for changes
blocks `main`; changing the decision appends a new row. Removed/downgraded reviewers
do not count. Identical review retries add no rows. Rules are rechecked when the
ref moves, so a file owner cannot bypass project governance. Reviewer account locks
use NOWAIT; conflicting account actions return a retryable busy response without
moving the branch.

## Atomic receipts and authority setup

Each ref event stores its existing-format Ed25519 receipt in the **same database
transaction** as the ref and audit row. It binds installation/project/branch,
commit, monotonically increasing generation, previous receipt address and declared
time. Failed permission, review, CAS, quota or transaction checks return no accepted
receipt. Responses are released only after commit. Ref reads verify the stored
signature, authority and head columns. Receipt pages contain at most 256 records.

The operator must install a public authority key with migration credentials and
mount the matching secret into the API separately. No key is generated at API
startup, no private key is stored in PostgreSQL and no unsigned ref fallback exists.
Existing document APIs/staging can run without receipt signing; shared branch
writes are refused until it is configured.

Example operator sequence (use private paths outside the repository):

```sh
openssl genpkey -algorithm ED25519 -outform DER -out /private/ref-authority.der
chmod 600 /private/ref-authority.der
engine-cli device public-key /private/ref-authority.der
# Copy that public hex value, not the secret, into the following operator command.
engine-cli install-ref-authority PUBLIC_HEX
# API environment: DYNODOC_REF_SIGNING_KEY_FILE=/run/secrets/ref-authority.der
```

`install-ref-authority` uses `MIGRATION_DATABASE_URL`, verifies the exact schema,
and accepts an identical repeated installation. A different key is refused.
The installed key is epoch 1. Schema 41 adds an append-only epoch chain: the
operator rotates with `rotate-ref-authority` (offline, both keys sign) and
`install-ref-authority-rotation`, or after loss/compromise records
`revoke-ref-authority` then `install-ref-authority-recovery --confirm-break`. The
installation binding itself is never overwritten. Key sequence on a rotation:
stop writers, `show-ref-authority --out`, sign offline, install, mount the new key,
start the API (it must match the active epoch or it exits). See
[REF-AUTHORITY-ROTATION.md](REF-AUTHORITY-ROTATION.md).
The API checks the active epoch at startup, on each ref mutation and again in a
database trigger. The runtime database login has SELECT only on the epoch chain.

Clients must independently verify the installation UUID and public key before
pinning. The authority endpoint merely reports the server's claimed binding and
epoch chain; clients check every statement signature locally and refuse a recorded
break or revoked epoch unless explicitly re-pinned.
Receipt pages feed the existing [client pin/journal](REF-RECEIPTS.md), which rejects
rollback, gaps and incompatible forks relative to previously trusted receipts.
Self-declared receipt time is not trusted time. Signatures do not independently
prove that reviewers or policy checks were honest: those remain trusted API code.
A compromised API/signing key can sign forks; independent witnesses/gossip and
secure key distribution remain necessary for stronger claims. No HSM or external
timestamp/witness service is introduced here.

## HTTP contract

All paths begin `/projects/{id}/history` and require an ordinary account session or
a [project device session](PROJECT-SCOPED-DEVICE-SESSIONS.md) for that project with
`history:read` (reads), `history:write` (publish, refs) or `history:review` (review decisions). The same current
role, policy and review rules apply to both. File connector grants are not
broadened to project authority.

| Method/path | Input and result |
|---|---|
| POST `/commits` | `{address}` publishes an own staged candidate; exact accepted retry is free. |
| GET `/commits/{address}` | Original signed commit envelope. |
| POST `/commits/{address}/acknowledge` | `{document,addresses}` (1-256); privately acknowledge published objects without retransmitting their bytes. |
| GET `/commits/{address}/objects/{document}` | `after` address and `limit` (1–256); returns only that file state's reachable keys. |
| GET `/commits/{address}/objects/{document}/{object}` | Hash-verified `{address,bytes}` base64 packet. |
| POST `/refs` | `{branch,candidate,expected}`; returns head and `signed_receipt`. |
| GET `/refs` | Optional exact `branch`, exclusive `after` branch name, `limit` (1–256); heads and matching receipts. |
| POST `/reviews` | `{target:{branch,candidate,expected},decision}`; decision is `approve` or `request_changes`. |
| GET `/reviews` | Required `candidate`, exclusive numeric `after`, `limit` (1–256); append-only review history without emails. |
| GET `/authority` | Installation UUID, active key and epoch, revocation marker and signed epoch statements after `from_epoch` (`limit` 1-64); requires independent verification. |
| GET `/authority/rotations` | Bounded page of the epoch statement chain (`from_epoch`, `limit`) with each epoch's marker. |
| GET `/receipts` | Required `branch`, exclusive `after_generation` (default 0), `limit` (1–256); signed receipts oldest first, plus the active `epoch` and statements bridging the page's epochs. |
| GET `/storage` | Logical published metadata usage and configured limits. |

Branch names with `/` are JSON/query values. A refs page fails closed if any
included head is inaccessible; request a specific branch/smaller page as needed.
There is no global candidate directory yet: retain the address when publishing.

## Storage, work and erasure

Per-project metadata defaults are 1 GiB charged bytes, 1,000,000 retained rows and
4,096 distinct branches. Commit envelopes/scopes/parent indexes, root reachability
entries, reviews and signed ref events are charged transactionally. The runtime
cannot raise limits; exact candidate/review retries and reused root indexes are
free. Limit failures return 507 and roll back the entire publication/ref/review.
Shared physical bytes and private staging have their existing separate budgets.
Database indexes, WAL, events, native uploads and filesystem usage are not covered
by these logical metadata counters.

New publication retains the staging limits of 256 historical files, 65,536 object
reads and 64 MiB encoded reads across all files. Fast-forward traversal is batched
and capped at 10,000 visited commits. At most 256 distinct reviewers participate
per candidate/base. The HTTP mutation body cap is 20 KiB. Existing workload admission
and database/request timeouts still apply. Full graph validation/materialization
and complete commit file manifests remain; this is not changed-node publication
or constant-memory native-file ingest. A changed state root currently gets a full
reachability index; physical packing and finer persistent authorization indexes
remain optimizations.

Audited whole-document erasure deletes affected published envelopes, all their
descendant envelopes/reviews through transitive scope, that document's reachability
entries and shared bytes. Ref events/receipts retain opaque commitments, branch
names and pseudonymous actor IDs; they never rewind to an older generation.
An erased branch head is explicitly unavailable (409), including its receipt
download, rather than silently showing a prior version. Audited reinitialization
of such a branch is future work. Account erasure removes private staging and
revokes keys but preserves shared publications. Unreferenced reachability entries
for other retained documents remain charged until a future safe GC or their own
document erasure. Arbitrary sensitive text copied into unrelated messages or branch
names cannot be inferred and purged automatically.

## Acceptance boundary

Real-router tests under restricted credentials cover reconstruction, private-object
isolation, multi-author ancestry, approval/base/role checks, CAS races, quotas,
key/account revocation and document erasure. Receipt tests use a separately supplied
synthetic authority key and the existing client pin verifier, including failed-write
rollback. Final source is application `23ed7bf` / engine `5f86db2`; full Linux CI passed
350 tests and authenticated browser workflows passed. Windows passed the 349
tests in the run explicitly excluding its failing 100 ms SSE deadline. The actual
operator CLI/key-file startup smoke also passed. Exact evidence and deployment
boundaries are recorded in [the handoff](ENGINE-EVOLUTION-HANDOFF.md).

[Selected-history CLI sync](SELECTED-HISTORY-SYNC.md) now automates reviewed
push/fetch, fast-forward pull and pinned remote tracking. Projects UI adoption,
connector project authentication,
complete offline same-file convergence, installed-host/marketplace acceptance,
independent authority distribution and large-file/service measurements remain open.
The existing web editors and pilot upload cap remain in place.
