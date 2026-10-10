# Engine evolution handoff

## Source increment: bounded checkpoint encoding and desktop distribution (10 October 2026)

Part 2e serializes borrowed checkpoint values into a capped output buffer and
validates canonical bytes by streaming comparison. It removes full-value clones,
intermediate JSON trees and the reader's second encoded buffer without changing
v1 objects, roots, events, API schemas or migrations. The 1 MiB object limit and
remaining operation byte budget apply before output growth. Five new compatibility,
property and boundary tests cover the byte contract. See
[the contract and measured allocation trace](BOUNDED-CHECKPOINT-ENCODING.md).

The application installer and `/connectors` source now distinguish Windows/Mac
desktop from Office web, local files from hosted panes, and test distribution from
publication. Google ZIP packages include the revised instructions. Read-only local
inventory found Office 2021 version 16.0.14326.20348: PowerPoint lacks the required
PowerPointApi 1.4; Word/Excel still need actual runtime acceptance. No host was
sideloaded, account signed in, publisher registered or listing submitted. See
[publication requirements and the host acceptance matrix](ADDIN-PUBLICATION-AND-ACCEPTANCE.md).

Verification: all 238 Rust tests pass in each repository, including five new codec
tests; both lint checks and Rust formatting pass. Frontend units: 282 passed;
separate production build and all 16 public browser checks passed (51 signed-in
cases skipped locally). Connector package consistency passes. Standalone docs build
passes with existing rustdoc link warnings. Mirrored Rust files match byte-for-byte.

The initial `task test` completed Rust and frontend units but stayed in Vitest watch
mode; the test script now uses `vitest run` correctly. A browser build timed out
under competing local checks; a separate build and direct installed browser runner
passed. The global pnpm CI auto-install also stopped on existing dependency-build
policy; no policy was relaxed and its incidental workspace-file additions were
reverted. One standalone run missed the existing 100 ms SSE deadline under load;
the complete quiet 238-test rerun passed, including SSE. Owned preview/watch
processes and the labelled test database/volume are removed; existing local
containers are preserved. Source commit/CI references follow after publication.

This source increment has not been deployed. VPS remains application `ecbc643`,
schema 26, with shared periodic writes disabled. Existing deployment/rollback
evidence in the following section remains current. The new instructions become
hosted only after a future web deployment.

Continue with:

1. Versioned, bounded text/value/asset chunks and reconstruction tests. Then
   packing/incremental publication and shared named-version/draft references.
   Current values above 1 MiB still fail shared encoding; full materialized state,
   property-key metadata and legacy fallback are not newly memory-bounded.
2. Part 3 portable project commits/parents/branches and durable local repositories,
   followed by resumable exchange, format fidelity and offline convergence. Current
   drafts still belong to individual files; a queue is not an offline commit graph.
3. Run the installed-host matrix on compatible Office and real Google add-on hosts
   using test identities. PowerPoint on this workstation needs a supporting host
   version or the existing edited-file fallback. Complete publisher identity/listing
   preparation before any marketplace submission. Capture still needs an open pane;
   loading, pairing and remote sync need connectivity.

## Source increment: Projects front page and member profiles (10 October 2026)

Application `6f1ac21` and engine `8eef6dd` implement the Projects-first interface: minimal
profiles visible only to shared Project members, repository file lists, read-only
file previews, per-file main/private-draft selection, pull requests, history and
recovery, with the existing editors under a separate Web editor tab. Migration
0026 adds private profile metadata and revision checks. Original uploaded files
are limited to 3,000,000 bytes in the browser, API and conversion/import services.
Derived provenance envelopes and expanded archive safety budgets are separate.

Both source commits are pushed to main. All 233 Rust tests pass in each repository;
standalone lint, docs and API generation pass (rustdoc retains existing link
warnings). All 282 frontend unit tests, 16 public and 67 signed-in browser tests
pass, with clean Svelte/ESLint/format/build checks. Profile tests cover absent/shared/
removed membership, disabled accounts, email omission, stale saves and preserved
names after sign-in. Upload tests cover exact 3 MB, excess bytes, request-body
bounds and denied writers without extra stored uploads. Matching Rust/migrations
and API schemas are mirrored into the standalone engine.

The production-mode browser journeys include external edits/review/revocation
and profile setup, multiple files, file moves, draft comparison, merge, historical
native download and selected recovery. Desktop/mobile layouts were inspected and
table contrast fixed. The final read-only Word preview has no editing ribbon;
its build and browser assertion pass. Account erasure clears the bio and invalidates
the profile revision, with a passing regression. Production Nginx configuration
tests pass for Projects, profile and file routes, including the large auth-header
fixture. Source CI: [application Rust](https://github.com/ArefinAlter/dynodoc/actions/runs/38061246753),
[frontend and signed-in stack](https://github.com/ArefinAlter/dynodoc/actions/runs/38061246764),
[standalone engine](https://github.com/ArefinAlter/dynodoc-engine/actions/runs/38061236800).

Local admin failures from starting the API without ADMIN_EMAILS and reusing a
rate-limited synthetic account were resolved in the isolated test environment.
The unchanged 100 ms SSE test also passes in a quiet local run and both source CI
suites. All owned API/web servers are stopped; the labelled disposable database
and its volume are removed. Existing local containers were preserved.
The source is now deployed as application `ecbc643`, schema 26, at 21:22 Asia/Dhaka
(15:22 UTC). The VPS backup was validated before activation; an isolated synthetic
upgrade/compatible-rollback/re-upgrade rehearsal passed, including retained profile
edits and erasure cleanup through the rollback API. Only API/web/admin/converter
were replaced; PostgreSQL, Nginx and all 18 unrelated containers were preserved.
All 17 live workspace and 22 connector checks pass, plus a clean browser check.
Existing document/event counts and their aggregate event fingerprint are unchanged.
No real production sign-in, email or installed-editor acceptance was used.
Project-wide portable branches/commits remain part 3; current drafts belong to one
file. Shared checkpoint writes remain disabled and part 2 storage work is pending.
The schema-25 deployment below is historical. Current release evidence, backup and
compatible rollback instructions are in the application
[deployment record](https://github.com/ArefinAlter/dynodoc/blob/main/docs/operations/2026-10-10-projects-workspace-release.md).
See also [workspace source details](PROJECTS-WORKSPACE-RELEASE.md).

Continue with:

1. Resume part 2 bounded values/assets, compression/incremental publication and
   shared draft/version references. Then implement part 3 portable commits/parents,
   project-wide branches and resumable exchange; retain the fidelity, offline,
   independent audit and measured-scale gates in the evolution plan.
2. Run installed-host acceptance for Word/Docs, Excel/Sheets and PowerPoint/Slides
   alongside independent engine work when host installations/test accounts are
   available. Development packages and browser tests are not host approval.

## Previous deployment handoff: history and editor installations (10 October 2026)

That matching API/web/admin release ran on the VPS as application `536a448`
(implementation `0478bf2`), schema 25, activated at 13:36 UTC. The standalone
engine source remains `a44b8ad`; documentation head `7bfd16c` also passed CI.
This increment changes deployment/operations documentation and adds the app's
read-only `infra/demo/check-connectors.py`; no Rust, event or codec changes.

Validated backup, all applied migration checksums and previous images/source were
retained. A disposable empty database rehearsed the actual previous API at schema
24, new API upgrade to 25 and previous-source-plus-0025 rollback API on schema 25.
The unmodified old API was confirmed to reject the newer migration. All labelled
rehearsal containers/network and their temporary credentials are removed.

Live checks: all 22 delivery/auth checks pass, including exact six-package/icon/
command bytes, pane frame headers, protected return URLs and invalid-key rejection.
A clean browser shows six downloads and approval-to-login without page errors.
Synthetic page shells pass directly and through Nginx with identical preload links.
API/web/admin are healthy. Existing counts/event fingerprint are unchanged;
PostgreSQL, converter and all 18 unrelated containers retain their IDs/images/start
times. Nginx is unchanged. Shared checkpoint writes remain disabled.

Installation page: https://app.dynodoc.online/connectors. Detailed evidence and the
rehearsed rollback command are in the application
[release record](https://github.com/ArefinAlter/dynodoc/blob/main/docs/operations/2026-10-10-history-connector-release.md).
The deployed source archive stays at `536a448`; later handoff/checker commits are
documentation/operations updates and are not rebuilt application images.

Continue with:

1. Installed Word/Docs, Excel/Sheets and PowerPoint/Slides acceptance using test
   accounts/files, including browser launch, consent, two users, push/review/pull
   and revocation. No real production sign-in/email or native-host acceptance was
   performed in this rollout. Marketplace/legal/publisher gates remain open.
2. Independently resume part 2 bounded text/assets, compression/incremental
   publication and shared named-version/draft references, retaining v1 readers
   and old histories. Current full-state memory and legacy bases remain limits.
3. Portable commits/parents/branches, local repositories and resumable exchange,
   then format fidelity, offline convergence, stronger audit and measured scale.

The source verification/feature details below remain applicable. Deployment alone
does not improve algorithmic complexity or certify native-editor fidelity.

## Latest increment: history ranges, native exports and editor sign-in (10 October 2026)

Implemented and pushed to both main branches. The application offers net revision-range
revert/cherry-pick with explicit overlap choices, historical DOCX/XLSX/PPTX downloads,
and browser sign-in/file approval in all six editor clients. Microsoft ribbon commands,
icons and three XML manifests plus three reproducible Google Editor add-on ZIP packages
are available through the application `/connectors` page and Sync & connections.

Final implementation commits:
- Application: `0478bf2fe3e9c4c6e171ec4fb38e61580c788a7a` (feature implementation `4df76ad`).
- Standalone engine: `a44b8adac296aa6414bcafa12c56f3a4bd446e73` (feature implementation `e26d51c`).

No migration, event encoding, hash, storage codec or canonical-history rewrite.
Minimum schema remains 25; shared writes remain opt-in. At source completion there was
no VPS deployment. The then-recorded
production was 8c7461d/schema 24 plus the earlier Nginx repair; it did not have these
new endpoints/assets. This is superseded by the deployment handoff above.
The standalone repository contains the Rust API and contracts; native exporters and
installation/ribbon/sidebar clients live in the application repository.

### Behavior and boundaries

- Restore takes selected blocks from one retained revision. Revert/cherry-pick uses the
  selected net content delta over `(from_seq, through_seq]`, then three-way merges it
  into the current team state. Old endpoints/anchors and explicit conflict choices
  are recorded by the server. Permission, stale-head, retry, dependency and ordinary
  review/approval checks apply. Private drafts do not change canonical content.
- Native downloads reconstruct the requested checkpoint's supported content and include
  document/revision origin metadata. Unsupported original formatting/assets and exact
  original bytes are not reconstructed. No native-host fidelity certification was run.
- Editor sign-in keeps a random 256-bit bearer secret in the requesting client; the
  approval browser sees only its hash and approves a particular file/host. The scoped
  connection cannot merge or manage access. Current membership, account/session state,
  expiry and revocation are rechecked. Links last ten minutes; server validation allows
  one minute for an ahead-running browser clock. Grants last seven days. No account-wide
  token, refresh token or service key is passed to the editor or approval URL.
- Office keys stay in pane memory; Google approved keys use existing private user/file
  properties. Capture is explicitly enabled and still limited to supported paragraph,
  cell/formula/date and shape-text observations while the pane is open. Durable local
  commits, background sync and arbitrary formatting/structural capture remain open.

Exact contracts: [history operations](HISTORY-RECOVERY.md),
[connection and installation](CONNECTOR-PAIRING-AND-INSTALLATION.md).
[Tester installation instructions](https://github.com/ArefinAlter/dynodoc/blob/4df76ad789038dc208cb0ea9a5dc2a4137b28133/extensions/INSTALL.md).

### Verification

Application fmt/Clippy and all 231 Rust tests pass locally. After the final clock fix,
all six connector API tests pass again, including ahead-clock approval, expired and
too-distant link rejection. Standalone task lint, all 231 tests, docs and regenerated
OpenAPI pass on the final source; existing rustdoc warnings remain (16 core / 4 API).
Four changed Rust source/test files and all 22 unchanged SQL migrations match byte for
byte across repositories. OpenAPI schemas match with intended MIT/contact metadata in
the standalone distribution and AGPL metadata in the application.

Frontend lint, zero-diagnostic Svelte check, build and connector artifact consistency
pass. Local unit coverage: full 277-test suite plus three added Google pairing bridge
tests (also reran shared pairing tests). Four real signed-in browser journeys pass:
auth approval after login; DOCX download/recovery/review/merge/range controls; pinned
XLSX export; pinned PPTX export. Seven existing Office/Google connector bridge browser
scenarios pass. All three Office XML manifests pass Microsoft's official validator.
Its minimum manifest version is 1.0.0.0; this does not establish marketplace approval.

Initial feature source CI passed:
- [Application Rust: 231 tests](https://github.com/ArefinAlter/dynodoc/actions/runs/38053822154).
- [Application frontend: 280 unit, 16 public and 66 signed-in browser tests](https://github.com/ArefinAlter/dynodoc/actions/runs/38053822926).
- [Standalone engine: 231 tests, lint/docs/OpenAPI](https://github.com/ArefinAlter/dynodoc-engine/actions/runs/38053895976).

Final clock-only follow-up CI also passed (231 Rust tests per repository,
280 frontend unit tests, 16 public browser tests and 66 tests against the authenticated stack):
[application Rust](https://github.com/ArefinAlter/dynodoc/actions/runs/38054623731),
[frontend/browser](https://github.com/ArefinAlter/dynodoc/actions/runs/38054623648),
[standalone engine](https://github.com/ArefinAlter/dynodoc-engine/actions/runs/38054667212).
The initial range test used wrong merge request field names and was corrected; the
DOCX browser assertion was corrected to read the exporter's existing custom properties.
No exporter fix was needed. No large-file/load benchmark or installed-host acceptance
was performed for this increment; time/space complexity remains unchanged.

### Cleanup and continuation

Both owned disposable containers/volumes (dynodoc-connect-history-20261010 and
dynodoc-pair-clock-20261010) were removed after task-label/loopback-port checks.
Owned API/web processes 14912/9260 were stopped after PID/port verification. Ports
5553, 8087 and 4178 are closed; existing dynodoc-postgres and margin-qdrant are untouched.
Synthetic-only test logs remain in system temp under dynodoc-connect-history-*,
dynodoc-pair-clock-* and dynodoc-*-manifest.log. No production email or document was used.

Next work recorded at source release (deployment is now complete):
1. Deploy the matching API/web build to a pilot environment with backup/schema checks;
   run installed Word/Docs, Excel/Sheets and PowerPoint/Slides acceptance, including
   browser launch, Google consent, ribbon/menu commands, two accounts and revocation.
   Store publisher setup, legal/listing assets, OAuth review and publication remain open.
2. Resume part 2 bounded text/assets, compression/incremental publication and shared
   named-version/draft references while retaining old histories and v1 readers.
3. Add portable commit identities/parents, branch refs, durable local repositories,
   resumable object exchange and checked commit-based revert/cherry-pick. Current
   commands operate on centralized revision ranges; indexed date/author/message history
   discovery and field/run-level selection are also still needed.
4. Continue format/opaque-asset fidelity, offline replica convergence, signed/witnessed
   provenance and measured large-file/concurrent-service acceptance. These are explicit
   remaining product gates, not capabilities inferred from the current tests.

## Historical recovery increment (10 October 2026)

Complete and pushed to both main branches, with local and source CI checks passing.
Following the owner's clarification, historical content retrieval and
recovery are explicit product requirements under current permissions. The product
specification distinguishes historical download, selected restoration, commit
revert and cherry-pick. This increment implements centralized whole-block recovery;
portable commits, revert and cherry-pick remain future work.

Implementation: account-authenticated history-recovery preview/private-draft API,
server-derived source revision/hash, checked current base, idempotent UUID retries,
ordinary review/merge policies, Version history selection/download/draft controls,
Activity entrypoint and source display in review. No migration or canonical event
change. Existing shared checkpoint writers remain opt-in; no VPS deployment.

Application fmt/Clippy and the full 229 Rust tests pass. All five review integration
tests passed again after reserving the server-verified recovery source label (three
are new recovery regressions). Frontend lint, 274 unit tests, production build and
Svelte validation with zero diagnostics pass. The new signed-in browser workflow
passes locally through download, private recovery, submission and actual merge.
OpenAPI and TypeScript definitions are regenerated. Four API source/test files and
all 22 unchanged migrations match the standalone repository byte for byte.

Standalone task lint, all 229 Rust tests, docs and OpenAPI pass; documentation
retains the existing 16 core / 4 API link warnings. Source CI passes all 229 Rust
tests in each repository. Application CI passes 274 frontend unit tests, 16 public
browser tests and all 63 signed-in workflows, including the new recovery journey
and the production proxy regression.
Initial sandboxed frontend check could not spawn esbuild; rerunning with subprocess
permission passed. No source/test failure remains. No native host acceptance or
large-file/performance benchmark was run for this UI/API increment.

The owned loopback PostgreSQL container dynodoc-history-20261010 and its disposable
volume were removed after label/port verification. Existing containers remain untouched.
Owned API/web processes on 8087/4178 have been stopped after the browser check.
Logs are in system temp under dynodoc-history-*; fixtures are entirely synthetic.

Starting main heads: application de49ccf; standalone b05796f. Application source
`d83bf86782380860788a5edadf4bda342008031a` and engine source
`fdba931985cc8aa34f57f535a597842fcb8f6cb1` are pushed to main. This documentation
follow-up records completed checks without changing the tested source.

- [Application Rust](https://github.com/ArefinAlter/dynodoc/actions/runs/38050861763): passed, 229 tests.
- [Application frontend/browser](https://github.com/ArefinAlter/dynodoc/actions/runs/38050861752): passed, 274 unit / 16 public / 63 signed-in tests.
- [Standalone engine](https://github.com/ArefinAlter/dynodoc-engine/actions/runs/38051048267): passed, 229 tests plus lint/docs/OpenAPI.
- Final engine handoff/status: `be355d1bde86740e4babe7d8245e53b2f498c76f`
  ([checkout CI](https://github.com/ArefinAlter/dynodoc-engine/actions/runs/38051507307));
  source is unchanged from the verified implementation commit.

Next: resume part 2 bounded text/assets, compression/incremental construction and
shared version/draft references, followed by local commit exchange. Preserve old
revision recovery while migrating storage. Indexed history discovery, commit
revert/cherry-pick and native historical export/application remain explicit future
gates. No VPS deployment; last recorded production remains 8c7461d/schema 24 plus
the documented Nginx repair. Mixed readers require schema 25. See
[the exact recovery contract](HISTORY-RECOVERY.md).

## Part 2d: concurrent content writes during checkpoints (10 October 2026)

Part 2d is complete, synchronized and pushed to both main branches. Full local
checks and source CI passed in both repositories, including application browser
workflows. Shared writes remain opt-in. No new migration, event/codec/hash bytes,
HTTP schema or VPS deployment; schema 25 remains required for mixed readers.

### Implementation and correctness

Core append and the API begin_write entrypoint take NO KEY UPDATE on the document.
Writers still serialize before reading permissions, state, constraints and the
next event sequence; core no longer upgrades the API lock. Checkpoint KEY SHARE
guards can coexist with those writers while retaining protection against erasure.
Checkpoints select immutable history through pinned sequence N, never mutable node
rows, so later appends form the suffix rather than contaminating checkpoint N.

Stronger SHARE/UPDATE locks remain on the existing authorized export, management,
project and erasure paths. Membership changes through begin_write retain the same
exclusive writer serialization. A queued stronger operation can still delay writers.
Graph verification, object/manifest publication, rollback and erasure remain in
one transaction; no staged objects or new crash-cleanup protocol is introduced.
[The audited lock table and primary PostgreSQL reference](CHECKPOINT-CONCURRENCY.md)
document the precise paths and limitations.

### Verification and measurement

Application fmt/Clippy, all 226 Rust tests and unchanged OpenAPI pass. Tests pause
real explicit-shared, periodic-shared and legacy publishers before commit, then
require ten concurrent core writers to finish while publication stays uncommitted.
Consecutive sequences, chain integrity, old checkpoint content and replay of all
ten later edits are checked. Strong UPDATE locks still fail NOWAIT at the guard.
The real ten-collaborator API test now runs under a checkpoint guard. A new test
observes a real lock wait, commits membership revocation, and verifies that the
queued API writer is forbidden without appending any event. Existing access,
erasure, merge, snapshot, codec/corruption and stale-write suites still pass.

Standalone task lint, all 226 Rust tests, task docs and unchanged OpenAPI pass.
Rustdoc retains 16 core / 4 API pre-existing link warnings. All seven changed Rust
files and all 22 immutable migrations match byte-for-byte. MIT packaging remains
unchanged. Source CI passed all 226 Rust tests in each repository. Application
frontend CI passed 274 unit tests, 16 public browser tests and 62 signed-in workflows,
including the production proxy regression. No real native-host acceptance was run.

Controlled core benchmark: separate fresh databases, identical 8,000 fixed-ID
blocks / 6.4 MB initial state, ten concurrent actor transactions. Prior entry lock
commit times: 1,925-2,035 ms (median 1,987); compatible lock: 38-212 ms (median 129).
Zero old-mode commits versus all ten current-mode commits completed before the
publisher returned. Both roots match, all edits survive and chains verify.
Publication after the barrier release still takes about two seconds. Client peaks
were 104.38/100.59 MiB, excluding PostgreSQL. These one-run traces exclude API
full-state materialization/authorization/validation and native/network work; they
are not service latency percentiles or a large-file/scale acceptance result.
Raw artifacts and reproduction are in [CHECKPOINT-CONCURRENCY](CHECKPOINT-CONCURRENCY.md).
The old database spike example now calls its FOR UPDATE probe strong_lock_wait_ms;
historical artifacts keep their previous append_lock_wait_ms meaning.

### Next concrete work

1. Specify and implement versioned bounded text/asset chunks while retaining v1
   readers and old histories. A single encoded object over 1 MiB is still rejected;
   periodic fallback still stores full JSON. Measure expanded/native-file memory.
2. Evaluate compressed packing/backend options and incremental state construction.
   Full API/checkpoint materialization and CPU/IO scans remain; measure concurrent
   API workloads and longer/edit-dense histories before enabling shared writes.
3. Replace full named-version and draft-base copies with versioned shared references,
   then add durable local commit parents/branches and resumable missing-object
   exchange, with cross-language canonical conformance before portable exchange.
4. Offline convergence, signed/witnessed provenance, structural/formatting/background
   capture in all six native hosts, marketplace and large-service acceptance remain
   later gates. Do not resume Office web-editor parity or the civic roadmap.

### Publication and environment

Starting app c64a9ff; standalone 11399df. Both implementation commits are pushed
to main; this documentation follow-up records completed checks without changing
the tested source.
Owned container dynodoc-evolution-part2d-20261010 and its disposable volume were
removed after label/loopback-port verification. Existing dynodoc-postgres and
margin-qdrant remain running. Logs are in system temp; two synthetic JSON artifacts
are tracked. No local API/web servers started and no VPS deployment. Last recorded
production remains 8c7461d, schema 24, plus the documented Nginx repair.

Publication:

- Application source: `c19dbdab8c4e0e724e6510cf475fa97fcd9a56ac`.
- [Application Rust CI](https://github.com/ArefinAlter/dynodoc/actions/runs/38048638516): passed, 226 tests.
- [Application frontend/browser CI](https://github.com/ArefinAlter/dynodoc/actions/runs/38048638534): passed, 274 unit tests, 16 public browser tests and 62 signed-in workflows.
- Standalone source: `cc12b29e0197304cdfebb86ee1c16d5653b800e4`.
- [Standalone engine CI](https://github.com/ArefinAlter/dynodoc-engine/actions/runs/38048849334): passed, 226 tests.
- Final engine handoff/status: `b05796fb85003ee6f87278853d8d1d57a5bd5d2c`;
  [final checkout CI](https://github.com/ArefinAlter/dynodoc-engine/actions/runs/38049284279)
  passed. Source is unchanged from the verified implementation commit.

## Part 2c: bounded database batches (10 October 2026)

Part 2c is complete, synchronized and pushed to both main branches. Local checks
and both source CI runs passed, including the application browser workflows.
Part 2 remains open and shared writes remain off by default. No new migration,
event encoding or HTTP schema; schema 25 remains required by mixed readers.

- Writes buffer at most 64 objects / 1 MiB encoded bytes, plus the current encoding
  and input/driver state. PostgreSQL checks canonical input and exact stored bytes
  in a bounded VALUES join returning booleans, inserts missing objects in one
  statement, and rechecks conflicts won by concurrent inserts. Unchanged batches
  issue no INSERT; duplicate addresses count as new at most once.
- Index traversal uses breadth-first batches of up to 16 objects, as do value
  reads. Route/depth/duplicate/subtree checks and byte/object budgets remain;
  queued references count against the remaining object budget before expansion.
  Complete state/index/traversal metadata still resides in memory.
- Explicit publishers acquire the same document advisory lock as periodic jobs
  before the row lock, serializing overlapping historical graphs and preventing
  cross-batch lock-order deadlocks. Periodic jobs keep try-lock behavior. Objects
  and manifests remain in one verified transaction with rollback/erasure guarantees.
- The row KEY SHARE lock still blocks FOR UPDATE appends through publication.
  Batching shortens work but does not provide staged/unlocked or incremental
  publication. V1 bytes, roots, document scope and full reconstruction are unchanged.
  ObjectStore now requires Send, which graph operations already required.

### Verification and measured limits

Application fmt/Clippy, all 224 Rust tests and unchanged OpenAPI pass. New database
regressions cover bounded bulk writes, duplicate reuse/scope/capacity/corruption,
2,000-node index batching with byte-triggered flushes, overlapping historical
publishers, and an invalid concurrent INSERT winner checked after a real database
lock wait. Oversized-value fallback now proves rollback after an actual batch has
reached the database. Existing hash vectors and event/graph equivalence tests pass.

Standalone source is synchronized; task lint, all 224 Rust tests, task docs and
unchanged generated OpenAPI pass. The full suite passed on its first run here.
All five changed Rust files and all 22 immutable migration files match byte for
byte. Existing Rustdoc link warnings remain. MIT/OpenAPI packaging is preserved.
Remote source CI passed in both repositories (224 Rust tests each). Application
frontend CI passed 274 unit tests, 16 public browser tests, 62 signed-in workflows
and the production proxy regression. Source/CI records are linked below.

Four separate fresh PostgreSQL databases: 8,000 fixed-ID blocks, initial 6.4 MB
state, two one-block edits per trace; identical 600-byte repeated or varied payload
lengths. Individual/batched paired roots and canonical totals match, reconstructed
states match event application, and event chains verify. Publication fell from
14.6-20.9 s to 2.1-2.9 s; the competing append-lock wait remains 2.1-2.9 s. Object
INSERT statements: 9,817 each checkpoint becomes 154 initially and 4/3 for edits.
Verified graph SELECTs: 9,817 becomes 619. This compares individual SQL and batched
SQL using the current codec, not different release binaries.

Physical shared relations are about 10.9 MB. Three compressed legacy snapshots
use 0.64 MB for repeated content and 20.78 MB for varied content. Canonical bytes
alone do not predict disk savings. Client process peaks were 91.0-104.8 MiB,
excluding PostgreSQL. The four one-run, three-checkpoint traces are not percentile,
ten-collaborator, native 100-500 MB or service-scale acceptance. Exact results,
methodology and four portable raw artifacts are in [SHARED-CHECKPOINTS](SHARED-CHECKPOINTS.md).

### Next concrete work

1. Reduce the publication lock window: audit lock-mode requirements first, then
   design incremental/staged publication where necessary. Preserve append/access
   serialization, pinned sequence/chain validation, complete-root verification, erasure
   isolation and crash/retry cleanup before moving writes outside one transaction.
   Measure hot-document contention; keep the writer opt-in until this passes.
2. Specify bounded text/asset chunks and evaluate compressed packing/backend
   alternatives with edit-density and longer-history traces. Current objects still
   reject a single encoded value over 1 MiB; fallback retains a full JSON checkpoint.
3. Migrate named versions and draft bases to explicit versioned shared references,
   then add parent/root/actor commit envelopes, durable local history and resumable
   missing-object exchange. Cross-language canonical conformance precedes exchange.
4. Offline convergence, signed/witnessed provenance, all-six-host structural and
   formatting/background capture, native large-file and service-load acceptance
   remain later gates. The basic web editor is not the product priority.

### Publication and environment

Starting application: 5ce3566; standalone: 2d93630. Both implementation commits are
pushed to main. This documentation follow-up records verified results without
changing the tested source. The owned local container
dynodoc-evolution-part2c-20261010 and its disposable volume were removed after
validating its ownership label and loopback port 5553. Existing dynodoc-postgres
and margin-qdrant remain running. Logs are in system temp; synthetic JSON artifacts
are tracked. No API/web server or VPS deployment. Last recorded production remains
8c7461d, schema 24, plus the documented Nginx repair.

Publication:

- Application source: `40a80be7e11ae8b10add920c787573fab822d0f9`.
- [Application Rust CI](https://github.com/ArefinAlter/dynodoc/actions/runs/38046287959): passed, 224 tests.
- [Application frontend/browser CI](https://github.com/ArefinAlter/dynodoc/actions/runs/38046287986): passed, 274 unit tests, 16 public browser tests and 62 signed-in workflows.
- Standalone source: `c8ac6bcb108419cf40fb20199c6e3797353f197c`.
- [Standalone engine CI](https://github.com/ArefinAlter/dynodoc-engine/actions/runs/38046494935): passed, 224 tests.
- Final engine handoff/status: `11399dfd5525d86d65dcf57c84b1cb54bdf4c882`
  ([checkout CI](https://github.com/ArefinAlter/dynodoc-engine/actions/runs/38046840829)); source is unchanged.

## Part 2b: mixed readers and gated automatic writer (10 October 2026)

Part 2b is complete, synchronized and published to both main branches. Source CI
passed in both repositories, including application browser workflows. This is a
slice of part 2, not product completion.

- Current/history readers select the nearest legacy/shared checkpoint (shared wins
  ties), validate shared graphs and stream only the suffix. Pinned current seq
  remains the SSE resume boundary. API write materialization uses the same reader
  inside existing access checks and locks. Selected corrupt shared data fails.
- Automatic writes default to legacy. API background jobs opt in via
  DYNODOC_PERIODIC_CHECKPOINT_STORAGE=shared-v1; core exposes PeriodicStorage.
  Both formats count toward cadence; per-document advisory locking skips duplicate
  builders. No-op jobs check cadence before locking, then due jobs recheck it.
- Shared capacity errors roll back partial objects at a savepoint before legacy
  fallback at the same seq. Other errors propagate. Named/deployed snapshots,
  snapshot audit IDs and draft/merge bases still contain full JSON.
- PgStore prefetches at most 16 values, scoped by document; ordinary validation and
  budgets remain. Latest-batch cache has a 16 MiB encoded-byte maximum, not a whole
  request memory bound. Index entries/state still reside in memory.
- Explicit operator publication still derives from legacy/event reconstruction,
  independent of shared roots. No new schema/event/HTTP format; schema 25 required.
- Publication holds KEY SHARE on the document, which blocks the API's FOR UPDATE
  appends, and still scans/writes individual objects. Keep writer default off until
  measured publication latency and concurrent acceptance improve.

Verification: all 220 application Rust tests, fmt/Clippy and unchanged OpenAPI
passed. Targeted tests cover every revision, mixed nearest/tie/scope selection,
resume seq, duplicate builders, cadence/rollback, oversized-value partial rollback,
batched duplicates/budgets/scope and corruption. Existing API workflows now run from
shared state for stale edits and version/draft/restore compatibility. Standalone
task lint, all 220 tests and task docs passed (existing Rustdoc link warnings).
Its first full run hit the existing 100 ms SSE deadline once; the full rerun passed
without changing the test. That timing check is not a production latency guarantee.
Standalone generated OpenAPI is unchanged, including MIT metadata. Both copies of
all seven changed Rust files and all 22 immutable migrations match exactly.
Counterpart source commits and successful CI runs are linked below.

PostgreSQL spike: 8,000 blocks / six checkpoints, all reconstruction/chain checks
passed. Batching lowers graph object SELECTs 9,885 -> 2,385 and median read
6,425 -> 1,831 ms; direct legacy JSONB read is 117 ms. Shared edit publication
15.3-17.4 seconds blocks appends for much of that transaction. Shared relations
11.26 MB versus compressed legacy snapshots 2.92 MB on this repetitive fixture,
despite fewer canonical bytes and edit WAL. Observed client peak 101.1 MiB.
No production gate passes from these figures; see the raw artifact and limitations
in SHARED-CHECKPOINTS.md. Prior in-memory ratios are not database disk ratios.

Next: optimize/batch publication and shorten its append-blocking transaction;
repeat controlled traces with fixed IDs, varying text entropy/edit density/history
length and compressed backend alternatives;
bounded text/asset chunks; versioned draft/named references; cross-language codec
and parent/root/actor commit envelope; durable local history and resumable exchange.
Offline convergence, independent signatures/witnesses and native/scale acceptance
remain separate gates. The basic web editor is not the product priority.

Temporary DB dynodoc-evolution-part2b-20261010 and its disposable volume were
removed after validating its ownership label and loopback port 5553. Existing
dynodoc-postgres and margin-qdrant services remain running. Logs are in system temp;
the portable synthetic measurement artifact is committed under docs/benchmarks/.
No API/web server or VPS deployment in this increment. Production remains last
recorded 8c7461d, schema 24. Starting app 0718ee6; standalone a70647d.

Publication:
- Application source: `12be6a2d5444b7044b06749ca9d2b3fafea815ec`.
- [Application Rust CI](https://github.com/ArefinAlter/dynodoc/actions/runs/38043426193): passed.
- [Application frontend/browser CI](https://github.com/ArefinAlter/dynodoc/actions/runs/38043426145): passed (62 signed-in, 16 public browser tests and 274 frontend unit tests).
- Standalone source: `970d78ad49c94e63f902ac4f97dcc61cf212db27`.
- [Independent engine CI](https://github.com/ArefinAlter/dynodoc-engine/actions/runs/38043671852): passed.
- Final engine handoff/status commit: `2d9363072788de69a19df38752852a24fe230c09`
  ([checkout CI](https://github.com/ArefinAlter/dynodoc-engine/actions/runs/38044102949)).

## Previous completed increment

Updated 10 October 2026. **Part 2a complete, published and CI-verified. Part 2 remains in progress.** Part 1 remains published. Read [PRODUCT-SPEC](PRODUCT-SPEC.md),
[ENGINE-EVOLUTION-PLAN](ENGINE-EVOLUTION-PLAN.md) and
[SHARED-CHECKPOINTS](SHARED-CHECKPOINTS.md) before continuing.

## Part 2a: shared checkpoint foundation

- Versioned/domain-separated canonical objects cover all `DocumentState` fields,
  including tombstones, comment order, suggestions and removed choices. Logical IDs
  remain separate from addresses. Fixed hash vectors pin the Rust byte codec.
- Stable-key radix maps share unchanged objects across checkpoints and avoid fixed
  array chunk boundaries shifting on insertion. A one-node edit rewrites its value,
  map path and root; creation still scans/hashes the complete state.
- Document-scoped memory and PostgreSQL stores; no cross-document reuse or public
  hash lookup. Migration 0025 adds immutable bytes and checkpoint manifests without
  changing applied migrations or event encodings.
- Transactional opt-in publication derives an exact historical state, verifies full
  graph reconstruction before publishing, rejects mismatches, rolls back failures
  and supports same-revision concurrent retries. Reads reject missing/corrupt roots
  and check their recorded chain position. They still trust the root/history
  association; external attestations are not implemented.
- Operator `shared-snapshot` CLI; audited erasure removes manifests/objects and the
  admin preview/receipt records their counts and bytes. Ordinary snapshots and
  draft/version bases still use full JSON. No production default is switched.

## Verification and measurement

Seven codec/property tests and five isolated database tests cover round trips,
canonical bytes, all state components, reuse/insertion, malformed references/routes,
limits, legacy equivalence, rollback/retry, concurrency, scope and erasure. The
existing admin API erasure test also checks new counts and cleanup.

The first full application run failed the existing 100 ms SSE timing assertion
while a release build was running concurrently. That test was not weakened; the
full suite then passed all **216 tests** without competing work. Application fmt,
Clippy and unchanged generated OpenAPI passed. Standalone task lint and all 216
tests also passed. Standalone task docs passed with existing Rustdoc link warnings
in unchanged files. Standalone generated OpenAPI is also unchanged, including MIT
metadata. Application Rust, frontend/browser and independent engine source CI all
passed; their runs are linked below.

Synthetic release benchmark: 8,000 blocks, 21 checkpoints (20 one-block edits),
135,244,119 bytes as repeated full-state JSON versus 7,831,404 unique object bytes;
4,727-7,284 additional bytes per edit. Initial write 163.55 ms, median full scan/write
168.54 ms, final reconstruction 115.87 ms, observed process peak working set
54.5 MiB. Read the codec document for method and limitations: no PostgreSQL/WAL,
large native files, transfer, concurrency or production backend acceptance.

## Next concrete task: part 2b adoption and bounded content

1. Add a compatibility-preserving default reader/writer selection between legacy
   and shared checkpoints. Compare legacy/shared-plus-tail reconstruction at every
   revision. Migrate draft bases/named versions by explicit versioned references;
   preserve historical readers, erasure and stale-base checks. Do not silently
   delete full snapshots or switch production before rollout evidence.
2. Introduce bounded typed text/format/asset chunks so a single large logical node
   does not exceed the current 1 MiB object cap. Benchmark PostgreSQL/index/TOAST/WAL
   and durable local/remote backends, streaming, cancellation and memory. The memory
   store is a test backend; write/read budgets do not bound the prior materializer.
3. Settle cross-language numeric canonicalization and a separate commit envelope
   binding repository, parents, root, operations/capture capabilities and actor
   evidence. Then implement durable local commits, resumable missing-object exchange
   and atomic branch refs (part 3). Offline convergence/audit/native host acceptance
   remain separate explicit gates, not consequences of checkpoint deduplication.

Current limits: whole-state residency/scans, up to 1 MiB per object, 512 MiB/one
million object operations by default, Rust-specific numeric encoding, no chunked
assets, no GC, no default-path storage savings yet. No complete product or capacity
claim. The radix tree is an implementation choice; comparative prolly-tree results
remain unmeasured. Backup retention still applies after live audited erasure.

## Source, deployment and temporary resources

Working from application `1907eea` and standalone `6e7fdf9`. Both copies must match
for changed Rust sources/tests/examples and migration 0025, while preserving engine
MIT metadata and its standalone rich-text fixture. Published implementation:

- Application: `7a8e28ed1bfdf5828deb341fdf9b533fad9f5469`.
- Standalone engine: `6ccdc2277b187491eabbfa089a416bcdbcdaddac`; final handoff and
  LF-checkout guard: `a70647d553be610f41c005d87ce6de26aeb955dd`.
- [Application Rust CI](https://github.com/ArefinAlter/dynodoc/actions/runs/38040472956): passed.
- [Independent engine CI](https://github.com/ArefinAlter/dynodoc-engine/actions/runs/38040495733): passed.
- [Final engine checkout-guard/handoff CI](https://github.com/ArefinAlter/dynodoc-engine/actions/runs/38041109485): passed at `a70647d`.
- [Frontend/authenticated browser CI](https://github.com/ArefinAlter/dynodoc/actions/runs/38040472960): passed (62 signed-in and 16 public browser tests, frontend units and the production-proxy regression).

A final portability check found old Windows engine migration checkouts had CRLF
although committed/app/production bytes were LF. The engine now pins SQL checkouts
to LF and restores the committed bytes locally (no migration Git blob was edited).
All 22 migration files match the app. Applying app migrations to a fresh isolated
DB and then validating with engine migrations passed. All 216 engine tests passed
again with canonical checkout bytes. The packaging follow-up is recorded with the
finalized handoff.

No VPS deployment; last recorded production remains `8c7461d`, schema 24, with the
previous Nginx header fix. The owned disposable PostgreSQL container
`dynodoc-evolution-part2a-20261010` and its volumes were removed after verification
and ownership-label checks. No test API/web servers were started. Existing local
services were left running. Temporary logs and synthetic benchmark JSON are under
the system temp directory. The second owned checksum/test container
`dynodoc-checksum-part2a-20261010` and its volumes were also removed after its
checksum validation and 216-test rerun passed. No increment-owned servers remain.

## Earlier completed increment: part 1

The following is retained as the dated Part 1 release record. Its "next task" and
shared-storage limits predate Part 2a; use the continuation above for current work.

## Part 1 completed

- Made existing-editor document repositories the authoritative product. Replaced the
  duplicated combined volume with a current guide and marked legacy questionnaire/
  civic stages as historical. Corrected mathematical independence, complexity,
  shared-storage and audit claims, including their repeated cross-references.
- Defined stable logical identity separately from content addresses and formatting
  runs; initial graphs contain content, while the target avoids redundant full-file
  versions. Defined explicit offline/merge/trust/capacity acceptance boundaries.
- Added core `read_state_at`: nearest checkpoint at/before the exact revision,
  streamed suffix, revision/gap rejection and checkpoint/replay-count evidence.
  Historical batches/draft creation use the same reader as review/provenance.
- Streamed chain verification and made cadence checks query only sequence metadata.
- Bounded browser Myers extra work to input key count plus 1,000,000 work steps and
  trace to 1,000,000 integer cells (about 4 MB, excluding frontier/objects/input).
  Removed repeated frontier copies and quadratic duplicate occurrence lists.
  Exhaustion uses unique anchors/LIS; fewer matches can require additional review.
- Added exact historical replay/checkpoint/scope tests, duplicate-heavy matching
  regressions and uniqueness/crossed-move counterexamples to disjoint-ID independence.
- Synchronized the seven changed engine source/test files byte-for-byte to the MIT
  repo and preserved its packaging metadata. No event/schema/migration changes.

## Verification

| Check | Result |
|---|---|
| Application `cargo test --workspace`, isolated PostgreSQL | 204 passed. After final streaming/cadence changes, all 13 event/snapshot tests passed again. |
| Application Rust formatting/Clippy | Passed, including all targets after final changes. |
| Frontend full unit suite | 274 passed, including document/spreadsheet/presentation import regressions. |
| Changed frontend files: Prettier/ESLint; complete Svelte/type check | Passed; zero Svelte errors/warnings. |
| Standalone engine `task lint`, `task test` | Passed; all 204 tests, including the unchanged 100 ms SSE check. |
| Standalone `task docs` | Passed with previously documented Rustdoc link warnings. |
| Standalone `task openapi` and generated diff | Passed; HTTP schema unchanged. |
| New documentation links, source equality and whitespace checks | Passed. |
| Application remote CI | Rust passed; frontend lint/build, 274 unit tests, 62 signed-in browser tests, 16 public browser tests and proxy regression passed. |
| Engine implementation and final documentation-head CI | Both passed. |

The isolated container `dynodoc-evolution-part1-20261010` and its volumes were removed.
No test servers started; unrelated local containers retained. No VPS deployment,
real native-host acceptance or marketplace publication was performed. Last recorded
production remains app `8c7461d`, schema 24, plus the Nginx repair. Local tests do not
establish hundreds-of-MB or large-service capacity. Remote browser acceptance used an isolated CI stack; it does not certify native
Microsoft/Google installations, marketplace publication or a new VPS deployment.

## Measurements and complexity boundary

Pre-change audit (10 October) release medians: synthetic 8,000-block/6,030,116-byte
JSON parse 58.44 ms; classify 41.36 ms; format patch 0.256 ms; replay 0.099 ms; small
paragraph merge 0.032 ms; 8,000-block fingerprint 8.19 ms. These exclude native
conversion, network, database writes, rendering and concurrent load. They are a
baseline, not a fresh end-to-end benchmark of this increment.

Current reads still hydrate full state; checkpoints and draft bases still duplicate
full JSON. Missing historical checkpoints still require genesis replay. The periodic
threshold is best-effort, not a hard 500-event tail bound. Key matching budgets do
not cover all shape/cell candidate matching or key-byte comparison costs. Signed
heads, shared object storage and a complete local commit DAG remain unimplemented.

## Next concrete task: part 2 shared checkpoints

1. Specify typed/versioned canonical objects, logical identity versus hash address,
   commit manifests, ordering, chunk/reference limits and all committed state.
2. Implement a tenant-scoped object-store boundary and shared checkpoints alongside
   legacy snapshots; no rewriting event hashes or applied migrations.
3. Compare reconstruction with legacy replay at every fixture checkpoint. Verify
   unchanged objects are reused after a one-node edit, publication is recoverable,
   missing/corrupt objects fail explicitly and tenant/erasure boundaries are intact.
4. Measure unique-byte growth and peak memory. Then migrate full draft bases/named
   versions and develop local commits/object negotiation in part 3.

Bounded parsing/object contracts start now; full-load acceptance is part 7. Real-host
experiments may proceed alongside settled contracts. Do not resume Office web-editor
parity or the questionnaire/civic roadmap. Update this handoff and both project
status records at every increment and before context/token exhaustion.

## Source and publication record

Starting application `f054ad1`; starting standalone `67060cf` (service `d6666a2`).
Implementation is pushed to both main branches:

- Application: `667ecb6ecef5b64debcca6817e73b6be807bde37`.
- Standalone engine implementation: `647249e7ed692534f7ab21a1ad522912c7d58385`;
  finalized handoff/status at `6e7fdf91810677b3dc8778b7d9789882125d2bb1`.
- [Application Rust CI](https://github.com/ArefinAlter/dynodoc/actions/runs/38038078571): passed.
- [Frontend/authenticated browser CI](https://github.com/ArefinAlter/dynodoc/actions/runs/38038078512): passed (274 unit, 62 signed-in, 16 public browser tests; proxy regression).
- [Independent engine CI](https://github.com/ArefinAlter/dynodoc-engine/actions/runs/38038142218): passed.
- [Final engine documentation-head CI](https://github.com/ArefinAlter/dynodoc-engine/actions/runs/38038345736): passed.

All listed runs passed. The documentation follow-up records results and synchronization
without changing the verified implementation. There are no remaining part-1 test
failures; the larger unfinished product capabilities are explicitly listed above.
