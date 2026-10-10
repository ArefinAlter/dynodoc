# Shared checkpoints v1

10 October 2026. Part 2a of the engine evolution plan. This is an opt-in derived
checkpoint codec, not the future portable commit protocol. The event chain and
legacy JSONB snapshots retain their existing formats and default read paths.

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
existing historical reader, stores objects, reconstructs/verifies the reachable
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
that exact checkpoint with verification. Ordinary UI snapshot creation/history
reads continue using legacy storage. There is no public object upload/lookup API.

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

## Still required after this slice

Migrate default snapshots, draft bases and named versions after compatibility and
operational acceptance; bound large text/assets; add durable local files/object
negotiation, commit parents and atomic branch refs; measure database/TOAST/WAL,
native large-file peak memory and production backend alternatives. Keep current
connectors and ordinary snapshots operational during those changes.
