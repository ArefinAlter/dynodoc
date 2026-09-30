# Dynodoc: intent, current implementation and remaining work

Assessment date: 22 September 2026. This distinguishes executable functionality from the longer design documents. The old README's “feature-complete” description was too broad.

## Continuation: production reset and future directions (30 September)

The owner decided to permanently delete every document in production while keeping
all accounts and sign-ins; it was done through the audited erasure path after a
validated backup ([operations record](operations/2026-09-30-document-reset.md)).
Production now has no documents and no workspaces. Planned next work is documented,
not built: a GitHub-style [Projects workspace](proposals/PROJECTS-WORKSPACE-HANDOFF.md),
[agents that edit documents with provenance](proposals/AGENT-PROVENANCE-PROPOSAL.md) and
[assessing how students use AI](proposals/AI-ASSESSMENT-PROPOSAL.md); see
[proposals/README.md](proposals/README.md) for the index and research.

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

The original immediate product was a questionnaire authoring tool. The owner's expanded brief adds document and spreadsheet editing and a hosted ten-person pilot. The public-consultation design in `05-public-consultation-part1.md` describes a later product: large-scale citizen input, moderation, identity tiers, deliberation and synthesis. Those services are not implemented by the current researcher workspace.

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

## Remaining product and engineering work

- Broader document features: pagination refinements (oversized objects, repeated table headers, widow/orphan control), tracked moves/formatting and structural revisions, per-page footnote layout and rich notes, citations/bibliographies, section-specific layouts and exact Office round trips.
- Broader spreadsheet features: advanced charts, native pivot caches, structured table formulas, full formula-based validation, protected cell ranges/passwords, full Excel formula/date/locale compatibility and higher-fidelity workbook formatting.
- Survey execution beyond the supported preview subset: repeat instances, translations/media workflows, external datasets, advanced XPath/date functions and vendor-specific expression engines.
- Research-platform synchronization: scheduled refresh, incremental reconciliation and writeback. All six implemented API import paths need real study-account verification. CSPro dictionary/form/logic support is not implemented; exported CSV/XLSX data can be edited.
- Stronger review: e-mail delivery of notifications and mentions, richer tracked suggestions, per-block/protected-range permissions and shared review of another person's draft. In-app notifications, approvals, per-change comments and review rules shipped in decision 009.
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
