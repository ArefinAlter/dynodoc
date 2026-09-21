# Dynodoc: intent, current implementation and remaining work

Assessment date: 21 September 2026. This distinguishes executable functionality from the longer design documents. The old README's “feature-complete” description was too broad.

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
| Document editing | Document tabs, common ribbons, paragraph styles/direction/spacing, shared font uploads, text tracking with accept/reject, threaded comments with resolve/reopen, table/context actions, equations, versioned page setup and paginated print preview. Native DOCX import runs in a bounded worker; LaTeX/Overleaf interchange remains. |
| Spreadsheet editing | Typed cells, ranges, relative formula fill, persistent calculation graph, formatting, validation subset, merges, freeze/hide/resize, sheet lifecycle, find/replace/filter, values-only sorting and styled XLSX exchange. |
| Version workflows | Personal drafts, selected-block sharing, get latest, conflict choices, named versions, comparisons, attribution and restoration as new events. |
| Research files | DOCX/DOC, CSV/XLSX, XLSForm and REDCap dictionary interchange; original files retained. |
| Research connections | ODK/Kobo form and response imports; REDCap metadata/records, SurveyCTO wide responses and SurveyMonkey bulk responses. Google Forms adds separate read-only OAuth definition/response imports. Live study-account acceptance remains open. |
| Questionnaire preview | Supported XLSForm relevance, calculations, required rules and constraints; explicit unsupported-logic errors. Answers stay in browser memory. |
| Administration | Allowlisted dashboard with typed confirmation, impact previews, whole-document/account erasure, disable/restore, ownership transfer, forced sign-out, link revocation, CSV export and immutable audit/receipts, database/storage/service/backup status, anonymous page loads and web vitals. |
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

- Broader document features: live editable pagination, tracked moves/formatting and structural revisions, footnote/endnote bodies, citations/bibliographies, section-specific layouts and exact Office round trips.
- Broader spreadsheet features: charts, pivots, formula-based validation, protected ranges, advanced sort behavior with formulas, Excel formula compatibility and high-fidelity workbook formatting.
- Survey execution beyond the supported preview subset: repeat instances, translations/media workflows, external datasets, advanced XPath/date functions and vendor-specific expression engines.
- Research-platform synchronization: scheduled refresh, incremental reconciliation and writeback. All six implemented API import paths need real study-account verification. CSPro dictionary/form/logic support is not implemented; exported CSV/XLSX data can be edited.
- Stronger review: mentions and notification delivery, richer tracked suggestions, per-block/protected-range permissions, notifications and shared review of another person's draft.
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
