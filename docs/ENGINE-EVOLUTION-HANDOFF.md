# Engine evolution handoff

## Part 2c: bounded database batches (10 October 2026)

Part 2c is complete, synchronized and pushed to both main branches. Local checks
and both source CI runs passed, including the application browser workflows.
Part 2 remains open and shared writes remain off by default. No new migration,
event encoding or HTTP schema; schema 25 remains required by mixed readers.

- Writes buffer at most 64 objects / 1 MiB encoded bytes, plus the current encoding
  and input/driver state. PostgreSQL checks canonical input and exact stored bytes
  in a bounded VALUES join returning booleans, inserts missing objects in one
  statement, and rechecks conflicts won by concurrent inserts. Unchanged batches
  issue no INSERT; duplicate addresses count as new at most once.
- Index traversal uses breadth-first batches of up to 16 objects, as do value
  reads. Route/depth/duplicate/subtree checks and byte/object budgets remain;
  queued references count against the remaining object budget before expansion.
  Complete state/index/traversal metadata still resides in memory.
- Explicit publishers acquire the same document advisory lock as periodic jobs
  before the row lock, serializing overlapping historical graphs and preventing
  cross-batch lock-order deadlocks. Periodic jobs keep try-lock behavior. Objects
  and manifests remain in one verified transaction with rollback/erasure guarantees.
- The row KEY SHARE lock still blocks FOR UPDATE appends through publication.
  Batching shortens work but does not provide staged/unlocked or incremental
  publication. V1 bytes, roots, document scope and full reconstruction are unchanged.
  ObjectStore now requires Send, which graph operations already required.

### Verification and measured limits

Application fmt/Clippy, all 224 Rust tests and unchanged OpenAPI pass. New database
regressions cover bounded bulk writes, duplicate reuse/scope/capacity/corruption,
2,000-node index batching with byte-triggered flushes, overlapping historical
publishers, and an invalid concurrent INSERT winner checked after a real database
lock wait. Oversized-value fallback now proves rollback after an actual batch has
reached the database. Existing hash vectors and event/graph equivalence tests pass.

Standalone source is synchronized; task lint, all 224 Rust tests, task docs and
unchanged generated OpenAPI pass. The full suite passed on its first run here.
All five changed Rust files and all 22 immutable migration files match byte for
byte. Existing Rustdoc link warnings remain. MIT/OpenAPI packaging is preserved.
Remote source CI passed in both repositories (224 Rust tests each). Application
frontend CI passed 274 unit tests, 16 public browser tests, 62 signed-in workflows
and the production proxy regression. Source/CI records are linked below.

Four separate fresh PostgreSQL databases: 8,000 fixed-ID blocks, initial 6.4 MB
state, two one-block edits per trace; identical 600-byte repeated or varied payload
lengths. Individual/batched paired roots and canonical totals match, reconstructed
states match event application, and event chains verify. Publication fell from
14.6-20.9 s to 2.1-2.9 s; the competing append-lock wait remains 2.1-2.9 s. Object
INSERT statements: 9,817 each checkpoint becomes 154 initially and 4/3 for edits.
Verified graph SELECTs: 9,817 becomes 619. This compares individual SQL and batched
SQL using the current codec, not different release binaries.

Physical shared relations are about 10.9 MB. Three compressed legacy snapshots
use 0.64 MB for repeated content and 20.78 MB for varied content. Canonical bytes
alone do not predict disk savings. Client process peaks were 91.0-104.8 MiB,
excluding PostgreSQL. The four one-run, three-checkpoint traces are not percentile,
ten-collaborator, native 100-500 MB or service-scale acceptance. Exact results,
methodology and four portable raw artifacts are in [SHARED-CHECKPOINTS](SHARED-CHECKPOINTS.md).

### Next concrete work

1. Reduce the publication lock window: audit lock-mode requirements first, then
   design incremental/staged publication where necessary. Preserve append/access
   serialization, pinned sequence/chain validation, complete-root verification, erasure
   isolation and crash/retry cleanup before moving writes outside one transaction.
   Measure hot-document contention; keep the writer opt-in until this passes.
2. Specify bounded text/asset chunks and evaluate compressed packing/backend
   alternatives with edit-density and longer-history traces. Current objects still
   reject a single encoded value over 1 MiB; fallback retains a full JSON checkpoint.
3. Migrate named versions and draft bases to explicit versioned shared references,
   then add parent/root/actor commit envelopes, durable local history and resumable
   missing-object exchange. Cross-language canonical conformance precedes exchange.
4. Offline convergence, signed/witnessed provenance, all-six-host structural and
   formatting/background capture, native large-file and service-load acceptance
   remain later gates. The basic web editor is not the product priority.

### Publication and environment

Starting application: 5ce3566; standalone: 2d93630. Both implementation commits are
pushed to main. This documentation follow-up records verified results without
changing the tested source. The owned local container
dynodoc-evolution-part2c-20261010 and its disposable volume were removed after
validating its ownership label and loopback port 5553. Existing dynodoc-postgres
and margin-qdrant remain running. Logs are in system temp; synthetic JSON artifacts
are tracked. No API/web server or VPS deployment. Last recorded production remains
8c7461d, schema 24, plus the documented Nginx repair.

Publication:

- Application source: `40a80be7e11ae8b10add920c787573fab822d0f9`.
- [Application Rust CI](https://github.com/ArefinAlter/dynodoc/actions/runs/38046287959): passed, 224 tests.
- [Application frontend/browser CI](https://github.com/ArefinAlter/dynodoc/actions/runs/38046287986): passed, 274 unit tests, 16 public browser tests and 62 signed-in workflows.
- Standalone source: `c8ac6bcb108419cf40fb20199c6e3797353f197c`.
- [Standalone engine CI](https://github.com/ArefinAlter/dynodoc-engine/actions/runs/38046494935): passed, 224 tests.
- Final engine handoff/status: `11399dfd5525d86d65dcf57c84b1cb54bdf4c882`
  ([checkout CI](https://github.com/ArefinAlter/dynodoc-engine/actions/runs/38046840829)); source is unchanged.

## Part 2b: mixed readers and gated automatic writer (10 October 2026)

Part 2b is complete, synchronized and published to both main branches. Source CI
passed in both repositories, including application browser workflows. This is a
slice of part 2, not product completion.

- Current/history readers select the nearest legacy/shared checkpoint (shared wins
  ties), validate shared graphs and stream only the suffix. Pinned current seq
  remains the SSE resume boundary. API write materialization uses the same reader
  inside existing access checks and locks. Selected corrupt shared data fails.
- Automatic writes default to legacy. API background jobs opt in via
  DYNODOC_PERIODIC_CHECKPOINT_STORAGE=shared-v1; core exposes PeriodicStorage.
  Both formats count toward cadence; per-document advisory locking skips duplicate
  builders. No-op jobs check cadence before locking, then due jobs recheck it.
- Shared capacity errors roll back partial objects at a savepoint before legacy
  fallback at the same seq. Other errors propagate. Named/deployed snapshots,
  snapshot audit IDs and draft/merge bases still contain full JSON.
- PgStore prefetches at most 16 values, scoped by document; ordinary validation and
  budgets remain. Latest-batch cache has a 16 MiB encoded-byte maximum, not a whole
  request memory bound. Index entries/state still reside in memory.
- Explicit operator publication still derives from legacy/event reconstruction,
  independent of shared roots. No new schema/event/HTTP format; schema 25 required.
- Publication holds KEY SHARE on the document, which blocks the API's FOR UPDATE
  appends, and still scans/writes individual objects. Keep writer default off until
  measured publication latency and concurrent acceptance improve.

Verification: all 220 application Rust tests, fmt/Clippy and unchanged OpenAPI
passed. Targeted tests cover every revision, mixed nearest/tie/scope selection,
resume seq, duplicate builders, cadence/rollback, oversized-value partial rollback,
batched duplicates/budgets/scope and corruption. Existing API workflows now run from
shared state for stale edits and version/draft/restore compatibility. Standalone
task lint, all 220 tests and task docs passed (existing Rustdoc link warnings).
Its first full run hit the existing 100 ms SSE deadline once; the full rerun passed
without changing the test. That timing check is not a production latency guarantee.
Standalone generated OpenAPI is unchanged, including MIT metadata. Both copies of
all seven changed Rust files and all 22 immutable migrations match exactly.
Counterpart source commits and successful CI runs are linked below.

PostgreSQL spike: 8,000 blocks / six checkpoints, all reconstruction/chain checks
passed. Batching lowers graph object SELECTs 9,885 -> 2,385 and median read
6,425 -> 1,831 ms; direct legacy JSONB read is 117 ms. Shared edit publication
15.3-17.4 seconds blocks appends for much of that transaction. Shared relations
11.26 MB versus compressed legacy snapshots 2.92 MB on this repetitive fixture,
despite fewer canonical bytes and edit WAL. Observed client peak 101.1 MiB.
No production gate passes from these figures; see the raw artifact and limitations
in SHARED-CHECKPOINTS.md. Prior in-memory ratios are not database disk ratios.

Next: optimize/batch publication and shorten its append-blocking transaction;
repeat controlled traces with fixed IDs, varying text entropy/edit density/history
length and compressed backend alternatives;
bounded text/asset chunks; versioned draft/named references; cross-language codec
and parent/root/actor commit envelope; durable local history and resumable exchange.
Offline convergence, independent signatures/witnesses and native/scale acceptance
remain separate gates. The basic web editor is not the product priority.

Temporary DB dynodoc-evolution-part2b-20261010 and its disposable volume were
removed after validating its ownership label and loopback port 5553. Existing
dynodoc-postgres and margin-qdrant services remain running. Logs are in system temp;
the portable synthetic measurement artifact is committed under docs/benchmarks/.
No API/web server or VPS deployment in this increment. Production remains last
recorded 8c7461d, schema 24. Starting app 0718ee6; standalone a70647d.

Publication:
- Application source: `12be6a2d5444b7044b06749ca9d2b3fafea815ec`.
- [Application Rust CI](https://github.com/ArefinAlter/dynodoc/actions/runs/38043426193): passed.
- [Application frontend/browser CI](https://github.com/ArefinAlter/dynodoc/actions/runs/38043426145): passed (62 signed-in, 16 public browser tests and 274 frontend unit tests).
- Standalone source: `970d78ad49c94e63f902ac4f97dcc61cf212db27`.
- [Independent engine CI](https://github.com/ArefinAlter/dynodoc-engine/actions/runs/38043671852): passed.
- Final engine handoff/status commit: `2d9363072788de69a19df38752852a24fe230c09`
  ([checkout CI](https://github.com/ArefinAlter/dynodoc-engine/actions/runs/38044102949)).

## Previous completed increment

Updated 10 October 2026. **Part 2a complete, published and CI-verified. Part 2 remains in progress.** Part 1 remains published. Read [PRODUCT-SPEC](PRODUCT-SPEC.md),
[ENGINE-EVOLUTION-PLAN](ENGINE-EVOLUTION-PLAN.md) and
[SHARED-CHECKPOINTS](SHARED-CHECKPOINTS.md) before continuing.

## Part 2a: shared checkpoint foundation

- Versioned/domain-separated canonical objects cover all `DocumentState` fields,
  including tombstones, comment order, suggestions and removed choices. Logical IDs
  remain separate from addresses. Fixed hash vectors pin the Rust byte codec.
- Stable-key radix maps share unchanged objects across checkpoints and avoid fixed
  array chunk boundaries shifting on insertion. A one-node edit rewrites its value,
  map path and root; creation still scans/hashes the complete state.
- Document-scoped memory and PostgreSQL stores; no cross-document reuse or public
  hash lookup. Migration 0025 adds immutable bytes and checkpoint manifests without
  changing applied migrations or event encodings.
- Transactional opt-in publication derives an exact historical state, verifies full
  graph reconstruction before publishing, rejects mismatches, rolls back failures
  and supports same-revision concurrent retries. Reads reject missing/corrupt roots
  and check their recorded chain position. They still trust the root/history
  association; external attestations are not implemented.
- Operator `shared-snapshot` CLI; audited erasure removes manifests/objects and the
  admin preview/receipt records their counts and bytes. Ordinary snapshots and
  draft/version bases still use full JSON. No production default is switched.

## Verification and measurement

Seven codec/property tests and five isolated database tests cover round trips,
canonical bytes, all state components, reuse/insertion, malformed references/routes,
limits, legacy equivalence, rollback/retry, concurrency, scope and erasure. The
existing admin API erasure test also checks new counts and cleanup.

The first full application run failed the existing 100 ms SSE timing assertion
while a release build was running concurrently. That test was not weakened; the
full suite then passed all **216 tests** without competing work. Application fmt,
Clippy and unchanged generated OpenAPI passed. Standalone task lint and all 216
tests also passed. Standalone task docs passed with existing Rustdoc link warnings
in unchanged files. Standalone generated OpenAPI is also unchanged, including MIT
metadata. Application Rust, frontend/browser and independent engine source CI all
passed; their runs are linked below.

Synthetic release benchmark: 8,000 blocks, 21 checkpoints (20 one-block edits),
135,244,119 bytes as repeated full-state JSON versus 7,831,404 unique object bytes;
4,727-7,284 additional bytes per edit. Initial write 163.55 ms, median full scan/write
168.54 ms, final reconstruction 115.87 ms, observed process peak working set
54.5 MiB. Read the codec document for method and limitations: no PostgreSQL/WAL,
large native files, transfer, concurrency or production backend acceptance.

## Next concrete task: part 2b adoption and bounded content

1. Add a compatibility-preserving default reader/writer selection between legacy
   and shared checkpoints. Compare legacy/shared-plus-tail reconstruction at every
   revision. Migrate draft bases/named versions by explicit versioned references;
   preserve historical readers, erasure and stale-base checks. Do not silently
   delete full snapshots or switch production before rollout evidence.
2. Introduce bounded typed text/format/asset chunks so a single large logical node
   does not exceed the current 1 MiB object cap. Benchmark PostgreSQL/index/TOAST/WAL
   and durable local/remote backends, streaming, cancellation and memory. The memory
   store is a test backend; write/read budgets do not bound the prior materializer.
3. Settle cross-language numeric canonicalization and a separate commit envelope
   binding repository, parents, root, operations/capture capabilities and actor
   evidence. Then implement durable local commits, resumable missing-object exchange
   and atomic branch refs (part 3). Offline convergence/audit/native host acceptance
   remain separate explicit gates, not consequences of checkpoint deduplication.

Current limits: whole-state residency/scans, up to 1 MiB per object, 512 MiB/one
million object operations by default, Rust-specific numeric encoding, no chunked
assets, no GC, no default-path storage savings yet. No complete product or capacity
claim. The radix tree is an implementation choice; comparative prolly-tree results
remain unmeasured. Backup retention still applies after live audited erasure.

## Source, deployment and temporary resources

Working from application `1907eea` and standalone `6e7fdf9`. Both copies must match
for changed Rust sources/tests/examples and migration 0025, while preserving engine
MIT metadata and its standalone rich-text fixture. Published implementation:

- Application: `7a8e28ed1bfdf5828deb341fdf9b533fad9f5469`.
- Standalone engine: `6ccdc2277b187491eabbfa089a416bcdbcdaddac`; final handoff and
  LF-checkout guard: `a70647d553be610f41c005d87ce6de26aeb955dd`.
- [Application Rust CI](https://github.com/ArefinAlter/dynodoc/actions/runs/38040472956): passed.
- [Independent engine CI](https://github.com/ArefinAlter/dynodoc-engine/actions/runs/38040495733): passed.
- [Final engine checkout-guard/handoff CI](https://github.com/ArefinAlter/dynodoc-engine/actions/runs/38041109485): passed at `a70647d`.
- [Frontend/authenticated browser CI](https://github.com/ArefinAlter/dynodoc/actions/runs/38040472960): passed (62 signed-in and 16 public browser tests, frontend units and the production-proxy regression).

A final portability check found old Windows engine migration checkouts had CRLF
although committed/app/production bytes were LF. The engine now pins SQL checkouts
to LF and restores the committed bytes locally (no migration Git blob was edited).
All 22 migration files match the app. Applying app migrations to a fresh isolated
DB and then validating with engine migrations passed. All 216 engine tests passed
again with canonical checkout bytes. The packaging follow-up is recorded with the
finalized handoff.

No VPS deployment; last recorded production remains `8c7461d`, schema 24, with the
previous Nginx header fix. The owned disposable PostgreSQL container
`dynodoc-evolution-part2a-20261010` and its volumes were removed after verification
and ownership-label checks. No test API/web servers were started. Existing local
services were left running. Temporary logs and synthetic benchmark JSON are under
the system temp directory. The second owned checksum/test container
`dynodoc-checksum-part2a-20261010` and its volumes were also removed after its
checksum validation and 216-test rerun passed. No increment-owned servers remain.

## Earlier completed increment: part 1

The following is retained as the dated Part 1 release record. Its "next task" and
shared-storage limits predate Part 2a; use the continuation above for current work.

## Part 1 completed

- Made existing-editor document repositories the authoritative product. Replaced the
  duplicated combined volume with a current guide and marked legacy questionnaire/
  civic stages as historical. Corrected mathematical independence, complexity,
  shared-storage and audit claims, including their repeated cross-references.
- Defined stable logical identity separately from content addresses and formatting
  runs; initial graphs contain content, while the target avoids redundant full-file
  versions. Defined explicit offline/merge/trust/capacity acceptance boundaries.
- Added core `read_state_at`: nearest checkpoint at/before the exact revision,
  streamed suffix, revision/gap rejection and checkpoint/replay-count evidence.
  Historical batches/draft creation use the same reader as review/provenance.
- Streamed chain verification and made cadence checks query only sequence metadata.
- Bounded browser Myers extra work to input key count plus 1,000,000 work steps and
  trace to 1,000,000 integer cells (about 4 MB, excluding frontier/objects/input).
  Removed repeated frontier copies and quadratic duplicate occurrence lists.
  Exhaustion uses unique anchors/LIS; fewer matches can require additional review.
- Added exact historical replay/checkpoint/scope tests, duplicate-heavy matching
  regressions and uniqueness/crossed-move counterexamples to disjoint-ID independence.
- Synchronized the seven changed engine source/test files byte-for-byte to the MIT
  repo and preserved its packaging metadata. No event/schema/migration changes.

## Verification

| Check | Result |
|---|---|
| Application `cargo test --workspace`, isolated PostgreSQL | 204 passed. After final streaming/cadence changes, all 13 event/snapshot tests passed again. |
| Application Rust formatting/Clippy | Passed, including all targets after final changes. |
| Frontend full unit suite | 274 passed, including document/spreadsheet/presentation import regressions. |
| Changed frontend files: Prettier/ESLint; complete Svelte/type check | Passed; zero Svelte errors/warnings. |
| Standalone engine `task lint`, `task test` | Passed; all 204 tests, including the unchanged 100 ms SSE check. |
| Standalone `task docs` | Passed with previously documented Rustdoc link warnings. |
| Standalone `task openapi` and generated diff | Passed; HTTP schema unchanged. |
| New documentation links, source equality and whitespace checks | Passed. |
| Application remote CI | Rust passed; frontend lint/build, 274 unit tests, 62 signed-in browser tests, 16 public browser tests and proxy regression passed. |
| Engine implementation and final documentation-head CI | Both passed. |

The isolated container `dynodoc-evolution-part1-20261010` and its volumes were removed.
No test servers started; unrelated local containers retained. No VPS deployment,
real native-host acceptance or marketplace publication was performed. Last recorded
production remains app `8c7461d`, schema 24, plus the Nginx repair. Local tests do not
establish hundreds-of-MB or large-service capacity. Remote browser acceptance used an isolated CI stack; it does not certify native
Microsoft/Google installations, marketplace publication or a new VPS deployment.

## Measurements and complexity boundary

Pre-change audit (10 October) release medians: synthetic 8,000-block/6,030,116-byte
JSON parse 58.44 ms; classify 41.36 ms; format patch 0.256 ms; replay 0.099 ms; small
paragraph merge 0.032 ms; 8,000-block fingerprint 8.19 ms. These exclude native
conversion, network, database writes, rendering and concurrent load. They are a
baseline, not a fresh end-to-end benchmark of this increment.

Current reads still hydrate full state; checkpoints and draft bases still duplicate
full JSON. Missing historical checkpoints still require genesis replay. The periodic
threshold is best-effort, not a hard 500-event tail bound. Key matching budgets do
not cover all shape/cell candidate matching or key-byte comparison costs. Signed
heads, shared object storage and a complete local commit DAG remain unimplemented.

## Next concrete task: part 2 shared checkpoints

1. Specify typed/versioned canonical objects, logical identity versus hash address,
   commit manifests, ordering, chunk/reference limits and all committed state.
2. Implement a tenant-scoped object-store boundary and shared checkpoints alongside
   legacy snapshots; no rewriting event hashes or applied migrations.
3. Compare reconstruction with legacy replay at every fixture checkpoint. Verify
   unchanged objects are reused after a one-node edit, publication is recoverable,
   missing/corrupt objects fail explicitly and tenant/erasure boundaries are intact.
4. Measure unique-byte growth and peak memory. Then migrate full draft bases/named
   versions and develop local commits/object negotiation in part 3.

Bounded parsing/object contracts start now; full-load acceptance is part 7. Real-host
experiments may proceed alongside settled contracts. Do not resume Office web-editor
parity or the questionnaire/civic roadmap. Update this handoff and both project
status records at every increment and before context/token exhaustion.

## Source and publication record

Starting application `f054ad1`; starting standalone `67060cf` (service `d6666a2`).
Implementation is pushed to both main branches:

- Application: `667ecb6ecef5b64debcca6817e73b6be807bde37`.
- Standalone engine implementation: `647249e7ed692534f7ab21a1ad522912c7d58385`;
  finalized handoff/status at `6e7fdf91810677b3dc8778b7d9789882125d2bb1`.
- [Application Rust CI](https://github.com/ArefinAlter/dynodoc/actions/runs/38038078571): passed.
- [Frontend/authenticated browser CI](https://github.com/ArefinAlter/dynodoc/actions/runs/38038078512): passed (274 unit, 62 signed-in, 16 public browser tests; proxy regression).
- [Independent engine CI](https://github.com/ArefinAlter/dynodoc-engine/actions/runs/38038142218): passed.
- [Final engine documentation-head CI](https://github.com/ArefinAlter/dynodoc-engine/actions/runs/38038345736): passed.

All listed runs passed. The documentation follow-up records results and synchronization
without changing the verified implementation. There are no remaining part-1 test
failures; the larger unfinished product capabilities are explicitly listed above.
