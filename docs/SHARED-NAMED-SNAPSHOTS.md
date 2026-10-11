# Shared named snapshots

12 October 2026. Schema 32 extends shared state to named versions, public-copy
pins and deployed snapshots. `DYNODOC_NAMED_SNAPSHOT_STORAGE=shared-v2` selects
the API writer; its default is legacy. Core callers explicitly select
`NamedStorage::SharedV2`. Periodic checkpoints and draft bases keep their separate
writer settings. This capability is not enabled on the VPS.

The existing snapshot row retains its identity, document, exact event sequence,
node Merkle root, chain hash and creation time. Its state column holds
`{"$dynodoc_state":2,"root":"<64 lowercase hex characters>"}`. A strict shape
constraint and a deferred document-scoped foreign key bind it to the object
store. Existing rows are never converted. Saving the same sequence returns its
existing verified snapshot even if the requested writer differs.

Publication reconstructs the exact pinned sequence from previously attested
snapshots and a verified event suffix, excluding periodic shared manifests as
independent evidence. It writes the typed graph, reconstructs it, compares the
complete state with replay, and inserts the snapshot and receipt in the caller's
transaction. A shared publication lock serializes named and periodic builders;
mixed concurrent writers cannot leave a losing graph. Rollback removes all new
objects and envelopes. The caller continues to own authorization and document
locking. No arbitrary uploaded state receives an event-replay receipt.

The v2 receipt fingerprints this canonical JSON array with SHA-256:

```
["dynodoc.shared-snapshot.v2", snapshot_id, document_id, through_seq,
 node_merkle_root_bytes, event_chain_hash_bytes, created_at, stored_state_reference]
```

UUIDs and time retain their existing serde representation. Byte hashes are JSON
integer arrays; object keys are sorted recursively. The domain is distinct from
the unchanged `dynodoc.legacy-snapshot.v1` vector. The fixed synthetic v2 test
vector is `3108937000f9ea2d5fef24341ac085f2886808ba24f9197613ea603e23ff3d76`,
independently computed with Python JSON/SHA-256. This is an internal database
attestation, not a server signature or external witness.

Reads check the **stored** envelope receipt and event-chain position before
decoding document-scoped objects with the typed graph budgets. The node Merkle
root is checked after reconstruction. API responses contain hydrated state,
preserving the existing versions, public links, deployed pins, audit download,
history and restore contracts. A hydrated response is not itself the stored v2
receipt envelope. Corrupt objects, missing roots and malformed/new reference
versions fail closed. Public-copy filtering still removes internal review state.

`verify-snapshots` independently replays from genesis and compares every legacy
or shared snapshot, including comments, suggestions and removed choices. Backfill
can only issue a receipt after that comparison. A valid content hash for the
wrong historical revision cannot be blessed. The ordinary read path does not
repeat this genesis audit.

Shared drafts, named snapshots and periodic v2 checkpoints reuse the same typed
objects within a document. A snapshot after an event that changes no materialized
content reuses its entire graph. Permission checks, immutable events/snapshots,
restricted runtime grants and audited whole-document erasure remain in force.

Resource limits still apply per operation. Materialized state is resident, writes
still inspect full state, and retained history can grow without a total storage
quota. Packing/compression, incremental publication, garbage-collection policy,
realistic growth benchmarks and large native/service acceptance remain open.
There is no automatic fallback to legacy if an explicit named v2 write exceeds
its limits: its transaction fails without publishing a partial snapshot.

Deployment must use schema-32-compatible readers and rollback binaries, even
when this optional writer is disabled. Schema-29 production remains unchanged.
Disabling the setting stops new shared writes; readers retain both formats.
