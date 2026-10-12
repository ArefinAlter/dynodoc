# Engine evolution handoff

## Source: selected-history CLI synchronization (12 October 2026)

The operator CLI now connects local project history to the schema-36 API through
reviewed push plans, selected reachable-object transfer, authorized reuse of
published bytes, verified fetch and separate receipt-journal remote tracking.
Fast-forward pull advances only an explicitly selected local branch with exact
address/generation CAS; divergent local work remains intact. No command writes an
open/native file. Publication plans disclose private ancestry and recovery sources;
execution cannot add an unreviewed dependency. Retries reuse accepted objects and
commits; stale shared refs, corrupted objects and inconsistent receipt proofs fail
closed. See [the contract and procedure](SELECTED-HISTORY-SYNC.md).

This adds no migration/hash format change (PostgreSQL 36 / local SQLite 2). HTTP
origins, credentials, response sizes, request deadlines and cumulative work are
bounded. Initial authority trust comes from independently supplied configuration;
accepted heads remain in the separate journal. First trust is a checkpoint, not
proof of unseen older receipts. Multi-commit publication can partially succeed;
complete file-state validation/scans remain, and local quotas include partial work.

Code is pushed to main: application `47cb87f`, standalone engine `4ca7f16`.
[Application Rust CI](https://github.com/ArefinAlter/dynodoc/actions/runs/38248717652)
and [standalone engine CI](https://github.com/ArefinAlter/dynodoc-engine/actions/runs/38248983325)
passed on those exact revisions. Each Linux CI run passed all **356 tests** with
no failures, ignored or filtered cases. Both Rust workflows cover formatting,
Clippy, build/test and unchanged OpenAPI consistency; standalone also builds docs.
Both frontend build/test and authenticated browser workflows passed in
[frontend CI](https://github.com/ArefinAlter/dynodoc/actions/runs/38248717732).

The Windows standalone workspace run passed all **355 tests** before the final
up-to-date race regression was added, including the unchanged 100 ms SSE check.
After that final fix, task lint and all **seven CLI tests** passed. Six new sync
scenarios cover restricted-runtime real HTTP with two authors, privacy and private
ancestry disclosure, shared byte reuse, publish-only review, stale/lost-response
retries, fast-forward/local-CAS/divergence behavior, resumable corruption failure,
receipt rollback/fork/gap/key/revocation and a concurrent up-to-date response race.
Task docs passed with existing rustdoc warnings; the actual sync command help
smoke passed. No test threshold or coverage was relaxed. The dedicated PostgreSQL
container was verified by its task label and auto-remove setting, then stopped
and removed; existing development containers were untouched. Test HTTP listeners
and temporary SQLite databases were closed/removed by the tests. Logs remain in
ignored application target/sync-*.log. No real email or production data was used.

Production is still recorded at application `858801c`, schema 29, shared writers
off. No VPS rollout or installed Office/Google acceptance occurred in this increment.
Schema-36 deployment still needs a production-copy rehearsal, compatible rollback
images and operator receipt-key provisioning. Next priorities are project-scoped
device sign-in/token renewal and native key storage, Projects/native branch/review
adoption, incremental storage/GC, richer same-file convergence, independent audit
witnesses/rotation and measured large-file/service acceptance. The owner runs the
prepared installed-host worksheet. The full product roadmap is not complete.

## Previous increment: shared publication, protected refs and receipts (12 October 2026)

Schema 36 adds [shared project history](SHARED-PROJECT-HISTORY.md): explicit
candidate publication, reachable-object downloads, published multi-author ancestry,
fast-forward branch CAS and exact candidate/base reviews. Main applies current
project/file policy floors and independent approvals; contributors can advance
feature branches. Ref generation, signed receipt and audit entry commit atomically.
Receipts bind an operator-installed public authority and use the existing client
pin/journal contract. API signing keys must be explicitly mounted outside the DB;
there is no unsigned fallback or silent replacement of the installed authority.

Published metadata has per-project byte/row/branch quotas. Identical file roots
reuse reachability indexes. Contributors can acknowledge up to 256 objects from
an explicitly selected authorized publication without uploading their bytes again.
Private hashes remain inaccessible; mixed invalid/quota-limited batches roll back
and exact retries are free. Full graph scans, complete commit file manifests and
changed-root index amplification remain. Document erasure removes affected
published envelopes and descendants/reviews while retaining opaque receipt
commitments and generations. Affected heads become unavailable and never fall
back. Account erasure clears private work while retaining shared publications
under current access checks. Reading an older commit does not rewind any branch.

Final code is pushed to main: application `23ed7bf`, standalone engine `5f86db2`
(initial publication/receipt implementation `0d501ca` / `15cc6c4`).
[Application Rust CI](https://github.com/ArefinAlter/dynodoc/actions/runs/38225348149),
[standalone engine CI](https://github.com/ArefinAlter/dynodoc-engine/actions/runs/38225347973)
and [frontend CI](https://github.com/ArefinAlter/dynodoc/actions/runs/38225348138)
all passed, including both frontend build/test and authenticated browser workflows.
Linux ran all **350 tests**, including the unchanged 100 ms SSE deadline. Five new
restricted-runtime scenarios plus the core authority-installation test cover
publication/privacy, cross-author ancestry and content reuse, policy/base/role
checks, concurrent CAS, quota rollback, key/account/document erasure, immutable
boundaries, atomic receipts and independent client pin advancement/rollback.

Local standalone task lint and task docs pass; existing documentation warnings
remain. Contracts/types are regenerated with distinct shared-history operation
names and preserved standalone MIT/contact metadata. Frontend lint/Svelte checks
pass with zero diagnostics. The expanded object-reuse regression passes, including
rejection of private hashes, all-or-nothing quota failure, free retries and the
256-address boundary. Its initial 16 KiB request cap was too small for that boundary;
it is now 20 KiB. There is no migration/hash/commit format change in that follow-up.

The full Windows task test run again failed the existing 100 ms SSE assertion.
A complete run explicitly excluding only that test passed all **349 remaining
tests**, with no other ignored/filtered cases. No threshold or CI coverage was
relaxed; this is not a Windows latency guarantee. The actual operator CLI migrated
an isolated database to schema 36, installed a synthetic public authority and
provisioned a restricted runtime login. A mismatched DER key prevented API startup;
the matching key served health and the final API contract. That hidden test process
was stopped, and the task-owned PostgreSQL container and synthetic key scratch
directory were removed. Existing development containers were untouched. Ignored
verification logs remain under application target/. No real email, production data
or installed Office/Google host was used.

Production remains recorded at application `858801c`, schema 29, shared writers off.
No VPS upgrade was made in this pass. Schema 36 needs matching runtime grants,
receipt-key provisioning, a production-copy rehearsal and schema-compatible
rollback images before deployment. The owner runs installed-editor acceptance
using the prepared worksheet; source/package tests do not certify those hosts.

Next: automatic selected-root push/fetch and remote tracking, with explicit
publication of dependencies; project-scoped device sessions; Projects/native
branch/review adoption. Private/unreachable local inventory must stay private,
and the server-reported authority key must not silently become a trusted pin.
Server key rotation/distribution, witnesses, audited erased-branch reinitialization,
reachability GC/packing, complete same-file offline convergence and realistic
large-document/service acceptance remain open. These APIs do not advance centralized
per-file events or replace web drafts. The full roadmap is not complete.

## Source: private project history transfers (12 October 2026)

Schema 35 adds account-private authenticated object/commit exchange, described in
[Private project transfers](PRIVATE-PROJECT-TRANSFERS.md). Changed typed objects
are shared physically per document; immutable per-account upload acknowledgements
prevent guessed hashes from exposing other drafts. New signed multi-file commits
verify active enrolled keys, dependencies, recovery semantics and complete typed
graphs. Accepted ancestry remains readable after key revocation, subject to current
project/file access. No project branch, team document or review is published.

Persistent account staging budgets cover logical acknowledged bytes and signed
commit/scope metadata: 4 GiB, 1,000,000 acknowledgements and 100,000 commits. Batches
are atomic, retries free, quota failures return 507. Current API limits include
256 historical files per transitive scope and 64 MiB/65,536 graph reads per commit.
Audited document erasure removes affected envelopes and all descendant envelopes,
even after they remove the file; account erasure removes its private staging.
The CLI can inventory/export/import the same bounded base64 object packets.

Final tested code is pushed to main: application `be538a6`, standalone engine
`554afec` (initial implementation `2d5f613` / `1a4e6fb`).
[Application Rust CI](https://github.com/ArefinAlter/dynodoc/actions/runs/38212242810),
[standalone CI](https://github.com/ArefinAlter/dynodoc-engine/actions/runs/38212247413)
and [frontend CI](https://github.com/ArefinAlter/dynodoc/actions/runs/38212242806)
all passed, including both frontend build/test and authenticated browser workflows.
The complete Linux engine suite passed all 344 tests, including the unchanged
100 ms SSE assertion. Five new restricted-runtime tests cover transfer into a
second SQLite repository, privacy, revoked-key history, recovery, current scopes,
quota rollback/concurrent spending, permission changes during a lock wait and
both erasure paths.

Local verification: standalone task lint and task docs pass; documentation retains
its existing warnings. Generated OpenAPI exactly matches the committed Git blob,
including standalone MIT/contact metadata; the initial raw working-file hash check
was sensitive to Windows line endings. Web API types are regenerated, and local
frontend lint/Svelte checks pass with zero diagnostics. The actual CLI packet smoke
reconstructs the same state in another database, inserts zero objects on retry,
rolls back invalid packets and leaves the local branch unchanged.

The full Windows task test run and an isolated recheck both failed the existing
100 ms SSE deadline. A separate full run explicitly excluding only that assertion
passed all 343 remaining tests. Its earlier attempt exposed a test-cleanup race:
SQLite releases its final Windows handle shortly after Pool::close. The test now
uses the existing bounded retry for sharing errors 32/33; other cleanup failures
remain fatal, and the final run passed. This is not a clean local full-suite result
or a certified Windows latency guarantee. No timing threshold or CI coverage was
relaxed. Linux full-suite acceptance is recorded above.

The first frontend CI attempt failed during package-manager setup: floating pnpm
10 resolved to an unavailable tarball. Application `fdb7624` pins pnpm 10.34.6,
whose npm archive was verified with HTTP 200; both subsequent frontend/browser CI
runs passed. Task-owned PostgreSQL and synthetic SQLite scratch files are removed.
No task test servers remain. Ignored logs are retained under application target/.
No installed Office/Google host, real invitation or production data was exercised.

Last recorded VPS release: application `858801c`/schema 29, shared writers off.
No VPS deployment or production migration occurred in this pass. Source schema 35 needs compatible runtime grants
and rollback binaries plus a production-copy rehearsal before rollout. Installed
Office/Google acceptance remains the owner's local worksheet, not a completed test.

Next: shared candidate publication with explicit reader reachability, protected
project refs/reviews and atomic server receipts; selected-root push/fetch and remote
tracking; then Projects/native adoption. Private acknowledgements must never become
shared discovery. Whole-document erasure must cover future accepted history too.
Complete same-file convergence, changed-node publication/packing, trusted witnesses,
large native/service measurements and marketplace acceptance remain open. The full
roadmap is not complete.

## Source: persistent local repository capacity (12 October 2026)

Implementation: application `962c9e1`, standalone engine `5822e57`, both pushed
to main. [Application Rust CI](https://github.com/ArefinAlter/dynodoc/actions/runs/38207702226)
and [standalone CI](https://github.com/ArefinAlter/dynodoc-engine/actions/runs/38207706865)
passed; the full Linux engine run passed all 339 tests including the unchanged
SSE timing assertion. [Frontend CI](https://github.com/ArefinAlter/dynodoc/actions/runs/38207702234)
also passed, including both the build/test job and authenticated browser workflows.

The [two reproducible storage experiments](LOCAL-STORAGE-BUDGETS.md#measured-small-history-growth)
retained 33 revisions of 128/512-paragraph synthetic graphs. Encoded objects plus
commits used 273,586/736,893 bytes versus 1,896,632/7,599,032 bytes for uncompressed
full JSON states. Actual SQLite files used 589,824/1,695,744 bytes. Median debug CLI
state writes were 280/1168 ms; unchanged retries stored zero bytes but took
277/1523 ms. This exposes the remaining full-state scan cost; it does not certify
large native files, peak memory, HTTP transfer or service throughput. Source
revisions, binary digest and every timing are in the reports. Synthetic benchmark
databases and keys were removed after recording the reports.

SQLite schema 2 now enforces [persistent local capacity](LOCAL-STORAGE-BUDGETS.md)
for retained objects, signed commits and branches: 4 GiB of encoded object/commit
payloads, 1,000,000 objects, 100,000 commits and 4096 branches by default. Staged
and unreachable objects count. Exact retries are free, failed batches roll back,
and existing refs remain usable at capacity. Explicit CLI/library limit changes
never prune history and cannot reduce capacity below current usage. Schema 1
upgrades transactionally without changing roots/signatures/refs; above-default
usage is retained. Older schema-1 clients reject schema 2. Physical disk/WAL,
trust-journal budgets, packing and explicit retention/GC remain separate work.

Validation: all 32 application repository tests pass, including five new migration,
quota, rollback, concurrency and missing-accounting regressions. Both Rust Clippy
checks and formatting pass. Rust documentation builds with the existing warnings.
Two complete local Windows runs failed the existing 100 ms SSE assertion; its
isolated run passed. A separate full run explicitly excluding that one timing test
passed all 338 remaining tests. This is not a clean full-suite local result or a
certified latency guarantee. The check includes the write request, database work
and per-frame authorization, not just broadcast delivery. Its threshold and CI
coverage were left unchanged. Full Linux CI passed, as recorded above.
[Actual CLI acceptance](benchmarks/local-storage-cli-20261012.json)
checks persisted capacity across processes, rejected-write rollback, explicit
increase, zero-cost reuse at capacity and reading the retained root. Logs are
under ignored target/. No native Office/Google host was exercised in this pass.

The preceding device/shared-budget changes are application `a72333a` and engine
`932f48a`: application Rust CI 38204054464, frontend CI 38204054384 (including
authenticated workflows), and standalone CI 38204058559 all passed. Source hashes
and CI for local capacity are recorded above. The prior frontend verification
also passed 289 unit tests, lint, build and three local Projects browser tests.

Application sources are mirrored to D:/dynodoc-engine; MIT metadata is preserved.
Task-owned PostgreSQL and synthetic CLI scratch data are removed; API/web test
servers remain stopped. Only ignored validation logs and the synthetic report remain.
PostgreSQL source stays schema 34. Production remains `858801c`/schema 29, shared
writers off; no deployment or production migration occurred. Before rollout,
prepare a schema-34-compatible rollback image and rehearse with a production copy.
Installed-editor acceptance remains the owner's prepared local worksheet.

Next: authenticated project commit/object discovery and exchange with explicit
private-stage/reachability/whole-document-erasure rules; protected project refs,
review and atomic server receipts; then project-wide UI and native connector
adoption. Local inventory must not become a public remote listing. Recheck current
account/key/project/document rights in the publication transaction, distinguish
already-accepted signed ancestry from new actions, and retain access checks after
revocation/erasure. Complete same-file convergence, incremental native capture,
packing/changed-node publication, trusted witnesses, large-document/service
acceptance and marketplace publication remain open in [the ledger](ROADMAP-ACCEPTANCE.md).
This checkpoint does not complete the product roadmap.

## Source: device identity and retained shared storage (12 October 2026)

Schema 33 adds [account-bound signing keys](DEVICE-SIGNING-KEYS.md): one-use
five-minute account/installation challenges, Ed25519 possession proofs, immutable
bindings, permanent revocation, current-key commit verification, offline CLI proof
signing and own-profile enrollment/revocation. Account removal revokes keys;
restoration cannot revive them. Transactional login rechecks cover the logout race.
Historical signatures remain verifiable after revocation. Signature verification
does not store/publish a commit or confer any project permission.

Schema 34 adds [durable shared-object budgets](SHARED-STORAGE-BUDGETS.md):
1 GiB/250,000 objects per document and 4 GiB/1,000,000 per creator by default.
Transition-table accounting charges only inserted objects, preserves reuse at
capacity, rolls back failed graphs and releases quota through audited erasure.
Counters/limits are read-only to the runtime. Own-profile usage and HTTP 507 errors
make capacity failures visible. This is encoded shared-history accounting, not a
physical disk, event/legacy snapshot, upload or local SQLite quota. The pilot
native-file cap remains 3,000,000 bytes. No shared writer was enabled in production.

Validation: all 334 standalone Rust tests pass, including the schema-33→34
backfill regression, restricted-runtime byte/count quota tests, concurrent account
spending, key enrollment/retry/revocation and logout races. The final targeted
quota run also tests byte and object limits independently. Both Rust Clippy checks
pass. Frontend lint/Svelte checks, 289 unit tests, production build and all three
authenticated Projects browser tests pass with shared draft/named writers enabled
on an isolated schema-34 stack. The browser test caught and verified a fix for the
Svelte-interpolated key-length pattern. The original full engine attempt missed
the existing 100 ms SSE deadline under concurrent linking; its isolated and full
reruns passed unchanged. No performance threshold was relaxed. The CLI smoke
matches the independent Node/OpenSSL vector, rejects wrong scope and preserves
existing output files. Synthetic private key files were removed. OpenAPI and web
types are regenerated; standalone OpenAPI retains its MIT metadata and matches
its generator. Rust documentation builds with 20 existing warnings. CI is pending
the source push.

Application source is mirrored into D:/dynodoc-engine with MIT metadata preserved.
The task-owned API/web acceptance servers are stopped and the labeled disposable
PostgreSQL test container has been removed. Logs live under ignored target/. Production remains 858801c/schema 29. No production migration or deployment
occurred; a schema-34-compatible rollback build and production-copy rehearsal are
still required before rollout. Never start a schema-29 image on schema 34.

Next implementation order: authenticated remote commit/object discovery and
exchange with explicit private-stage/reachability/whole-document-erasure rules;
protected project refs/reviews and atomic server receipts; portable project UI and
connector adoption. Do not expose the local inventory as an HTTP endpoint or treat
a prior verify-commit response as a grant. Recheck the live account/key/permissions
inside publication. Retained ancestry stays readable after signer revocation under
the reader's current access. Physical packing/incremental publication, local-store
retention, complete same-file convergence, background capture, RTF/native fidelity,
large-document/service evidence and marketplace acceptance remain open in
[the ledger](ROADMAP-ACCEPTANCE.md). Installed Office/Google acceptance is prepared
for the owner to run locally. This is a source checkpoint, not roadmap completion.

## Source: portable project revert and cherry-pick (12 October 2026)

Application `7d889ac` and engine `bdde0f3` add core/CLI
[commit-addressed project recovery](PROJECT-COMMIT-RECOVERY.md).
Preview applies a source delta (or its inverse) to an exact current commit; new
signed v2 commits retain source/mainline as provenance and current commit as their
only ancestry parent. Existing v1 hashes/signatures remain unchanged. Root sources
use an empty baseline; merge sources require a chosen parent. Same-file competition
and shared identity/path constraints require explicit resolutions. Import verifies
the sources, recomputes the plan and rejects unrelated settled-path changes.
Publication remains a separate generation-aware branch CAS.

All 25 repository regressions pass, including recovery fifty commits back,
preservation of later independent work, restart/retry, missing/forged dependencies,
mainline selection, conflict validation and independent Python/OpenSSL v2 vectors.
Application Clippy passes. The [actual CLI smoke](benchmarks/project-recovery-cli-20261012.json)
passes preview, revert, exact retry, historical reconstruction, later-file retention,
branch publication, stale-CAS rejection and cherry-pick. Synthetic key files were
removed. All 325 standalone Rust tests pass, along with task lint and task docs
(the same 20 existing rustdoc warnings). Application focused tests, formatting and
Clippy pass. The disposable PostgreSQL container and its volume were removed after
validation; no local API/frontend server was started. Synthetic reports/databases
and logs remain under ignored target. SQLite schema 1 and PostgreSQL schema 32 are
unchanged. Production remains application 858801c/schema 29, with no new writer
enabled. [Application Rust CI](https://github.com/ArefinAlter/dynodoc/actions/runs/38171109983)
and [standalone CI](https://github.com/ArefinAlter/dynodoc-engine/actions/runs/38171113730)
pass all 325 tests plus their lint/build/OpenAPI checks. The application source
packaging regression and standalone documentation check also pass.
[Frontend CI](https://github.com/ArefinAlter/dynodoc/actions/runs/38171109919)
passes both jobs, including frontend checks, unit tests, the shared Google connector
drift check, public browser tests, production-proxy regression and authenticated
browser workflows. These CI browser sessions are not installed Office/Google
acceptance. No deployment occurred.

This completes local commit-addressed recovery, not hosted project branches or
native recovery adoption. Next remains authenticated project transport and device
identity, permission/private-stage/erasure rules, protected reviews and atomic
server receipts. Packing/incremental publication, complete same-file convergence,
native/background capture, representative scale and installed-host/marketplace
acceptance remain open in the [ledger](ROADMAP-ACCEPTANCE.md). Installed-editor
acceptance is prepared for the owner to run locally. Continue across this checkpoint.

## Source: signed ref receipts and durable client trust (12 October 2026)

Application `3218315` and standalone engine `20caed4` add [signed ref receipts](REF-RECEIPTS.md) and a separate SQLite client
trust journal. Scope, generation, commit, predecessor, declared time and authority
key are signed. Contiguous proofs reject rollback, conflicting heads, missing
updates and scope/key substitution. Verified-prefix retries are idempotent.
Authority rotation requires both keys at the exact pin; local revocation persists
across restart and cannot be undone by retries or a replacement bootstrap.
Journal publication is transactional and concurrent conflicting updates have one
winner. The CLI exposes verification, first trust, advance, rotation and revocation.

Validation: workspace check/Clippy passes; all 18 repository tests pass,
including eight new receipt/journal regressions. Independent Python/OpenSSL vectors
pin canonical hashes and receipt/rotation signatures. The local demonstration
creates real synthetic project commits, exercises a separate client journal,
reopens it and verifies fork/rollback/retry/rotation/revocation behavior. See
[its record](benchmarks/receipt-journal-20261012.json). The built CLI successfully
reads the reopened, revoked journal without PostgreSQL or network access.
A follow-up fixes both signing commands to accept standard unencrypted DER
PKCS#8 v1 keys as well as v2, retaining v2 public/private component checks.
The new CLI regression passes against independent OpenSSL signatures; an actual
CLI smoke signs/verifies a v1 key, trusts its receipt, signs/accepts a dual-key
rotation, retries and revokes the journal. Synthetic key files were removed.
Source follow-up: application `07db4bf`, standalone engine `8386adb`.
All 318 Rust tests pass in the standalone local suite and in both
[application CI](https://github.com/ArefinAlter/dynodoc/actions/runs/38169393679)
and [engine CI](https://github.com/ArefinAlter/dynodoc-engine/actions/runs/38169397678).
Formatting/Clippy pass; documentation builds with the same 20 pre-existing warnings.
[Frontend CI](https://github.com/ArefinAlter/dynodoc/actions/runs/38169393675)
passes, including 19 public browser tests and 72 authenticated workflows (the
public job intentionally skips its 53 authenticated cases). The disposable test
PostgreSQL container and its volume have been removed; unrelated containers were
preserved. No local application servers were started. Source is schema 32;
production remains schema 29/application `858801c`. No new production migration,
shared-writer enablement, installed-editor or marketplace acceptance occurred.

These are core/CLI integration primitives. Production does not yet issue signed
ref receipts or distribute authority keys. Account/device registration and remote
revocation, atomic protected-ref publication with receipts, proof endpoints,
witnesses/gossip and connector adoption remain open. A declared time is not
trusted timestamping. Initial trust cannot prove unseen history, and an attacker
who can roll back the client's own trusted journal can destroy its pin.

Next: authenticated remote project history with explicit staged-object visibility,
current project/file permissions, protected refs/reviews, account/device key
binding and audited erasure of multi-file history. Do not expose object inventory
through private drafts or treat a local commit signature as an account grant.
Physical packing/incremental publication, same-file offline convergence, native
format/background capture, realistic large/concurrent workloads and installed
Office/Google acceptance also remain in the [ledger](ROADMAP-ACCEPTANCE.md).
The owner will run installed-editor acceptance locally. Continue across this
checkpoint; the complete roadmap is not certified.

## Source: shared named/deployed snapshots (12 October 2026)

Application `f8f0adb` and standalone engine `107e45e` add [shared named snapshots](SHARED-NAMED-SNAPSHOTS.md),
schema 32. Named versions, public-copy pins and deployed snapshots can refer to
the same typed objects as drafts and periodic checkpoints. The API hydrates the
existing response shape after verifying the stored receipt and referenced graph.
New receipts use a separate v2 hash domain; the legacy vector and stored rows are
unchanged. The new explicit setting is DYNODOC_NAMED_SNAPSHOT_STORAGE=shared-v2;
the default remains legacy and production is still schema 29/application 858801c.

Targeted checks pass: 19 snapshot integration tests and all 45 API integration
tests, including mixed histories/concurrent writers, object reuse, failed-write
rollback, root/object tampering, independent replay/backfill, historical restore,
public-copy privacy and audited erasure using restricted runtime credentials.
Formatting/Clippy and all 309 Rust tests pass in both repositories. Standalone
task docs passes with the same 20 pre-existing warnings. [Application Rust CI](https://github.com/ArefinAlter/dynodoc/actions/runs/38167894976)
and [engine CI](https://github.com/ArefinAlter/dynodoc-engine/actions/runs/38167901880)
pass. [Browser CI](https://github.com/ArefinAlter/dynodoc/actions/runs/38167894975)
passes both its build/unit/public job and all authenticated workflows.

No production migration, optional-writer enablement or hosted package update has
occurred. The task-owned disposable PostgreSQL container
dynodoc-snapshots-test-20261012 and its volume were removed after the remaining
suites completed. Logs and synthetic demo artifacts are under ignored target.
Both repositories carry matching source; preserve standalone metadata/fixtures.

Remaining storage work is physical packing, changed-node publication and realistic
growth/resource acceptance. Remote project history must bind signing keys to
accounts/devices, enforce current file/project membership, preserve private drafts,
serialize protected refs and review, and make whole-document erasure explicit for
multi-file ancestry. Local signed commits alone do not supply these boundaries.
The complete [acceptance ledger](ROADMAP-ACCEPTANCE.md) remains open; installed
Office/Google acceptance is assigned to the owner using the local worksheet.
Continue the roadmap across this checkpoint without requiring another prompt.

## Source: typed state, shared drafts and local project history (12 October 2026)

This pass continues across storage, repository and algorithm boundaries. The
[acceptance ledger](ROADMAP-ACCEPTANCE.md) tracks the whole roadmap without marking
unfinished integration as complete. Application implementation: `8dd816b`;
standalone engine implementation: `f1cdf8c`. Newer entries above record shared
named/deployed snapshots and client audit tools; the evidence below covers these commits.

Implemented:

- [Typed v2 checkpoints](TYPED-CHECKPOINTS.md), chunked text/value/asset references,
  global encoded-object and semantic reconstruction budgets, exact replay comparison,
  mixed historical readers and explicit codec selection. Schema 30 adds capability;
  existing v1 roots/events remain unchanged.
- [Shared draft bases](SHARED-DRAFT-BASES.md) at schema 31: private drafts, pushed
  requests, recovery and rebase bases can store typed roots instead of repeated
  full JSON. Transactions, document-scoped FKs, immutable initial bases and verified
  reconstruction preserve access/review/stale checks and existing API responses.
  DYNODOC_SHARED_DRAFT_BASES=shared-v2 opts in; default remains legacy.
- [Portable local project history](PORTABLE-PROJECT-HISTORY.md): SQLite WAL/FULL
  durability, signed multi-file commits/parents, generation-aware branch CAS,
  bounded/idempotent missing-object transfer, all best merge bases, conservative
  file comparison, and explicit CLI commands. Different paths still conflict when
  they share a document identity or namespace constraint. Signatures do not supply
  account binding, device revocation or independent witnesses.
- Cached iterative parent-depth preparation replaces repeated walks during diff
  sorting. Its ordering step is O(N log N) with O(N) auxiliary metadata. Other
  diff/state costs remain; the regression covers cyclic/missing-parent graphs and
  a 20,000-level chain.

Verification: formatting/Clippy and **303 Rust tests pass in each repository**
against isolated PostgreSQL 16. Standalone task docs passes with the same 20
pre-existing rustdoc warnings. Fixed content/commit hashes and previous v1 event
vectors remain green. [Application Rust CI](https://github.com/ArefinAlter/dynodoc/actions/runs/38166114708)
and [engine CI](https://github.com/ArefinAlter/dynodoc-engine/actions/runs/38166122646)
pass, including OpenAPI compatibility and source packaging where applicable.
[Frontend CI](https://github.com/ArefinAlter/dynodoc/actions/runs/38166114711)
passes 289 units, 19 public browser checks (53 authenticated checks intentionally
skipped in that public-only job), and all 72 authenticated-stack checks. The local
repository CLI reads the demo's common head at generation 2. Standalone package
metadata, licensing, toolchain and independent fixtures remain preserved.

The synthetic two-replica example reopens both databases, exchanges only missing
objects, publishes the same reviewed project merge and recovers the initial
report. Repeating transfer sends zero objects. Initial transfer: 18 objects/2534
encoded bytes; each changed side sends eight objects (~1.2 KiB). See the
[record](benchmarks/local-repository-20261012.json). This is not native-editor,
realistic large-file, HTTP, physical SQLite storage or service-load acceptance.

Production remains application 858801c, schema 29. No live migration, writer
enablement, hosted package/archive update or marketplace submission occurred.
Schemas 30/31 require matching readers and a compatible rollback build; old
schema-29 binaries must not be reused after migration. Shared periodic writes
remain off. The task-owned disposable database dynodoc-roadmap-test-20261012
and its anonymous volume were removed after both suites. Unrelated containers
were preserved. Synthetic demo databases/logs remain under ignored target.
The owner still runs installed
Office/Google acceptance locally using the existing worksheet.

Next integration priorities: shared named/deployed versions, physical packing and
incremental publication; authenticated project transport/protected refs/device
identity and connector/UI adoption of local history. Then complete same-file
offline merge/constraint handling, native graph/fidelity/background capture,
independent receipts/witnesses and realistic large/concurrent service acceptance.
Current hosted drafts/branches remain per file. Do not call the total roadmap
complete or treat the new local library as an installed Office/Google integration.

## Source: streaming shared content and local acceptance kit (12 October 2026)

Application implementation `3462520` adds evolution part 2f. Standalone engine
implementation: `746d373`. Read [the binary contract, resource
bounds and measurements](CHUNKED-CONTENT.md). A separate content hash domain stores
64 KiB pieces under bounded 128-way trees; streaming I/O and object/byte budgets
avoid a full input/output buffer. The document-scoped memory/Pg stores retain
byte validation, reuse, transactional rollback, immutability and audited erasure.
V1 state/event hashes and migrations are unchanged. No root is silently published
as a checkpoint/commit, and no default writer or upload limit changes.

Verification: application formatting, workspace Clippy and all 284 Rust tests pass
against an isolated local PostgreSQL 16. Eight new tests cover independent fixed
hashes, chunk/tree boundaries, fragmented I/O, reuse, scope, repeated-reference
budgets, malformed graphs, I/O errors and Pg rollback/durability. Existing v1
compatibility, history and erasure checks pass. Standalone task lint, all
284 Rust tests and task docs also pass;
rustdoc retains the same 20 pre-existing link warnings. The
[application Rust CI](https://github.com/ArefinAlter/dynodoc/actions/runs/38162089809)
and [engine CI](https://github.com/ArefinAlter/dynodoc-engine/actions/runs/38162187293)
pass, including OpenAPI compatibility and source packaging where applicable.
The [frontend CI](https://github.com/ArefinAlter/dynodoc/actions/runs/38162089764)
passes lint/type/build/package checks, 289 frontend units, 19 public browser
checks, and all 72 tests against the authenticated stack, including the
production-proxy regression. Source inputs are the commits above; later
documentation-only commits record evidence without changing those inputs.
All changed engine sources/fixtures are mirrored; application/engine packaging
and existing independent fixture/toolchain differences are preserved.

The 100/250/500 MiB release probe streams generated repeated bytes to a sink. Peak
working set was 5.95/5.54/5.53 MB and each trace retained five unique objects. This
checks streaming behavior only; it is not native-file, realistic-entropy database,
concurrent-service or user-capacity acceptance. Fixed boundaries can rewrite later
chunks after insertions/deletions; equal-length edits reuse unaffected pieces.

The owner chose to run installed-editor tests locally. The application now includes
the [acceptance worksheet](https://github.com/ArefinAlter/dynodoc/blob/main/extensions/ACCEPTANCE.md)
in each Google test ZIP and links it from Office installation instructions. It
covers all six hosts, exact versions, synthetic inputs, pairing/capture/review/pull,
conflicts, stale application, retries, restart, revocation and historical recovery.
The stale schema-26 installation prerequisite is corrected to 29. Package rebuild,
drift verification and script formatting pass. No native session was connected;
real host results remain not run, and PowerPoint 2021 still fails its API baseline.
No publisher registration, Marketplace submission or public listing is claimed.

VPS remains application `858801c`, schema 29. This increment is source-only; hosted
packages/archive still belong to that deployed release. Shared periodic writes
remain off. The task-owned local PostgreSQL container and its anonymous data
volume were auto-removed after both suites. Docker Desktop was left running
because unrelated containers were active. No local app server was started;
ignored target files retain only synthetic probe/review evidence.

Next concrete dependency: part 2g typed node/value/asset manifests referencing
content roots, with bounded semantic decode (not encoded bytes alone), exact
state/replay equivalence, and explicit versioned publication/read selection. Keep
v1 roots readable and add any schema change as a new migration. Ordinary v1 values
over 1 MiB still reject/fall back; full DocumentState memory is not solved here.
Then packing/incremental publication and shared draft/named-version references.
Portable project-wide commits/parents/atomic refs, durable local history and
resumable exchange follow those contracts; offline convergence, independent signed/
witnessed commitments and native fidelity/large-service acceptance remain open.
Current drafts/branches are per file. This does not complete parts 2-8.

## Source: v1 event hash regression synchronized (11 October 2026)

Engine PR [#7](https://github.com/ArefinAlter/dynodoc-engine/pull/7), source
`e31f093`, merged as `d6be03a`. Application mirror `648aa27` copies the test and
fixture byte-for-byte with contributor attribution. The 22 synthetic events cover
all 15 v1 event types, fixed content/chain hashes, replay state and node Merkle root.
No runtime, event encoding, migration or API behavior changed; no deployment is
needed. The live application remains `858801c`, schema 29, as recorded below.

Local application verification: all six log unit tests, workspace Clippy with
warnings denied, formatting and diff checks pass. Prior independent review of
`e31f093` verified hashes/root and the chain-input-order mutation; its engine CI
passed all 276 Rust tests. Application mirror CI is tracked in
[Rust](https://github.com/ArefinAlter/dynodoc/actions/runs/38160929510) and
[frontend](https://github.com/ArefinAlter/dynodoc/actions/runs/38160929481).
Both application mirror runs completed successfully.

Optional follow-up: pin typed serialization round-trips and per-event validate_op
inside the test; both passed in the separate review probe. This regression does
not add independent audit commitments. Next product dependency remains bounded
shared content and historical references, before portable project commits.

## Live: security and runtime safeguards (11 October 2026)

VPS workspace/API/admin run application `858801c`, schema 29. Engine source is
`80cd148` (workload implementation `f718c11`); application security implementation
is `812f97a`, hydration fixture correction `e22a103`, Office command-frame fix
`858801c`. Read [the release, live evidence and compatible rollback](operations/2026-10-11-security-runtime-release.md).
Documentation follow-ups do not change those deployed images/archive.

Deployed protections include fail-closed service auth, server-side logout,
checked immutable snapshots, separate operator migrations/restricted API login,
project policy floors, audited metadata, bounded REST/SSE row reads, expression
limits, committed-tree source packaging, nonce/hash CSP and HTTPS HSTS. Process-local
response admission, account/project expensive-read limits, database deadlines and
container/proxy resource limits are live. See [the exact bounds and gaps](BROWSER-AND-WORKLOAD-SECURITY.md).
The static Office manifest command frame also has a scoped policy and external
initializer; ordinary pages keep their restrictive script/embedding policy.

[App Rust CI](https://github.com/ArefinAlter/dynodoc/actions/runs/38116813490) and
[engine CI](https://github.com/ArefinAlter/dynodoc-engine/actions/runs/38116978805)
pass all 275 tests each. Deployed Rust inputs are identical to that app run.
[Final frontend CI](https://github.com/ArefinAlter/dynodoc/actions/runs/38118877490)
passes lint/type/build/drift checks, 289 units, 19 public and 72 signed-in browser
workflows, including Nginx regression. Local command-frame/CSP checks pass.
Windows full-run SSE/accessibility timing misses are recorded in the release;
Linux acceptance passes unchanged. Rustdoc passes with 20 existing warnings.

The verified backup/isolated restore, schema 26-to-29 upgrade, actual runtime
privilege denials and compatible API rollback rehearsal pass. Production retains
two documents, 36 events, zero snapshots and identical history fingerprints.
Only API/web/admin restarted; PostgreSQL/converter and unrelated containers were
preserved. Live health/headers, 24 connector delivery/auth checks, committed archive,
large signed-shell headers and anonymous Chromium checks pass. No real user sign-in,
email or document mutation was used. A reload/header-case issue in the deployment
checker was corrected before traffic reopened; no database rollback occurred.

All task-owned local test processes/database and VPS rehearsal containers/volume/
network are removed. Backups and private evidence remain under
`/opt/dynodoc-releases/20261011-security/`. Shared periodic checkpoint writes remain
off. Never use the old schema-26 API as rollback; use the documented schema-29
compatible API with the new web/proxy and preserve current data/credentials.

Next, in dependency order:

1. Part 2: versioned bounded text/value/asset chunks, decode/reference budgets,
   compatibility/reuse fixtures, packing/incremental publication and shared
   draft/version references. Current full-state memory and legacy fallback are
   still unbounded by the new workload admission limits. Keep the writer off
   until its storage/concurrency/peak-memory acceptance passes.
2. Part 3: durable local repositories, portable project-wide commits/parents,
   atomic branch refs and resumable missing-object exchange. Current drafts and
   main selection still belong to individual files.
3. Parts 4–7: native format fidelity, constraint-aware offline convergence,
   independently signed/witnessed audit commitments and measured large-file/
   multi-tenant performance. V1 hashes still omit envelope metadata and cannot
   detect an owner-level complete rewrite without an external commitment.
4. In parallel where contracts are stable: real installed Word/Docs, Excel/Sheets
   and PowerPoint/Slides acceptance and marketplace/publisher setup. Current
   desktop packages need host tests; pane capture needs the pane open, and hosted
   loading/pairing/sync need connectivity. No background/offline completeness or
   hundreds-of-thousands-of-users capacity claim is made.

## Verified source: project policy and audited metadata (11 October 2026)

Project rules now form a minimum for file rules, including old weak overrides.
File ownership/nested-folder management no longer grants project review bypasses.
Rule setters, direct edits, merges/approval overrides, metadata and moves use the
project authority. Source/destination project locks serialize project-root policy
and membership changes against affected writes. Metadata accepts bounded supported
settings patches and records actual changes atomically; settings/archive need governing management and rename follows team-edit
protection. Web import initializes files before attachment to a protected project.
See [the behavior and compatibility contract](PROJECT-POLICY-AND-METADATA.md).

Implementation is pushed to both main branches: application `6a1926f`, browser
regression correction `4dbe3f4`, standalone engine `2100213`. Implementation CI:
[application Rust](https://github.com/ArefinAlter/dynodoc/actions/runs/38112653973) and
[standalone engine](https://github.com/ArefinAlter/dynodoc-engine/actions/runs/38113414584)
pass all 269 Rust tests each. The corrected
[frontend CI](https://github.com/ArefinAlter/dynodoc/actions/runs/38113581840) passes
286 units, 16 public browser tests and all 69 signed-in workflows. Lint, builds,
OpenAPI consistency, source packaging, connector drift and proxy regressions pass.
These are implementation references; later documentation commits record evidence
without changing the verified runtime sources.

Local checks pass: 269 Rust tests in each repo, 286 frontend units, five affected
signed-in workflows, lint, builds, OpenAPI/client generation and standalone rustdoc.
Rustdoc retains 20 existing warnings (16 core, four API). The new project-editor
import/protection test also verifies the 80 KiB metadata request cap. Its initial CI
run attached a file while the upload control was disabled during loading; the trace
shows no import request. An enabled-control wait corrects the test, with three
consecutive local repeats and the full CI run passing, without relaxing assertions
or timeouts.

The first local app Rust run hit the existing 100 ms SSE deadline while frontend
lint ran concurrently; the final isolated run passes without changing the deadline.
The standalone parallel build exhausted Windows paging memory; `CARGO_BUILD_JOBS=1`
completed all remaining checks. Run heavy build/lint tasks separately from browser
and timing-sensitive Rust checks. An erasure assertion now includes the metadata
receipt and verifies that it retains no readable title.

Task-owned API/web servers are stopped, and the labelled disposable PostgreSQL
container and its volume are removed. Unrelated services are unchanged. Ignored
logs are under `target/policy-*`; process records are historical and PIDs may be
reused. No production data or real invitation emails were used.

Schema stays 29; no migration, hash or canonical event changes. The VPS workspace/API
remain recorded at `ecbc643`, schema 26, shared periodic writes off. Rollout still
needs session/snapshot backfill/runtime-role upgrade and compatible rollback rehearsal;
old policy-vulnerable API images are not a safe rollback.

Next security source work: CSP/HSTS compatible with Office panes, then expensive
search/deadline/rate/concurrency budgets. Afterwards resume bounded shared chunks,
shared draft/version references, portable project commits/branches, durable offline
exchange/convergence, independent commitments and installed-editor acceptance.

### Next increment acceptance: browser security and resource budgets

In the application repo, inspect `frontend/researcher/svelte.config.js`, its
`src/hooks.server.ts`, connector routes and `infra/demo/nginx-app.conf` together.
Current connector frame ancestors
do not provide a script CSP. Add nonce/hash-based script restrictions and HTTPS-only
HSTS, preserving scoped Office embedding, sign-in/pairing and document exports.
Exercise ordinary pages and all three hosted Office panes in browser/proxy checks;
keep actual installed Office/Google host acceptance explicitly separate. Verify
redirect/error/static responses as well as successful pages, and avoid duplicate
conflicting policies between the app and proxy. Google Apps Script sidebars run in
Google's host; web checks alone cannot establish acceptance there.

Then bound search and expensive operation admission by request size, time and
concurrency, including per-user/project budgets and database statement deadlines.
REST/SSE row pagination already exists; it does not bound event bytes or aggregate
memory. Preserve authorization, cancellation, retry behavior and bounded error
responses. Only after those source gates, resume evolution part 2 storage work.

## Previous increment: restricted database runtime (11 October 2026)

Migration 0029 and a separate operator CLI now remove migrations from API startup.
The API requires a non-owner login with explicit table/function grants and checks
the complete migration version/checksum list without writing. History UPDATE,
DELETE and TRUNCATE, trigger/schema changes, role elevation and migration writes
are denied. Existing audited whole-resource erasure uses tightly scoped trigger
entrypoints; ordinary application authorization remains required.

Implementation is pushed to both main branches: application `880fc29`, standalone
engine `40bd959`. All implementation CI passes:
[application Rust](https://github.com/ArefinAlter/dynodoc/actions/runs/38095749693),
[frontend and restricted-login browser workflows](https://github.com/ArefinAlter/dynodoc/actions/runs/38095749692),
[standalone engine](https://github.com/ArefinAlter/dynodoc-engine/actions/runs/38096155982).
CI ran all 266 final Rust tests in each repo, 286 frontend units, 16 public browser
tests and 68 signed-in workflows. Lint, builds, source packaging, connector drift,
proxy regression and OpenAPI consistency checks pass. Rustdoc retains 20 existing
warnings (16 core, four API). These references identify implementation; subsequent
documentation commits record the evidence without changing runtime behavior.

Local application lint/test passes (265 Rust tests plus the final disguised-owner
regression, 286 units and 68 browser workflows). Standalone lint, all 266 Rust tests,
rustdoc and OpenAPI checks pass. Fresh final-schema CLI migration/grant/check and
API startup also passed; generated API specifications are unchanged.
CI browser workflows now provision a separate runtime login. Source deployment
configuration and local tasks distinguish runtime credentials from operator-only
credentials. See [the setup, trust and rollout contract](DATABASE-PRIVILEGES.md).
Owned test servers are stopped; the labelled disposable PostgreSQL database and
volume are removed. Unrelated local services were left running.

Local browser fixtures used the initial schema-29 default-function ACL during
development. The final correction revokes global as well as schema-specific PUBLIC
execution; a future-function regression passes, and a fresh database plus final
API binary passed startup separately. No applied fixture checksum was rewritten.
Final CI exercised the final migration and restricted-login bootstrap end to end,
including all 68 signed-in workflows.
Local logs are ignored under `target/runtime-*` in each repo.

The VPS workspace/API are unchanged. Existing production is still recorded as `ecbc643`,
schema 26. Source requires schema 29, verified snapshot backfill and a provisioned
runtime login. A production-copy upgrade and compatible-image rollback rehearsal
remain required; pre-boundary API images are not safe rollback.

Next source priority: project policy authority/floors and audited metadata, then
CSP/HSTS and resource budgets. The product roadmap remains bounded shared chunks,
portable project commits/branches, durable offline repositories/exchange/convergence,
stronger audit commitments, measured scale and native-editor acceptance/distribution.

### Next increment acceptance

Start with `access.rs::policy_with_source`, `Policy::can_edit_team`/`can_merge`,
`people.rs::set_policy`, `workspace.rs::metadata` and project move/settings paths.
The current file override returns before loading project rules, and document
ownership grants the management bypass independently of the person's project role.
Enforce the project floor and authority together, including existing weak overrides,
file creators who are only project editors, direct writes and reviewed merges.
Cover moving files/folders between projects and concurrent policy changes; a
protection-flag OR alone does not close the role bypass. Keep capability/policy-source
responses and the sharing UI consistent with server enforcement.

Inventory settings consumed by current adapters before narrowing metadata writes.
Specify field-level authority and bounded schemas, preserve permitted editor flows,
and record actual before/after changes in the same transaction. Verify denied/no-op
requests and audit rollback, plus a project-editor/file-owner regression. Mirror
the engine changes and regenerate OpenAPI/types only if the public contract changes.

## Latest source work: verified legacy snapshots (11 October 2026)

Security increment implemented: migration 0028 protects legacy snapshots and
adds immutable replay-verification receipts. New snapshots use a canonical checked
writer; current/historical state, public copies, deployed downloads and restore
use checked reads. API/CLI audits compare every legacy snapshot against event
replay. An explicit per-document/all-document backfill verifies old states without
rewriting them. Normal reads retain checkpoint-plus-tail work; fingerprints stream
without another encoded state buffer. See [the contract](VERIFIED-SNAPSHOTS.md).

Application `task lint` passes (Svelte: zero errors/warnings). `task test` passes:
258 Rust tests, 286 frontend units and all 68 browser workflows. The final strict
unknown-field regression and receipt index also pass the 15-test snapshot suite
and final clippy check; there are now 259 distinct Rust tests. The schema-27 upgrade,
backfill atomicity, erasure and historical/public/restore corruption checks pass.
Standalone `task lint`, `task test` (all 259 Rust tests), `task docs` and OpenAPI
generation pass. Rustdoc reports 20 existing warnings (16 core, four API). CLI
backfill verified all 52 synthetic browser-test documents; an independent audit
also passed for a document with a stored snapshot. Owned browser-test servers are
stopped and the labelled disposable database/volume is removed. This is source
verification, not a production release.

Implementation is pushed to both main branches: application `6ccd5f3`, standalone
engine `e8fe900`. All source CI passes:
[application Rust](https://github.com/ArefinAlter/dynodoc/actions/runs/38076177189),
[frontend and signed-in workflows](https://github.com/ArefinAlter/dynodoc/actions/runs/38076177211),
[standalone engine](https://github.com/ArefinAlter/dynodoc-engine/actions/runs/38076557853).
CI ran all 259 final Rust tests in each repo, plus frontend checks, 16 public browser
tests and 68 signed-in workflows. These references track implementation; the later
documentation-only commits record this evidence without changing runtime behavior.

All 16 changed Rust/migration files are mirrored; the standalone's existing Merkle
proof style, test fixture paths and MIT/API package metadata are preserved. Local
logs are ignored under `target/snapshot-*`; CI provides the durable check record.
No VPS workspace/API deployment has occurred; the last recorded VPS is still
`ecbc643`, schema 26, with shared periodic writes off. Upgrade must include both
schema-27 login revocation and schema-28 snapshot verification/backfill; an
unmodified old API is not a compatible rollback.

Next security gate: separate migration-owner and runtime database credentials and
exercise forbidden operations under the real runtime role. Then project-policy
floors/authority, audited metadata, CSP/HSTS and resource budgets. Resume bounded
chunks/assets/shared versions, portable project commits/branches, durable offline
repositories and convergence afterwards. All six native hosts still need actual
installed acceptance; marketplace submission/background capture remain unfinished.

## Current security increment: authentication and bounded history (10 October 2026)

The security findings are being addressed before the next storage increment.
Current changes: fail-closed token issuance; server-side logout with migration 0027;
bounded REST/SSE history reads; expression admission limits and iterative cycle
traversal; committed-tree source archives; Google shared-helper drift checks;
OpenAPI CI and JSON-LD escaping. Application `task lint`, production build and
`task test` pass: 248 Rust tests, 286 frontend unit tests and 68 signed-in browser
tests. The browser regression confirms copied-cookie revocation and preservation
of a separate login. Source packaging and generated connector checks also pass.
Standalone `task lint`, `task test` (248 Rust tests), `task docs` and API generation
also pass; rustdoc retains 16 existing link/HTML warnings. All 17 changed Rust and
migration files match across repos; standalone fixture paths and package metadata
are preserved. Implementations are pushed to main: application `c9d4994`, standalone
engine `2e75859`. [Application Rust CI](https://github.com/ArefinAlter/dynodoc/actions/runs/38070661483)
and [standalone CI](https://github.com/ArefinAlter/dynodoc-engine/actions/runs/38071036482)
passed. [Frontend CI](https://github.com/ArefinAlter/dynodoc/actions/runs/38070661567)
passed build/unit/public-browser checks, but its first authenticated run passed 67
of 68 tests: the existing personal-draft typing test inserted its sentence before
the second paragraph. The new copied-cookie logout test passed. The failed-job
rerun (attempt 2) passed all authenticated workflows; the overall workflow is now
green. Retain the initial failure as an unresolved intermittent editor/test issue.
See [the full findings matrix and rollout contract](SECURITY-HARDENING.md).

The retained browser trace shows cursor movement before the first autosave. Twenty
local diagnostic repetitions passed, including CPU throttling and clicking at the
paragraph start before Control+End. The exact cause is unconfirmed. No editor
change, artificial wait, weakened assertion or retry policy was introduced to mask
the failure. If it recurs, instrument DOM and ProseMirror selection around focus,
Control+End and scheduled layout; do not assume an autosave regression from the
test name alone. Ignored local evidence is under `target/security-ci-artifact/`
and `target/security-cursor-*.json`; CI retains the original failure artifact for
seven days. Preserve it before expiry if follow-up needs the browser trace.

Local verification setup initially hit Windows' lock on the running Cargo output;
running a separate copy of the API executable allowed the complete test rerun to
pass. All subsequent tests used synthetic local identities/documents. Owned API/web
processes and both disposable database instances/volumes used during verification
are removed. Existing `dynodoc-postgres` and `margin-qdrant` containers are preserved.

Highest-priority remaining security work: legacy snapshot verification/immutability,
runtime database privilege separation, project policy floor and audited metadata,
CSP/HSTS and remaining resource budgets. Then resume bounded shared storage,
portable project commits/branches, offline convergence and installed-editor acceptance.
This increment is not deployed to the VPS; the workspace/API remain ecbc643/schema
26. The new API needs schema 27. Rollback must retain login revocation enforcement
or deliberately invalidate credentials; an unmodified old API is not safe rollback.
No real production sign-in, email or document mutation was used.

## Source increment: bounded checkpoint encoding and desktop distribution (10 October 2026)

Implementation commits pushed to main: application `2f69c87`, standalone engine
`3ac2ad6`. CI evidence: [application Rust](https://github.com/ArefinAlter/dynodoc/actions/runs/38066279174),
[frontend and signed-in workflows](https://github.com/ArefinAlter/dynodoc/actions/runs/38066279185),
[standalone engine](https://github.com/ArefinAlter/dynodoc-engine/actions/runs/38066287205).
These links track source verification; a later documentation-only commit records
the references without changing implementation or deployment.

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
containers are preserved. Source commits and CI references are above.

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
