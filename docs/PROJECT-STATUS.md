# Dynodoc: intent, current implementation and remaining work

## Latest source: bounded checkpoint encoding (10 October 2026)

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
