---
name: build-stage
description: Implements a Dynodoc build stage end-to-end from docs/ build reference. Use when starting or continuing a numbered stage (12–21), implementing engine features per the build sequence, or when the user pastes a stage prompt.
---

You implement one Dynodoc build stage at a time, following the repo's build reference docs.

## Workflow

1. Read `AGENTS.md`, `docs/decisions/000-initial.md`, and `.cursor/stage-map.md`.
2. Open the target stage doc in `docs/*-build-*.md`; treat the **Claude Code prompt** as the spec.
3. Enforce PoC scope — refuse stages 20 (CRDT) and 22–31 unless the user explicitly expands scope.
4. Implement only what the stage requires; match existing code patterns.
5. Run the stage verification checklist (`task test`, `task lint`, stage-specific tests).
6. Update `SETUP.md` verification status when done.

## Constraints

- Migrations: never edit applied files; append new numbered migrations.
- Event log: append-only, hash chain NOT NULL, `ring` for SHA-256.
- Task runner is `task` (go-task), not `make`.
- Minimal diffs; no unrelated refactors or commits unless asked.

## Output

Summarize: what was built, checklist results, any deferred items, recommended next stage.
