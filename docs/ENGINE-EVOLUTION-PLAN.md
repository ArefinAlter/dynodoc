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

## Historical recovery clarification

The owner's added journey requires permission-controlled retrieval of old/unnamed
revisions and selected recovery without losing later work. The bounded
[history-recovery increment](HISTORY-RECOVERY.md) reuses the existing centralized
reader and review path, independently of the remaining storage work. Whole-block
restoration is distinct from the implemented net revision-range revert/cherry-pick.
Portable commit-addressed commands remain part 3. Test-installation packages,
Microsoft ribbon commands and browser pairing are now early part-8 increments;
real host acceptance and marketplace publication remain open. Parts 3 and 6
must retain source/ancestry/actor links; part 8 must provide indexed discovery and
native download/application across retained history. Current role and approval
checks apply to all of these actions. This does not mark part 2 complete or defer
bounded chunks, packing or shared draft/version references.

The owner also prioritized the Projects front page as an early part-8 increment:
private member profiles, repository file browsing, per-file main/draft comparison,
pull requests and history, with separate web editors and a 3 MB original-upload
cap for testing. This reuses the current centralized contracts; it does not implement
project-wide branches or atomic multi-file commits. See [decision 013](decisions/013-projects-first-test-workspace.md).

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
legacy JSON. Part 2b measurements gated writer rollout: publication scanned full
state, performed per-object writes and blocked document FOR UPDATE appends. Part
2c below batches those operations; full scans and locking remain. Large-value
chunks and version/draft references still precede local commits/exchange.

## Part 2c implementation boundary

Bounded batch writes (64 objects / 1 MiB) and breadth-first index reads replace
individual database round trips. Stored bytes, roots, limits, rollback and erasure
contracts remain v1. Exact-byte reuse checks and concurrent INSERT conflict checks
remain mandatory. Explicit/periodic publishers share a per-document advisory lock.
Fixed-ID repetitive/varied fixtures compare individual and batched publication,
with real FOR UPDATE wait probes. This optimization preserves the atomic
transaction; full-state scans, resident state, compressed storage/backend choices
and short staged publication remain open. The writer stays opt-in until acceptance.

## Part 2d implementation boundary

Core and API content transactions use NO KEY UPDATE, compatible with checkpoints'
KEY SHARE erasure guard. They still serialize writers and permission/state checks.
Existing stronger permission/lifecycle/erasure locks remain. Checkpoints retain an
exact immutable sequence and atomic object/manifest publication. No staging or
migration is required for this lock correction. See [the concurrency contract](CHECKPOINT-CONCURRENCY.md).
Full-state materialization, bounded text/assets, compressed packing and shared
version/draft references remain the next work before portable local commit exchange.
Default shared writes stay off until concurrent API/resource acceptance.

## Part 2e implementation boundary

Borrowed canonical encoding caps output growth before allocation/copy, preserving
v1 bytes and hashes. Streaming canonical validation avoids another decoded-object
clone and encoded buffer. Compatibility/property/boundary tests and a reproducible
oversized-rejection allocation probe are recorded in
[bounded checkpoint encoding](BOUNDED-CHECKPOINT-ENCODING.md). This does not add
chunks or bound full-state memory, property-key sorting metadata, legacy fallback
or all serialization time. Large-value/asset chunks, packing/incremental publication
and shared draft/version references still precede portable project commits.

Installed-host acceptance and publisher setup can proceed independently using
[the acceptance/publication record](ADDIN-PUBLICATION-AND-ACCEPTANCE.md).
The locally installed PowerPoint 2021 fails the current connector API prerequisite;
Word/Excel inventory alone is not runtime acceptance.

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
