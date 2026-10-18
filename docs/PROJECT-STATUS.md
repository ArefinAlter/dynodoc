# Dynodoc: intent, current implementation and remaining work

## Source: memory-hard key derivation for encrypted key files (18 October 2026)

`dynodoc-key-v1` files written for a passphrase now use Argon2id (RFC 9106, 3 passes, 64 MiB,
4 lanes, version 0x13) instead of PBKDF2. PBKDF2 files stay readable, and key-store-bound files
keep PBKDF2 because their wrapping secret is a random 256-bit value. The Argon2id parameters are
bounded when read (19 to 256 MiB, 2 to 6 passes, 1 to 8 lanes) and authenticated in the
associated data; the PBKDF2 associated data is unchanged, so earlier files and the first vector
still verify. A new `device rewrap` command moves an existing passphrase file to Argon2id without
the key touching the disk unencrypted. One dependency was added (`argon2` 0.5, RustCrypto,
MIT OR Apache-2.0) and the dev profile optimises it, since key derivation is unusably slow
unoptimised.

Evidence: an independent vector produced by the reference Argon2 C implementation and OpenSSL
(`gen_dynodoc_key_v1_argon2id.py`) is decrypted and reproduced byte for byte by the Rust code;
bound, authentication and format tests; the whole engine-cli suite and
Clippy pass. Format, bounds, timings and residual risks: [KEY-FILE-FORMAT.md](KEY-FILE-FORMAT.md).
Not done: hardware-token keys (needs a device and a platform API to test against).

## Source: engine PR integration and dependency migrations (18 October 2026)

Reviewed the thirteen open standalone-engine PRs with four independent read-only reviews, then
merged twelve through one integration PR (merge commits, authors kept): #14, #16, #17 (conflict
handling for parent cycles, blocks placed in deleted containers and clashing variable names),
#29 (names freed in the same change), #30 (merge-law property tests), #38 (restore a moved or
renamed block), #27 (inline atoms in the rich-text merge), #34 (node rows follow the event fold),
#35 (per-link pseudonyms in public copies), #37 (no canonical replay for member settings,
metadata and draft saves), and the two dependency PRs #5 (ulid 3) and #22 (axum 0.8), which
needed code migrations (`Ulid::generate`, brace path parameters, native async extractors).
#13 was closed by its author. **#39 (keep each side's insert runs together) is held**; see below.

Patches added on top of the contributor work, each with a test that fails without it:

| PR | Problem found in review | Fix |
|---|---|---|
| #34 | every op read `current_fields` and created blocks did a lookup; property test still accepted the null-field difference | creates skip the lookup, only field edits read fields; the test accepts only the removed-choice marker; hand test for a removed choice that is edited, deleted and restored |
| #27 | atoms align by position, so a deletion or swap on one side plus an edit on the other moved the edit to a different image | both sides changing the atom at one slot differently is a conflict; a literal U+FFFC is refused |
| #35 | nested editor `blockId`s and an orphan `parent_id` still carried a real ULID into every link; unlabelled HMAC | those ids get the same pseudonyms; the HMAC input is domain-separated |
| #14 | a cycle through a draft-new block could not be repaired with "team" | the repair picks a block the team also has |
| #16 | one scan per orphaned block under the document lock | all blocks in the missing container leave in one pass |
| #17 | typed names unchecked; rejecting a draft-only block orphaned its children | names must be non-empty, trimmed, at most 128 bytes; the draft-only subtree is removed with it |
| #14, #16, #17 | contradictory explicit choices ran to the 2N+1 pass budget under the lock | meeting the same repair twice reports the conflict and stops |
| #30 | random seeds (red runs not reproducible), thin laws, no proof the generator reaches its cases | fixed seed with environment override; identity, label-conflict, determinism and replay laws; coverage floors |
| #38 | a second nameless restore failed; a restore that kept its name came before its holder's deletion | clearing a name is never a clash; such a restore waits for the holder to leave |

Behaviour to know. Public-copy node ids change per link (clients must use ids from the response).
Member settings, metadata and draft saves now succeed when canonical history does not replay;
content reads and review transitions still return `replay_failed`
([HISTORY-RECOVERY.md](HISTORY-RECOVERY.md)). Deleted blocks may be moved and renamed (not edited)
so a restore can bring them back elsewhere.
Siblings with exactly equal position keys may render in a different order in a shared link than
in the owner's view, because a public copy lists blocks by pseudonym and the viewer breaks ties by
list order; the viewer does not sort by id.

**Held: #39.** It flips insert runs that do not interleave, loses deleted or moved siblings as
separators and grows keys on every pull, and its "team run first" rule contradicts the
documented tie-break (order same-position inserts by node id and mark them for review). It needs
a design decision and fixes before it can merge.

**Owner decision still open.** The merge changes (#14 to #17, #21, #27, #38) alter the result of
`merge` for existing inputs, and `verify_same_file_resolutions` re-runs it for strategy
`dynodoc.same-file.node-field.v1`. No v3 commits exist in production, so nothing breaks today.
Before the first v3 commit is created anywhere, either bump the strategy id (the
[node-conflict design](DESIGN-NODE-CONFLICT-RESOLUTION.md) requires a new id for a different
algorithm) or freeze merge behaviour.

Not deployed. Production runs the schema 41 release; none of these changes alter the schema.

## Source: engine PR integration (15 October 2026)

Reviewed all 24 open standalone-engine PRs. Fourteen were merged through one integration PR
([#36](https://github.com/ArefinAlter/dynodoc-engine/pull/36), merge commits, authors kept) after
a green engine CI run on the combination: #12 intra-doc links, #15 selected blocks bring new
parents and required child deletions, #18 node table checked against verified replay, #19
`verify --expect-head`, #20 unused `merge.rs` removed (its integrity cases are tests in
`integrity.rs`), #21 sorted removed choices and size-bounded text diffs, #23 tower-http 0.7, #24
proptest 1.12, #25 anyhow 1.0.104, #26 page setup and image attributes merge key by key, #28
synthetic corpus and benchmarks, #31 `replay_failed` error naming document and event, #32 admin
counters by error code and edit kind, #33 generated-history property test. Independent PRs that
touched neighbouring lines were resolved by hand; both sides of each test conflict were kept.

The application tree carries the same change set (`projection.rs`, `counters.rs`, the benchmark
examples, tower-http 0.7, no `merge.rs`). Mirrored paths are identical apart from the known
engine-only differences: crate and workspace metadata, `Taskfile.yml`, `ci.yml`, the
`is_multiple_of` form in `audit.rs`, and the rich-text fixture path in `richtext_test.rs`.

**Held back.** #13, #14, #16, #17, #27, #29, #30, #34 and #35 need changes or a rebase on the
merged `main` (comments on #14 and #34). #22 (axum 0.8) needs the `async_trait` removal and a
route-syntax migration across about 99 routes. #5 (ulid 3) needs call-site migration.

**Owner decision.** #21 changes the diff spans that `verify_same_file_resolutions` replays for
strategy `dynodoc.same-file.node-field.v1` when a changed span exceeds 4,000 characters. No v3
commits exist in production; decide before any are created whether old spans must keep replaying
(then gate the new bound on a strategy revision).

Not deployed: production still runs `f62d62c` at schema 41; none of these changes alter the
schema.

## Live: schema 41 release (14 October 2026, 08:45 UTC)

The VPS workspace, API and admin site run application `f62d62c`, schema 41: receipt-authority
epochs and rotation (no authority installed), and the in-house spreadsheet formula engine that
replaced `hyperformula`. Rehearsal on a production copy, a verified backup and unchanged
event/snapshot digests preceded activation. Evidence, rollback and an environment note:
[release record](operations/2026-10-14-schema41-release.md).

## Source: formula engine, authority rotation, key files, compression and design suites (14 October 2026)

Source is PostgreSQL schema 41 (migration 0041) and local repository schema 3; schema 41 was
deployed to production the same day (see the entry above).

**Formula engine.** `hyperformula` (GPL-3.0-only or commercial) is removed; spreadsheet
calculation uses an in-house, dependency-free engine in
`frontend/researcher/src/lib/workspace/formula/`. 458 function names, incremental
recalculation, reference rebasing for structural edits, Excel-evaluated test corpus. Evidence,
differences and bundle size (−646 KB): [FORMULA-ENGINE-REPLACEMENT.md](FORMULA-ENGINE-REPLACEMENT.md).
Decision 013's open point is resolved.

**Receipt-authority rotation (schema 41).** [REF-AUTHORITY-ROTATION.md](REF-AUTHORITY-ROTATION.md):
authority epochs, dual-signed rotation statements, a recorded recovery path, revocation
markers, API additions and client rules that refuse a silent break. The authority is still not
installed in production ([decision](RECEIPT-AUTHORITY-DECISION.md)).

**Key files and compression (CLI/local).** `dynodoc-key-v1` passphrase-encrypted key files with
optional Windows DPAPI, macOS Keychain and Linux Secret Service binding
([KEY-FILE-FORMAT.md](KEY-FILE-FORMAT.md)); local repository schema 3 stores objects compressed
(addresses and quotas unchanged), `compact` re-encodes existing objects
([LOCAL-STORAGE-BUDGETS.md](LOCAL-STORAGE-BUDGETS.md),
[DESIGN-PHYSICAL-PACK-FILES.md](DESIGN-PHYSICAL-PACK-FILES.md)).

**Design suites (not implemented).** [EXPANSION-ROADMAP.md](EXPANSION-ROADMAP.md) indexes about
19,500 lines: self-hosted edition, AI-agent connectors (Claude first), end-to-end encryption,
and the remaining engine items (commit pruning, cross-document merge, node-conflict resolution,
witnesses, large-file and installed-host acceptance). Decision 015 records the customer profile
and the copyright line (Verne Ltd.).

**Verification.** Rust fmt and Clippy clean; all tests pass except the Windows-only SSE timing
test. Frontend Prettier, ESLint and svelte-check clean; unit tests pass (461). The isolated
authenticated browser suite result is recorded in the release record below.

**Still open.** Pack files with shared dictionaries and commit pruning, hardware keys
(memory-hard derivation shipped 18 October 2026), cross-document merges and automatic resolution of real node
conflicts, installing the receipt authority and witnesses, large-file and installed-editor
acceptance, and all three planned products above.

## Live: schema 40 release (13 October 2026, 11:36 UTC)

The VPS workspace, API and admin site run application `0f67372`, schema 40: per-project
device-key grants and the review permission, the profile grants editor and starred projects,
and legal pages without placeholders. Rehearsal on a production copy, a verified backup and
unchanged event/snapshot digests preceded activation; the receipt authority stays
uninstalled. Evidence, rollback and the behaviour change for new keys:
[release record](operations/2026-10-13-schema40-release.md).

## Source: licence removal, device-key grants, storage collector and merge apply (13 October 2026)

Source is PostgreSQL schema 40 (migration 0040); it was deployed to production the same
day (see the entry above).

**Licence and copyright.** The AGPL-3.0 text and metadata are removed from the private
repository, the frontend package and the generated API description: `LICENSE` is a proprietary
notice (Verne Ltd.), the workspace sets `license-file` and `publish = false`, and
`package.json` is `UNLICENSED`. The engine repository's copyright holder is Verne Ltd.
([decision 013](decisions/013-private-application-licence.md)). Open point: `hyperformula`
is GPL-3.0-only or commercial and ships in the browser bundle.

**Legal pages.** Undecided facts are omitted instead of shown as placeholders
([LEGAL-PAGES.md](LEGAL-PAGES.md)); the operator facts the owner supplied stay.

**Device-key grants (schema 40).** [Contract](DEVICE-KEY-GRANTS-API.md): new keys default to
"granted" mode and open only projects and scopes their owner grants; existing keys stay
"all" until switched; grant changes apply on the next request. A fourth scope,
`history:review`, is required to approve reviews. The profile UI manages grants and lists
starred projects; `GET /profiles/{id}/starred` is added. The CLI applies a current-user-only
ACL to key and token files on Windows and refuses over-permissive key files on Unix
(`--allow-insecure-key-file` opts out).

**Storage collector and merge apply (local repository).** `gc --plan-hash` removes only staged
objects that no stored commit references, after an exact reviewed plan; commits, signatures
and refs are never pruned ([LOCAL-RETENTION-PLAN.md](LOCAL-RETENTION-PLAN.md)). `merge
--same-file-merge clean` applies node-level merges of a file edited on both sides and writes a
new signed commit format (v3; old readers reject it) with a fixed vector
([PORTABLE-PROJECT-HISTORY.md](PORTABLE-PROJECT-HISTORY.md)). `verify-merge` re-runs it.

**Decision 014.** Browser-authored project-wide branches and a follow graph are not built, with
reasons ([decision](decisions/014-web-branch-authoring-and-follows.md)).

**Not done.** Pack files and commit pruning, encrypted key files and operating-system key
storage, applying merges across documents or resolving real node conflicts automatically,
server-side receipt-authority rotation, witnesses, large-file acceptance, marketplace
publication and installed-editor acceptance.

## Live: schema 39 release (13 October 2026, 03:02 UTC)

The VPS workspace, API and admin site run application `1b469a2`, schema 39: project issues,
labels and stars, the Verne Systems operator details on the legal pages, and `/source` as a
redirect to the public engine repository. Rehearsal on a production copy, a verified backup
and unchanged event/snapshot digests preceded activation. The receipt authority stays
uninstalled (see [the decision](RECEIPT-AUTHORITY-DECISION.md)). Evidence, rollback and
follow-ups: [release record](operations/2026-10-13-schema39-release.md). The entry below
describes this revision as source; it was deployed after CI passed.

## Source: operator details, private application, issues and retention plan (13 October 2026)

Source is PostgreSQL schema 39 (migration 0039); this was deployed to production the same day
(see the entry above).

**Operator and legal.** The public legal, trust, support and brand pages now carry the
owner's operator facts from one module (`frontend/researcher/src/lib/site/operator.ts`):
Verne Systems, trading as Verne; registered address 8/A Sher-e-Bangla Road, Zighatola,
Dhaka-1209, Bangladesh; company registration C-201411-1; trade licence
TRAD/DSCC/035498/2024; `info@vernesystems.com`; +880 1870 034384. The TIN and D-U-N-S
numbers are deliberately not published. Governing law (Bangladesh) and forum (Dhaka) are
shown as proposed terms pending counsel review. The policies remain drafts and are
"Not yet effective"; the effective date, hosting provider and location, DNS provider,
representative/DPO and log-retention values are still "Awaiting confirmation".

**Application stays private.** Only the engine, API and command-line tools are open source
(MIT, public repository `ArefinAlter/dynodoc-engine`). The site and workspace are no longer
described as open source or downloadable: `/source` is a 302 to the engine repository, the
footer, FAQ, `llms.txt`, JSON-LD and licenses page carry the owner's sentence, and the
compose file no longer mounts a source archive. The engine repository's README, NOTICE,
SECURITY and CODE_OF_CONDUCT use the company contact. **Open decisions for the owner:**
the private repository's `LICENSE`, `Cargo.toml`, `package.json` and `openapi.yaml` still
say AGPL-3.0; and `hyperformula` is GPL-3.0-only (or commercial) and ships in the
workspace bundle delivered to browsers, so a commercial licence or a replacement is needed
before the workspace can be called closed source.

**Receipt authority.** Decision recorded in [RECEIPT-AUTHORITY-DECISION.md](RECEIPT-AUTHORITY-DECISION.md):
the signing authority stays uninstalled in production (immutable key, no server-side
rotation or recovery, no consumer, no rehearsal, no custodian), with an enablement checklist.

**Issues, labels and stars (schema 39).** [Project issues API](PROJECT-ISSUES-API.md):
invite-only project issues with labels, assignees, immutable comments, optimistic edit
conflicts, per-project caps, audit entries and erasure rules; project stars. The Projects UI
gains an Issues tab, a labels manager, a star button and palette actions. Project-scoped
device tokens are not accepted on these routes. The engine API proxy now forwards `PUT`.

**Storage and convergence (local, read-only).** `engine-cli repository … gc-plan`
([LOCAL-RETENTION-PLAN.md](LOCAL-RETENTION-PLAN.md)) reports unreachable commits and objects
and what stays pinned; there is still no garbage collector. `compare` gains a dry-run
`same_file_convergence` report for files edited on both sides (node and field conflicts,
order-independent address, nothing stored or signed; see
[PORTABLE-PROJECT-HISTORY.md](PORTABLE-PROJECT-HISTORY.md)).

**Verification.** Application Rust: fmt and Clippy clean; all tests pass except the
Windows-only `sse_delivers_a_new_event_within_100ms` timing assertion (Linux CI is the
acceptance run). Frontend: Prettier, ESLint and svelte-check clean; unit tests pass;
isolated authenticated browser suite 103/103 (23 specs) on a schema-39 stack.

**Not done / still open.** Packing and incremental publication, a real retention policy and
collector, applying (not just reporting) same-file merges, per-project device-key grants,
operating-system key storage, Windows key-file ACL hardening, web authoring of project-wide
branches (a browser holds no signing key), follows/profile-starred lists, marketplace
publication, installed-editor acceptance, witnesses and large-file acceptance.

## Live: schema 38 release (12 October 2026, 19:18 UTC)

The VPS workspace, API and admin site now run application `49ada1e`, schema 38, with the
new public site, GitHub-style Projects/profile/admin and project-scoped device sessions.
A production-copy rehearsal and a verified backup preceded activation; row counts and
event/snapshot digests were identical before and after (two documents, 36 events).
Shared writers stay off and no receipt-signing key is installed, so shared branch
publication is refused. Read [the release record](operations/2026-10-13-schema38-release.md)
for evidence, rollback and next steps. CI on both repositories passed on the deployed
revisions (application `49ada1e`, engine `e5b9d94`). Older entries below that record production at
`858801c`/schema 29 describe the state before this release.

## Source: public site redesign, GitHub-style workspace, device sessions (13 October 2026)

This increment changes the public site, the signed-in Projects and admin interfaces,
the add-in marketplace kit and the API. Source is PostgreSQL schema 38 (migrations
0037 and 0038); this source was deployed to production later the same day (see the entry above).

**Public site.** The marketing pages are rebuilt as a "Paper and Ink" design system
([SITE-DESIGN.md](SITE-DESIGN.md)): warm paper palette, Geist and Geist Mono, mono
`[ LABEL ]` eyebrows, dropdown header, large footer, scroll reveals, count-ups, typing
diffs and eight original pencil-and-halftone dinosaur plates that animate in on scroll
and respect reduced motion. Pages: home, features, why-dynodoc, roadmap, changelog,
brand, help, compatibility, add-ins, support, and the legal/trust set (privacy, terms,
cookies, security, subprocessors, acceptable use, accessibility, DPA draft, licenses,
`/.well-known/security.txt`). All research/questionnaire framing is removed from the
public copy, FAQ, `llms.txt` and structured data. Legal pages remain drafts; every
operator, address, email, jurisdiction and effective-date field still reads
"Awaiting confirmation" and must be filled by the owner ([LEGAL-PAGES.md](LEGAL-PAGES.md)).
Public links now point at the public MIT engine repository because the application
repository is private.

**Workspace.** Projects, the repository page, file page and profile follow GitHub's
layout: global header with command palette (Ctrl/Cmd+K, `/`, `t`, `?`, `g p`),
file table with last-change summaries, README panel, formats bar, contributors,
pull-request list with Open/Merged/Declined counts, activity feed, contribution graph
and history list. A read-only **Branches** tab lists shared project history
(refs, generation, head commit, signed receipt metadata, reviews); the browser never
claims to verify a receipt. Creating and merging project-wide branches in the
browser, stars, issues and labels are not built.

**Admin.** `/admin` is reorganised as a left-nav console (overview with needs-attention
list, people/projects/content, audit log with filters and detail, storage, services,
traffic, database, danger zone). All existing controls and accessible names are kept.

**API (schemas 37 and 38).** Read-only APIs back the new UI ([contract](PROJECT-ACTIVITY-API.md)):
file `last_change`, project activity feed, contributors, profile activity calendar,
admin project overview and audit detail/filters; migration 0037 adds only indexes.
[Project-scoped device sessions](PROJECT-SCOPED-DEVICE-SESSIONS.md) (migration 0038):
a device holding an enrolled Ed25519 key signs a one-use challenge and receives a
one-hour token for one project with a subset of `project:read`, `history:read`,
`history:write`; renewal is another signature, so the key file is the only long-lived
credential. Token confusion is closed with a Paseto implicit assertion; the allow-list is
21 explicit handlers; every request rechecks key, account, generation and current role.
`engine-cli device keygen`, `sync --device-key` and `sync --refresh-token-file` are added.
Admin "sign out all sessions" now also revokes the account's device keys. An independent
review found no critical or high issue; accepted residual risks are an unencrypted key
file as a single factor for account-wide history write/approval until per-project key
grants or a review scope exist, no Windows ACL hardening, and no OS key store.

**Add-ins.** Manifests are production-named (Dynodoc for Word/Excel/PowerPoint), carry a
64 px icon and the support URL, and are covered by 32 offline invariant tests; the online
Microsoft validator accepted all three. Marketplace assets, listing copy, reviewer notes
and an owner checklist are in `docs/listings/` and `extensions/marketplace/`. Panes drop
"development" wording and make browser sign-in primary. No listing exists, none is
submitted, and installed-host acceptance has not been run.

**Verification.** Application Rust: all tests pass except the known Windows-only
`sse_delivers_a_new_event_within_100ms` timing assertion (385 tests; Linux CI is the
acceptance run). Clippy and rustfmt clean; OpenAPI regenerated. Frontend: lint,
Prettier and svelte-check 0 errors; 340 unit tests; 25 public browser tests; axe on 20
public pages; authenticated suite 99/99 on an isolated stack built before migration 0038.

**Not done:** a retention/GC plan command and same-file convergence reporting were
started and set aside incomplete (partial work retained outside the repository);
storage packing, offline convergence, witnesses and large-file acceptance remain open.
Per-project key grants, OS key storage and project-wide branch authoring in the web UI
remain open. Marketplace publication, legal operator details and real-host acceptance
need the owner.

## Source: engine PR review and application synchronization (12 October 2026)

Reviewed all ten open standalone-engine PRs and merged **#1, #2, #3, #4, #6,
#8, #9, #10 and #11**. PR #5 remains open: ULID 3 removes `Ulid::new()`;
[fresh CI](https://github.com/ArefinAlter/dynodoc-engine/actions/runs/38251056776)
fails with E0599. It needs an explicit call-site/API migration and fresh tests.
The engine and application retain ULID 1.2.1.

The merged updates use checkout v7, UUID 1.28.0 (the old PR title said 1.27.0),
clap 4.6.7, serde 1.0.229 and similar 3.2.0. PR #1's old failure was generated
OpenAPI metadata drift; updating its branch made the full workflow pass. PR #4's
overlapping lockfile changes were resolved to retain syn 3 for both derive crates.
The final lockfile matches the combined locally tested dependency graph.

Replay now decodes borrowed payload JSON. Merge canonicalization borrows states
without removed choices, retaining the newer cached tree-depth implementation
when resolving PR #9. Saving a named version avoids a redundant state load while
keeping the document/project locks, authorization and verified snapshot path.

PR #10 adds streaming `log::verify_from` for an explicitly trusted chain position.
Review caught two regressions in the proposed implementation: full-chain audits
could omit negative sequence rows, and a maximum-sequence anchor could overflow.
Both new regression tests failed against the original implementation and pass
with the corrections; all ten event-log tests pass. Genesis verification still
checks every stored row. A non-genesis trusted position intentionally excludes
earlier events; it needs an independently trusted anchor, not a database-only
claim of independent audit security.

The original combined candidate passed **358 Windows tests**, none failed,
ignored or filtered. After the audit fixes, targeted event-log tests, final
`task lint`, `task docs` and unchanged generated OpenAPI passed. Documentation
builds retain existing rustdoc warnings. Historical hash, snapshot/receipt and
rich-text replay fixtures were not regenerated. Similar 3.2 changes new diff
heuristics; the tested replay and merge cases pass, without claiming identical
hunks for all inputs or complete offline convergence.

Final engine source: `422e011`; application synchronization: `15f42c6`.
[Final engine CI](https://github.com/ArefinAlter/dynodoc-engine/actions/runs/38253849857) and
[application Rust CI](https://github.com/ArefinAlter/dynodoc/actions/runs/38253873729) passed all **360 tests**
on those code revisions. Application package/license metadata is preserved.
Both frontend jobs, including authenticated browser workflows, passed in
[application frontend CI](https://github.com/ArefinAlter/dynodoc/actions/runs/38253873733).
Handoff/status updates follow separately so the evidence identifies tested code.

No migration, stored hash format or API schema changed (PostgreSQL 36 / SQLite 2).
The disposable PostgreSQL container was identified by its task label and removed;
existing development containers were untouched. The two clean task review
worktrees were removed after verification. Logs remain under ignored
application `target/engine-pr*.log`. No production data or real email was used.
No VPS deployment or installed-editor acceptance occurred. Production remains
recorded at `858801c` / schema 29; the preceding selected-history sync handoff and
roadmap acceptance ledger still describe the remaining product work.


## Previous increment: selected-history CLI synchronization (12 October 2026)

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

The VPS workspace/API are unchanged. Existing production is still recorded as `ecbc643`,
schema 26. Source requires schema 29, verified snapshot backfill and a provisioned
runtime login. A production-copy upgrade and compatible-image rollback rehearsal
remain required; pre-boundary API images are not safe rollback.

Next source priority: project policy authority/floors and audited metadata, then
CSP/HSTS and resource budgets. The product roadmap remains bounded shared chunks,
portable project commits/branches, durable offline repositories/exchange/convergence,
stronger audit commitments, measured scale and native-editor acceptance/distribution.

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

## Latest source work: authentication and bounded history (10 October 2026)

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
engine `2e75859`. Both Rust CI runs passed. Frontend build/unit/public-browser CI
passed; the first authenticated run passed 67 of 68, with an intermittent cursor
failure in the existing personal-draft typing test. Twenty local diagnostic runs
and the failed-job CI rerun passed. The exact cause is still unconfirmed; no editor
or test behavior was changed to hide the failure. CI links and evidence are in
[the handoff](ENGINE-EVOLUTION-HANDOFF.md).
See [the full findings matrix and rollout contract](SECURITY-HARDENING.md).

Highest-priority remaining security work: legacy snapshot verification/immutability,
runtime database privilege separation, project policy floor and audited metadata,
CSP/HSTS and remaining resource budgets. Then resume bounded shared storage,
portable project commits/branches, offline convergence and installed-editor acceptance.
This increment is not deployed to the VPS; the workspace/API remain ecbc643/schema
26. The new API needs schema 27. Rollback must retain login revocation enforcement
or deliberately invalidate credentials; an unmodified old API is not safe rollback.
No real production sign-in, email or document mutation was used.

## Latest source: bounded checkpoint encoding (10 October 2026)

Implementation is pushed to both main branches: application `2f69c87` and standalone
engine `3ac2ad6`. Source CI evidence is linked in the handoff.

Part 2e now borrows checkpoint values, caps encoded output growth before allocation
and validates canonical bytes without another full encoded buffer. V1 hashes and
roots remain unchanged. An oversized-value rejection probe measured 1,048,724
additional requested heap bytes versus 75,500,265 previously; this is not total
document memory or service capacity. See [scope and evidence](BOUNDED-CHECKPOINT-ENCODING.md).

Desktop/web installation instructions and Google test packages are clarified.
The installed Office 2021 PowerPoint lacks the connector's required API. Actual
Word/Excel/PowerPoint and Google-host acceptance is still open. Public distribution
requires Microsoft Partner Center or Google Cloud/Workspace Marketplace setup and
review; development sideloading does not require a public listing. No registration
or submission was performed. See [the acceptance/publication record](ADDIN-PUBLICATION-AND-ACCEPTANCE.md).

This increment is source-only; production remains `ecbc643`, schema 26. Shared
periodic writes remain disabled. Next: large-value/asset chunks, packing/incremental
publication and shared draft/version references, then portable project-wide commits/
branches and resumable exchange. Offline convergence, background capture and native
acceptance remain separate gates. Exact checks and source refs are recorded in
[the handoff](ENGINE-EVOLUTION-HANDOFF.md).

## Current release: Projects first (10 October 2026)

Application `6f1ac21` and standalone engine `8eef6dd` are pushed to main with green
source CI. Projects/files are now the default signed-in workspace, with member-only
profiles, per-file main/draft navigation, pull requests, history/recovery and a
separate Web editor tab. Test uploads are capped at 3,000,000 original bytes, with
backend enforcement. Account erasure clears the added profile metadata.

Verification: 233 Rust tests in each repository, 282 frontend unit tests, 16 public
and 67 signed-in browser tests pass, as do lint/build and Nginx header regressions
for the new routes. Desktop/mobile and dark file previews were inspected. Owned
local test servers are stopped and the labelled disposable database is removed.
Application `ecbc643` (same implementation) is now live on schema 26. A fresh
validated backup and an isolated upgrade/rollback rehearsal preceded activation.
API/web/admin/converter were updated; PostgreSQL, Nginx and 18 unrelated containers
were preserved. All 17 workspace and 22 connector delivery checks pass, including
the new page-shell proxy paths. Existing document/event counts and the aggregate
event fingerprint are unchanged. No real production sign-in/email or installed-host
acceptance was performed. Shared checkpoint writes remain disabled.

Next engine increment: bounded values/assets, compression/incremental publication
and shared draft/version references in part 2, followed by portable project-wide
commits/branches and resumable exchange in part 3. Installed-host acceptance for all
six editors can proceed alongside that work. See the [handoff](ENGINE-EVOLUTION-HANDOFF.md),
[workspace release record](PROJECTS-WORKSPACE-RELEASE.md) and
[deployment/rollback evidence](operations/2026-10-10-projects-workspace-release.md).

## Previous deployment: history and six-editor test packages (10 October 2026)

That release ran application `536a448` (implementation `0478bf2`), schema 25.
History-range recovery, supported native historical downloads and the six editor
sign-in/install packages are live at [editor installations](https://app.dynodoc.online/connectors).
The database was backed up first; only API/web/admin containers were replaced.
Existing document/event counts and aggregate event fingerprint are unchanged.
PostgreSQL, converter, Nginx and 18 unrelated containers were preserved.

All 22 read-only delivery/auth checks passed; a clean browser found six downloads
and followed approval to login without page errors. Synthetic Workspace/Projects
page shells pass through live HTTPS with their complete preload headers. No real
account sign-in, email, document mutation or installed-editor acceptance was used.
Upgrade and schema-compatible rollback startup passed on an isolated database;
all rehearsal containers/network are removed. Shared checkpoint writes remain off.
See the [release and rollback record](operations/2026-10-10-history-connector-release.md).

Next: installed Word/Docs, Excel/Sheets and PowerPoint/Slides acceptance. The next
independent engine increment is part 2 bounded text/assets and shared draft/version
references, followed by portable commits/branches and resumable exchange. No new
algorithm, complexity, native fidelity or marketplace claim follows from deployment.

## Latest increment: history ranges and editor installations (10 October 2026)

Implemented revision-range revert/cherry-pick through checked private drafts,
explicit overlap choices and existing review rules; native historical DOCX/XLSX/PPTX
downloads reuse the supported exporters and retain document/revision origins.
All six connector clients now support browser sign-in/file approval; Microsoft
ribbon manifests and Google Editor add-on ZIP packages are downloadable at
`/connectors`. This remains test distribution, not store publication or full fidelity.
Application fmt/Clippy and all 231 Rust tests pass. Frontend lint/check/build pass;
the 277-test suite plus three new Google pairing tests pass. Four signed-in history/
auth/export journeys and seven existing connector bridge scenarios pass locally.
All three Office manifests pass Microsoft validation. Both repos are synchronized
and pushed: application 0478bf2, engine a44b8ad. Standalone final lint/docs/OpenAPI
and all 231 tests pass. Final source CI passed all 231 Rust tests in each repository and 280 frontend
unit / 16 public / 66 authenticated-stack browser tests. Source/check links and
installation limits are recorded in the handoff.
Both disposable test databases and owned API/web servers are removed/stopped.
At source completion there was no production deployment or schema/event change;
the later schema-25 rollout is recorded above. Shared writes remain opt-in.
[History contract](HISTORY-RECOVERY.md), [connection contract](CONNECTOR-PAIRING-AND-INSTALLATION.md).

Deployment is now complete; next complete six-host acceptance and resume bounded
text/assets and shared draft/version references, then
portable commits/branches and exchange. Current range commands use event revisions.

## Previous increment: permission-controlled historical recovery (10 October 2026)

The product now explicitly includes retrieving old or unnamed revisions and
recovering selected content through current role/approval rules. The implementation
adds a checked preview and private recovery draft using any retained event revision,
with source hash/sequence, idempotent retry and the existing review/merge path.
The web Version history panel offers checkpoint download and block selection;
Activity links to the state before an edit. Canonical history is never rewound.

Application fmt/Clippy and 229 Rust tests pass, followed by the five review tests
after the source-label guard. Frontend lint, 274 unit tests, production build and
the new signed-in recovery workflow pass. API schemas are regenerated. Standalone
source is synchronized and lint, all 229 tests, docs and OpenAPI pass. Application
source d83bf86 and engine source fdba931 are pushed to main. Both source CI runs
passed, including 274 frontend unit tests, 16 public browser tests and 63 signed-in
workflows. The owned disposable database and API/web servers are removed/stopped.
[Contract and limits](HISTORY-RECOVERY.md), [continuation handoff](ENGINE-EVOLUTION-HANDOFF.md).
No migration or VPS deployment. Shared checkpoint writes remain opt-in. This is
whole-block state recovery; commit revert/cherry-pick and native historical file
fidelity remain unfinished. Bounded chunks and shared references remain next.

## Previous increment: content writes during checkpoints (10 October 2026)

Part 2d makes the core/API writer lock compatible with checkpoint erasure guards.
Writers still serialize before permission/state/sequence checks; checkpoints retain
an exact immutable revision. Stronger management/erasure locks and atomic graph
publication remain. No new schema or staged-object protocol. Shared writes stay
opt-in while full-state, chunking, compression and concurrent API load work remain.

A controlled 8,000-block / 6.4 MB trace with ten real core writers records commit
times of 38-212 ms instead of 1.93-2.04 s under the old entry lock. All ten current
commits overlap publication, with identical checkpoint roots and verified final
states/chains. Checkpoint work itself remains about two seconds; the trace excludes
full API validation/materialization and native-host processing. See
[the lock audit, tests and measurements](CHECKPOINT-CONCURRENCY.md).

Both repositories pass fmt/Clippy, all 226 Rust tests and unchanged OpenAPI;
standalone docs pass with existing link warnings. Application source c19dbda and
engine source cc12b29 are pushed to main. Both source CI runs passed, including
274 frontend unit tests, 16 public browser tests and 62 signed-in workflows. The
disposable database/volume is removed. Exact continuation steps
are in [the handoff](ENGINE-EVOLUTION-HANDOFF.md). No VPS deployment.

Next: versioned bounded text/asset chunks, compressed packing and incremental
materialization, shared version/draft references, then durable local commits and
resumable exchange. Native/offline/provenance/scale acceptance remains unfinished.

## Previous increment: bounded checkpoint batches (10 October 2026)

Part 2c batches PostgreSQL checkpoint writes (64 objects / 1 MiB encoded) and index
reads (16 objects), retaining v1 hashes, exact-byte validation, document scope and
atomic rollback/erasure. Publishers serialize overlapping historical graphs before
object writes. Unchanged batches issue no INSERT. Complete state still resides in
memory and publication still scans it while blocking document appends.

Four controlled 8,000-block / three-checkpoint traces preserve identical paired
roots and canonical bytes. Publication falls from 14.6-20.9 s to 2.1-2.9 s; the
append-lock wait remains 2.1-2.9 s. Graph reads use 619 object SELECTs instead of
9,817. Shared relations use about 10.9 MB; compressed legacy snapshots use 0.64 MB
for repeated text and 20.78 MB for varied text. These are synthetic local traces,
not native large-file or concurrent-service acceptance. The shared writer remains
off by default. [Measurements/limits](SHARED-CHECKPOINTS.md).

Application fmt/Clippy, 224 Rust tests and unchanged OpenAPI pass. Standalone
lint, all 224 tests, docs and unchanged OpenAPI pass. Both source CI runs passed,
including 62 signed-in workflows, 16 public browser tests and 274 frontend unit
tests. Application source: `40a80be`; standalone: `c8ac6bc`. Both are pushed to main;
the disposable database/volume has been removed. [Verification/publication record](ENGINE-EVOLUTION-HANDOFF.md).
No new migration or deployment; schema 25 remains required for mixed readers.

Next: incremental/staged publication with a short final lock, bounded text/asset
chunks and compression/backend measurements, shared version/draft references,
then durable local commits and resumable exchange. Offline convergence, independent
provenance and native/scale acceptance remain open.

## Previous increment: mixed checkpoint adoption (10 October 2026)

Part 2b adds normal current/history and API write readers for legacy/shared
checkpoints, with verified shared graphs and streamed suffixes. An optional periodic
shared writer reuses objects; capacity failures roll back partial data and retain
a legacy checkpoint at the same revision. Missing/corrupt selected shared data
fails explicitly. Duplicate builders skip work; PostgreSQL value reads use batches
of at most 16 objects. Named/deployed versions and draft bases still use full JSON.

Shared writes remain off by default. Publication still scans all state and writes
objects individually while blocking FOR UPDATE appends on the document. Database
measurements and verification are in the [checkpoint contract](SHARED-CHECKPOINTS.md)
and [handoff](ENGINE-EVOLUTION-HANDOFF.md). All 220 application Rust tests, fmt/Clippy
and unchanged OpenAPI pass. Standalone lint, all 220 tests, docs and unchanged
OpenAPI pass. Both source CI runs and application browser CI passed. Published
source: application `12be6a2`, engine `970d78a`; final records are in the handoff.
No VPS deployment. Schema 25 is required by the new reader.

Next: reduce publication round trips and lock duration, add bounded text/assets,
migrate version/draft references, then durable local commits and resumable exchange.
Offline convergence, independent audit and native/scale acceptance remain open.
This increment does not certify the complete Git-style product.

## Previous increment: shared checkpoints (10 October 2026)

Part 2a implements opt-in shared checkpoint objects, stable-ID radix indexes,
document-scoped memory/PostgreSQL stores, verified atomic publication/retry and
audited erasure (migration 0025). Admin deletion previews include checkpoint/object
counts and bytes. Default snapshots and draft/version bases still use full JSON.

Application verification: 216 Rust tests, fmt/Clippy and unchanged OpenAPI passed.
Standalone task lint, 216 tests and documentation also passed. Implementation CI
passed in both repositories, including signed-in browser workflows. Source:
application `7a8e28e`, engine `6ccdc22`; engine final checkout-guard/handoff `a70647d`
also passed CI. Final records are linked below. An 8,000-block synthetic
trace used 7.83 MB of unique object bytes for 21 checkpoints versus 135.24 MB of full
JSON copies; each one-block edit added 4.7-7.3 KB. This excludes database/native-file
and service-load costs. [Contract and benchmark](SHARED-CHECKPOINTS.md).

Next: adopt shared checkpoints in normal readers/writers and version/draft bases,
add large text/asset chunks, measure durable backends and then implement local
commits/object exchange. Offline convergence, signatures/witnessing, full native
capture and large-file/load acceptance remain open. No VPS deployment in this
increment. [Exact handoff and publication record](ENGINE-EVOLUTION-HANDOFF.md).

## Earlier increment: repository foundations (10 October 2026)

Decision [012](decisions/012-local-first-document-version-control.md) establishes
local/remote provenance and reviewed change synchronization around all six native
editors. Read [PRODUCT-SPEC](PRODUCT-SPEC.md), [ENGINE-EVOLUTION-PLAN](ENGINE-EVOLUTION-PLAN.md)
and [ENGINE-EVOLUTION-HANDOFF](ENGINE-EVOLUTION-HANDOFF.md) first. Research-instrument
and civic stage prompts are historical references, not the current product roadmap.

Part 1 corrects mathematical independence/cost claims, centralizes nearest-checkpoint
historical reads, streams historical tails and chain verification, avoids decoding
snapshots just to check cadence, and bounds browser key-alignment work/trace while
removing quadratic duplicate lists. Stable IDs, event formats and migrations remain
compatible. Verification: 204 Rust tests in each repo and 274 frontend unit tests passed,
with the affected event/snapshot tests rerun after final streaming changes. Lint,
frontend type checks and standalone docs/OpenAPI passed. Exact publication and CI
evidence are recorded in the evolution handoff. Remote CI also passed: 62 signed-in
browser tests, 16 public browser tests and the production-proxy regression.

Part-1 code is pushed to main: application `667ecb6`, standalone engine `647249e`.

Part 1 handed off to part 2 typed object/commit contracts and shared checkpoints,
with legacy replay equivalence and tenant-scoped reachability/erasure tests. Complete
offline histories/convergence, signatures/witnessing, native structural/formatting
capture and hundreds-of-MB/load acceptance remain unfinished. This increment has
not changed the VPS; dated production records below remain historical evidence.


Previous deployment assessment: 6 October 2026, including decisions 010/011, date-only provenance, the verified VPS rollout and the workspace 502 repair. This distinguishes executable functionality from longer design documents and real-host/production acceptance.

Production workspace 502 was traced to asset preload response headers exceeding
Nginx's 4 KiB default buffer. Both app/admin HTTPS proxies now allow 16 KiB headers;
the identical failing workspace request passes after a validated graceful reload.
Containers remain at `8c7461d`, schema 24. CI now includes the actual proxy
templates. [Incident and verification](WORKSPACE-502-FIX.md).

## Previous release record: external-editor provenance and Projects (6 October)

The owner clarified on 6 October that Dynodoc is a GitHub-style provenance and
change-sync platform around existing editors. **Word/Docs, Excel/Sheets and
PowerPoint/Slides are all primary targets.** Further Office-parity work is
deprioritized; the existing basic web editors stay. [Decision 010](decisions/010-external-editor-provenance-and-projects.md)
supersedes the earlier editor-expansion backlog; [decision 011](decisions/011-six-editor-provenance.md) extends the workflow to all six editors.

The first Projects implementation includes creation/listing, Files, cross-file
requests, private own Drafts, Settings/members, inherited rules, new-request Watch
and breadcrumbs. The engine stores/retrieves immutable change bundles, checks
exact base hashes and returns durable retry receipts. File-scoped, revocable
keys let connectors read/propose without account-wide access or merge permission.

The recovery follow-up adds versioned sidecar export/import to both connectors,
with original-base verification through scoped historical checkpoints, unchanged
retry IDs and refusal to replace an existing queue. Word stops the connection
after Save As/rename and keeps the queue exportable. This provides explicit
working-copy/device recovery; background capture remains open. See
[recovery release evidence](PROVENANCE-RECOVERY-RELEASE.md).

Word task-pane and Google Docs sidebar development sources share an opt-in,
durable paragraph-observation queue and checked pull previews. **Actual Word/Docs
host acceptance and marketplace publication are not complete.** Existing
paragraph text is the initial supported exchange; formatting and structural
changes require file push. Capture is pane-dependent, not a background local
agent or a complete keystroke/authorship attestation.

Excel/Sheets and PowerPoint/Slides now have development connectors using the same
field-level queue, retry/recovery and checked pull flow. Spreadsheet cells/formula
source, date-only values and slide shape text are supported; structural/formatting/timestamp/media sync
remains open. Migration 0024 extends file/host-scoped keys with file-kind checks.
See [six-editor release](SIX-EDITOR-PROVENANCE-RELEASE.md).

The date follow-up uses checked native calendar/timezone conversions, rejects
invalid days/hidden times and unsupported date masks, and preserves exact retry
and recovery IDs. Excel now needs API 1.13 for merged-cell checks; existing Sheets
installations must add generated `Date.gs`. Dates encode civil days rather than
instants. [Date release](SPREADSHEET-DATE-SYNC-RELEASE.md),
[native acceptance checklist](NATIVE-CONNECTOR-ACCEPTANCE.md).

Projects and all six host scopes are deployed on the VPS at `8c7461d`, schema 24.
All 62 signed-in browser tests, 271 unit tests and 200 engine tests passed. Existing
accounts/history and unrelated services were preserved. Native connectors remain
development installations, not marketplace releases.

Next: accept all three pairs in real hosts; extend structural and
formatting block sync; add durable background capture and secure pairing/publication;
then Project Compare/Activity/search/releases.
See [release evidence](PROJECTS-PROVENANCE-RELEASE.md), [sync contract](PROVENANCE-SYNC.md)
and [installation](../extensions/README.md). The current VPS rollout is recorded
in the date release; earlier increments retain their dated evidence.

Invitation emails shipped in `b70f048`; release documentation followed in
`52c2447`. Document/workspace invitations are sent by the web gateway after the
engine grants access. Other notifications remain in-app. Production delivery was
not exercised during rollout, so the owner's own second-address invitation test
is still an acceptance check, not a reason to reinstall credentials. See
[release evidence](COPIES-AND-ROLES-RELEASE.md#follow-up-releases).

The standalone engine capability records are current at `67060cf` (service code `d6666a2`) from
application `348bef9`,
including Projects, inherited rules, scoped connector keys and immutable
provenance bundles and historical checkpoints alongside review, roles, copies
and notification APIs, plus all six editor key hosts and file-kind checks. The
date clients at application `8c7461d` reuse that unchanged service contract. Its
[capability inventory](https://github.com/ArefinAlter/dynodoc-engine/blob/main/docs/CAPABILITIES.md)
separates implemented service behavior from native-host acceptance, invitation
delivery and future work. [Synchronization evidence](ENGINE-EXTRACTION.md#date-only-client-follow-up-6-october-2026).

## Continuation: production reset and future directions (30 September)

The owner decided to permanently delete every document in production while keeping
all accounts and sign-ins; it was done through the audited erasure path after a
validated backup ([operations record](operations/2026-09-30-document-reset.md)).
The reset left no documents or workspaces on 30 September; the six-editor release
records the latest live aggregate counts. At that handoff, planned work included
a GitHub-style [Projects workspace](proposals/PROJECTS-WORKSPACE-HANDOFF.md),
[agents that edit documents with provenance](proposals/AGENT-PROVENANCE-PROPOSAL.md) and
[assessing how students use AI](proposals/AI-ASSESSMENT-PROPOSAL.md); see
[proposals/README.md](proposals/README.md) for the index and research. The first
Projects increment is now implemented and deployed; agent/assessment proposals
remain future work.

## Continuation: copy detection, roles, review rules, notifications and night mode

On 30 September the owner asked for renamed-file recognition with owner
notifications, a clearly defined owner/editor/reviewer ladder, fast similarity
detection, conflicts for same-name files with different content, day and night mode
everywhere, and permanent deletion from the admin panel, followed by a push to
`main` and deployment. [Decision 009](decisions/009-copies-roles-and-notifications.md)
records the design and limits; [release notes](COPIES-AND-ROLES-RELEASE.md) record
verification and rollout; [the user guide](GIT-STYLE-WORKFLOW.md) now covers roles,
review rules, notifications, copies, Excel/PowerPoint uploads, night mode and a
2–3-person test plan. Migration `0021` is new; no applied migration changed; no new
event variants, so `office-v5` editors remain compatible.

## Continuation: file push, change requests, presentations and UX fixes

On 27 September the owner asked for a GUI-testable Git-like workflow with real
local Word files from several people, a rebase feature, a PowerPoint-style editor,
UX fixes and a plain-language walkthrough. [Decision 008](decisions/008-change-requests-presentations-and-ux.md)
records the design; [release notes](CHANGE-REQUESTS-RELEASE.md) record exactly what
was verified on an isolated local stack; [the user guide](GIT-STYLE-WORKFLOW.md)
explains the workflow without Git terms. Migration `0020` is new; no applied
migration changed. The editor header is now `office-v5`. This work was
released as `9c1ec9d` and deployed to the VPS on 27 September with passing CI;
the release record lists the backup, rollback tags and live checks.

## Continuation: Office analysis and references

The owner subsequently authorized broad Office implementation and a push to `main`.
[Decision 007](decisions/007-office-analysis-and-references.md) and
[release evidence](OFFICE-EXPANSION-RELEASE.md) supersede earlier local-only wording
and the old charts/pivots/note-body deferrals. The new work adds the Excel reference
UI, native supported charts, pivot summaries, named ranges, data tools, server-checked
sheet protection, and Word note/contents editing. Full Office parity, native pivot
caches and live per-page footnote layout remain unfinished. Publication and runtime
status is recorded in the release record: `b0ab062` (including the `a413c20` UI
release and the immediate stream acknowledgement) is pushed to main and deployed
to the VPS workspace on 22 September, with passing frontend/authenticated/Rust CI
and live synthetic browser checks. The MIT engine is published independently.

## What this project is for

Dynodoc helps teams keep documents, spreadsheets and questionnaires together with an understandable record of changes. A paragraph, question, choice or spreadsheet row has a permanent identity. Edits are recorded as semantic events, attributed to their author and chained by hashes. Collaborators can work in personal drafts, compare with the team version, resolve overlapping edits, include selected changes and restore previous content without deleting history.

The original immediate product was a questionnaire authoring tool. The current product direction is provenance/review/sync around Microsoft and Google editors, with the existing basic web editors retained as secondary tools. The public-consultation design in `05-public-consultation-part1.md` describes a later product: large-scale citizen input, moderation, identity tiers, deliberation and synthesis. Those services are not implemented by the current researcher workspace.

## Implemented document workflows

| Area | Current behavior |
|---|---|
| Core | Append-only event log, hash verification, stable IDs, snapshots, role checks and serialized writes. |
| Privacy | Direct and inherited organization/team/folder roles protect reads, writes, streams and search. Revocable public links expose explicit frozen copies only. |
| Sessions | Encrypted HttpOnly cookies, same-origin API gateway, refresh rotation; Google authorization-code login and Resend email-link implementation. |
| Homepage | Responsive cream/blue homepage, self-hosted sans serif typography, interactive editor/draft/integration examples, document version-control explainers, reduced-motion behavior, Node and Vercel builds. |
| Legal pages | Privacy, terms and cookie pages describe actual storage and sharing; browser measurement preference is available. Operator details and legal review remain open. |
| Workspace | Create/import documents, organize them in a hierarchy, search, share roles, make copies, move to Trash and restore. |
| Document editing | Document tabs, common ribbons, paragraph styles/direction/spacing, shared font uploads, text tracking with accept/reject, threaded comments with resolve/reopen, table/context actions, equations, versioned page setup, live editable pagination and paginated print preview. Footnotes/endnotes have editable text and native DOCX parts; contents can be inserted/refreshed. Native DOCX import runs in a bounded worker; LaTeX/Overleaf interchange remains. |
| Spreadsheet editing | Typed cells, ranges, relative formula fill, persistent calculation graph, formatting, validation subset, merges, freeze/hide/resize, sheet lifecycle, find/replace/filter, formula-aware row sorting, names, charts, pivot summaries, data tools, whole-sheet protection, numeric conditional formatting, worker-based XLSX import and styled XLSX exchange. |
| Version workflows | Personal drafts, selected-block sharing, get latest, conflict choices, named versions, comparisons, attribution and restoration as new events. Change requests from Word, Excel and PowerPoint files, approvals and review rules. |
| Copies and roles | Renamed/edited copies recognised by content fingerprints; owners notified in an in-app inbox; Owner/Manager/Editor/Reviewer/Contributor/Viewer roles; same-name unrelated uploads need explicit choices. |
| Themes | Day, night and system modes across the website, workspace, editors and admin site. |
| Research files | DOCX/DOC, CSV/XLSX, XLSForm and REDCap dictionary interchange; original files retained. |
| Research connections | ODK/Kobo form and response imports; REDCap metadata/records, SurveyCTO wide responses and SurveyMonkey bulk responses. Google Forms adds separate read-only OAuth definition/response imports. Live study-account acceptance remains open. |
| Questionnaire preview | Supported XLSForm relevance, calculations, required rules and constraints; explicit unsupported-logic errors. Answers stay in browser memory. |
| Administration | Allowlisted dashboard with typed confirmation, impact previews, one-step and batch permanent document deletion, whole-document/account erasure, disable/restore, ownership transfer, forced sign-out, link revocation, CSV export and immutable audit/receipts, database/storage/service/backup status, anonymous page loads and web vitals. |
| Search and identity | Original SVG logo, favicons/social card, canonical metadata, structured data, sitemap and private-page noindex rules. |
| Deployment | Isolated VPS containers and networks, HTTPS, private database, constrained Word converter, database backup and restore procedure. |

There is no defensible single “percent complete” for both the pilot and the full platform. The researcher pilot has working end-to-end paths; the full product requested is still incomplete. In particular, having common editor controls does not establish parity with every Google Docs or Google Sheets tool.

The narrower [pilot checklist](decisions/001-research-team-demo.md) currently has
11 of 14 criteria implemented. The three open criteria concern real provider
acceptance, external study-tool validation and full researcher/accessibility/load acceptance; this count is not an estimate
of the engineering effort left for the full product.

The owner explicitly pivoted the product to document version control on 21 September. Research remains a supported use case. See [decision 004](decisions/004-document-version-control.md) and [office compatibility and verification](OFFICE-EDITOR-CAPABILITIES.md). The historical pilot count is not a product-completeness score.

## Owner configuration still needed

- Supply the legal operator, public contact email, mailing address, governing jurisdiction, hosting/subprocessor locations and retention arrangements; complete review before adopting the privacy/terms drafts. [Drafting notes](LEGAL-PAGES.md).
- Complete real Google login and Resend email delivery tests. Credentials are installed and the owner confirmed Google publishing/callbacks. [Instructions](GOOGLE-AND-EMAIL-SETUP.md).
- Complete Search Console domain ownership verification and submit the sitemap. Vercel deployed this release automatically. [Instructions](SEARCH-AND-ADMIN-SETUP.md).
- Add the ten actual researchers and run a study-specific acceptance session using their files. The automated concurrency exercise uses synthetic researchers.

## Other remaining work and deferred editor features

The external-editor and Projects order above controls implementation. The Office
features below record known compatibility limits; they are not the current build
priority. Existing web editors remain supported.

- Broader document features: pagination refinements (oversized objects, repeated table headers, widow/orphan control), tracked moves/formatting and structural revisions, per-page footnote layout and rich notes, citations/bibliographies, section-specific layouts and exact Office round trips.
- Broader spreadsheet features: advanced charts, native pivot caches, structured table formulas, full formula-based validation, protected cell ranges/passwords, full Excel formula/date/locale compatibility and higher-fidelity workbook formatting.
- Survey execution beyond the supported preview subset: repeat instances, translations/media workflows, external datasets, advanced XPath/date functions and vendor-specific expression engines.
- Research-platform synchronization: scheduled refresh, incremental reconciliation and writeback. All six implemented API import paths need real study-account verification. CSPro dictionary/form/logic support is not implemented; exported CSV/XLSX data can be edited.
- Project-level follow-ups: Compare/Activity/search and consistent multi-file releases. Creation, files, aggregate requests, private own drafts, inherited rules, settings and submission Watch now have a first implementation.
- Stronger review: optional e-mail for notices beyond invitations and mentions, richer tracked suggestions and per-block/protected-range permissions. In-app notifications, approvals, per-change comments and review rules shipped in decision 009; invitation email shipped in `b70f048`. Other people's unsubmitted drafts must remain private.
- Versioning refinements: full offline synchronization, branch-to-branch review, more detailed side-by-side rich-content comparison, cryptographically anchored metadata/permission audit and long-lived retry/idempotency handling for every new workspace endpoint.
- Production operation: off-site backups (explicitly deferred by the owner), monitoring/alerts, recovery objectives, resource/load tests beyond ten synthetic concurrent edits, complete keyboard/screen-reader audit and an external security review.
- Public consultation stages 22–31, civic identity, moderation, citizen frontend, deliberation and ML services remain future work. The CRDT live-typing layer is also deferred; overlapping block edits require review instead of character-level co-editing.

## Web operations increment

Decision 005 implements the approved erasure policy, optional browser recovery,
canonical discussion replies/resolution, persistent per-draft exclusions, expanded
OpenAPI and authenticated browser CI. The homepage/workspace/admin receive a more
spacious cream/blue interface. Off-site backups are deferred; this is web-only work.
See [the release evidence](PRODUCT-OPERATIONS-RELEASE.md). No production user data
is deleted by the release. Broader Office parity and real-provider acceptance below
remain open; the release does not relabel the complete roadmap as finished.

## Verification evidence

The development verification log is recorded in `SETUP.md`. Automated tests cover membership isolation, atomic batches, stale writes, ten simultaneous independent edits, draft revisions/conflicts, stable-ID restoration and hash integrity. Browser tests exercise real local API sessions, documents, spreadsheets, sharing, version history, draft merging and file imports. Google/Resend provider tests remain open until actual browser login and mail delivery succeed. Credentials are configured.

The 21 September rich-text increment adds compact formatting-aware history,
same-paragraph merging for compatible wording/formatting changes, and a
permissioned structure/provenance view. Initial imports can contain up to 20,000
creation operations within the existing byte limit. [Research and benchmarks](RICH-TEXT-VERSIONING-RESEARCH.md)
distinguish the measured classifier speed from the unverified full-file import target.

See [versioning conformance](VERSIONING-CONFORMANCE.md) for evidence against the mathematical design: the current system implements centralized semantic version control, not the full distributed/CRDT roadmap. See [admin and search setup](SEARCH-AND-ADMIN-SETUP.md) for access and indexing steps.

See [format compatibility](research-compatibility.md) for specific import/export limits. No unsupported tool should be represented as a working live integration.

The product expansion adds replayable onboarding, larger typography, research-cited explanations, features/help/roadmap pages and integrated concurrent SEO/IndexNow work. See [product workflows](PRODUCT-WORKFLOWS.md) for permissions, public sharing and exact limits. Automated accessibility checks are evidence for the tested views, not certification of complete WCAG conformance.

## Office workspace and open engine increment

[Decision 006](decisions/006-editable-pages-and-open-engine.md) prioritizes the
owner's Word-style workspace and editable pagination. The workspace now uses an
icon rail, template strip and recent-file rows. The editor uses compact icon ribbon
groups, live page layout and a status/zoom bar. Numeric conditional formatting and
bounded worker-based XLSX imports extend spreadsheet functionality. This increment
is local application work until a separate runtime deployment is recorded.

The [standalone engine](https://github.com/ArefinAlter/dynodoc-engine) is public under
MIT, expressly authorized by the copyright owner. It includes independent builds,
tests and community conventions. The application remains AGPL and retains its
compatible engine copy; [dependency-boundary migration](ENGINE-EXTRACTION.md) remains
open. See [verification and exact limits](EDITABLE-OFFICE-RELEASE.md).
