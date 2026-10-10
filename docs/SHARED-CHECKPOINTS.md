# Shared checkpoints v1

10 October 2026. Parts 2a/2b of the engine evolution plan. A derived checkpoint
codec with normal mixed-format readers and a gated automatic writer. This is not
the future portable commit protocol. Event and legacy JSONB formats are unchanged.

## Contract

Objects have a canonical JSON envelope `{ "version": 1, "object": ... }`.
The object is tagged by `kind`, with its body in `value`. Supported kinds are
`node`, `comment`, `suggestion`, `removed_choice`, `map_leaf`, `map_branch`, and
`state`. The first four preserve the corresponding materializer values exactly.
A state has four map roots: nodes, comments, suggestions and removed choices.
Tombstones, parent IDs, positions, arbitrary current fields and review state are
included. Document title, permissions and operational review tables are outside
`DocumentState` and outside this commitment. Ordering of nodes is reconstructed
from their existing parent/position fields; comment order is its original vector
order. A content root is distinct from stable logical identity and event ancestry.

JSON object keys sort by UTF-8 bytes; arrays retain order; whitespace is omitted;
strings and numbers use the pinned Rust serde_json compact encoding. This is a
versioned Rust codec, **not RFC 8785/JCS**. No Unicode normalization is applied.
Objects are stored as bytes, avoiding PostgreSQL JSONB numeric normalization.
Cross-language numeric conformance is a gate before exposing portable exchange.
Hash = SHA-256(`dynodoc.shared-checkpoint\0v1\0` followed by canonical bytes).
Addresses/references are exactly 64 lowercase hexadecimal characters. Unknown
versions, noncanonical bytes, wrong hashes/types and incomplete graphs fail.

Each map is a deterministic radix tree over SHA-256(UTF-8 entry key), with sixteen
possible children per branch. A leaf holds at most 32 entries sorted by key.
Overflow splits on successive hash nibbles, at most 64 levels. Node/removal keys
are stable IDs; suggestion keys are their existing IDs; comment keys are zero-based
16-digit hexadecimal ordinals. Thus an ordinary value edit changes one value,
its map path and the state root. Insertion does not shift every subsequent leaf.
The writer still scans/hashes all state; this is storage sharing, not incremental
materialization or a proof of logarithmic total checkpoint creation time.

Limits: 1 MiB per encoded object, 32 leaf references, 16 branch references,
64 radix levels. Reads/writes have configurable total byte/object budgets. One
oversized node/comment/suggestion is explicitly unsupported by this codec; future
chunked text/assets need separate typed objects. Full state and the map entry
index remain in memory. No 100–500 MB native-file or multi-tenant load claim follows.

## Storage and publication

The storage boundary always takes a document UUID plus object address. In-memory
and PostgreSQL backends use that composite key; there is no cross-document reuse,
existence endpoint or authorization by hash. The caller must authorize access
before calling core. This narrower boundary also isolates tenants. Plain SHA-256
is an integrity address, not encryption or evidence of authorship.

Migration 0025 adds immutable object bytes and checkpoint manifests. A manifest
binds document, exact event sequence, chain hash and state root. Publication uses
one transaction, protects the document against erasure, derives state through the
legacy/event reference reader (explicit CLI) or common mixed-format reader
(automatic writer), stores objects, reconstructs/verifies the reachable
graph and compares it with the input state before publishing. Failure rolls back
objects and manifest together. A retry at the same revision returns the existing
verified result; conflicting roots fail. No arbitrary uploaded root is published.

The PostgreSQL backend is a bounded checkpoint implementation and measurement
baseline, not the selected large-asset production backend. Reads validate bytes
and reachability but trust the checkpoint's association with canonical history;
external signatures/witnesses and independent full replay remain separate work.

Objects and manifests cannot ordinarily be updated/deleted. Audited whole-document
erasure deletes manifests then objects in the same transaction as the document.
No garbage collection or cross-document retention is enabled. Backups may retain
erased content until their retention expiry; immediate backup erasure and
cryptographic key shredding are not implemented.

## Operator use

Apply migration 0025 through the normal migration workflow, then, against an
explicitly selected database and document:

```sh
cargo run -p engine-cli -- shared-snapshot DOCUMENT_UUID --through-seq 42
```

This creates a derived checkpoint only. `shared_checkpoint::postgres::load` reads
that exact checkpoint with verification. Current/history reads and API write
materialization select the nearest legacy/shared checkpoint at or before the
requested sequence; shared wins ties. Selection loads metadata only, then the
chosen representation and a streamed suffix. Current reads pin the head first so
the returned SSE resume sequence matches the reconstructed state. Authorization
and API append locks remain in their callers. A missing/corrupt selected shared
graph fails explicitly; readers do not silently select older or legacy state.
Legacy checkpoints retain their existing trust model. Explicit shared publication
still compares against legacy/event reconstruction, independent of a shared root.

The automatic writer defaults to legacy. Set
`DYNODOC_PERIODIC_CHECKPOINT_STORAGE=shared-v1` **in the API process environment**
to opt background periodic jobs in, or select
`SnapshotEngine::with_periodic_storage(PeriodicStorage::SharedV1)` in core.
Other/unset values select legacy. Changing this setting controls future writes;
readers continue supporting shared history. All new mixed-format readers require
schema 25, even with legacy writes selected.
A deployment/container must explicitly pass the variable to its API process;
this increment does not enable it in production.

A due job takes a per-document try-advisory lock, rechecks cadence and publishes at
one pinned event sequence. Other builders skip duplicate work. Both formats count
toward cadence. On shared codec capacity failure only, a savepoint rolls back
partial objects before writing a legacy checkpoint at the same revision. This
preserves support for larger existing values; it does not chunk them or bound
their prior serialization/materialization allocations. Corruption, database and
other codec errors fail, with background failures logged. Explicit CLI shared
publication still returns capacity errors without fallback.

Named versions, deployed snapshots, audit snapshot IDs and draft/merge bases retain
legacy full JSON. No old data is rewritten or removed. There is no public object
upload/lookup API. This is a staged adoption, not completion of part 2.

## Database read bounds and rollout limitation

Map traversal validates each index object. Value reads use batches of at most 16
addresses; the PostgreSQL cache retains only the latest batch, keyed by document
and address. At the 1 MiB per-object limit it retains at most 16 MiB of encoded
content, plus the current decoded/cloned value and driver overhead. Every value
still passes hash/type and operation/byte-budget checks, including repeated
references. Missing/corrupt objects fail. Full map entries and full document state
remain resident; this cache bound is not a total request memory bound. Reads may
fetch one batch before its cumulative byte budget is exhausted.

The writer still scans/hashes all state and issues individual INSERT/reuse checks.
Its transaction holds a document KEY SHARE lock to protect against erasure; API
appends use FOR UPDATE on that same row and therefore wait during publication.
This is a material performance gate. Shared writing remains off by default until
bounded batching/incremental publication and concurrency acceptance improve the
measured lock window. Disabling the flag is safe with the new mixed readers, but
rolling back to older reader code can restore expensive genesis/legacy replay and
does not promise the same shared-checkpoint corruption detection.

## Measured storage spike (10 October 2026)

Release-mode `shared_checkpoint_bench` on Windows, Intel Core i7-1255U
(10 cores / 12 logical processors), pinned Rust 1.96.0: 8,000 synthetic
paragraphs, initial checkpoint plus 20 separate one-block edits. Checks reconstruction
against the final state. The memory backend retains all old objects during the run.

| Measurement | Observed |
|---|---:|
| One full state JSON | 6,440,063 bytes |
| 21 full-state JSON copies | 135,244,119 bytes |
| 21 shared checkpoints, unique canonical object bytes | 7,831,404 bytes |
| First shared checkpoint | 7,711,597 bytes |
| Additional bytes per one-block edit | 4,727-7,284 bytes |
| Initial write | 163.55 ms |
| Median edit checkpoint write | 168.54 ms |
| Final full reconstruction | 115.87 ms |
| Observed process peak working set | 57,151,488 bytes (54.5 MiB) |

The shared representation uses about 17.3 times less canonical content storage
for this trace (94.2% reduction); the first shared checkpoint is larger than plain
state JSON because of indexes and object envelopes. Write time still scans the
whole state. Working set was sampled from Windows' OS peak counter every 25 ms;
it includes the test store, input and reconstructed states and runtime, not an
allocator proof or per-request memory guarantee. These are one-run observations,
not a latency percentile or throughput claim. They exclude PostgreSQL tuple/index/
TOAST/WAL overhead, event bytes, compression, transfer, native parsing/assets,
permissions, concurrent collaborators and hundreds-of-MB inputs. Production
backend selection and those measurements remain open.

## PostgreSQL acceptance spike (part 2b, 10 October 2026)

Release example `shared_checkpoint_pg_bench`, same Windows CPU/Rust, local Docker
PostgreSQL 16. An empty migrated disposable database; 8,000 synthetic blocks
(6,400,063 bytes of initial state JSON), six checkpoints with five one-node edits.
Every result is compared with the event-applied state, and the event chain is
verified. [Raw measurements](benchmarks/shared-checkpoint-pg-20261010.json).

| Measurement | Observed |
|---|---:|
| Object SELECTs per verified unbatched read | 9,885 |
| Object SELECTs per verified batched read | 2,385 |
| Median unbatched graph read | 6,425.1 ms |
| Median batched graph read | 1,831.2 ms |
| Median normal shared checkpoint read | 1,819.5 ms |
| Median direct legacy JSONB decode/read | 117.2 ms |
| Initial shared publication including verification | 11,650.1 ms |
| Subsequent shared publication including verification | 15,299.2-17,355.0 ms |
| Six shared checkpoints: unique canonical bytes | 7,711,664 |
| Shared object + manifest relations, including indexes/TOAST | 11,255,808 bytes |
| Six legacy snapshots, including indexes/TOAST | 2,916,352 bytes |
| Additional shared WAL per edit | 7,912-38,704 bytes |
| Additional legacy WAL per edit | 481,728-482,672 bytes |
| Observed client process peak working set | 106,016,768 bytes (101.1 MiB) |

**This backend does not pass the production rollout gate.** Batching helps but the
index objects still require individual reads, and full-state individual object
writes dominate publication. API appends can wait through most of that transaction.
On this highly repetitive six-checkpoint fixture, whole-state JSONB compression
beats the current uncompressed small objects plus indexes on allocated disk space,
despite shared storage using fewer logical canonical bytes and less edit WAL.
The prior in-memory storage ratio is not a PostgreSQL disk-efficiency claim.

These are one-run observations, not percentile or concurrency acceptance. Reads
run sequentially/warm (normal, batched, unbatched, legacy). Each legacy snapshot is
created first; shared publication then derives from that matching legacy snapshot.
Later legacy writes themselves read the previous shared checkpoint plus one event.
Therefore the two write timings do not compare identical starting paths. WAL is
cluster-wide and can include full-page/background work. Relation sizes include
allocated free space, table/TOAST/index overhead, and exclude event/node storage
from the checkpoint comparison; the raw artifact records event sizes separately.
Peak working set samples the Windows OS peak counter every 250 ms and includes
expected plus multiple reconstructed states/client buffers, excluding PostgreSQL.
No native assets, 100-500 MB files, network transfer or multi-user load is measured.

Reproduce only on an empty, migrated disposable database, with explicit
`DATABASE_URL` and `DYNODOC_CHECKPOINT_BENCH=disposable`:

```sh
cargo run --release -p engine-core --example shared_checkpoint_pg_bench
```

The benchmark writes synthetic events/checkpoints and deliberately leaves them for
inspection; remove the disposable database/container afterward. It rejects a
database that already contains documents. Next evaluate bounded bulk object
writes/index reads, incremental reuse without scanning all values, compressed
packing/backend alternatives and publication with a short final lock. Preserve
erasure atomicity and root verification during those changes.

## Still required after this slice

Enable the automatic writer only after operational acceptance, then migrate
draft bases and named versions with explicit versioned references; bound large text/assets; add durable local files/object
negotiation, commit parents and atomic branch refs; measure database/TOAST/WAL,
native large-file peak memory and production backend alternatives. Keep current
connectors and ordinary snapshots operational during those changes.
