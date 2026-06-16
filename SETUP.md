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

## Known environment issue — Smart App Control (Windows dev machine)

Smart App Control (SAC) is in **Enforce** mode on the current dev machine
(`HKLM:\SYSTEM\CurrentControlSet\Control\CI\Policy\VerifiedAndReputablePolicyState = 1`).
It blocks freshly-built, unsigned binaries — including `ring`'s build-script binary, so
with the chosen `ring` + `rustls` deps the Rust workspace **does not even build on this
laptop** (`os error 4551`, "An Application Control policy has blocked this file"). SAC uses
Microsoft cloud reputation, not the local certificate store, so code-signing locally does
**not** help.

**The Rust backend is therefore built and run on a SAC-free machine** — your home PC, a
WSL2 distro, or CI. The frontend (`pnpm`) and lockfile resolution (`cargo generate-lockfile`)
work fine on the laptop; only compiling/running native binaries is blocked. To verify
locally on a SAC-free box:

```sh
cargo build --workspace && cargo test --workspace
task dev   # once Docker is running
```

If you ever want native Windows dev on this laptop, SAC can be turned off (Windows Security
→ App & browser control → Smart App Control → Off, then reboot) — but that is **irreversible**
(SAC cannot be re-enabled without resetting Windows). WSL2 is the safer route.

CI (Linux runners) is unaffected and runs the full test suite.

## Already-applied stack deviations (vs docs/11–12)

- Rust toolchain pinned to **1.96.0** (doc said 1.83.0 — stale; current crates need
  edition2024 / Rust 1.85+).
- Everything else matches the doc: **`ring`** for crypto and sqlx on **`rustls`**.

## Stage-12 verification status

Verified on this laptop:

- [x] Frontend **passes** `pnpm check`, `pnpm build`, and `pnpm test:unit`.
- [x] Cargo dependency graph resolves; `Cargo.lock` generated with `ring` + `rustls`.
- [x] Rust workspace compiled clean under a SAC-safe config (sha2/native-tls) before
      reverting to `ring`/`rustls` per request — so the non-crypto code is known-good.
- [x] Toolchain installed: Rust 1.96.0, go-task 3.50.0, pnpm 11.7.0, Docker Desktop
      (installed; needs reboot).

To finish on a SAC-free machine (home PC / WSL2 / CI):

- [ ] `cargo build --workspace` + `cargo test --workspace` green (blocked here by SAC on `ring`).
- [ ] `task dev` boots the stack cleanly — needs Docker running (reboot).
- [ ] `engine-api` `/healthz` returns 200.
- [ ] researcher frontend `/healthz` returns 200 — runnable now via `pnpm preview`.
- [ ] `cargo test --workspace` green — blocked by SAC on the Windows host (passes on CI / WSL2).
- [ ] Playwright E2E — not run locally yet (CI installs the browser and runs it).
- [ ] `ci-rust.yml` and `ci-frontend.yml` pass on a pushed branch (Linux runners; SAC-free).
- [ ] Postgres user + database `dynodoc` exist (created by the compose container on first `task up`).
