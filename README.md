# Dynodoc — version control for documents

Latest security source work (11 October 2026): schema 0029 separates operator
migrations from a restricted API database login. Schema 0028 also requires verified
snapshot receipts; existing data needs explicit replay backfill before rollout. See
[the database boundary](docs/DATABASE-PRIVILEGES.md), [snapshot contract](docs/VERIFIED-SNAPSHOTS.md) and
[checks/remaining gates](docs/ENGINE-EVOLUTION-HANDOFF.md).

> Version control and provenance for documents edited in your own tools. Collect block changes in Projects, review and merge them, and pull a shared history into another working copy.

**Current direction (10 October 2026):** Local/remote document repositories around
Word/Docs, Excel/Sheets and PowerPoint/Slides. The target is compact change transfer,
shared content storage, offline history, reviewed merges and verifiable provenance.
Research instruments remain one supported adapter, not the product roadmap.
[Product specification](docs/PRODUCT-SPEC.md) · [Ordered implementation plan](docs/ENGINE-EVOLUTION-PLAN.md)
· [Current handoff](docs/ENGINE-EVOLUTION-HANDOFF.md).

**Status:** The centralized engine, Projects/review, checked proposal bundles and
six development connectors exist. Current source hardening adds revocable logins,
fail-closed service credentials, checked snapshots, runtime database privileges and
bounded history reads. Deployment acceptance and project policy authority remain
urgent open gates; see
the [security findings and rollout contract](docs/SECURITY-HARDENING.md).
Shared checkpoint readers and bounded database
batches reuse unchanged objects. Shared writes remain opt-in; default snapshots
and draft bases still use full JSON. Content edits can proceed during checkpoint
publication; full-state scans and [measured limits](docs/CHECKPOINT-CONCURRENCY.md)
still gate rollout. [Historical recovery](docs/HISTORY-RECOVERY.md) adds selected
old content to a private draft under current review rules. Revision-range
revert/cherry-pick and native history downloads are available in
Version history; [test add-in packages and sign-in](extensions/INSTALL.md) cover all
six editor clients. Native acceptance and store publication remain open. A local commit DAG,
complete offline convergence, full native format capture and large-file/service
capacity still require implementation and acceptance. **License:** [AGPL v3](LICENSE).

The new signed-in front page is **Projects**: member-only profiles, multiple files,
per-file main/private drafts, pull requests, comparisons and history. Basic web
editors have their own tab. Test uploads are limited to **3 MB (3,000,000 bytes)**.
Project-wide portable branches remain planned; see [the handoff](docs/ENGINE-EVOLUTION-HANDOFF.md)
for source verification and the separate production release state.

- [Standalone engine — MIT](https://github.com/ArefinAlter/dynodoc-engine) · [extraction and integration notes](docs/ENGINE-EXTRACTION.md)
- [Detailed engineering and product handoff](docs/HANDOFF.md)
- [Current implementation and remaining work](docs/PROJECT-STATUS.md)
- [Google and email setup](docs/GOOGLE-AND-EMAIL-SETUP.md)
- [Vercel and VPS deployment](docs/VERCEL-DEPLOYMENT.md)
- [Research file compatibility](docs/research-compatibility.md)
- [ODK Central and KoboToolbox connection](docs/ODK-KOBO-SETUP.md)
- [All research connections and supported preview logic](docs/RESEARCH-CONNECTIONS.md)
- [Versioning conformance and mathematical limits](docs/VERSIONING-CONFORMANCE.md)
- [Admin, branding and Search Console](docs/SEARCH-AND-ADMIN-SETUP.md)
- [Expanded pilot scope](docs/decisions/001-research-team-demo.md)

---

## The problem

Teams revise proposals, policies, reports, contracts and spreadsheets together.
Whole-file copies lose the relationship between wording, formatting and decisions.
Dynodoc gives structural blocks permanent identities and records changes to text,
formatting and position. Teams can explore personal drafts, review overlapping
changes, share selected work and restore earlier versions without erasing history.
Research instruments remain a supported use case, not the product's organizing principle.

See [the product pivot](docs/decisions/004-document-version-control.md) and
[office editor support and limits](docs/OFFICE-EDITOR-CAPABILITIES.md).

Dynodoc makes version control and the audit trail **part of the data model**. Every
content change is a typed semantic event, attributable, hash-linked, and replayable.
Supported independent edits can merge after validation; overlapping edits and shared
constraints can require explicit resolution. A reviewer's suggestion is a proposal the author accepts or rejects with
a recorded outcome. Document metadata and permission changes still need equivalent audit events.

## How it works

The system rests on an **append-only, hash-chained event log** as the system of record:

- **Typed semantic events.** Every change is an event such as `NodeCreated`, `FieldEdited`,
  `NodeMoved`, `CommentAdded`, `SuggestionProposed`, `SuggestionAccepted` — operating on nodes,
  not text.
- **Stable-identity nodes.** Each supported logical node gets a **ULID** at creation that
  never changes for the node's lifetime. Stable IDs provide exact correspondence; total merge cost also includes
  content comparison, ordered indexes, text diff and validation.
- **Materialized state by fold.** Current document state is derived by folding the event log;
  periodic **snapshots** pin intermediate states so reads are fast. Snapshots are derived, never
  authoritative.
- **Tamper-evidence.** Each event is chained: `chain_hash = SHA256(content_hash ‖ prev_chain_hash)`,
  genesis from 32 zero bytes. A **Merkle root** over snapshots commits to the whole tree. Anyone
  with document access can verify the chain's internal consistency. Detecting a wholesale
  rewrite by a database administrator also requires an independently retained trusted hash
  or backup; the current deployment does not publish external integrity checkpoints.

The versioning volumes explain the design and its intended mathematical properties.
The pilot uses serialized server writes and explicit merge review; the CRDT convergence
layer is deferred. See [docs/02-versioning-design-vol1.md](docs/02-versioning-design-vol1.md)
and [docs/03-versioning-mathematics-vol2.md](docs/03-versioning-mathematics-vol2.md).

### The editor and the review workflow

The original prototype provides four questionnaire views. The hosted workspace now has separate document, spreadsheet and questionnaire editors; these legacy views remain in the codebase:

| Mode | What it is |
|------|-----------|
| **Document** | Prose-shaped rendering — reads like a printed methodology appendix. Default mode. |
| **Outline** | Two-pane tree view for navigation and structural reorganization. |
| **Spec** | The underlying XLSForm as an editable spreadsheet table. |
| **Preview** | Legacy control-only view. The hosted questionnaire editor executes the documented XLSForm logic subset. |

On top of it sits the **suggesting workflow**: a reviewer proposes a change without applying it;
it lives as a proposal in the author's queue until accepted or rejected. When two edits touch the
*same field*, the system surfaces a **four-card conflict** (Original / current version /
incoming version / compose-new) rather than `<<<<<<<` markers. Every merge runs a **referential
integrity pass** — no dangling variable references, no cycles, no duplicate var-names — that
blocks a `Deployed` event when the instrument would break.

## Two regimes, one engine

The architectural commitment: **the engine never changes** between regimes.

1. **Document version control** (the immediate target) — teams editing and reviewing rich text and spreadsheets; research instruments are one use case.
   This is the current implementation focus.
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
  Postgres). Core Playwright workflows are now exercised against a running local stack; Lighthouse and broader accessibility/load audits remain open. See [SETUP.md](SETUP.md).

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
| 17 | Auth & identity (email link and Google) | ◐ | **implemented; provider setup pending** |
| 18 | Web API surface (REST + SSE) | ✅ | **done** |
| 19 | Research IDE frontend (four-mode editor) | ✅ | **done** |
| 20 | CRDT convergence layer | ⏭ | **skipped (PoC)** |
| 21 | Referential integrity & semantic merge | ✅ | **done** |

The original PoC stages have implementations, with verification limits recorded in
[SETUP.md](SETUP.md). The expanded researcher product is still in development; see
[the current assessment](docs/PROJECT-STATUS.md). Public consultation, ML and civic
identity remain future work.

The rich-text engine now records compact formatting-aware changes and merges
independent edits within text paragraphs. See [the research and implementation
record](docs/RICH-TEXT-VERSIONING-RESEARCH.md) for the run model, prior art,
benchmarks, current import limits, and the remaining storage/CRDT work.

## Getting started

Full prerequisites and verified setup notes are in **[SETUP.md](SETUP.md)**. Quick start:

```sh
task setup          # creates missing runtime/operator env files, installs frontend deps
task up             # local PostgreSQL
# Configure separate logins using docs/DATABASE-PRIVILEGES.md.
task migrate        # operator process reads .env.migrate
task db:grant-runtime # grant the already-created dynodoc_runtime login
task db:check       # verify runtime login and schema
task dev            # API (:8080) + frontend (:5173); never migrates
task lint           # clippy + eslint + svelte-check
task fmt            # format everything
```

Health checks once `task dev` is up:

```sh
curl http://127.0.0.1:8080/healthz   # engine-api
curl http://127.0.0.1:5173/healthz   # researcher frontend
```

Run `task test` in a separate shell with `DATABASE_URL` pointing to an isolated
test database owner with CREATE DATABASE and CREATE ROLE. Never use hosted data or
start the API from that test shell. See [the database setup contract](docs/DATABASE-PRIVILEGES.md).

> **Windows note:** `ring`'s
> build script is blocked by Windows Smart App Control in Enforce mode — build on a SAC-free
> machine or in WSL2. See [SETUP.md](SETUP.md).

## Current roadmap

Finish the security boundaries and rollout checks, then bounded shared content,
portable project commits and branches, durable offline exchange and convergence.
Native format fidelity, independently verifiable audit commitments and measured
large-file/service limits follow the [engine evolution plan](docs/ENGINE-EVOLUTION-PLAN.md).
Word/Docs, Excel/Sheets and PowerPoint/Slides connectors also need installed-host
acceptance and distribution. The older civic/public-consultation documents are
historical adapter proposals, not the current product roadmap.

## Contributor & AI-agent tooling

This repo ships parallel assistant assets so any agent or editor can work within the project's
rules (current product scope, immutable migrations, append-only events):

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

[AGPL v3](LICENSE) for this application. The [standalone engine](https://github.com/ArefinAlter/dynodoc-engine)
is separately published under MIT with the owner's authorization.
