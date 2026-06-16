---
name: dynodoc-architecture
description: Explains Dynodoc engine architecture, event log invariants, and design rationale from docs/. Use when designing features, reviewing engine changes, or answering how versioning, merge, audit, or identity work.
---

# Dynodoc architecture reference

## Core model

- **System of record:** append-only hash-chained `event` log per document.
- **Derived state:** `node` current fields and `snapshot` rows are materialized by replay.
- **Stable identity:** `node.id` is load-bearing; diffs merge by id, not heuristic matching.
- **Semantic ops:** typed operations such as `FieldEdited` and `NodeMoved`; once written they are public schema.

## Layer stack

1. **CRDT** (stage 20, skipped in PoC) - live co-edit cache
2. **Event log** (stage 14) - durable truth, tamper-evident chain
3. **Review protocol** (stages 16, 21) - suggesting, merge, four-card conflicts

## Key docs

| Topic | File |
|-------|------|
| Design narrative | `docs/02-versioning-design-vol1.md` |
| Proofs and formalism | `docs/03-versioning-mathematics-vol2.md` |
| UX modes | `docs/01-uiux-theory.md` |
| Stack choices | `docs/11-build-index-and-stack.md` |
| PoC boundaries | `docs/dynodoc_poc_prd.md` section 3 |

## Invariants

- Events are immutable at DB and application layer.
- `verify_chain(document_id)` must detect payload tampering.
- Appends are linearizable per document (`FOR UPDATE` on latest event).
- Referential integrity is enforced before deploy (stage 21).
