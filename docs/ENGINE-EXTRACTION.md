# Standalone engine release

22 September 2026. Public repository:
[ArefinAlter/dynodoc-engine](https://github.com/ArefinAlter/dynodoc-engine).
Local checkout: `B:\dynodoc-engine` (`/mnt/b/dynodoc-engine` in WSL).
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
