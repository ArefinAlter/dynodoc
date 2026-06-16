# 01 · Repository and Development Environment Setup

### Claude Code prompt to start this stage

> I'm building a monorepo for a system called "dynodoc" that combines a Rust backend (Axum + SQLx + Tokio), Python services (FastAPI + scikit-learn + sentence-transformers), and SvelteKit frontends. The full stack is documented in `docs/00-index-and-stack.md`. For this stage, please:
>
> 1. Scaffold the repository structure exactly as specified in the layout in `00-index-and-stack.md`.
> 2. Set up `docker-compose.yml` with services for: PostgreSQL 16, MinIO (S3-compatible), and a small Redis (for ephemeral session/rate-limit state). Use named volumes for persistence.
> 3. Create a Rust workspace `Cargo.toml` with the four crates (`engine-core`, `engine-api`, `engine-cli`, `engine-shared`) and add starter dependencies: `axum`, `tokio` (full features), `sqlx` (postgres, runtime-tokio-rustls, migrate, uuid, chrono, json), `serde`, `serde_json`, `tracing`, `tracing-subscriber`, `ring` (for SHA-256), `ulid`, `thiserror`, `anyhow`. For dev: `tokio-test`, `insta` (snapshot tests).
> 4. Create a Python `pyproject.toml` for each service in `services/` using uv-style layout. Starter deps: `fastapi`, `uvicorn`, `pydantic`, `httpx`, `sqlalchemy[asyncio]`, `asyncpg`, `numpy`, `scikit-learn`, `sentence-transformers`, `pytest`. Use Python 3.12.
> 5. Create the SvelteKit projects in `frontend/researcher` and `frontend/citizen` with TypeScript, ESLint, Prettier, Vitest, Playwright. Use `@sveltejs/adapter-node` (for SSR) and shared package linkage to `frontend/shared`.
> 6. Add a top-level `Makefile` (or `Taskfile.yml`) with targets: `dev`, `test`, `lint`, `fmt`, `migrate`, `clean`. The `dev` target should boot docker-compose, run migrations, then start the Rust API server, the Python services, and both SvelteKit frontends in parallel with concurrent output.
> 7. Add `.editorconfig`, `.gitignore` (with the right Rust/Python/Node ignores), `.gitattributes`, and `LICENSE` (AGPL v3).
> 8. Set up GitHub Actions (or Forgejo Actions) workflows in `.github/workflows/`: `ci-rust.yml` (cargo build, test, clippy, fmt-check), `ci-python.yml` (uv install, pytest, ruff, mypy), `ci-frontend.yml` (npm ci, build, test, playwright).
>
> Don't write any business logic yet. The goal is a working `make dev` that boots everything cleanly, with empty placeholder endpoints (a `/healthz` on each service is enough). All four CI workflows should pass on a clean checkout.

### What this stage accomplishes

A clean, reproducible dev environment for the entire team. After this stage, anyone cloning the repo can run `make dev`, get a working local stack, and see all components responding to a healthcheck.

### Specific decisions to make in this stage

**Rust toolchain pinning.** Use a `rust-toolchain.toml` file pinning to a specific stable version (e.g. 1.83.0). This avoids the "works on my machine" class of issues across the team. Update quarterly.

**Python version and dependency manager.** Python 3.12 with **uv** (not pip, not poetry). uv's lockfile format is fast and deterministic; the install time matters for CI.

**Node version.** Pin to Node 22 LTS via `.nvmrc` and `volta.json`. Use **pnpm** as the package manager (faster, disk-efficient, strict about peer deps).

**Docker Compose conventions.** All services bound to localhost only (no `0.0.0.0` exposure during dev). Postgres on 5432, MinIO on 9000/9001, Redis on 6379, Rust API on 8080, deliberation service on 8081, moderation service on 8082, summarizer on 8083, researcher frontend on 5173, citizen frontend on 5174.

**Environment variables.** A single `.env.example` at the root with all required variables documented (DB connection string, MinIO creds, JWT/Paseto secret, LLM API endpoint, etc.). The real `.env` is gitignored.

**Commit conventions.** Use Conventional Commits (`feat:`, `fix:`, `chore:`, `docs:`, `refactor:`). Set up commitlint to enforce. This pays off later when generating changelogs from the git history.

### Verification checklist before moving to stage 02

- [ ] `git clone && make dev` works on a fresh machine.
- [ ] `curl localhost:8080/healthz`, `localhost:8081/healthz`, `localhost:8082/healthz`, `localhost:8083/healthz` all return 200 OK.
- [ ] Both frontends load at `localhost:5173` and `localhost:5174` and show a "hello world" page.
- [ ] `make test` runs (empty test suites are fine; just verify the runners work).
- [ ] CI passes on a pushed branch.
- [ ] Postgres has the user and database created; MinIO has a bucket created (call it `snapshots`).
