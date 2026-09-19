# dynodoc — local development setup

The PoC stack: **Rust** engine (Axum + SQLx + Tokio), **PostgreSQL 16** (Docker),
**SvelteKit** researcher frontend, orchestrated by **go-task**. See
`docs/decisions/001-research-team-demo.md` for the expanded hosted pilot. The numbered
stage notes below are historical; the latest verification is at the end of this file.

## Prerequisites

| Tool | Version | Why | Install (Windows) |
|---|---|---|---|
| Rust | 1.96.0 (pinned) | engine crates | `winget install Rustlang.Rustup` then re-open shell |
| sqlx-cli | latest | migrations | `cargo install sqlx-cli --no-default-features --features postgres,rustls` |
| Docker Desktop | latest | Postgres container | `winget install Docker.DockerDesktop` (needs WSL2 + restart) |
| Node | 22 LTS (`.nvmrc`); 24 works | frontend | already installed |
| pnpm | 9+ | frontend package mgr | `corepack enable pnpm` |
| go-task | 3+ | task runner | `winget install Task.Task` |

> The `rust-toolchain.toml` pins Rust 1.96.0; `rustup` auto-installs it on first
> `cargo` invocation in this directory.

## First run

```sh
task setup          # creates .env only if absent, installs frontend deps
# Before task dev, edit .env as described below.
task dev            # boots Postgres, applies migrations, starts API + frontend
```

Then verify the Stage-12 gate:

```sh
curl http://127.0.0.1:8080/healthz        # -> {"status":"ok","service":"engine-api"}
curl http://127.0.0.1:5173/healthz        # -> {"status":"ok","service":"researcher-frontend"}
# Browser: http://127.0.0.1:5173 shows the homepage.
task test           # cargo test --workspace + frontend vitest/playwright
task lint           # clippy + eslint + svelte-check
```

Generate independent random values for `PASETO_LOCAL_KEY` (32 bytes as 64 hex
characters), `WEB_SERVICE_KEY`, and `SESSION_SECRET` (at least 32 characters).
Use `DEV_AUTH=true` only on your local machine to display a development email
sign-in button. `task dev` passes the root `.env` to both processes. When running
`pnpm dev` separately, also configure `frontend/researcher/.env` from its example,
using the same `WEB_SERVICE_KEY`. Keep `DATABASE_URL` consistent with the local
Compose port (5432 by default). Existing environment files are preserved by setup.

For the public pilot use [Google and Resend setup](docs/GOOGLE-AND-EMAIL-SETUP.md)
and [Vercel/VPS operations](docs/VERCEL-DEPLOYMENT.md). The public deployment always
disables development sign-in.

## Useful tasks

`task --list` shows everything. Common: `task up` / `task down` (Postgres only),
`task migrate` / `task migrate-reset` (dev), `task fmt`, `task clean`.

## Environment notes (this Windows dev machine)

**Smart App Control — disabled (resolved).** SAC was in Enforce mode and blocked
freshly-built unsigned binaries, including `ring`'s build-script binary, which prevented
the workspace from building. SAC has since been turned **off** on this machine
(`…\CI\Policy\VerifiedAndReputablePolicyState = 0`), so `cargo build`/`test`/`run` work
natively. On a fresh machine with SAC in Enforce, either turn it off (Windows Security →
App & browser control → Smart App Control → Off, reboot — **irreversible**) or build inside
WSL2 (SAC governs only Windows .exe).

**Docker / WSL2 — enabled (resolved).** WSL2 is installed (`wsl --status` → "Default
Version: 2") and Docker Desktop's Linux engine starts cleanly. `docker compose -f
infra/docker-compose.yml up -d` brings up `dynodoc-postgres` (Postgres 16.14) healthy,
with the `dynodoc` user + database created and bound to `127.0.0.1:5432`. Start Docker
Desktop first (wait for the whale icon to settle), then `task up`.

CI (Linux runners) is unaffected and runs the full test suite.

## Already-applied stack deviations (vs docs/11–12)

- Rust toolchain pinned to **1.96.0** (doc said 1.83.0 — stale; current crates need
  edition2024 / Rust 1.85+).
- Everything else matches the doc: **`ring`** for crypto and sqlx on **`rustls`**.

## Stage-12 verification status

Verified natively on this machine (Rust 1.96.0, SAC off):

- [x] `cargo build --workspace` clean (with `ring` + `rustls`).
- [x] `cargo test --workspace` green (engine-shared roundtrip test; all binaries execute).
- [x] `cargo clippy --workspace --all-targets -- -D warnings` clean.
- [x] `cargo fmt --all -- --check` clean.
- [x] `engine-api` `/healthz` → 200 `{"status":"ok","service":"engine-api"}`.
- [x] researcher frontend `/healthz` → 200; home page renders ("dynodoc").
- [x] Frontend `pnpm check`, `pnpm build`, `pnpm test:unit` pass.
- [x] Toolchain installed: Rust 1.96.0, go-task 3.50.0, pnpm 11.7.0, Docker Desktop,
      sqlx-cli 0.9.0 (`--features postgres,rustls`).
- [x] `task up` — `dynodoc-postgres` (Postgres 16.14) boots healthy on `127.0.0.1:5432`.
- [x] Postgres user + database `dynodoc` exist (verified via `\conninfo`).
- [x] `sqlx migrate run --source migrations` succeeds against the live DB and creates the
      `_sqlx_migrations` ledger (no migrations yet — schema lands in Stage 13).
- [x] `/healthz` → 200 with Postgres running (API is a Stage-12 placeholder; no DB calls yet).

Pending (run on CI Linux runners, not this machine):

- [ ] Playwright E2E — not run locally yet (CI installs the browser and runs it).
- [ ] `ci-rust.yml` and `ci-frontend.yml` pass on a pushed branch (Linux runners).

## Stage-13 verification status (database schema)

Schema scope is **engine + auth only** (per the PoC brief): migrations `0000`–`0004`
plus `0009_views`; the consultation-layer tables (`0005`–`0008`: moderation,
deliberation, closeout, rate/signals) are intentionally skipped. Verified against the
live `dynodoc-postgres` container:

- [x] `sqlx database drop -y && sqlx database create && sqlx migrate run` applies all six
      migrations cleanly (`task migrate-reset`).
- [x] `\dt` shows `document`, `node`, `event`, `snapshot`, `identity`, `magic_link`
      (+ `_sqlx_migrations`); `\dv` shows `current_documents`.
- [x] `cargo test --workspace` green — **7 schema integration tests** + 3 unit tests.
      The integration tests use `#[sqlx::test]` (fresh migrated DB per test): insert +
      FromRow read-back, all expected indexes present (`pg_indexes`), 32-byte hash
      columns store/round-trip, non-32-byte hash rejected by `octet_length` CHECK,
      `event` UPDATE/DELETE raise (append-only), `node.id` reassignment raises
      (stable-identity trigger), `content_hash` NOT NULL enforced.
- [x] `cargo clippy --workspace --all-targets -- -D warnings` and `cargo fmt --check` clean.

> **Note:** from Stage 13 onward, `cargo test --workspace` requires a reachable Postgres
> (`DATABASE_URL`). Run `task up` first locally; CI provides a Postgres service.

## Stage-14 verification status (event log core)

The append-only hash-chained log is the system of record. `engine-shared` gains the
typed, internally-tagged `EventPayload` vocabulary (the 11 PoC event types);
`engine-core::log` implements `append` / `verify_chain` / `read_range` /
`read_since_snapshot`; `engine-cli verify <document_id>` wires the audit check.

Append serialization deviates from the doc's pseudocode on purpose: it locks the
**`document` row** (`FOR UPDATE`), not the latest event row — locking the latest
existing event does not stop a concurrent txn computing a duplicate next `seq`.
`content_hash` is SHA-256 over four **length-framed** fields (type, target, canonical
JSON payload, actor UUID) so the concatenation is unambiguous; payload is canonicalized
(recursively sorted keys, no whitespace) for reproducibility across the JSONB round-trip.

- [x] `cargo build --workspace` clean.
- [x] `cargo test --workspace` green — **5 event-log integration tests** (`#[sqlx::test]`,
      fresh DB each) + **5 `log` unit tests** including three `proptest` properties at
      **1000 cases** each: an intact chain always verifies; tampering any payload byte is
      detected at the right `seq`; reordering is detected.
- [x] Integration tests cover: a built chain verifies; genesis `seq=1` with zero
      prev-hash and a gap-free 1-based sequence; `read_range` / `read_since_snapshot`;
      append to a missing document → `DocumentNotFound`; two **concurrent** appends get
      distinct consecutive seqs (document-row lock); and the tamper path — trigger
      disabled, `payload` of `seq 2` forged, trigger re-enabled → `verify_chain` returns
      `ChainBroken { seq: 2 }` (the automated equivalent of the "corrupt a byte in psql"
      checklist item; the CLI prints the same `seq`).
- [x] `cargo clippy --workspace --all-targets -- -D warnings` and `cargo fmt --check` clean.
- [x] `engine-cli verify <uuid>` connects, verifies, prints `OK` (exit 0) for a clean log;
      a malformed id gives a friendly error (exit 1); `CHAIN BROKEN at event seq N` (exit 1)
      on a broken chain.

Deferred (not blocking the stage's correctness goals):

- [ ] **`criterion` append/verify benchmarks** (P99 < 5 ms append; 100k-event verify < 1 s).
      `criterion` is not yet a workspace dependency and DB-backed benches need a running
      Postgres; add when wiring perf gates. Functional latency is well within target at
      PoC scale.

## Stage-15 verification status (snapshot engine & materialization)

Reads must cost `O(events since last snapshot)`, not `O(N)`. `engine-core::materializer`
folds the event stream into a `DocumentState` (`BTreeMap<NodeId, MaterializedNode>` plus
comment / suggestion / removed-choice side-state); `engine-core::snapshot` takes snapshots,
maintains a node-hash **Merkle root**, and serves the snapshot-backed read path. The CLI's
`replay` and `snapshot` subcommands are now implemented (were stubbed).

PoC scope: snapshots stay in **Postgres JSONB** (no MinIO offload); the only mandatory
immediate-snapshot trigger is **`Deployed`** (`ConsultationOpened/Closed` are out of scope).

Merkle root is over the **materialized node set** (leaf `H(0x00 ‖ canonical_json(node))`,
internal `H(0x01 ‖ left ‖ right)`, balanced binary padded with zero hashes, deterministic
ULID order), per docs/15 §E.3.1 — reusing the stage-14 canonical-JSON encoder.

Snapshotting is wired into the append happy path (per docs/15 §4/§6): `Deployed` takes an
immediate snapshot synchronously after commit; other appends spawn a best-effort background
`ensure_recent` (threshold 500). `append`'s FK on `event.target_node_id → node(id)` means a
node row must exist before an event targets it — in the live system the stage-16 op handlers
do this atomically; the stage-15 tests insert the node row directly (the fold itself reads
only the event stream).

- [x] `cargo test --workspace` green — **+1 materializer proptest** (128 cases): for every
      cut `k`, `fold(events) == from_snapshot(snapshot@k, events[k..])` (snapshots are
      correct); a snapshot Merkle test (changing one node byte changes the root); and **4
      snapshot integration tests** (`#[sqlx::test]`): take + `read_current_state` round-trip;
      `Deployed` triggers an immediate snapshot; `ensure_recent` snapshots past threshold;
      and a correctness-at-scale test (1000-node snapshot + 500-event tail folds to the
      right state). The wall-clock perf target (<10 ms, release) is the deferred
      `criterion` bench — a hard timing assert in a debug `cargo test` flakes on slow CI
      runners (it was the cause of the first ci-rust failure on this stage).
- [x] `cargo clippy --workspace --all-targets -- -D warnings` and `cargo fmt --check` clean.
- [x] `engine-cli replay <uuid>` folds the log and prints `DocumentState` JSON; `engine-cli
      snapshot <uuid>` takes a snapshot and prints its id, `through_seq`, and Merkle/chain
      hashes — both verified live against the dev DB.

> **Fix applied while completing this stage:** the snapshot integration `seed` appended a
> `NodeCreated` event without first inserting the node row, violating
> `event_target_node_id_fkey` (3 tests failed). Inserting the node row in `seed` resolved it.

## Stage-16 verification status (semantic ops & governance gate)

Every change now passes through a **governance gate** before it may be appended. The gate
is pure decision logic over the materialized `DocumentState` — no I/O — so it is
exhaustively testable without a database (`cargo test -p engine-core --lib`). Three new
modules in `engine-core`:

- `ops` — `validate_op(state, op)` validates each `EventPayload` variant against current
  state: parent exists and containment is legal (a choice cannot hold a section), `var_name`
  is unique, a `NodeMoved` would not place a node inside its own subtree (the Vol II §C.2.2
  cycle check), choices target an item, suggestions are pending, and a `SuggestionProposed`
  wraps a *valid content* op. Returns a typed `OpError`; validation runs **before** append.
- `governance` — the PoC **capability matrix** `can_perform(role, op)` over the three PoC
  roles (`Author` / `Reviewer` / `Auditor`, PRD §6.8 FR-27) — **not** the full platform's
  T0–T3/Sponsor/Moderator tiers or consultation ops (`VoteCast`, `ModerationDecided`), which
  are out of PoC scope (PRD §3.2 NG2). `authorize(role, op, state)` composes capability →
  validity → (for `Deployed`) the integrity gate. `proposal_status` and `plan_acceptance`
  carry the suggestion lifecycle (see below).
- `integrity` — `validate_integrity(state)` is the §C.3.6 "silent semantic breakage" defense:
  it flags dangling `${var}` references, duplicate `var_name`s, and relevance dependency
  cycles. The gate refuses a `Deployed` if any violation exists (PRD §6.6). The richer
  three-way semantic merge that also uses this graph lands in stage 21.

PoC deviations recorded for later stages:

- **Roles, not tiers.** The capability matrix is `Author`/`Reviewer`/`Auditor` per the PRD,
  replacing docs/16's consultation tier matrix.
- **Proposal lifecycle without new event types.** A `SuggestionProposed.detail` carries the
  internally-tagged JSON of the wrapped content op (no schema/migration change). The
  materializer already tracks suggestion side-state (`accepted` / `rejected_reason`);
  `plan_acceptance` returns the now-canonical wrapped op an accept must append.
- **Two-event accept is a primitive, not yet wired.** FR-19 ("accepting atomically applies
  the wrapped event") is realized by `log::append_in_tx` (a new transaction-composable
  append, factored out of `append` with identical behavior) + `post_commit_snapshot`. The
  stage-18 API drives the actual `append_in_tx(wrapped) + append_in_tx(SuggestionAccepted)`
  in one transaction, alongside materializing the `node` table — which the event-log FK
  `event.target_node_id → node(id)` requires and which no stage before 18 wires.

- [x] `cargo test --workspace --lib` green — **+44 lib tests** total (39 engine-core,
      5 engine-shared), including: per-variant `validate_op` unit tests; an integrity suite
      (dangling refs, duplicate vars, relevance/self cycles, constraint-is-not-a-cycle); a
      capability **proptest** (256 cases — `authorize` returns `Unauthorized` iff
      `can_perform` is false, before any validation); and fold-level tests proving an
      accepted proposal's wrapped op folds into canonical state while a rejected one does
      not. Two `#[sqlx::test]` integration tests cover `append_in_tx` (two events atomic and
      gap-free; rollback leaves no events) — **DB-gated, run by `ci-rust`** (no local
      Postgres on this machine: Docker daemon down, host PG creds unknown).
- [x] `cargo clippy --workspace --all-targets -- -D warnings` and `cargo fmt --check` clean.
- [x] `cargo test --workspace --no-run` compiles every target (the `append_in_tx` refactor
      preserves `append`'s signature, so the stage 14/15 integration tests are unchanged).

## Stage-17 verification status (authentication & identity)

PoC auth is **email + magic link only** (PRD §6.8, NFR-9): no passwords, no tiers, no
phone/OAuth, no Redis/rate-limits/Sybil signals — those are consultation scope and were
trimmed from docs/17. The flow: `POST /auth/magic-link` (email) issues a one-time link →
`POST /auth/verify` exchanges it for a **Paseto v4 local** access token (NFR-8, not JWT) plus
a rotating refresh token → `POST /auth/refresh` rotates → protected handlers use the
`Authorization: Bearer` extractor. A document's capability role is **not** in the token; it is
per-document state in `document_access`, read at request time and handed to the stage-16 gate.

Structure / deviations recorded for later stages:

- **`engine-api` is now lib + bin.** The app is exposed as a library (`engine_api::app`,
  `AppState`, `auth::*`) with a thin binary, so the router/handlers are integration-testable
  from `tests/` (the bin-only crate could not be). The binary connects Postgres, runs
  `sqlx::migrate!` on boot, and serves.
- **Migration `0010_auth.sql`** (not 0005): 0005–0008 are reserved for consultation tables and
  0009 is views, so the PoC auth tables (`document_access`, `refresh_token`) take the next free
  number. Only the SHA-256 of every magic-link / refresh token is stored.
- **Roles, not tiers** (continued from stage 16): `document_access.role ∈ {author, reviewer,
  auditor}`; `governance::Role` gained `FromStr` to parse the column.
- **Magic link returned in the response** (`magic_link_token`) is a PoC/local-dev affordance —
  a deployment emails it instead. The all-zero `PASETO_LOCAL_KEY` placeholder is allowed for
  local dev with a startup warning.
- **Deferred to stage 18** (kept minimal here): wiring `store::role_for` into endpoint
  authorization (the `Forbidden` path), HttpOnly refresh-cookie + UA/IP binding, and the
  document/event REST + SSE surface.

- [x] `cargo test --workspace` green against a local Postgres — **+12 stage-17 tests**: 6 Paseto
      unit tests (mint/verify round-trip, expired/tampered/wrong-key/malformed rejected, short
      key invalid) and 6 DB+HTTP integration tests driving the real router via
      `tower::oneshot` (register→verify→`/auth/me`; magic link single-use; expired link
      rejected; refresh rotates and the old token is then rejected; `/auth/me` needs a valid
      bearer; `role_for` resolves the access list). All prior DB suites (14/15/16) still pass.
- [x] `cargo clippy --workspace --all-targets -- -D warnings` and `cargo fmt --check` clean.
- [x] Live binary smoke test: `engine-api` boots, applies migrations, and serves the full flow
      — `/auth/magic-link` → `/auth/verify` (returns a `v4.local.…` token) → `/auth/me` 200 with
      the bearer, **401** without it.

> **Local DB note (this machine):** a host PostgreSQL 17 service occupies `127.0.0.1:5432`
> (unknown creds), so the compose container cannot bind it. For verification, run a throwaway
> container on **5433** and point `DATABASE_URL` at it:
> `docker run -d --name dynodoc-pg-5433 -e POSTGRES_USER=dynodoc -e POSTGRES_PASSWORD=dynodoc -e POSTGRES_DB=dynodoc -p 127.0.0.1:5433:5432 postgres:16`
> then `DATABASE_URL=postgres://dynodoc:dynodoc@127.0.0.1:5433/dynodoc`. To use the documented
> compose stack on 5432 instead, stop the host service (`postgresql-x64-17`) first.

## Stage-18 verification status (web API surface — REST + SSE)

The full HTTP surface every frontend talks to now exists in `crates/engine-api`, layered
on the stage-17 identity routes. New modules: `error` (the single `ApiError` +
`IntoResponse`), `pagination` (opaque base64 cursors), `idempotency` (Postgres-backed),
`documents/` (read + create), `ops/` (the governance-gated write path + the apply funnel),
`stream` (SSE + per-document broadcast registry), and `audit` (verify / snapshot /
merkle-proof). `AppState` gained a `Subscriptions` registry; `app()` mounts every router
and adds gzip + CORS + trace middleware. A generated `ApiDoc` (utoipa) backs
`engine-cli openapi` and `GET /openapi.json`.

PoC deviations recorded for later stages:

- **No Redis.** Idempotency keys are Postgres-backed (`migrations/0011_idempotency.sql`,
  the next free number after `0010_auth`). A key is `(identity_id, key)`-scoped; a
  reserve-then-complete two-step (`INSERT … ON CONFLICT DO NOTHING RETURNING`) makes
  first-vs-replay atomic, with a 24h TTL. A replayed key returns the cached response
  without re-executing, so a retried POST never duplicates an event.
- **No `/ops/vote` route.** The PoC `EventPayload` vocabulary has no `VoteCast`
  (consultation-layer, out of scope per `docs/decisions/000-initial.md`), so the doc's
  `[T1+]` vote endpoint is intentionally omitted. The other 7 op/proposal/deploy
  endpoints go through `governance::authorize` (capability → validity → integrity).
- **Rate limiting is minimal (PoC).** Idempotency is implemented fully; identity/IP rate
  limiting is reduced to the SSE concurrent-subscriber cap (`503`/429 above 100 per
  document) per the stage brief's prioritization. A full limiter is post-PoC.
- **`node` table materialization moved here.** The write path materializes the `node`
  table inside the append transaction (so the `event.target_node_id → node(id)` FK
  holds — the stage-16 note flagged this as stage-18 work). Reads still fold from
  snapshot+tail, so the node table is a convenience/FK-anchor, not the read source.
- **Merkle-proof endpoint scope.** `snapshot.merkle_root` commits to the node set; the
  `merkle-proof/:event_id` endpoint builds an *event-content* Merkle tree over events
  `seq ≤ through_seq` (same `H(0x00‖leaf)` / `H(0x01‖l‖r)` domain separation) and returns
  a sibling-path proof + that root. Binding an event root into the snapshot row is a small
  post-PoC schema addition.
- **`engine-cli` depends on `engine-api`** so the OpenAPI generator and the spec share one
  `ApiDoc`. New workspace deps: `base64`, `utoipa` (axum_extras + yaml), `tokio-stream`,
  `futures`.

### Verified natively on this machine (SAC off, Rust 1.96.0, no Postgres)

SAC is off on this machine (see the environment note above), so the engine built and the
non-DB suites ran locally:

- [x] `cargo check --workspace --all-targets` clean (lib + bins + both test crates).
- [x] `cargo clippy --workspace --all-targets -- -D warnings` clean.
- [x] `cargo fmt --all -- --check` clean.
- [x] `cargo test --workspace --lib` green — 11 engine-api lib tests (cursor round-trip /
      clamp / malformed; the Merkle `inclusion_path_recomputes_root` proof check; the
      stage-17 Paseto tests), plus all 39 engine-core + 6 engine-shared lib tests (the
      6th locks the enum wire-form regression fixed below).
- [x] **OpenAPI generated + committed; CI drift gate wired.** `engine-cli openapi --out
      openapi.yaml` writes the spec; `openapi.yaml` is committed at the repo root and was
      generated by the tool (not hand-authored), so `cargo run -p engine-cli -- openapi |
      diff - openapi.yaml` is empty. New tasks: `task openapi` (regenerate) and
      `task openapi-check` (CI drift gate).

### Verified against a local Postgres (Docker on 5432)

`task up` (compose Postgres on 5432) was reachable, so the full DB-backed
`#[sqlx::test]` suite ran green via `DATABASE_URL=postgres://dynodoc:dynodoc@127.0.0.1:5432/dynodoc cargo test --workspace`:

- [x] Integration tests pass for every endpoint — `crates/engine-api/tests/api_test.rs`
      (**11 tests**): create→read document, auth + role rejection (auditor write 403,
      non-participant 403, unauth create 401), 404 for missing document,
      node-create→field-edit→node read, suggest→accept applies the wrapped op, idempotency
      (same key twice → one event, identical body), cursor pagination on `/events` (no
      overlap), public `verify` OK, and SSE delivery. The stage-13/14/15/16/17 DB suites
      stay green too.
- [x] **SSE delivery latency under 100ms** — `sse_delivers_a_new_event_within_100ms`
      subscribes, appends from another connection, and asserts the frame arrives within
      100ms (also asserts the `Cache-Control: no-cache, no-transform` header).
- [x] **Idempotency cache works** — `idempotency_key_dedupes_writes`.
- [x] **Auth/tier enforcement** — the role-rejection tests + the reused stage-17
      `AuthContext` extractor.

> **Two bugs were found and fixed by running this suite** (the write path and pagination
> are exercised over the wire for the first time in stage 18):
> 1. **Enum wire form.** `string_enum!` (`engine-shared/src/enums.rs`) derived serde's
>    default representation, so `NodeType` (de)serialized as the variant name `"Form"`
>    instead of its stored `as_str()` repr `"form"` — diverging from the SQL `CHECK`
>    columns and the JSONB payload. Every `node-create` request (`"node_type":"form"`)
>    was rejected by the `Json` extractor (422). Fixed by implementing `Serialize`/
>    `Deserialize` over `as_str()`/`FromStr` in the macro; a regression test locks it.
> 2. **`#[serde(flatten)]` + query strings.** `ListQuery`/`EventsQuery` flattened
>    `PageParams`, but `serde_urlencoded` buffers flattened values as strings, so a
>    flattened `limit: Option<i64>` failed to parse from `?limit=2` (the documents-list
>    test only passed because it sent no params). Fixed by inlining `cursor`/`limit` as
>    top-level fields.

### Rate-limit checklist item (deferred)

- [ ] **Strict per-identity write rate limits** (docs/18 §1 / checklist) are **not**
      implemented (no Redis in the PoC; deprioritized per the stage brief in favor of
      idempotency + SSE). Only the per-document SSE subscriber cap is enforced. Revisit
      with the consultation layer.

## Stage-19 verification status (Dynodoc researcher frontend)

The mock-driven editor scaffold in `frontend/researcher/` is now wired to the real
stage-18 REST + SSE API. The four-mode editor (Document / Outline / Spec / Preview)
renders live document state folded from the event log, with an authenticated SSE stream
applying incoming events incrementally, optimistic write ops, a suggesting workflow, a
proposals queue, and a history/audit view.

### What was built

- **Typed API client** (`src/lib/api/`): `pnpm gen:api` regenerates `openapi.d.ts` from
  `../../openapi.yaml` via `openapi-typescript`; `client.ts` layers `openapi-fetch` (typed
  requests) plus hand-written response types (`types.ts`) — needed because stage-18
  annotated every response body as `serde_json::Value`, so the generated responses are
  `unknown`. The client attaches `Authorization: Bearer <token>` and, on a `401`, attempts
  one refresh-token rotation then replays the request; if refresh fails it clears the
  session and the layout routes the user to `/login`.
- **Materializer reducer** (`src/lib/materializer.ts`): a TS port of the Rust fold in
  `crates/engine-core/src/materializer.rs`, mirroring each event arm (incl.
  `object_or_value` → non-object fields stored under `"value"`, `ChoiceAdded` `or_insert`,
  `removed_choices` tombstoning). 17 vitest cases lock it to the Rust rules.
- **Adapter** (`src/lib/adapter.ts`): folds the flat `DocumentState` node tree into the
  existing `EditorDocument` view model (sections → items → choices, ordered by `pos`,
  dropping deleted nodes / removed choices). 5 vitest cases.
- **Document store** (`src/lib/stores/documentStore.svelte.ts`, Svelte 5 runes): `load()`
  GETs `/documents/{id}`, seeds folded state, then opens the SSE stream resuming from
  `detail.latest_seq` — the seq the server folded the state through (see the backend note
  below); events apply through the reducer with no refetch. Tracks
  `connected | reconnecting | offline`. Write ops apply optimistically and reconcile;
  the SSE echo of our own write is deduped by seq.
  > **Backend follow-up done in this stage.** `GET /documents/{id}` now returns
  > `latest_seq` (`crates/engine-api/src/documents/routes.rs` + a new
  > `SnapshotEngine::read_current_state_with_seq` in `engine-core`). This replaced a
  > frontend workaround that paged `/events` to the last page on every editor open just
  > to find the resume seq — an O(events) request fan-out with a race (events landing
  > between the two requests were dropped from the live view). The seq now comes from the
  > same tail read as the fold, so it's exact: no extra request, no re-fold, no missed
  > window. Response stays `serde_json::Value` in `openapi.yaml`, so no spec drift.
- **SSE client** (`src/lib/api/sse.ts`): a fetch-based reader (native `EventSource` can't
  send the bearer header) parsing `text/event-stream` frames with `?since_seq=` resume and
  backoff reconnect.
- **Routes**: real document list at `/` (GET `/documents` + create), `/login`
  (magic-link → verify), the `/documents/[documentId]` editor with in-page mode switching,
  a **Proposals** panel (author accept/reject of pending suggestions), and a **History**
  panel (GET `/events` + a `verify` chain badge). A toast host surfaces op errors.
- **Suggesting interaction**: with the Topbar toggle on, an Inspector field edit POSTs
  `/ops/suggest` (wrapping a `FieldEdited` in `detail`) instead of `/ops/field-edit`, and
  the proposal appears in the queue.

### Verification commands that passed (Node 24, pnpm 11, this machine)

- `pnpm install` — clean.
- `pnpm gen:api` — regenerated `src/lib/api/openapi.d.ts` (committed).
- `pnpm check` — **0 errors, 0 warnings** (330 files).
- `pnpm lint` (prettier + eslint + svelte-check) — clean.
- `pnpm test:unit -- --run` — **30 tests passed** (4 tokens + 17 materializer + 5 adapter
  + 4 editor-view component tests). Note: vitest needs Svelte's `browser` resolve
  condition to `mount()` components in jsdom — set in `vite.config.ts`, scoped to
  `process.env.VITEST` so the production build is unaffected.
- `pnpm build` — succeeds (SSR + client bundles).

> The Playwright E2E (`e2e/smoke.spec.ts`) was updated to match the new home/login shells
> and `pnpm exec playwright test` stays wired, but it is **not required to pass** here (no
> live backend / browsers installed). The two full E2E flows from the doc (login → open →
> edit → reload, and the two-user suggesting flow) need a running stack and are deferred.

### CORS status

No backend change needed. `crates/engine-api/src/lib.rs` already applies a permissive
`CorsLayer` (`allow_origin(Any)`, `allow_methods(Any)`, `allow_headers(Any)`) for the
local-only PoC, so the dev frontend on `http://localhost:5173` / `http://127.0.0.1:5173`
can call the API as-is.

### PoC deviations (deliberate, per the stage brief and `000-initial.md`)

- **In-page mode switching, not `/d/[id]/*` sub-routes.** The doc sketches one route per
  mode; we reuse the working scaffold's `ModeToggle` (Document / Outline / Spec / Preview
  switch in one `/documents/[documentId]` page). Mode switches are state-only and focus
  the main region for keyboard users. Proposals + history are in-page panels, not separate
  routes.
- **API client lives in `frontend/researcher/src/lib/api`, not `frontend/shared`** — only
  one frontend in the PoC (`000-initial.md`).
- **Thin hand-written auth client** (`src/lib/api/auth.ts`) for `/auth/{magic-link,verify,
  refresh}` — these routes predate utoipa and are **not** in `openapi.yaml`, so they can't
  go through the generated client. This is the one documented exception; the rest of the
  surface uses the generated `openapi-fetch` client. Tokens persist in `localStorage` for
  the PoC (a production build would move the refresh token to an httpOnly cookie).
- **Hand-written response types** layered over the generated client, because stage-18
  responses are `serde_json::Value` in `openapi.yaml`. A future stage can annotate the
  handlers' response bodies (utoipa) so responses are typed end-to-end and these
  hand-written shapes can be dropped.

### Deferred (documented, not built)

- **Service worker / offline write queue** — the doc itself says defer if the team is slim.
- **Drag-and-drop reordering in Outline** — selection/navigation only for now.
- **Cross-language materializer fuzz parity** — the doc's watch-out is a fuzz harness that
  runs random event sequences through both the Rust and TS folds and asserts equality. The
  PoC substitute is the per-event-type `materializer.test.ts` suite mirroring the Rust
  rules. A true cross-lang parity harness is deferred.
- **Full Playwright E2E against a live stack + Lighthouse scoring** — not gated for the PoC.
- **Typed API responses** (utoipa response-body annotations) — see above.

## Stage-21 verification status (referential integrity & semantic merge — engine core)

The engine-core half of stage 21 is implemented in `crates/engine-core/`. The API
`/conflicts` + `/resolve` endpoints and the frontend ConflictModal wiring were a separate
follow-up (now complete — see "API + frontend half" below).

### What was built

- **Expression parser** (`src/expr.rs`): a hand-rolled recursive-descent parser for the
  ODK/XLSForm subset (`${var}`, comparison / arithmetic / boolean operators, `.`, string
  and numeric literals, `name(arg, …)` calls). No new crate dependency (no `nom`/`winnow`).
  Public API: `parse() -> Result<Expr, ParseError>`, `referenced_vars()`, and
  `selected_choice_refs()` (extracts `(var, choice_literal)` from `selected()`, both ODK
  arg orders). It does **not** evaluate — only shape + reference extraction.
- **Integrity additions** (`src/integrity.rs`): two new `IntegrityViolation` variants —
  `TypeMismatch` (conservative: a numeric operator over a non-numeric `${var}`; equality
  and unknown types never flag) and `UndefinedChoice` (a `selected(${q},'lit')` whose
  select-type `q` exists but lacks a live choice `'lit'`; a dangling `q` stays a
  `DanglingReference`, never double-reported). The scan now also covers `calculation`.
  Choice-value convention: `current_fields["value"]`, falling back to `["name"]`. One pass,
  all violations, deterministic order. Unparseable expressions degrade gracefully (no panic,
  no invented violation); the existing `${var}` scan still backs dangling/cycle.
- **Three-way merge** (`src/merge.rs`): pure `three_way_merge(ancestor, branch_a,
  branch_b) -> MergeResult { auto_merged_events, pending_conflicts, integrity_warnings }`.
  Per-`(node, field)` classification (`OnlyInA/OnlyInB/BothSame` auto-merge,
  `BothDifferent` → `FourCardConflict`), whole-node adds auto-merge, post-merge
  `validate_integrity` over the proposed state, and `MergeResult::is_blocked()`. The new
  `materializer::apply_payload` helper (shared with the log fold) replays a branch's
  `EventPayload`s onto a cloned ancestor.
- The stage-16 `governance::authorize` deploy gate automatically also blocks on the new
  violation variants (it already runs `validate_integrity` on every `Deployed` event);
  existing governance tests still pass unchanged.

### Verification commands that passed (SAC-free environment, Rust 1.96.0)

- `cargo test -p engine-core` — all green: **67 lib unit tests** (incl. 12 `expr`, 18
  `integrity`, 9 `merge`) plus the unchanged event-log / schema / snapshot integration
  suites. The doc's 5 integrity/merge tests are covered.
- `cargo clippy -p engine-core --all-targets -- -D warnings` — clean.
- `cargo fmt --all -- --check` — clean.

### Structural-merge limitation (PoC, documented)

Field-level edits and whole-node additions present in only one branch merge precisely.
Other structural ops (move / delete / choice add-remove) are carried through additively
from each branch — concurrent *structural* divergence on the same node is not split into a
four-card decision in this PoC. No change is silently dropped; field conflicts surface and
the post-merge integrity pass catches semantic breakage regardless of op type.

### API + frontend half (completed follow-up)

The `/conflicts` + `/resolve` endpoints and the frontend ConflictModal wiring are now
implemented on top of the engine-core half above.

**Conflict source decision (PoC).** Stage 20 (CRDT) is skipped, so there is no branch
source. A conflict is instead **two or more concurrent *pending* suggestions that edit
the same `(node_id, field)` to different values** — the PoC instantiation of the merge
engine's `BothDifferent` classification, generalized to N proposals (the pure
`merge::three_way_merge` remains the 2-branch primitive). The author resolves by accepting
one proposal (the others are rejected as "superseded by conflict resolution") or entering a
custom value (all proposals rejected). Conflicts are **not stored** — they are recomputed
from current state on every `/conflicts` read and `/resolve` call; `conflict_id` is a
deterministic URL-safe-base64 encoding of `(node_id, field)`.

- **Backend** (`crates/engine-api/src/conflicts/`): pure `derive::derive_conflicts` /
  `find_conflict` (unit-tested), `GET /documents/:id/conflicts` `[auth]`, and
  `POST /documents/:id/conflicts/:conflict_id/resolve` `[Author]`. The resolve write
  reuses the `ops::apply` transaction pattern (materialize → `append_in_tx` →
  `post_commit_snapshot` → SSE `publish`). `openapi.yaml` regenerated (no drift);
  `ConflictView` / `ConflictOption` / `ResolveRequest` registered in `ApiDoc`.
- **Frontend** (`frontend/researcher`): typed `listConflicts` / `resolveConflict` client
  helpers + `ConflictView` / `ConflictOption` types; `DocumentStore.conflicts` with
  `loadConflicts()` (on load + refetched when a SuggestionProposed/Accepted/Rejected or
  FieldEdited event arrives over SSE) and `resolveConflict()`; the real four-card
  `ConflictModal` (Original card + one card per proposal + a Compose custom-value field),
  surfaced from the Topbar conflict button (enabled only when `conflicts.length > 0`). The
  adapter's `emptyConflict()` stub was removed.

### Verification commands that passed (API + frontend half)

- `DATABASE_URL=… cargo test -p engine-api` — green: 17 integration tests (5 new conflict
  tests) + lib unit tests (incl. the new `conflicts::derive` tests).
- `cargo clippy --workspace --all-targets -- -D warnings`, `cargo fmt --all -- --check`,
  `cargo run -p engine-cli -- openapi | diff - openapi.yaml` — all clean / no drift.
- Frontend `pnpm check`, `pnpm lint`, `pnpm exec vitest --run` (32 tests, 2 new), `pnpm
  build` — all green.

### Limitation

The modal resolves one conflict at a time (the first in the list) and advances to the next
on apply; with >2 proposals on a field all competing values are shown as cards and the
non-chosen ones are rejected together. Conflicts are refetched on every relevant SSE event
(simple and correct; not incrementally derived client-side).

## Hosted researcher pilot verification — 19 September 2026

This section supersedes historical statements above about browser token storage,
public audit access, missing E2E coverage and the absence of personal drafts.
Sessions now use encrypted HttpOnly cookies. Documents and audit endpoints require
membership. The pilot adds Google login, Resend delivery, document/spreadsheet
editing, file interchange, sharing and personal draft/version workflows.

- Rust 1.96: `cargo test --workspace -- --test-threads=4` — **143 tests passed**,
  using a separate PostgreSQL 16 test instance; no live research database reset.
  The cases include ten parallel researchers editing independent fields, concurrent
  suggestion acceptance, stale overlapping edits, membership isolation, batch
  rollback, a payload above 2 MB, archive restoration, draft conflicts and stable IDs.
- `cargo clippy --workspace --all-targets -- -D warnings` and `cargo fmt --all -- --check`
  passed. The committed OpenAPI schema and generated frontend declarations include
  workspace endpoints.
- Frontend `pnpm exec vitest --run` — **46 tests passed**. `pnpm lint` passed,
  including Svelte checks with **zero errors and zero warnings**.
- `pnpm build` passed with the Node adapter; `VERCEL=1 pnpm build` passed with the
  Vercel adapter. The large-editor bundle warning remains; no Lighthouse performance
  score is claimed.
- `PILOT_E2E=true PLAYWRIGHT_BASE_URL=http://127.0.0.1:4173 pnpm exec playwright test`
  — **11 browser scenarios verified** against a local running API and web server with
  `DEV_AUTH=true`. These cover the homepage, login/session cookies, document edits,
  sharing, versions, draft sharing/restoration, reviewer proposals accepted by a
  second user, spreadsheet formulas and row deletion, Word/XLSX/XLSForm/CSV imports,
  original-file download availability, archive restoration, access/CSRF rejection and
  the ODK/Kobo connection GUI using a provider fixture. One CSV selector was updated
  to distinguish the two import buttons and passed on rerun.
  Local email tests bypass delivery explicitly; they do not validate Google or Resend.
- Connector unit tests cover pagination, token exclusion, nested values, refusal of
  oversized exports and private/cross-host address rejection. An actual HTTPS request
  to Kobo with an intentionally invalid test token returned the expected 401. Real
  study-account verification is pending.
- The live homepage passed a 390 px mobile layout check with reduced motion enabled,
  working navigation and no browser runtime errors. This is not a complete
  accessibility audit.
- On the VPS, a synthetic document/version was created and its event chain verified.
  The synthetic document was archived afterward. Legacy binary DOC conversion
  produced a readable DOCX containing the expected text.
- Final deployment verification also exercised the encrypted-cookie web gateway,
  version creation, archive and chain verification using a synthetic account.
  The deployed connector refused a private server address and reached Kobo's
  expected invalid-token response. The public source download matched the release
  archive's SHA-256 hash, and anonymous document access returned 401.
- A VPS database backup was restored into a separate temporary database. Document,
  event and version counts and event-chain hashes matched the original. Only that
  temporary verification database was dropped. Daily local backups are scheduled;
  off-site backup is still required.

For complete local browser scenarios, run the API and Node web build with matching
`WEB_SERVICE_KEY`, a random `SESSION_SECRET`, `ENGINE_API_URL` pointing at the API,
`DEV_AUTH=true`, and `ORIGIN=http://127.0.0.1:4173`. Wait for both health endpoints
before running Playwright. Tests create synthetic accounts/documents in that local
instance. Use a dedicated database. Ordinary `pnpm test:e2e` runs public-page checks
and explicitly skips authenticated workflows unless `PILOT_E2E=true` is set.

The verification environment used Windows Rust/Docker and Linux Node 24/pnpm 10.34.5.
Frontend builds ran from a temporary Linux checkout to avoid filesystem executable
permission issues on the mounted Windows workspace. Windows workspace dependencies
were reinstalled successfully afterward. DB-backed tests used four workers and
an isolated temporary PostgreSQL instance to avoid disk-related database-creation
timeouts; those test-only durability settings are not used on the VPS.

Outstanding: owner Google/Resend configuration and delivery tests, manual Vercel
publication, study-specific acceptance with the ten real researchers, external
research platform validation, a complete accessibility audit and broader load tests.
See [the current assessment](docs/PROJECT-STATUS.md) for remaining product work.


## Public identity, operations and research connections — 19 September 2026

This verification supersedes the older counts and missing-feature notes above.

- Rust workspace: **149 tests passed** against disposable PostgreSQL 16, including
  admin allowlisting/denial, telemetry validation/deduplication, malformed survey
  expressions and workspace question-type checks. Clippy with warnings denied and
  Rust formatting passed. The final conditional-required field inclusion also
  passed the 72-test core suite.
- Frontend: **57 unit tests passed**, lint passed, Svelte reports zero errors and
  warnings. Node and Vercel builds passed. A large editor chunk warning remains.
- **18 browser scenarios passed** against the isolated local API/web stack.
  The final public/admin changes passed the 10 relevant scenarios again, with
  WCAG A/AA and accessibility best-practice rules enabled. New cases cover actual
  conditional visibility/constraints/calculations, absence of persisted preview
  answers, admin counters/member denial, sitemap/metadata/icons and legal pages.
  These automated rules do not replace keyboard/screen-reader testing with researchers.
- A local desktop Lighthouse run measured Performance **100**, Accessibility **100**,
  Best Practices **100**, LCP **0.6 s**, total blocking time **0 ms**, CLS **0.004**.
  This is one synthetic localhost run, not production field performance or a
  ten-researcher latency guarantee. SEO scored 69 because localhost deliberately
  returns noindex and Disallow; the public production host has different rules.
  The report summary is in `docs/verification/2026-09-19-lighthouse.json`.
- Connections now include REDCap metadata/records, SurveyCTO wide responses and
  SurveyMonkey discovery/bulk responses, alongside ODK/Kobo. Fixture tests verify
  request authentication, mapping, pagination and limits. No real study-account
  acceptance is claimed. See `docs/RESEARCH-CONNECTIONS.md`.
- Survey preview runs the explicitly documented XLSForm subset without eval and
  reports unsupported constructs. Conditional required expressions are preserved
  in new imports and older source columns. Vendor runtime parity remains outside
  this verification.

Local browser tests use synthetic admin `admin@example.test` in `ADMIN_EMAILS`.
The live allowlist uses only the owner's configured address. Development sign-in
is disabled on both public hosts. Google/Resend real sign-in, external study data,
manual accessibility, load acceptance and off-server recovery remain open.

Live release verification also passed for both HTTPS hosts, exact Google callback
routing with S256 PKCE, public provider flags, admin denial for a synthetic member,
anonymous data rejection, eleven applied migrations, a fresh collector/backup
checksum, preserved original files/version/hash chain, and admin certificate renewal
including its Nginx deploy hook. Existing unrelated container IDs were unchanged.
