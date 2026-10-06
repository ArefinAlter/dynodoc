# Standalone engine release

22 September 2026. Public repository:
[ArefinAlter/dynodoc-engine](https://github.com/ArefinAlter/dynodoc-engine).
Current local checkout: `D:\dynodoc-engine` (the original release used `B:`).
Initial commit: `6015014`. [v0.1.0 source prerelease](https://github.com/ArefinAlter/dynodoc-engine/releases/tag/v0.1.0): `5cd3a1b`.

The engine was extracted from application commit
`b1fc3b2` without changing event serialization or any migration bytes. Packaging follow-ups
align the generated OpenAPI license with MIT, declare the pinned Rust 1.96 minimum,
use its equivalent integer parity method for Clippy, and run database tests serially
to isolate the existing 100 ms SSE assertion from concurrent database setup. It includes
all four Rust crates because the existing HTTP service/CLI depend on core/shared,
and schema/replay tests need the complete compatible migration history. The API
still includes hosted-product and questionnaire adapters; this is documented in
the new README instead of claiming it is already a completely domain-neutral API.

The Rust golden replay fixture was relocated from the frontend into
`crates/engine-core/tests/fixtures`. The standalone engine therefore has no build
or test dependency on the application checkout. Its manifests point to the new
repository and inherit MIT. The owner expressly authorized this relicensing after
being informed of the original AGPL license. No other code is implicitly relicensed.

The extraction uses a new Git history. NOTICE links the original revision, preserving
origin without copying the full application's history or operational documents.
The existing application's engine source is retained as a compatible copy. For the
next engine fix, implement and validate it in the engine repository, then port the
specific change into the application with its source commit recorded. Applied SQL
migrations must remain byte-identical in both copies. Do not independently reuse the
same migration number for different changes.

A subsequent change should replace the duplicated Rust source with a pinned engine
dependency or subtree and update Taskfile, Docker builds, migrations and CI together.
That build-boundary migration is separate from this publishable, independently
buildable codebase. Do not delete the current crates merely because extraction has
been published.

The repository includes MIT LICENSE, origin NOTICE, README/setup, architecture/API
notes, CONTRIBUTING, CODE_OF_CONDUCT, SECURITY, GOVERNANCE, ROADMAP, CHANGELOG,
CODEOWNERS, issue/PR templates, Dependabot and CI. GitHub recognizes MIT and private
vulnerability reporting is enabled. No crates.io package or binary distribution is
implied by the source release.

## Synchronized governance fix

Engine commit [`02a7aa7`](https://github.com/ArefinAlter/dynodoc-engine/commit/02a7aa7)
adds whole-sheet protection and acceptance revalidation. The application's
`engine-core/src/governance.rs` and `engine-api/src/error.rs` are byte-identical ports
of those files. No migration changes; app licensing and packaging remain unchanged.

Engine commit [`e2a719d`](https://github.com/ArefinAlter/dynodoc-engine/commit/e2a719d)
orders snapshot protection changes around restored/merged content and accepts a
cleared flag from older snapshots. `engine-core/src/governance.rs` and
`engine-core/src/workspace_merge.rs` are byte-identical ports. Regression coverage
applies each diff operation through authorization and replay.

Engine commit [`a22bf71`](https://github.com/ArefinAlter/dynodoc-engine/commit/a22bf71)
adds an immediate SSE comment after subscription authorization, so an idle browser
does not wait for the heartbeat to establish the connection. The comment carries
no event ID and cannot advance replay. `engine-api/src/stream.rs` and
`engine-api/tests/api_test.rs` are byte-identical ports. The existing delivery test
now also checks idle acknowledgement before writing an event.

## Current capability synchronization (6 October 2026)

Engine commit [`c79d940`](https://github.com/ArefinAlter/dynodoc-engine/commit/c79d940)
synchronizes the current application Rust service from `52c2447`, including the
27/30 September change-request and copies/roles releases. It adds:

- Change-request lifecycle, historical starting points, approvals/per-block review
  comments and strict merge choices for unrelated content.
- Owner/Manager/Editor/Reviewer/Contributor/Viewer mapping, protected team versions,
  approval counts, merge restrictions and ownership operations.
- Core deterministic similarity fingerprints and service copy detection, owner
  responses, permission-filtered results and historical-notice suppression.
- Per-person in-app notification APIs and expanded administrative erasure handling.
- Existing regression tests, a similarity benchmark, generated MIT OpenAPI,
  [current capabilities](https://github.com/ArefinAlter/dynodoc-engine/blob/main/docs/CAPABILITIES.md)
  and updated architecture/API/roadmap/changelog documentation.

Only migrations `0020` and `0021` were added. Historical migration bytes in the
engine checkout were preserved; committed blobs match the application. Event
serialization and shared types are unchanged. MIT package metadata, Rust 1.96,
independent CI and the local replay fixture are retained. Rust sources match the
application except the existing Rust 1.96 parity method and fixture-path adaptation;
the golden fixtures match. The application remains AGPL.

Local Windows verification: format/Clippy passed; 194 tests passed against a
disposable PostgreSQL 16 container on loopback port 5549. The unchanged 100 ms
SSE check timed out in both the full run and an isolated retry; it was filtered
only for the remaining local checks. Documentation built with Rustdoc link
warnings, and OpenAPI was regenerated. Full Linux CI passed, including the
unchanged SSE check: [run 37412783835](https://github.com/ArefinAlter/dynodoc-engine/actions/runs/37412783835)
verified formatting, Clippy, build, all 195 tests, documentation and OpenAPI
consistency for `c79d940`. The disposable local database/container was removed.

Projects is still unimplemented. Invitation email (`b70f048`) belongs to the
application web gateway after an engine grant, not to the standalone sender/API.
No production deployment was performed for this synchronization. Replacing the
duplicated Rust sources with a pinned dependency/subtree remains separate work.
