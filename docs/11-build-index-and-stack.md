# 00 · Build Reference Index & Stack Decisions

### How to use this series

This is a numbered, sequential reference for building Dynodoc platform end to end. Each file represents one focused stage of work, in the order you should do them. They are written assuming you are working with Claude Code or a similar AI coding assistant, with **one Claude Code prompt at the top of each stage's file** that you can paste in to start.

The stages are sized so that each is roughly 1–3 weeks of focused work for a small team. Don't skip ahead. The decisions taken in earlier files constrain later files — for example, the database schema chosen in stage 02 is referenced by every subsequent stage, and the event-log invariants established in stage 03 are what the CRDT layer in stage 09 relies on.

### Companion reading

- `dynodoc_versioning_complete.md` — the design and mathematics (Volumes I & II). Read sections referenced by each stage.
- `dynodoc_public_consultation_design.md` — the public-scale extension. Referenced in stages 11+.
- `dynodoc_interface.html` / `dynodoc_interactive.html` — the UX prototype the Phase-0 frontend should converge toward.

### The full sequence

```
00 · Build Reference Index & Stack Decisions          ← you are here
01 · Repository and Development Environment Setup
02 · Database Schema and Migrations
03 · Event Log Core (the irreducible foundation)
04 · Snapshot Engine and State Materialization
05 · Semantic Operation Vocabulary and Governance Gate
06 · Authentication and Identity Tiers (T1–T2)
07 · Web API Surface (REST + Server-Sent Events)
08 · Dynodoc Frontend (the Phase-0 deliverable)
09 · CRDT Convergence Layer (for live co-edit)
10 · Referential Integrity and Semantic Merge Engine
11 · Public Consultation Surface (read-only public view)
12 · Citizen Participation (vote / comment / suggest)
13 · Moderation Pipeline (Stages 1–3 automated)
14 · Moderation Pipeline (Stage 4 human review + appeals)
15 · Deliberation Layer (Polis-style clustering)
16 · AI Summary with Validation Loop
17 · T3 Verified Citizen Identity (civic-ID integration)
18 · Closeout Reports and Verifiable Counts
19 · Transparency Reports and Audit Tools
20 · Multi-Tenant Hardening and Deployment
```

Stages 01–10 deliver the **Dynodoc Phase-0 product** (small expert teams). Stages 11–20 add the **public consultation layers** on top of the same engine. A working team should hit usable state after stage 08, deployable internally after stage 10, deployable to a pilot consultation after stage 14, and ready for national-scale deployment after stage 18.

---

## The stack: what to build each part in, and why

This is the single most consequential set of choices you make before writing code. The rule throughout: **boring, fast, sovereign-hostable, with strong types and a small surface.** No JavaScript on the backend. No exotic data stores. No closed-source dependencies on critical paths.

### Backend core: Rust

The event log, hash chaining, semantic operation handlers, governance gate, snapshot engine, API server — all of it in **Rust**, using **Axum** (web framework) + **SQLx** (async Postgres) + **Tokio** (runtime). The reasons are not aesthetic; they are concrete:

- The hash-chain insert path has hot-loop bytes-and-SHA-256 work where Rust outperforms anything garbage-collected by an order of magnitude.
- The audit guarantee depends on type-level correctness — events are immutable, fields are typed, the hash is computed once and never re-derived. Rust's ownership model makes "this struct cannot be mutated after signing" a compiler invariant, not a code review note.
- Memory safety matters because the platform runs untrusted public input and you cannot afford a parsing bug to become an arbitrary-write vulnerability. Bangladesh's government data is not the place to be debugging buffer overflows.
- Single static binary deployment. No JVM, no runtime, no `node_modules`. Drop the binary on a VPS and it runs.

The cost of Rust here is real (slower to write than Python, smaller hiring pool than Node), but the runtime hot path of this system is small and well-defined — once written, it doesn't churn. Use Rust for the engine; do *not* use it for things that don't need it (see Python and TypeScript below).

### Frontend: SvelteKit + TypeScript

For both Dynodoc (expert-team editor) and the citizen-facing consultation surface, use **SvelteKit** with **TypeScript**. Reasons:

- Dynodoc needs rich editing UI — track-changes, suggesting mode, four-mode editor, inline conflict resolution. Svelte's component model and store primitives map directly onto these patterns; the prototype HTML files we built can be ported into Svelte components mostly unchanged.
- The citizen surface needs phone-first, low-bandwidth, accessibility-compliant rendering. SvelteKit's server-rendered-by-default mode produces small initial payloads (~50–100 KB) compared to React or Next.js (~200–400 KB minimum), which matters on a 2G connection in Cox's Bazar.
- TypeScript everywhere so the API contract between Rust backend and Svelte frontend is type-checked end-to-end via OpenAPI codegen.
- Svelte's transition primitives make the suggesting-mode interaction (highlights appearing inline, accept/reject animations) feel native without library dependencies.

Alternative if your team strongly prefers it: SolidJS (similar performance, similar mental model). Avoid React for this product — bundle size, runtime cost, and the implicit state-management complexity don't pay back for what is fundamentally a server-state-mirroring application.

### Identity, moderation classifiers, deliberation: Python (FastAPI + scikit-learn + sentence-transformers)

The deliberation layer (Polis-style clustering), the soft-classifier moderation stage, and the AI summary loop are **Python with FastAPI**. The reasons are also concrete:

- This is the ML side of the system. PCA, k-means, HDBSCAN, sentence embeddings, LLM API calls — all of these have first-class Python libraries (scikit-learn, sentence-transformers, vLLM client) and no equivalent in Rust. Re-implementing them in Rust would be a project unto itself for no benefit.
- These services run independently of the engine: they read events from Postgres (or via SSE from the Rust backend) and write derived views back. A separate process in a separate language is the correct boundary.
- FastAPI gives you OpenAPI for free, which keeps the Rust↔Python contract honest.
- Use **uv** for Python dependency management (10–100× faster than pip, lockfile-clean).

### Database: PostgreSQL 16+

Decided already (Mode C analysis showed standard Postgres handles it). Specific configuration choices:

- Use `BIGSERIAL` event sequence + ULIDs as documented in the schema.
- Enable `pgcrypto` extension for SHA-256 (so chain hashes can be verified server-side without app-language dependency).
- Partition the `event` table by `document_id` once any single document approaches 1M events; before that, a single unpartitioned table is fine.
- Enable point-in-time recovery (WAL archiving) — for an audit-bearing system this is non-negotiable.

For development: docker-compose with a single Postgres container. For staging/production: managed Postgres (AWS RDS, Cloud SQL, or sovereign equivalent), or self-managed on a single VM with `pgbackrest` for backups.

### Real-time channel for live co-edit: Server-Sent Events (not WebSockets)

For the live co-edit layer (stage 09), use **SSE over HTTP/2**, not WebSockets. SSE is one-way (server→client), text-only, and reconnects automatically — which exactly matches what you need: the server pushes events to subscribed clients, clients send edits via standard POST. WebSockets add bidirectional state and connection management you don't need, complicate auth, and are harder to scale. The CRDT layer's "send my edit" path is a plain HTTP POST; the "receive others' edits" path is an SSE stream.

### LLM hosting: vLLM with an open model

For the summary loop in stage 16, **self-host an open model with vLLM**. Concretely:

- Llama 3.1 8B or Mistral 7B as the default for summary work; both are competent at the cluster-summary task with the validation loop catching errors.
- Bangla-capable models matter: BanglaT5, BanglaBERT for embeddings, and Llama 3.1's multilingual capability is adequate for Bangla summarization once fine-tuned briefly on policy text.
- vLLM serves on a single GPU (A10, T4, or any 24GB consumer card) at far higher throughput than naive HuggingFace serving. ~50–500 requests/min depending on prompt length, ample for the consultation's needs.
- For development and small consultations, an Anthropic or OpenAI API call is fine and cheap. Switch to self-hosted once a national consultation is on the calendar.

### Object storage for snapshots and large attachments: S3-compatible

Snapshots beyond ~100 KB go to S3-compatible object storage (MinIO if self-hosted, AWS S3 or Backblaze B2 if managed), with the metadata row in Postgres pointing at the object. Keeps the database lean. Anything attachable to a contribution (image, PDF) also goes here.

### Observability: OpenTelemetry + Grafana

Not optional for a system running public input. Every event gets a trace; every moderation decision logs structured telemetry; every API call records latency and status. Grafana dashboards for live consultation health (event rate, moderation queue depth, deliberation cluster recompute time, P99 API latency). Loki for logs, Tempo for traces, Prometheus for metrics — all open source, all self-hostable.

### CI/CD: GitHub Actions or Forgejo Actions

Build, test, lint, and deploy via the repository host's actions runner. For sovereign deployments, **Forgejo** (a self-hostable Gitea fork) with its actions runner replaces GitHub entirely — same workflow files, no vendor dependency.

### Summary table of stack choices

| Component | Stack | Why |
|---|---|---|
| Event log & engine core | Rust + Axum + SQLx + Tokio | Speed, safety, static binaries |
| Database | PostgreSQL 16+ | Boring, audited, sovereign |
| Dynodoc frontend | SvelteKit + TypeScript | Rich UI, small bundles, ports prototype |
| Citizen consultation frontend | SvelteKit + TypeScript (same codebase) | Phone-first, accessibility, sovereignty |
| ML / deliberation / moderation classifiers | Python + FastAPI + scikit-learn + sentence-transformers | First-class libraries |
| Real-time channel | SSE over HTTP/2 | Simpler than WebSockets, reconnects free |
| LLM serving | vLLM + open model on GPU | Self-hostable, fast |
| Object storage | S3-compatible (MinIO self-hosted) | Standard, swappable |
| Auth tokens | Paseto v4 (not JWT) | Same use, fewer footguns |
| Crypto for hash chain | SHA-256 via Rust ring crate | Audited, fast |
| Crypto for signatures | Ed25519 via Rust ring crate | Compact, fast |
| Observability | OpenTelemetry + Grafana stack | Open, self-hostable |
| CI/CD | GitHub Actions or Forgejo Actions | Sovereign option available |

### Repository layout

A single monorepo. The pieces have clean boundaries but enough cross-cutting concerns (the OpenAPI schema, the shared event types, the integration tests) that splitting them costs more than it saves at this scale.

```
dynodoc/
├── crates/
│   ├── engine-core/        Rust — event log, hash chain, materialization
│   ├── engine-api/         Rust — Axum HTTP server, SSE, auth
│   ├── engine-cli/         Rust — admin CLI (migrate, snapshot, verify, replay)
│   └── engine-shared/      Rust — types shared by core and api
├── services/
│   ├── deliberation/       Python — Polis clustering, statement embedding
│   ├── moderation/         Python — soft classifiers, summary loop
│   └── summarizer/         Python — LLM client + validation loop
├── frontend/
│   ├── researcher/         SvelteKit — Dynodoc itself
│   ├── citizen/            SvelteKit — the public consultation surface
│   └── shared/             TypeScript — shared types, components, OpenAPI client
├── infra/
│   ├── docker-compose.yml  local dev
│   ├── helm/               k8s charts for production
│   └── migrations/         SQL migration files
├── docs/
│   └── *.md                this build reference + the design documents
└── tools/
    ├── replay/             a tool that replays an event log and verifies the chain
    └── audit/              tools for transparency reports and verifiable counts
```

### What you should commit to up front

Before stage 01, write down (somewhere persistent — `docs/decisions/000-initial.md`) the answers to these four questions. They will be referenced repeatedly.

1. **Hosting model.** Managed cloud (where, which region), sovereign self-hosted (which provider), or hybrid? This shapes the auth and storage choices.
2. **Initial language for the citizen UX.** Bangla, English, or bilingual from day one? Bilingual is more work but exactly correct for Bangladesh; Bangla-only is easier but a research-team product can ship English-only first.
3. **First real user.** Who is the first research team that will use the Phase-0 product? Their constraints (offline-first or not, ODK integration needed or not, multilingual or not) shape stages 06–08.
4. **License.** AGPL v3 is the right answer for a system that will eventually run public consultation; commit early because changing later is hard.

The next file (`01 · Repository and Development Environment Setup`) assumes these are decided.
