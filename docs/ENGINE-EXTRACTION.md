# Standalone engine release

Latest storage source (12 October 2026): application `3462520`, engine `746d373`
add identical streaming byte-content objects, fixed hash vectors and transactional
store tests. V1 snapshots/events and schema 29 remain compatible; typed chunked
checkpoint integration and project commits are still future work. See
[the content contract](CHUNKED-CONTENT.md) and [verification/handoff](ENGINE-EVOLUTION-HANDOFF.md).

Latest test synchronization: engine PR #7 (`e31f093`, merged `d6be03a`)
is mirrored byte-for-byte in application `648aa27`. The v1 event fixture
and regression test do not change runtime behavior or schema. See
[verification and handoff](ENGINE-EVOLUTION-HANDOFF.md). The runtime
synchronization and release below remain the deployed baseline.

Deployed runtime baseline (11 October 2026): engine `80cd148` and application
`858801c` contain the same security/workload engine behavior and schema 29.
Both pass 275 Rust tests and CI; the application is deployed after upgrade/rollback
rehearsal. See [release evidence](operations/2026-10-11-security-runtime-release.md)
and [the current handoff](ENGINE-EVOLUTION-HANDOFF.md). Intentional differences are
MIT package/OpenAPI metadata, the standalone fixture path and Rust 1.96's equivalent
integer parity method. The earlier extraction records below are historical.

## Shared checkpoint synchronization - 10 October 2026

Part 2a adds the same checkpoint codec, PostgreSQL store, CLI, tests, benchmark and
migration 0025 to the application (`7a8e28e`) and `D:\dynodoc-engine` (`6ccdc22`).
Both passed independent CI. The engine also pins SQL checkouts to LF; all 22
migration files validated against the same isolated database. Nine changed Rust/SQL
files match byte-for-byte; engine MIT metadata and independent fixture paths remain.
See [SHARED-CHECKPOINTS](SHARED-CHECKPOINTS.md) for the bounded opt-in capability and
[evolution handoff](ENGINE-EVOLUTION-HANDOFF.md) for verification, source commits and
CI. Existing default JSON snapshots and event hashes remain compatible. No VPS
rollout is included; production remains schema 24 until a separate rollout.

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

## Earlier capability synchronization (6 October 2026)

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

At this earlier synchronization Projects was unimplemented. Invitation email (`b70f048`) belongs to the
application web gateway after an engine grant, not to the standalone sender/API.
No production deployment was performed for this synchronization. Replacing the
duplicated Rust sources with a pinned dependency/subtree remains separate work.

## Projects and provenance synchronization (6 October 2026)

Engine commit [`e73e4ba`](https://github.com/ArefinAlter/dynodoc-engine/commit/e73e4ba96c088ca08957250b9d29ec14976e5455)
synchronizes application [`2a289ce`](https://github.com/ArefinAlter/dynodoc/commit/2a289ce2fe5fc0071fc45596fdf39b66aca10af7).
It adds invite-only Projects, aggregate files/requests/private own drafts,
inherited review defaults, revision-checked settings and submission Watch.
Portable bundles retain immutable normalized envelopes/digests, authenticated
uploader/reception facts, exact base-chain anchors and atomic proposal/retry receipts.
Seven-day revocable Word/Google Docs credentials authorize read/propose on one
file; current access, account/session generation, expiry and host scope are checked.

Only migrations `0022` and `0023` are new. Historical migrations, event variants,
MIT metadata, Rust 1.96 and the local replay fixture are retained. Synchronized
service sources match the application; standalone OpenAPI is generated with MIT
metadata. No frontend, private files, credentials or production data were copied.
README, architecture/API notes, changelog, roadmap and capability inventory record
the engine's current behavior and limits. Native clients remain application code.

Local Windows checks passed `task lint`, all 198 tests through `task test`,
`task docs` and OpenAPI generation. This run passed the unchanged 100 ms SSE
assertion; earlier runs' intermittent timeout remains recorded above. Documentation
retains existing Rustdoc link warnings. Independent
[Linux CI run 37418637525](https://github.com/ArefinAlter/dynodoc-engine/actions/runs/37418637525)
passed formatting, Clippy, build, all 198 tests, documentation and OpenAPI
consistency for `e73e4ba`.

No production deployment, binary/crates.io release, actual editor-host acceptance
or marketplace publication was performed. Invitation email remains a web-gateway
responsibility. Source deduplication remains a separate build-boundary change.

## Historical checkpoint follow-up (6 October 2026)

Engine [`8e60c59`](https://github.com/ArefinAlter/dynodoc-engine/commit/8e60c59c30446fad1ad54fb839c7345ef3d20e14)
synchronizes application [`a286aee`](https://github.com/ArefinAlter/dynodoc/commit/a286aee276dff03f01c529485566115cc7024658).
Both checkpoint GET endpoints accept optional `through_seq`: exact historical
canonical state/hash with current membership/key/account/session checks. Invalid
or future sequences return 400; omitted returns head. Review statuses stay current.
Native sidecar recovery uses the verified original checkpoint before identity
binding. Its UI/shared clients remain application code.

Service source/test files match; OpenAPI is generated independently with MIT
metadata. No migration or event-format changes. Local engine lint and all four
Projects/provenance integration tests pass, including exact historical hashes/text,
initial state, out-of-range rejection, scope/access and revocation.
[Independent engine CI](https://github.com/ArefinAlter/dynodoc-engine/actions/runs/37424321559)
passed all 199 tests, formatting, Clippy, build, documentation and OpenAPI consistency.
The [application Rust run](https://github.com/ArefinAlter/dynodoc/actions/runs/37424245115)
also passed all 199 tests. Final client/browser evidence is recorded in the
[recovery release](PROVENANCE-RECOVERY-RELEASE.md).

## Six-editor host follow-up (6 October 2026)

Engine service [`d6666a2`](https://github.com/ArefinAlter/dynodoc-engine/commit/d6666a23e83a6b2a31f52d96d0866dda88a46a64)
synchronizes application [`348bef9`](https://github.com/ArefinAlter/dynodoc/commit/348bef9da0be3a72e1f3eeca57225d60aef71420).
Engine main [`4c067f3`](https://github.com/ArefinAlter/dynodoc-engine/commit/4c067f3f45a152e4baffaed15f681f4c09ec3d1d)
adds verified capabilities/priorities. All six editor host keys and file-kind checks
share the anchored retry/review service. Migration 0024 is new; earlier migrations
and event formats remain unchanged. Service/test/migration files match the app.

Local task lint, all 200 tests via task test, task docs and task openapi passed;
generated OpenAPI is unchanged. [Final independent CI](https://github.com/ArefinAlter/dynodoc-engine/actions/runs/37430925686)
passed all required checks. Native adapters/recovery UI are application code;
source publication does not establish complete format support or native acceptance.
The app is deployed on the VPS with Projects/provenance/schema 24; see the
[six-editor release](SIX-EDITOR-PROVENANCE-RELEASE.md) for separate runtime evidence.

## Date-only client follow-up (6 October 2026)

Application `7cc58e3` / `8c7461d` adds checked date-only Excel/Sheets capture and
pull, strict native date-mask/calendar/timezone validation and retained retry/
recovery IDs. This reuses the existing string `FieldEdited` contract. There are
no Rust, migration, event-variant or OpenAPI changes in this increment.

Engine main [`67060cf`](https://github.com/ArefinAlter/dynodoc-engine/commit/67060cf5950ba020d1d0a1665b0b0fcdf24b27a3)
updates README, capabilities, roadmap, contract and changelog. Service source
remains `d6666a2` from application `348bef9`; clients stay application code.
All 21 migration files and the provenance service/test files match byte-for-byte
as committed Git blobs. Windows working-tree line endings differ, so comparison
uses Git blobs rather than rewriting already-applied SQL.

[Final independent CI](https://github.com/ArefinAlter/dynodoc-engine/actions/runs/37439824200)
passes all 200 tests, format, Clippy, build, docs and OpenAPI consistency. Source
publication does not sign off real Microsoft/Google hosts or marketplaces.
[Application release evidence](SPREADSHEET-DATE-SYNC-RELEASE.md) records separate
271-unit/62-signed-in-browser verification and the web/admin-only VPS activation.


## Document repository foundations (10 October 2026)

Application `667ecb6` and engine `647249e` implement evolution part 1: a shared
nearest-checkpoint historical reader with streamed tails and replay accounting,
streamed chain verification, lightweight cadence checks and cross-node constraint
regressions. Seven changed source/test files match byte-for-byte. Existing migration
bytes, event formats and HTTP schemas are unchanged. The sole dependency-lock change
adds the already-pinned workspace futures dependency directly to engine-core.

The application also bounds browser key matching and replaces questionnaire-first
entrypoints with PRODUCT-SPEC, decision 012 and the engine evolution plan. Both
repositories have explicit PROJECT-STATUS and ENGINE-EVOLUTION-HANDOFF documents.
The standalone repository preserves MIT metadata and its local rich-text fixture.

Local verification passed: application 204 Rust tests plus affected tests rerun after
final streaming changes, 274 frontend units, Clippy, ESLint/type checks; engine task
lint, all 204 tests, docs (existing link warnings) and unchanged generated OpenAPI.
The disposable DB was removed. No VPS deployment or native-host acceptance is implied.
Remote CI/source details and the next shared-storage task are in
[the evolution handoff](ENGINE-EVOLUTION-HANDOFF.md).

Final remote verification: application Rust and frontend runs passed, including
274 unit, 62 signed-in browser and 16 public browser tests plus the proxy regression.
Engine implementation CI and the final handoff/status head `6e7fdf9` both passed.
[Application CI](https://github.com/ArefinAlter/dynodoc/actions/runs/38038078512) ?
[Engine CI](https://github.com/ArefinAlter/dynodoc-engine/actions/runs/38038345736).
