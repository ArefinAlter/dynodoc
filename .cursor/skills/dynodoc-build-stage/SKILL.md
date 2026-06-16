---
name: dynodoc-build-stage
description: Implements a numbered Dynodoc build stage from docs/ build reference files. Use when the user asks to start, implement, or continue a build stage (e.g. stage 13, 14, event log, database schema), or pastes a Claude Code prompt from a stage doc.
---

# Dynodoc build stage

## Before coding

1. Resolve stage number → doc file via [.cursor/stage-map.md](../../stage-map.md).
2. Read the full stage doc: prompt, decisions, watch-outs, verification checklist.
3. Read `docs/decisions/000-initial.md` — if stage is out of PoC scope, stop and tell the user.
4. Skim prior stage completion in `SETUP.md` verification sections.

## Implementation

1. Extract the **Claude Code prompt** block from the stage doc as the task spec.
2. Apply PoC deviations (no MinIO/Redis, no Python services, simplified auth, skip CRDT).
3. Match existing repo layout (`crates/`, `migrations/`, `frontend/researcher/`).
4. Keep diffs minimal; one stage's scope only.

## Verification

Run every item in the stage doc's checklist. Typical commands:

```sh
task migrate-reset   # when schema changed (dev only)
task test
task lint
task fmt-check
```

Update `SETUP.md` verification section for the stage when all checks pass.

## Stage-specific anchors

| Stage | Critical invariant |
|------:|-------------------|
| 13 | Migrations immutable; event append-only triggers |
| 14 | Hash chain verify; property tests with proptest |
| 15 | Snapshots derived from replay, not authoritative |
| 16 | Semantic op vocabulary is a public schema forever |
| 17 | Magic link only for PoC |
| 18 | SSE for live updates; POST for edits |
| 19 | Port prototype UX from design docs |
| 21 | Four-card conflict UX; validation pass post-merge |
