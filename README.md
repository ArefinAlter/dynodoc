# Research IDE (codename **dynodoc**)

> An authoring environment for structured research instruments — ODK / Kobo / REDCap-style
> questionnaires — with rigorous version control, auditability, and collaborative review built
> into the data model rather than bolted on. **Think GitHub for research instruments**, but with
> a typed semantic data model instead of plain text: concurrent edits, suggestions, and merges
> happen at the level of items, choices, and fields rather than lines of XML.

**Status:** Proof of Concept **feature-complete** — all PoC stages (12–21, CRDT layer 20
deferred) done: append-only event log, snapshots, semantic ops + governance, magic-link auth
(Paseto v4), the REST + SSE API, the four-mode SvelteKit editor, and the referential-integrity
+ semantic-merge engine with four-card conflict resolution. Verified end to end against a live
Postgres. &nbsp;·&nbsp; **License:** [AGPL v3](LICENSE)

---

## The problem

Survey methodologists, regulated-trial researchers, and policy drafters all work on artifacts
that are deeply *structured* — every item has a variable name, a type, a constraint expression,
a relevance condition, a translation set, choice options — but their tools treat those artifacts
as *text or spreadsheets*. XLSForm diffs in Excel are hostile; Git on XML exports produces
unreadable diffs and silently merges across semantic boundaries; the mainstream form designers
(Kobo, ODK Build, REDCap) have no real version control at all. For low-stakes work this is
annoying. For a regulated clinical-trial protocol or a national policy draft it is disqualifying.

The Research IDE makes version control and the audit trail **part of the data model**. Every
change is a typed semantic event, attributable, hash-linked, and replayable. Concurrent edits to
different fields merge automatically; same-field edits surface as a structured conflict, not a
textual one; a reviewer's suggestion is a first-class proposal the author accepts or rejects with
a recorded reason. The audit trail is not a feature layered on top — it is the substrate.

## How it works

The system rests on an **append-only, hash-chained event log** as the system of record:

- **Typed semantic events.** Every change is an event such as `NodeCreated`, `FieldEdited`,
  `NodeMoved`, `CommentAdded`, `SuggestionProposed`, `SuggestionAccepted` — operating on nodes,
  not text.
- **Stable-identity nodes.** Every section, item, and choice gets a **ULID** at creation that
  never changes for the node's lifetime. Diffs and merges match by id (`O(n)`), not by
  heuristic tree-edit distance.
- **Materialized state by fold.** Current document state is derived by folding the event log;
  periodic **snapshots** pin intermediate states so reads are fast. Snapshots are derived, never
  authoritative.
- **Tamper-evidence.** Each event is chained: `chain_hash = SHA256(content_hash ‖ prev_chain_hash)`,
  genesis from 32 zero bytes. A **Merkle root** over snapshots commits to the whole tree. Anyone
  with read access can replay the log and confirm it has not been altered — change any event and
  either the head hash differs (tamper *detected*) or SHA-256 has been broken.

This is the audit guarantee that makes the system trustworthy for high-stakes work. The math is
proven in the versioning volumes: **disjoint-node commutativity** (edits to different nodes/fields
converge in any order), **strong eventual consistency**, vector-clock causality, and cycle-safe
moves. See [docs/02-versioning-design-vol1.md](docs/02-versioning-design-vol1.md) (narrative) and
[docs/03-versioning-mathematics-vol2.md](docs/03-versioning-mathematics-vol2.md) (proofs).

### The editor and the review workflow

The frontend (ported from an existing HTML prototype) is a **four-mode editor**:

| Mode | What it is |
|------|-----------|
| **Document** | Prose-shaped rendering — reads like a printed methodology appendix. Default mode. |
| **Outline** | Two-pane tree view for navigation and structural reorganization. |
| **Spec** | The underlying XLSForm as an editable spreadsheet table. |
| **Preview** | The compiled form rendered as a respondent will see it (skip logic, validation). |

On top of it sits the **suggesting workflow**: a reviewer proposes a change without applying it;
it lives as a proposal in the author's queue until accepted or rejected. When two edits touch the
*same field*, the system surfaces a **four-card conflict** (Original / current version /
incoming version / compose-new) rather than `<<<<<<<` markers. Every merge runs a **referential
integrity pass** — no dangling variable references, no cycles, no duplicate var-names — that
blocks a `Deployed` event when the instrument would break.

## Two regimes, one engine

The architectural commitment: **the engine never changes** between regimes.

1. **Research IDE** (the immediate target) — a small expert team co-authoring an instrument.
   This is what the PoC delivers.
2. **Public consultation at national scale** (a later extension) — thousands of public
   contributors on a government policy draft. New layers (identity tiers, moderation, Polis-style
   deliberation clustering, citizen UX, closeout/legitimacy reports) are added *above* the engine,
   never replacing it. See the roadmap below and [docs/05–09](docs/).

## Proof of Concept scope

The PoC proves the engine for a 2–6 person expert team. See
[docs/dynodoc_poc_prd.md](docs/dynodoc_poc_prd.md) and
[docs/decisions/000-initial.md](docs/decisions/000-initial.md) (authoritative).

**In scope:** the Rust engine (event log, snapshots, semantic ops, governance gate, integrity +
merge), email magic-link auth, REST + SSE API, and the SvelteKit researcher frontend.

**Out of scope (deferred):** public consultation surface and citizen frontend, Python/ML services,
the CRDT live-typing layer (Stage 20 — same-field edits use the four-card conflict instead), T3
civic identity, moderation, deliberation, LLM summary, and multi-tenant deployment hardening.

### Definition of done

- Two browsers logged in as different authors open the same document; concurrent edits to
  different items merge cleanly via the log; live **SSE delivers updates within 100 ms**.
- A suggestion from author A appears in author B's proposals queue and applies on accept.
- Renaming a `varName` referenced by another item's relevance expression triggers an integrity
  violation that **blocks the `Deployed` event** with a clear message.
- The auditor CLI returns **OK** for a clean log and pinpoints the exact event `seq` if a row is
  corrupted.
- All Rust property/integration tests and frontend unit tests passing (verified against a live
  Postgres). **Lighthouse 95+** and the full Playwright E2E flows are scaffolded but deferred —
  they need a running stack + browsers; see [SETUP.md](SETUP.md).

## Tech stack

| Layer | Choice |
|-------|--------|
| Engine | **Rust** — Axum · SQLx · Tokio · `ring` (SHA-256) · `pasetors` (Paseto v4) |
| Crates | `engine-core` · `engine-api` · `engine-cli` · `engine-shared` (Cargo workspace) |
| Database | **PostgreSQL 16+** — append-only enforced at the DB layer via triggers |
| Frontend | **SvelteKit + TypeScript**, server-rendered (`frontend/researcher`) |
| Design system | Warm parchment background · Fraunces / Source Serif 4 / IBM Plex Sans · terracotta accent |
| Realtime | **Server-Sent Events over HTTP/2** — no WebSockets |
| Local dev | Docker Compose (one Postgres) + Rust binary + SvelteKit dev server |
| Task runner | [go-task](https://taskfile.dev) (`Taskfile.yml`) |

## Repository layout

```
crates/
  engine-core/      Rust — event log, hash chain, materialization, snapshots, merge
  engine-api/       Rust — Axum HTTP server, SSE, auth
  engine-cli/       Rust — admin CLI (migrate, snapshot, verify, replay)
  engine-shared/    Rust — types shared across crates
migrations/         SQL migrations (append-only once applied; 0000–0004, 0009_views, 0010_auth, 0011_idempotency in PoC)
frontend/researcher/  SvelteKit researcher UI (Phase-0 deliverable)
infra/              docker-compose.yml (Postgres only for the PoC)
docs/               Design + mathematics + sequential build reference (see docs/README.md)
```

## Build stages & progress

The build is sequenced; each stage assumes the prior ones are done. Full prompts and checklists
live in `docs/{NN}-build-*.md` (see [.cursor/stage-map.md](.cursor/stage-map.md) for the
stage-number → file map).

| Stage | Deliverable | PoC | Status |
|------:|-------------|:---:|:------:|
| 12 | Repository & dev environment | ✅ | **done** |
| 13 | Database schema & migrations | ✅ | **done** |
| 14 | Event log core (hash chain) | ✅ | **done** |
| 15 | Snapshot engine & materialization | ✅ | **done** |
| 16 | Semantic ops & governance gate | ✅ | **done** |
| 17 | Auth & identity (email magic-link only) | ◐ | **done** |
| 18 | Web API surface (REST + SSE) | ✅ | **done** |
| 19 | Research IDE frontend (four-mode editor) | ✅ | **done** |
| 20 | CRDT convergence layer | ⏭ | **skipped (PoC)** |
| 21 | Referential integrity & semantic merge | ✅ | **done** |

The PoC build (Stages 12–21, CRDT excluded) is **complete**. Stages 22–31 (public consultation,
ML, civic ID, closeout, deployment) are **out of PoC scope**.

## Getting started

Full prerequisites and verified setup notes are in **[SETUP.md](SETUP.md)**. Quick start:

```sh
task setup          # first-time: copies .env, installs frontend deps
task dev            # Postgres + migrate + API (:8080) + frontend (:5173)
task test           # cargo test --workspace + frontend vitest/playwright
task lint           # clippy + eslint + svelte-check
task fmt            # format everything
task migrate        # apply migrations   (task migrate-reset drops the DB — dev only)
```

Health checks once `task dev` is up:

```sh
curl http://127.0.0.1:8080/healthz   # engine-api
curl http://127.0.0.1:5173/healthz   # researcher frontend
```

> **Note:** From Stage 13 onward the Rust tests need a reachable Postgres (`DATABASE_URL`); run
> `task up` first locally (CI provides a Postgres service). One environment caveat: `ring`'s
> build script is blocked by Windows Smart App Control in Enforce mode — build on a SAC-free
> machine or in WSL2. See [SETUP.md](SETUP.md).

## Roadmap beyond the PoC

The same engine extends to **national-scale public consultation** by adding layers above it:
identity tiers (T0 observer → T1 reactor → T2 contributor → T3 verified citizen), a four-stage
moderation pipeline, a Polis-style deliberation layer that surfaces **bridge statements** (cross-
cluster consensus), an AI summary loop validated back to each opinion cluster, and mandatory
hash-linked **closeout reports** with cryptographically verifiable counts. Detail and phasing in
[docs/05–09](docs/) and the [docs series index](docs/README.md).

## Contributor & AI-agent tooling

This repo ships parallel assistant assets so any agent or editor can work within the project's
rules (PoC scope, immutable migrations, append-only events):

| Path | For |
|------|-----|
| [`.claude/`](.claude/) | Claude Code — subagents, skills, and safety hooks (`settings.json`) |
| [`.cursor/`](.cursor/) | Cursor — rules, skills, agents, hooks, and the canonical stage map |
| [`.codex/`](.codex/) | Codex — mirrored prompts and hook scripts |
| [`AGENTS.md`](AGENTS.md) | Single-page guide read by every agent before implementing |

## Documentation

Start with [docs/README.md](docs/README.md) for the full numbered series. Most-used entry points:

- **Why the engine works** — [docs/02](docs/02-versioning-design-vol1.md), [docs/03](docs/03-versioning-mathematics-vol2.md) (or combined: [docs/04](docs/04-versioning-combined.md))
- **UX theory** — [docs/01-uiux-theory.md](docs/01-uiux-theory.md)
- **Stack & layout** — [docs/11-build-index-and-stack.md](docs/11-build-index-and-stack.md)
- **PoC requirements** — [docs/dynodoc_poc_prd.md](docs/dynodoc_poc_prd.md)
- **Committed decisions** — [docs/decisions/000-initial.md](docs/decisions/000-initial.md)

## License

[AGPL v3](LICENSE). Committed early because changing it later is hard, and it is the correct
license for a system that will eventually run public consultation.
