# 012 - Local-first document repositories and scalable provenance

Accepted owner direction, 10 October 2026. Supersedes research-instrument product
priorities and the PoC exclusion of offline/convergence/storage work where needed
for the document version-control product. Extends decisions 010 and 011.

Dynodoc is a GitHub-style review/provenance/sync platform around existing editors.
The authoritative journey, fidelity, storage and correctness requirements are in
[PRODUCT-SPEC](../PRODUCT-SPEC.md). Implement them in ordered increments from
[ENGINE-EVOLUTION-PLAN](../ENGINE-EVOLUTION-PLAN.md), preserving existing history.

The owner explicitly requests shared snapshot storage, offline replica convergence,
stronger audit guarantees, algorithms suitable for large documents and a design
that can be validated at large service scale. These are now authorized product
work. Original stages remain historical implementation references, not a mandate
to build civic/consultation features. Do not restart the questionnaire roadmap or
attempt to reproduce Office inside the web product.

Decisions:

- Stable logical nodes contain formatting runs; uniform formatting alone is not
  a permanent identity. Name/content similarity proposes binding, never grants it.
- Store unique content/asset objects with commit roots and parent relationships.
  Ordinary push/pull sends missing objects/changes, not repeated whole files.
  An initial graph still contains document information; version history is retained.
- Define automatic merge independence using reads, writes and shared validity
  constraints. Keep human resolution for unsupported or ambiguous concurrency.
- Offline commit exchange and eventual agreement on accepted merges are required.
  Automatic CRDT convergence is scoped per supported operation, not asserted for
  arbitrary native files. Preserve observed versus inferred provenance boundaries.
- Retained historical states and changes are recoverable under current access and
  review rules, including old or unnamed revisions. Downloads, selected restoration,
  revert and cherry-pick are distinct operations. Recovery appends new work and
  records its source; it never rewinds the canonical chain or bypasses approvals.
- Introduce formats, storage and stronger commitments through versioned contracts
  and additive migrations. Do not rewrite existing event hashes or applied SQL.
- Maintain explicit status, evidence and a continuation handoff in both application
  and MIT engine repositories before ending an increment or exhausting context.

Hundreds-of-MB documents and hundreds of thousands of users are acceptance targets.
They are not grounds for increasing upload limits without bounded ingest, resource
controls and measured workloads. Real native-host and marketplace acceptance remain
separate from source-level tests.
