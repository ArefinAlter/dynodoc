# 000 · Initial Commitments

Per `docs/11-build-index-and-stack.md` ("What you should commit to up front"), these four
answers are recorded before stage 12 and referenced repeatedly. Scope is the **Research IDE
Proof of Concept** — public-consultation layers (stages 22–31), ML services, and the citizen
frontend are out of scope.

## 1. Hosting model
**Local-only for the PoC.** Docker Compose with a single Postgres container; the Rust API
binary and SvelteKit dev server run on the host. No cloud target is committed yet. Production
hosting (managed vs sovereign self-hosted) is deferred until after the PoC proves the engine.

## 2. Initial language for the UX
**English-only.** The PoC targets a small expert research team. The data model keeps `label`
as a `Map<language, string>` so bilingual/Bangla support can be added later without migration,
but the PoC UI ships English-only.

## 3. First real user
**A small (≈2–5 person) internal research team** co-authoring a single questionnaire — the
two-author live-edit scenario in the PoC definition of done. Not offline-first; no ODK Central
integration required for the PoC; single language.

## 4. License
**AGPL v3.** Committed now (changing later is hard), correct for a system that will eventually
run public consultation. See `LICENSE`.

---

## PoC-specific stack deviations from the full design

Recorded here so later stages don't re-litigate them:

- **Docker Compose = Postgres only.** MinIO and Redis omitted. Snapshots stay in Postgres
  JSONB (PoC questionnaires never approach the 100 KB MinIO offload threshold). Idempotency
  keys and rate windows are Postgres-backed.
- **No `services/` (Python/ML).** Deliberation, moderation, summarizer, and the LLM loop are
  out of scope.
- **Frontend = `frontend/researcher` only.** No `frontend/citizen`. The generated OpenAPI
  TypeScript client lives in `frontend/researcher/src/lib/api` rather than a `frontend/shared`
  package (only one frontend consumes it in the PoC).
- **Auth = email + magic link only** (stage 17 simplification). No phone verification,
  behavioral signals, or first-time gentling.
- **CRDT layer skipped** (stage 20). Same-field concurrent edits surface as a four-card
  conflict via the merge engine (stage 21).
- **Task runner = go-task** (`Taskfile.yml`), chosen over GNU Make for Windows-first dev.
- **Naming:** the repo directory and all internal names use `dynodoc` (the design docs say
  `research-ide`; the code uses `dynodoc`).
- **Crypto crate:** RustCrypto `sha2` (and later `ed25519-dalek`) instead of the doc's
  `ring`. Smart App Control (enforce mode) on the Windows dev machine blocks ring's
  freshly-built build-script binary; the pure-Rust crates have no build script and produce
  the same SHA-256 / Ed25519 primitives.
- **Rust toolchain:** pinned to current stable **1.96.0**, not the doc's 1.83.0 (stale —
  current crates require edition2024 / Rust 1.85+).
