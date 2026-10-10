# Checkpoint and content-write concurrency

10 October 2026, evolution part 2d. Implements the lock audit requested by the
[handoff](ENGINE-EVOLUTION-HANDOFF.md). Shared checkpoint writing remains opt-in.

## Change and invariant

`log::append_in_tx` and `ops::apply::begin_write` now lock the document with
`FOR NO KEY UPDATE`. They still serialize competing content transactions before
reading the head, permissions, state or constraints. Core append does not upgrade
the API's lock. These operations never change the document primary key.

Checkpoint creation/load retain `FOR KEY SHARE` until transaction completion,
protecting the document and its child history against physical erasure. Those two
lock modes are compatible. `FOR NO KEY UPDATE` still conflicts with itself,
`FOR SHARE`, `FOR UPDATE` and deletion. See the
[PostgreSQL 16 row-lock specification](https://www.postgresql.org/docs/16/explicit-locking.html#LOCKING-ROWS).
The change is supported by concurrency tests of the actual engine/API paths.

The checkpoint pins event sequence N before reconstructing state. Selection, event
suffix and chain lookup are bounded by N, with immutable events and objects. An
append after N is absent from that checkpoint and remains available in its tail.
Checkpoint materialization does not read mutable `node.current_fields`. Graph
verification and object/manifest publication remain in one transaction. There are
no staged objects, new migrations, changed hash bytes or new recovery obligations.

## Audited lock paths

| Path | Document lock and preserved responsibility |
|---|---|
| Core append | NO KEY UPDATE; allocate consecutive sequence/hash after acquiring the writer lock. |
| API operations, batch/merge/draft/version/publication paths through `begin_write` | NO KEY UPDATE; read permissions/state and validate after locking. |
| Workspace membership/settings through `begin_write` | Same writer lock; access changes and content writes remain mutually exclusive. |
| Explicit and periodic checkpoints, exact shared load | KEY SHARE; protect against erasure while reading a pinned immutable revision. |
| `provenance::read_checkpoint` export | Existing SHARE; continues excluding content/permission writers through its authorized read. |
| People policies/ownership, connector grants/revocation, product move/trash/public-link listing/revocation | Existing UPDATE locks remain; these paths may still wait for a checkpoint. |
| Admin erasure (`admin`, `admin_controls`) | Existing UPDATE lock before erasure/child operations; deletion still waits for checkpoints and writers. |
| Project permission/settings changes | Existing root SHARE/UPDATE ordering and checks unchanged; document writers still take the project read guard. |

Role checks still run after the content transaction acquires its lock. No hash or
checkpoint grants access. Normal reads and exports retain their existing access
checks. A queued stronger management/erasure operation can still delay new writes;
this change does not promise that writers never block. Existing transaction
deadlock handling, database scheduling, CPU/IO and full-state API reads still apply.

The per-document publication advisory lock still serializes builders; periodic
jobs use try-lock. It is independent of content-writer serialization. Erasure does
not need a new advisory lock or an object staging/garbage-collection protocol.

## Verification

Tests pause real explicit-shared, periodic-shared and legacy publishers at a
test-only manifest INSERT barrier. Ten core appends must commit while each
publisher is uncommitted, with consecutive sequences and a valid chain. A stronger
UPDATE lock must fail NOWAIT during the guard. After release, the checkpoint must
contain only the original revision and current reads must replay all ten edits.

The existing ten-collaborator API test now performs its real independent batch
edits under a held checkpoint guard, retaining all edits and chain verification.
A separate test observes a real database wait, revokes an editor's membership in
the preceding transaction, then proves that the queued API writer is forbidden
and no event is appended. Existing audited-erasure, stale-edit, project/access,
hash/codec and snapshot compatibility suites remain required.

Both repositories pass fmt/Clippy, all 226 Rust tests and unchanged OpenAPI.
Standalone documentation passes with existing Rustdoc link warnings. Source/browser
CI passed; verified runs and source commits are recorded in
[the handoff](ENGINE-EVOLUTION-HANDOFF.md).

## Measurement contract

`checkpoint_contention_bench` requires an empty migrated disposable database,
explicit DATABASE_URL and DYNODOC_CHECKPOINT_BENCH=disposable. Select
DYNODOC_CHECKPOINT_WRITER_LOCK=update (the previous entry lock, benchmark only) or
no-key-update (current). Use a separate fresh database per mode:

```sh
cargo run --release -p engine-core --example checkpoint_contention_bench
```

The same fixed-ID 8,000-block state and matching legacy checkpoint feed the actual
shared publisher. A test-only first-object INSERT barrier confirms that publication
holds its guard before releasing ten distinct-actor core transactions together.
Each transaction updates its derived node row and appends one canonical event.
Both checkpoint reconstruction at N and final state at N+10 must match expectations;
all sequences and the complete event chain are verified. Pool capacity is 16.

Writer timing includes connection acquisition, lock wait, node/event SQL and commit.
It excludes API authentication, governance/merge validation, full-state API
materialization, network transfer and native host processing. Publication time is
measured after barrier release, excluding initial legacy reconstruction/encoding.
The overlap counter compares client task completion with publication return,
not database commit timestamps. The deliberately paused regression test supplies
the stronger ordering evidence. No service percentile or large-file capacity
claim follows from one trace with ten requests.

The earlier `shared_checkpoint_pg_bench` still measures FOR UPDATE contention for
management/erasure. Its current output calls that `strong_lock_wait_ms`; historical
part 2b/2c artifacts retain their then-accurate `append_lock_wait_ms` field.

## Measured contention trace

Windows / Intel Core i7-1255U, Rust 1.96 release, local Docker PostgreSQL 16.
Separate empty migrated databases; old-lock mode ran first, current mode second,
without competing builds/tests. Each contains an 8,000-block / 6,400,063-byte
synthetic initial state, one shared checkpoint and ten real core edit transactions.
Both modes produce the same pinned checkpoint root, all ten edits survive current
reconstruction, sequences are consecutive and chains verify.

| Writer entry lock | Lock-acquisition range | Commit range | Median of ten commits | Completed before publisher returned | Publication after barrier release |
|---|---:|---:|---:|---:|---:|
| update | 1907.14-2024.84 ms | 1925.00-2035.01 ms | 1986.95 ms | 0/10 | 1913.44 ms |
| no-key-update | 12.35-193.66 ms | 38.02-212.34 ms | 129.35 ms | 10/10 | 2084.70 ms |

The checkpoint's work remains about two seconds; compatible locking lets content
transactions proceed during it. This is not a checkpoint throughput or asymptotic
complexity improvement. Peak client working sets were 104.38 MiB (old-lock mode)
and 100.59 MiB (current), including seeded/reference/reconstructed state and runtime,
excluding PostgreSQL. They are OS peak observations, not memory bounds or evidence
of reduced asymptotic space. Lock time includes connection acquisition and queuing
behind other writers. Ten samples from one trace per mode are not service latency
percentiles or an end-to-end API/native-host performance guarantee.

Raw artifacts: [previous entry lock](benchmarks/checkpoint-contention-part2d-update-20261010.json)
and [compatible entry lock](benchmarks/checkpoint-contention-part2d-no-key-update-20261010.json).

## Remaining work

This removes the checkpoint guard as a direct blocker of ordinary content writers.
Checkpoint and API materialization still scan full state; memory, CPU/IO, large
text/asset chunks, compression/packing and shared version/draft references remain
open. Measure those costs under concurrent API load before enabling shared writes.
Durable local history/exchange and native/service acceptance remain later stages.
