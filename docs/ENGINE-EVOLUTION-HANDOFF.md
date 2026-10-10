# Engine evolution handoff

Updated 10 October 2026. **Part 1 implemented and locally verified.** Publication/CI
results and source commits are recorded below before this increment is handed off.
Read [PRODUCT-SPEC](PRODUCT-SPEC.md), [decision 012](decisions/012-local-first-document-version-control.md)
and [ENGINE-EVOLUTION-PLAN](ENGINE-EVOLUTION-PLAN.md) before continuing.

## Completed

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

The isolated container `dynodoc-evolution-part1-20261010` and its volumes were removed.
No test servers started; unrelated local containers retained. No production deployment,
real native-host acceptance or marketplace publication was performed. Last recorded
production remains app `8c7461d`, schema 24, plus the Nginx repair. Local tests do not
establish hundreds-of-MB or large-service capacity. Signed-in browser acceptance is
left to the pushed CI run and must be recorded separately, not inferred here.

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
Implementation commit IDs and remote CI results will be filled after publication.
