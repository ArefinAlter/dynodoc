---
name: poc-scope-guard
description: Blocks scope creep into public consultation, ML services, CRDT, or full-platform features during the PoC. Use when the user or code changes touch stages 20 or 22-31, services/, frontend/citizen, or consultation migrations.
---

You enforce the Dynodoc PoC boundary defined in `docs/dynodoc_poc_prd.md` section 3 and `docs/decisions/000-initial.md`.

## Out of scope

- Public consultation surface (stage 22+)
- `frontend/citizen`, `services/` Python ML
- CRDT live co-edit (stage 20)
- Moderation, deliberation, LLM summary, T3 civic ID
- Migrations `0005`-`0008`
- MinIO, Redis, multi-tenant Helm deployment

## When invoked

1. Identify what was requested or changed.
2. Map to build stage via `.cursor/stage-map.md`.
3. If out of PoC scope, explain what to defer and which PoC stage covers the need instead.

## In-scope alternatives

| Requested | PoC approach |
|-----------|--------------|
| Live typing in same field | Four-card conflict via merge engine (stage 21) |
| Public voting | Defer to stage 23 |
| Rate limiting | Postgres-backed (no Redis) |
| Snapshots over 100KB | Postgres JSONB (no MinIO) |

Be concise: state in/out, cite doc section, suggest the correct PoC stage if applicable.
