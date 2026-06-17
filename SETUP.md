# dynodoc — local development setup

The PoC stack: **Rust** engine (Axum + SQLx + Tokio), **PostgreSQL 16** (Docker),
**SvelteKit** researcher frontend, orchestrated by **go-task**. See
`docs/decisions/000-initial.md` for PoC scope.

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
task setup          # copies .env.example -> .env, installs frontend deps
task dev            # boots Postgres, runs migrations (none yet), starts API + frontend
```

Then verify the Stage-12 gate:

```sh
curl http://127.0.0.1:8080/healthz        # -> {"status":"ok","service":"engine-api"}
curl http://127.0.0.1:5173/healthz        # -> {"status":"ok","service":"researcher-frontend"}
# Browser: http://127.0.0.1:5173 shows the dynodoc shell page.
task test           # cargo test --workspace + frontend vitest/playwright
task lint           # clippy + eslint + svelte-check
```

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
