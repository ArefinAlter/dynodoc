# Bounded checkpoint encoding

10 October 2026. Evolution part 2e, extending [shared checkpoints v1](SHARED-CHECKPOINTS.md).
This is a serialization allocation correction, not completion of bounded storage.

## Contract and compatibility

The old writer cloned each materialized value, converted its envelope into a full
JSON value tree and allocated canonical output before checking the 1 MiB object
limit. Oversized text could therefore allocate many times that limit just to fail.

The writer now serializes borrowed values directly into a capped buffer. Each
write checks available space before copying or growing the buffer; requested
capacity never exceeds the smaller of 1 MiB and the remaining operation byte
budget. Exhausted object budgets fail before encoding. Canonical validation on
reads compares emitted bytes with the stored bytes as it goes, avoiding another
decoded-object clone and a second encoded buffer.

All seven v1 object kinds, field ordering, hash domains, addresses and roots remain
unchanged. The canonical field views are an explicit compatibility contract: a
future materializer field change must update the codec deliberately and pass the
frozen reference tests. Nested JSON sorts borrowed key references, including when
another dependency enables serde_json's preserve_order feature. No migration,
public API or event format changes; old readers can read newly written objects.
Unknown versions, malformed graphs and noncanonical bytes still fail closed.

## Verification

Five added tests cover all seven object kinds against the previous implementation,
128 generated nested-value cases, existing Unicode/numeric representations, exact
1 MiB boundaries, JSON escaping, operation budgets and rejected writes that must
not grow the buffer. Streaming validation rejects alternate escaping, duplicate
fields, reordered fields and trailing whitespace. Existing fixed hash vectors,
storage reconstruction/reuse, corruption and transaction tests remain applicable.

The reproducible allocation probe is
`crates/engine-core/examples/checkpoint_encoding_bench.rs`:

```sh
cargo build -p engine-core --release --example checkpoint_encoding_bench
cargo run -p engine-core --release --example checkpoint_encoding_bench -- legacy
cargo run -p engine-core --release --example checkpoint_encoding_bench -- bounded
```

Local Windows x64 release build, rustc 1.96.0, three process runs per mode:

| Path | Peak additional requested heap bytes | Elapsed microseconds, three runs |
| --- | ---: | --- |
| Frozen previous encoder | 75,500,265 | 142,201 / 170,830 / 223,085 |
| Current writer | 1,048,724 | 4,675 / 7,236 / 3,950 |

The fixture is one already-materialized node containing 8 MiB of NUL text. JSON
escaping produces 50,331,837 bytes in the previous encoder before rejection. The
current writer returns the same object-limit error, with no object stored. The
allocator probe resets its baseline after fixture/runtime setup and counts requested
live allocations; it excludes existing document state, allocator metadata, transient
system reallocation copies, stack, mappings and database memory. Timing was measured
on a shared development machine with other checks running and is descriptive only.
This is an adversarial rejection trace, not typical document throughput, total RSS,
or a service-capacity benchmark. The new peak includes small writer metadata in
addition to the 1 MiB output buffer.

## Limits and next dependency

This change bounds encoded output allocation, not all work or process memory.
Materialized state and map entry indexes remain resident. Sorting property-key
references still allocates metadata proportional to the number of properties;
recursive traversal and serialization scans still depend on input shape and size.
Successful publication still traverses the full state, verifies reconstruction and
uses the existing bounded database batches. MemoryStore retains stored objects.

Values exceeding v1 limits remain unsupported. Periodic capacity fallback still
rolls back partial shared objects and writes a full legacy snapshot; that legacy
path has no new allocation bound. Named versions and draft bases still store full
JSON. Shared periodic writes remain opt-in and disabled in production.

Next: specify versioned chunked text/value/asset objects with bounded decode and
reconstruction, compatibility vectors and a large-value/reuse corpus. Then implement
packing/incremental publication and shared version/draft references before portable
project commits/branches and resumable exchange. Native capture, offline convergence,
independent audit and 100–500 MB/multi-tenant acceptance retain their separate gates.
