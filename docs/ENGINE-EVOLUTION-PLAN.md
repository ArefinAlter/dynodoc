# Engine and product evolution plan

10 October 2026. Implements [decision 012](decisions/012-local-first-document-version-control.md)
and [the product specification](PRODUCT-SPEC.md). Status is in
[ENGINE-EVOLUTION-HANDOFF](ENGINE-EVOLUTION-HANDOFF.md). Dependencies below replace
the old questionnaire/civic stage order for this work.

| Part | Deliverable | Exit evidence |
|---|---|---|
| 1 | Correct scope/math; one checkpoint-aware historical reader; bounded matching fallback; measured replay accounting. | Existing histories unchanged; exact historical replay and snapshot selection tests; adversarial matching regressions; both repos lint/test. |
| 2 | Versioned object/commit specification and shared snapshot storage. | Canonical object hashing vectors; tenant-scoped object store; unchanged content reused; verified old/new materialization equivalence; reachability/erasure tests; storage-growth benchmark. |
| 3 | Local repository and resumable object/commit exchange. | Durable local commits/branches, parents and merge bases; object negotiation; crash/retry/reorder tests; atomic publish/ref compare-and-swap; verified bundle-v1 migration. |
| 4 | Format graph and incremental capture/application. | Stable nested IDs, split/join/move rules; formatting and structure adapters for all six hosts; opaque asset preservation; RTF adapter; native round-trip fixtures with capability/version declarations. |
| 5 | Offline merge and replica convergence. | Deterministic materialization and explicit resolutions across partition/reorder/duplicate/restart schedules; constraint-aware conflict classification; concurrent structural and formatting corpus. |
| 6 | Independently verifiable provenance. | Versioned commit envelopes bind repository/parents/root/actor; signed device/server receipts; key rotation/revocation; client-pinned/witnessed heads; fork/truncation tests and documented trust limits. |
| 7 | Bounded large-file ingest and service scale. | Streaming/resumable 100/250/500 MB fixtures; per-tenant resource bounds; storage/transfer amplification; concurrent-workload latency and peak-memory results; recovery/backup/load-shedding exercise. |
| 8 | Complete repository workspace and distribution. | Branch/compare/activity/search/releases; recoverable pull UX; background capture/pairing; installed-host acceptance and Microsoft/Google publication for all three pairs. |

Parts may overlap only where contracts are settled. Bounded object/parser contracts apply from parts 2 and 4; part 7 is their full-load
acceptance, not permission to postpone memory discipline. Host experiments can proceed
early; existing connectors must keep working while engine contracts evolve. Each
part may require multiple reviewable increments. A finished part does not certify
the entire product. Ship evidence with each claim.

## Part 2a implementation boundary

The first slice implements [shared checkpoints v1](SHARED-CHECKPOINTS.md): complete
materialized-state objects, stable-key radix maps, document-scoped memory/Pg stores,
transactional verified publication, legacy equivalence and audited erasure. The
operator CLI opts in explicitly. Default snapshots and draft/version bases still
use full JSON; portable commit manifests, large text/assets, durable local storage
and backend/load acceptance remain open. Part 2 is therefore still in progress.

## Part 2b implementation boundary

Normal current/history and API write materialization now select mixed checkpoints.
The periodic shared writer is explicit opt-in; default legacy remains reversible.
Capacity fallback is transactional; corrupt shared data fails closed. PostgreSQL
value reads are bounded batches. Named/deployed versions and draft bases stay
legacy JSON. Database measurements must gate writer rollout: publication still
scans full state, performs per-object writes and blocks document FOR UPDATE appends.
Next reduce that publication cost/lock window, add large-value chunks, then migrate
version/draft references before exposing local commits/exchange. Part 2 stays open.

## Part 2 design gate and first implementation slice

Specify immutable typed objects for text/format runs, logical-node properties,
ordered children, assets and commit manifests. Keep stable identity separate from
content address. Define canonical bytes, schema version, hash-domain separation,
size/reference limits, ordering, missing-object behavior and roots over all relevant
content. Legacy snapshots remain readable; new roots must reconstruct equivalent
state including tombstones and any declared review state.

Use a storage trait with a small local/test backend and a tenant-scoped durable
backend. PostgreSQL stores refs, permissions, transaction receipts and indexed
metadata; large binary content must not require whole-file JSONB/HTTP requests.
Select a production object-storage backend through a measured spike. No cross-tenant
deduplication oracle. Validate all reachability before publishing roots. Explicit
erasure and backup retention need a design before enabling garbage collection.

Implement shared checkpoints behind a version/capability boundary first. Test
reconstruction against the existing event materializer at every fixture checkpoint,
one-node edits reusing unrelated objects, legacy reads and interrupted publication.
Only then migrate draft bases/named versions and expose commit/object negotiation.

## Algorithm work and acceptance

- Formalize operation footprints: changed fields, referenced identities, ancestor
  paths, order ranges, style dependencies, uniqueness indexes and shared validators.
  Conservative over-approximation may cause extra conflicts; missing dependencies
  can produce incorrect merges. Optimize only after counterexample/property tests.
- Keep text diff bounded with measured fallback quality. Repeated text, Unicode
  graphemes, mark boundaries, nested edits and adversarial wholesale replacements
  belong in the corpus. A deadline is not an end-to-end latency guarantee.
- Replace whole-state clones/scans with persistent/indexed affected-node work once
  object semantics are stable. Cache structural depth instead of recomputing it
  inside sorting; audit cell/shape matching candidate budgets and duplicate handling.
- Use checkpoint intervals and streamed tails for historic reads. Record selected
  checkpoint and replay count. Missing historical checkpoints still require work;
  no claim that a best-effort 500-event snapshot policy is a hard upper bound.
- Name/content/MinHash matching needs labeled multilingual and repeated-template
  evaluation, precision/recall, ambiguity reporting and explicit identity binding.

## Service design to validate

Partition work by tenant/repository/document. Stateless API replicas authorize
bounded requests; durable workers ingest/verify objects and compute expensive
comparisons. An immutable-object cache can be shared within its authorization
domain; refs use transactional compare-and-swap. Avoid holding document locks
through whole-file parsing, transfer or large historical reconstruction.

Backpressure, quotas, streaming limits, cancellation, idempotent job receipts and
observability precede raising file limits. Design SSE/fan-out and background workers
from measured active workload, not total accounts. Test 10 collaborators per large
document and many independent teams, including hot-document contention, long-lived
branches, degraded storage and reconnect storms. Record deployment hardware and
cost per workload. No current capacity promise for 100,000+ users.

## Handoff contract

Each increment records source commits in both repos, exact changes, compatibility,
tests/benchmarks actually run, unresolved failures, deployment state, temporary
resources and the next concrete task. Update PROJECT-STATUS and HANDOFF entrypoints.
Never label planned offline, storage or native capability as shipped code.
