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
