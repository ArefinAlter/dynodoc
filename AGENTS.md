# Dynodoc — Cursor Agent Guide

Monorepo for an append-only, hash-chained questionnaire authoring engine (Rust + PostgreSQL + SvelteKit). Read this before implementing features.

## PoC scope (authoritative)

`docs/decisions/000-initial.md` and `docs/dynodoc_poc_prd.md` define what is **in** and **out** of scope. When in doubt, defer to those files over the full-platform build docs (stages 22–31).

**In scope:** stages 13–21 except stage 20 (CRDT). Engine core, auth (email magic link), API, researcher frontend, merge engine.

**Out of scope for PoC:** public consultation (22+), Python ML services, citizen frontend, CRDT layer, MinIO/Redis, T3 civic ID, moderation, deliberation, LLM summary.

## Documentation map

| Need | Read |
|------|------|
| Build order & stack | `docs/11-build-index-and-stack.md` |
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

1. **Build stages are sequential.** Do not skip ahead; each stage assumes prior stages are done.
2. **Migrations are immutable.** Never edit a migration that has run in any shared environment; add a new numbered file instead.
3. **Events are append-only.** No UPDATE/DELETE on `event`; hash chain fields are NOT NULL.
4. **Stable node IDs.** `node.id` is a ULID; never reassign.
5. **PoC deviations** in `docs/decisions/000-initial.md` override conflicting text in full-platform docs.
6. **Minimize scope.** Match existing code style; no unrelated refactors.

## Cursor assets in this repo

| Path | Purpose |
|------|---------|
| `.cursor/rules/` | File-scoped coding standards |
| `.cursor/skills/` | Build-stage and verification workflows |
| `.cursor/agents/` | Specialized subagents (engine review, schema, build stage) |
| `.cursor/hooks/` | Safety gates (destructive commands, migration edits) |
| `.cursor/stage-map.md` | README stage number → doc file mapping |

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
