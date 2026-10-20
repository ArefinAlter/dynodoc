# Dynodoc — Cursor Agent Guide

Monorepo for an append-only, hash-chained document provenance engine (Rust + PostgreSQL + SvelteKit). Read this before implementing features.

## Current owner direction (10 October 2026)

Dynodoc is a local/remote document provenance, review and synchronization platform
around Word/Docs, Excel/Sheets and PowerPoint/Slides, with additional file adapters.
Read docs/PRODUCT-SPEC.md, decision 012, docs/ENGINE-EVOLUTION-PLAN.md,
docs/ENGINE-EVOLUTION-HANDOFF.md and docs/PROJECT-STATUS.md before continuing.
Stable logical nodes contain formatting runs; run boundaries are not permanent IDs.

Shared content storage, offline history/convergence, stronger audit commitments and
large-document/service optimization are authorized by decision 012. The staged
engine-evolution plan controls their build order. Keep current web editors without
prioritizing Office parity. Legacy questionnaire/civic plans do not define the
current product. Native connector sources are not host or marketplace acceptance.
Update explicit handoffs/status in both repos at each increment/context boundary.

## Historical PoC scope (baseline only)

`docs/decisions/000-initial.md` and `docs/dynodoc_poc_prd.md` define what is **in** and **out** of scope. When in doubt, defer to those files over the full-platform build docs (stages 22–31).

**In scope:** stages 13–21 except stage 20 (CRDT). Engine core, auth (email magic link), API, researcher frontend, merge engine.

**Out of scope for PoC:** public consultation (22+), Python ML services, citizen frontend, CRDT layer, MinIO/Redis, T3 civic ID, moderation, deliberation, LLM summary.

## Documentation map

| Need | Read |
|------|------|
| Product/build order | `docs/PRODUCT-SPEC.md`, `docs/ENGINE-EVOLUTION-PLAN.md` |
| Current stage prompt + checklist | `docs/{NN}-build-*.md` (see `.cursor/stage-map.md`) |
| Why the engine works this way | `docs/02-versioning-design-vol1.md`, `docs/03-versioning-mathematics-vol2.md` |
| UX principles | `docs/01-uiux-theory.md` |
| PoC requirements | `docs/dynodoc_poc_prd.md` |
| Local dev | `SETUP.md`, `Taskfile.yml` |

## Repository layout

```
crates/engine-{core,api,cli,shared}   Rust engine
migrations/                           SQL migrations (append-only once applied)
frontend/researcher/                  SvelteKit PoC UI
apps/desktop/                         Tauri 2 desktop/mobile shell (own Cargo workspace; see its README)
infra/docker-compose.yml              Postgres only (PoC)
docs/                                 Design + build reference
```

## Commands (use `task`, not `make`)

```sh
task setup          # first-time
task dev            # Postgres + migrate + API + frontend
task test           # Rust + frontend tests
task lint           # clippy + eslint/svelte-check
task fmt            # format all
task migrate        # apply migrations
task migrate-reset  # DEV ONLY — drops database
```

## Implementation rules

1. **Build increments respect dependencies.** Use ENGINE-EVOLUTION-PLAN.md for the current product; original numbered stages are historical references.
2. **Migrations are immutable.** Never edit a migration that has run in any shared environment; add a new numbered file instead.
3. **Events are append-only.** No UPDATE/DELETE on `event`; hash chain fields are NOT NULL.
4. **Stable node IDs.** `node.id` is a ULID; never reassign.
5. **Current scope** follows decision 012 and PRODUCT-SPEC.md; preserve legacy compatibility without restarting PoC product priorities.
6. **Minimize scope.** Match existing code style; no unrelated refactors.

## Cursor assets in this repo

| Path | Purpose |
|------|---------|
| `.cursor/rules/` | File-scoped coding standards |
| `.cursor/skills/` | Build-stage and verification workflows |
| `.cursor/agents/` | Specialized subagents (engine review, schema, build stage) |
| `.cursor/hooks/` | Safety gates (destructive commands, migration edits) |
| `.cursor/stage-map.md` | README stage number → doc file mapping |

## Claude Code assets in this repo

| Path | Purpose |
|------|---------|
| `.claude/agents/` | Subagents: `build-stage`, `engine-reviewer`, `poc-scope-guard`, `schema-guardian` |
| `.claude/skills/` | Skills: `dynodoc-build-stage`, `dynodoc-verify-stage`, `dynodoc-architecture` |
| `.claude/hooks/` | Safety hooks (destructive-command guard, migration-edit reminder) |
| `.claude/settings.json` | Hook wiring (`PreToolUse` on Bash, `PostToolUse` on Write/Edit) |

These mirror the Cursor/Codex assets in Claude Code's native formats. The
`.cursor/stage-map.md` table is the shared canonical stage → doc mapping.

## Codex assets in this repo

| Path | Purpose |
|------|---------|
| `.codex/skills/` | Codex-friendly copies of the stage and architecture workflows |
| `.codex/agents/` | Repo-local prompts for build stage, scope guard, schema, and engine review |
| `.codex/hooks/` | Hook scripts mirroring the destructive-command and migration safeguards |
| `.codex/hooks.json` | Hook wiring metadata for Codex-compatible setups |
| `.codex/README.md` | Quick entrypoint for future Codex sessions |

## Starting a build stage

Use the **dynodoc-build-stage** skill or the **build-stage** subagent:

> Implement build stage 14 (event log core)

The agent will read the stage doc, respect PoC scope, implement, and run the verification checklist.

For Codex, start from `AGENTS.md`, then reuse the mirrored assets in `.codex/` while still treating `docs/` as the source of truth.
