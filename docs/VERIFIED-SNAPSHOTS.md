# Verified legacy snapshots

11 October 2026. This source increment closes the legacy snapshot read boundary
described in [the security findings](SECURITY-HARDENING.md). It is not a VPS release
or an independently witnessed audit system. Final checks and source references are
recorded in [the handoff](ENGINE-EVOLUTION-HANDOFF.md).

## Contract

Migration 0028 adds immutable `snapshot_verification` receipts and guards legacy
snapshots against UPDATE, ordinary DELETE and TRUNCATE. Approved whole-document
erasure still removes snapshots and their receipts inside the existing audited
transaction. Existing snapshot bytes, IDs, event hashes and deployed/version/public
references are preserved. No old snapshot receives a receipt just because the
migration ran.

`snapshot_verification::verify_document` pins a repeatable-read view of one document,
validates and folds its event chain from genesis, and compares every legacy snapshot
at its own revision. Comparisons cover the complete interpreted `DocumentState`:
nodes/tombstones, comments, suggestions and removed choices, plus the node Merkle
root and chain binding. Negative/out-of-history revisions are rejected. A newer
healthy checkpoint cannot hide a corrupt earlier one. Event hashes retain v1's
existing envelope limitations.

Unknown fields on the state, node, comment or suggestion records are rejected,
rather than silently ignored by deserialization. Arbitrary document properties
inside `current_fields` and suggestion `detail` remain supported. Missing legacy
default collections remain compatible. New outer record fields need an explicit
schema/verification contract update.

Backfill records receipts only after those comparisons succeed. Failure rolls back
all receipts created for that document; a rerun never overwrites an existing receipt.
The ordinary checked reader verifies the receipt fingerprint and the event-chain
position. A missing/mismatched receipt fails closed, without silently falling back
to another checkpoint or serving the stored content. Shared checkpoints retain their
separate existing validation contract; this audit does not newly prove every shared
root by replay.

New legacy snapshots are reconstructed through the verified legacy/event path,
including hash validation of the replayed suffix. Callers cannot submit an arbitrary
state to be attested. The same checked writer is used for periodic checkpoints,
named versions, public copies and deployed pins. Duplicate requests reuse an
existing verified row without a no-op UPDATE. Historical/current materialization,
snapshot downloads, public projections, version restore and Merkle-proof boundaries
use checked reads. Pure `Materializer::from_snapshot` remains a fold of caller-trusted
input, not an authentication API.

The API `/documents/:id/verify` and CLI `verify` now independently verify the chain
and all legacy snapshots. The lower-level `log::verify_chain` still verifies only
event hashes; its name is not a snapshot-attestation claim. Access checks and public
link expiration/revocation remain in their existing callers.

## Receipt format and trust

Receipt version 1 is SHA-256 of a compact JSON array containing, in order:
`"dynodoc.legacy-snapshot.v1"`, snapshot UUID, document UUID, through-sequence,
node-root bytes, event-chain bytes, UTC creation timestamp, and stored state.
UUIDs/timestamps use the existing Serde encodings; byte vectors are JSON integer
arrays. State object keys are recursively sorted; array order is retained. The
envelope is hashed through a streaming writer, without a second encoded JSON buffer.
A fixed vector pins this byte contract. The raw stored envelope is fingerprinted
after the JSONB round trip; semantic replay comparisons handle removed-choice sets.

Receipts are internal attestations. A database owner or compromised authorized
writer able to replace both history and its evidence remains outside this guarantee.
Immutability triggers do not prevent an owner from disabling them. Runtime role
separation is the next security gate; signed envelopes and independent pinned or
witnessed heads remain evolution part 6. Do not label receipts as signatures.

## Upgrade and rollback gate

Do not perform a routine API-only rollout against old data. Keep old API writers
and public traffic stopped while applying migration 0028 and verifying old snapshots.
The current API still migrates at startup; migration/runtime credential separation
is not completed by this increment. A fresh empty database needs no backfill.

With the intended migrated database selected by `DATABASE_URL`, run:

```sh
engine-cli verify-snapshots DOCUMENT_UUID
engine-cli verify-snapshots --all
engine-cli verify DOCUMENT_UUID
```

The first two alternatives create receipts. `--all` visits documents in UUID keyset
order, commits each independently and stops on the first failed document. Previously
verified documents remain committed. It is safe to rerun; it rechecks rather than
trusting an earlier receipt. `verify` is an independent check and does not backfill.
Never fix a failure by rewriting history, recalculating a receipt over unverified
state or disabling the guards. Preserve evidence and investigate against a verified
backup/canonical history before repairing any derived state.

Before release: verified backup, migration/backfill rehearsal on existing data,
all documents passing independent verification, successful protected historical
downloads/restores/public copies, and the remaining runtime-privilege gate. Schema
27 session revocation must also remain effective. An unmodified old API bypasses
receipt checks and may attempt now-forbidden snapshot updates; it is not a safe
rollback. Retain a compatible build with both verification and revocation, or keep
traffic stopped while following an explicitly tested recovery procedure.

## Cost and acceptance limits

Normal reads cost O(snapshot bytes + tail bytes), including fingerprint hashing,
deserialization and checked tail replay. They do not replay from genesis when a
verified checkpoint is selected. Backfill/audit streams events once and keyset-reads
one snapshot at a time: work is O(event bytes + all compared snapshot bytes), plus
index lookups. It retains the current fold and the snapshot/decoded comparison in
memory. This is not bounded full-document memory, a request deadline, a tenant
budget or a large-file throughput claim. A document-wide verification transaction
can be long; resource admission/deadlines remain an open gate. Snapshot storage is
still full JSONB; shared chunks/draft/version references remain on the roadmap.

Regression coverage includes pre-0028 upgrade, unverified reads, idempotent backfill,
non-node corruption with an intact chain, all-or-nothing backfill, envelope changes,
wrong document scope, concurrent creation, checked event tails, invalid revisions,
public/deployed/historical reads, restore atomicity and approved erasure. Final
suite counts and any limitations belong in the handoff, not assumed here.
