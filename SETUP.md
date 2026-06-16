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

**Docker requires WSL2 — not yet enabled.** Docker Desktop is installed but its WSL2
backend can't start because WSL is not installed (`wsl --status` → "not installed"). To
enable the docker-compose Postgres:

```powershell
wsl --install            # run as Administrator; installs WSL2 + VirtualMachinePlatform
# reboot when prompted
```

After reboot: start Docker Desktop, wait for the whale icon to settle, then `task up`.
Until then, the Rust backend and frontend run fine without Docker (Stage 12 has no DB
calls); a database is first needed in Stage 13.

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
- [x] Toolchain installed: Rust 1.96.0, go-task 3.50.0, pnpm 11.7.0, Docker Desktop.

Pending:

- [ ] `task up` / `task dev` with Postgres — needs WSL2 enabled for Docker (see above).
- [ ] Postgres user + database `dynodoc` exist (created by the compose container on first `task up`).
- [ ] Playwright E2E — not run locally yet (CI installs the browser and runs it).
- [ ] `ci-rust.yml` and `ci-frontend.yml` pass on a pushed branch (Linux runners).
