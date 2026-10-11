# Streaming shared content v1

12 October 2026. Part 2f storage primitive. This is the first bounded byte-content
layer for future large values and assets, not completion of shared checkpoint
adoption or portable commits. `shared_checkpoint::content::{write, read_into}`
operate on the existing authorized, document-scoped ObjectStore, including its
transactional PostgreSQL backend. Existing v1 state hashes and event hashes do not
change. A returned content root is not a published checkpoint or branch.

## Binary contract

Addresses are SHA-256 of `dynodoc.content\0v1\0` followed by the complete object
bytes, represented externally as 64 lowercase hexadecimal characters. All objects
begin with the eight bytes `DYNOCNT\0`, one version byte (1), and one kind byte.
Integers are unsigned big-endian. References contain raw 32-byte addresses.

| Kind | Bytes after the ten-byte header |
| --- | --- |
| 1: chunk | Zero to 65,536 raw bytes. Only the empty stream uses an empty chunk. |
| 2: branch | Height u8, total byte length u64, child count u16, then ordered children, each address[32] and length u64. |
| 3: root | Height u8, total byte length u64, data-tree address[32]. |

Chunks have height zero. A branch has 1-128 children of height one less, up to
height four. Every child except the last must occupy its full subtree capacity
`65,536 * 128^(height - 1)`; the last is nonempty and no larger. Declared lengths
must equal the sum of children and the actual leaf bytes. The root has the minimum
height that contains its length. Unary branches are allowed only where needed to
fill the final partial path. Empty content is a root referencing an empty leaf.
No trailing bytes, alternate integer encodings, unknown kind/version, mismatched
height, oversized node or broken reference is accepted. Hashes and the entire
reachable graph are checked while reading. Decreasing heights bound traversal and
prevent cyclic traversal; a nested root cannot stand in for a branch or leaf.

This contract treats bytes as opaque. It does not assert valid UTF-8, JSON,
formatting runs, native-file fidelity or document-state equivalence. A future
typed checkpoint/asset manifest must establish those semantics. Chunking may split
a UTF-8 code point; exact reconstruction, not independent text decoding, is the
contract. Byte equality and semantic equality remain different guarantees.

## Bounds, sharing and publication

The input is AsyncRead and the output AsyncWrite. The writer fills one 64 KiB
input buffer across short reads, emits chunks through the existing 1 MiB / 64
object write batches and maintains a frontier of at most 128 references per tree
level. The reader keeps one encoded object and an explicit depth-first stack of
at most `1 + 127 * height` references; it never allocates a full content buffer.
Backend memory is separate: MemoryStore deliberately retains all unique objects,
and caller-selected sinks may buffer all output. PostgreSQL batch/cache bounds
remain as documented in [shared checkpoints](SHARED-CHECKPOINTS.md).

Reads and writes count every encoded object visit and byte against Limits, including
duplicate references and reused objects. Defaults remain one million visits and
512 MiB encoded bytes. Limits are resource admission, not compression or a native
file-size allowance. Objects are at most 65,546 bytes; index objects are at most
5,141 bytes. Tree capacity is not a product upload limit.

Identical pieces reuse addresses only inside an authorized document. There is no
cross-tenant existence endpoint or permission implied by a hash. An equal-length
edit changes its affected chunk and ancestor path. Fixed chunk boundaries mean an
insertion/deletion can change all later chunks; this is explicitly not a claim of
content-defined chunking or delta-proportional work. Full writes/reads are O(bytes)
plus object-store calls. No compression or packing is introduced here.

Callers own transactions, authorization and publication. A failed write may leave
unpublished objects in a nontransactional store; PostgreSQL callers must roll back
the owning transaction. Reads may have written a verified prefix before a later
error. Read into a staged file/sink and publish/apply only after success, never into
the user's live native file. No flush, close, rename or ref update is hidden inside
these primitives. Existing whole-document audited erasure includes these objects
because they share the same scoped table; no new retention/GC policy is introduced.

## Compatibility and remaining integration

The store validates this explicit binary domain as well as existing checkpoint
JSON. V1 state readers still require v1 state/map/value objects. They reject a
content root used as a state root. The ordinary writer, periodic-storage flag,
shared_checkpoint codec constraint, events and applied migrations remain unchanged.
No schema migration or production flag change is required for this internal API.
Older builds cannot validate these new content objects; do not expose or publish
references until the owning protocol advertises a compatible reader requirement.

Next: typed, versioned node/value/asset manifests that reference content roots,
bounded semantic decode/reconstruction and checkpoint publication/read selection.
Then packing, incremental publication and shared draft/named-version references;
portable project commits/branches and durable resumable exchange follow those
contracts. Current ordinary checkpoints still reject a single encoded value over
1 MiB or use their existing legacy fallback. This increment does not remove full
DocumentState memory, change upload limits, enable shared periodic writes, add
offline convergence or introduce independent audit signatures.

## Verification

Frozen independent Python hashlib/struct vectors are in
`crates/engine-core/tests/fixtures/content-v1.json`. Unit regressions cover empty
content, exact chunk boundaries, the 128-child tree boundary, fragmented input,
old-root recovery, bounded growth on a same-length edit, document isolation,
duplicate-reference accounting, missing/corrupt/wrongly typed objects,
noncanonical trees, and propagated input/output errors. PostgreSQL acceptance
checks durable read/reuse, transaction rollback, scope and absence of accidental
checkpoint publication. Actual run results and source references belong in
[the handoff](ENGINE-EVOLUTION-HANDOFF.md).

## Streaming probe (12 October 2026)

Release example `content_stream_bench`, Windows x64 / Rust 1.96.0, single-thread
Tokio runtime. Input is generated repeated byte 0x5a, output is a sink, and
MemoryStore retains only unique objects. Each size is one process run. These are
raw byte streams, not native Office files or realistic entropy/storage workloads.

| Raw input | Write | Verified read | Observed peak working set | Unique stored bytes |
| --- | ---: | ---: | ---: | ---: |
| 100 MiB | 163.07 ms | 78.66 ms | 5,951,488 bytes | 73,860 |
| 250 MiB | 420.87 ms | 171.12 ms | 5,537,792 bytes | 73,340 |
| 500 MiB | 849.97 ms | 352.96 ms | 5,533,696 bytes | 75,860 |

Every read checked the graph/hashes and exact output byte count. Each trace stores
only five unique objects because the content is deliberately repetitive; this is
not a compression ratio for real documents. OS peak working set was sampled every
25 ms and includes the process/runtime/store, but is not allocator accounting or a
service memory guarantee. No database/network/native parsing, concurrent load,
latency percentiles or realistic unique-asset storage is measured. The purpose is
to exercise bounded streaming across multiple tree levels without input/output
buffers growing with stream length. [Raw results](benchmarks/content-stream-20261012.json).

Reproduce with `cargo run --release -p engine-core --example content_stream_bench -- 100`
(also 250 and 500). Measure process memory externally; the example does not itself
sample working set. The 3,000,000-byte pilot upload limit is unchanged.
