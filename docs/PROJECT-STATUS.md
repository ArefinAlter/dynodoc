# Dynodoc: intent, current implementation and remaining work

Assessment date: 19 September 2026. This distinguishes executable functionality from the longer design documents. The old README's “feature-complete” description was too broad.

## What this project is for

Dynodoc helps a research team keep documents, spreadsheets and questionnaires together with an understandable record of changes. A paragraph, question, choice or spreadsheet row has a permanent identity. Edits are recorded as semantic events, attributed to their author and chained by hashes. Researchers can work in personal drafts, compare with the team version, resolve overlapping edits, include selected changes and restore previous content without deleting history.

The original immediate product was a questionnaire authoring tool. The owner's expanded brief adds document and spreadsheet editing and a hosted ten-person pilot. The public-consultation design in `05-public-consultation-part1.md` describes a later product: large-scale citizen input, moderation, identity tiers, deliberation and synthesis. Those services are not implemented by the current researcher workspace.

## Implemented researcher workflows

| Area | Current behavior |
|---|---|
| Core | Append-only event log, hash verification, stable IDs, snapshots, role checks and serialized writes. |
| Privacy | Document reads, audit endpoints, event streams and search require membership. |
| Sessions | Encrypted HttpOnly cookies, same-origin API gateway, refresh rotation; Google authorization-code login and Resend email-link implementation. |
| Homepage | Responsive one-page site, interactive workflow explanation, reduced-motion behavior, Node and Vercel builds. |
| Workspace | Create documents, spreadsheets and questionnaires; upload originals, see conversion reports, search content, share roles and make copies. |
| Document editing | Common text and paragraph formatting, lists, alignment, links, URL images, tables, find/replace, undo/redo, comments and paragraph suggestions. |
| Spreadsheet editing | Cell editing, ranges, paste, formatting, formulas, row/column operations, sheet tabs, find/filter, values-only sorting and CSV/XLSX export. |
| Version workflows | Personal drafts, selected-block sharing, get latest, conflict choices, named versions, comparisons, attribution and restoration as new events. |
| Research files | DOCX/DOC, CSV/XLSX, XLSForm and REDCap dictionary interchange; original files retained. |
| Research connections | ODK Central/Kobo project discovery and API imports of XLSForm sources and response snapshots; token used for the current connection only. Study-account verification remains open. |
| Deployment | Isolated VPS containers and networks, HTTPS, private database, constrained Word converter, database backup and restore procedure. |

There is no defensible single “percent complete” for both the pilot and the full platform. The researcher pilot has working end-to-end paths; the full product requested is still incomplete. In particular, having common editor controls does not establish parity with every Google Docs or Google Sheets tool.

The narrower [pilot checklist](decisions/001-research-team-demo.md) currently has
10 of 14 criteria implemented. The four open criteria include external setup and
validation as well as unimplemented survey logic; this count is not an estimate
of the engineering effort left for the full product.

## Owner configuration still needed

- Configure Google OAuth and Resend credentials, then complete real provider login and email delivery tests. [Instructions](GOOGLE-AND-EMAIL-SETUP.md).
- Deploy the homepage in the owner's Vercel account. [Instructions](VERCEL-DEPLOYMENT.md).
- Add the ten actual researchers and run a study-specific acceptance session using their files. The automated concurrency exercise uses synthetic researchers.

## Remaining product and engineering work

- Broader document features: tracked-edit fidelity, footnotes, citations/bibliographies, headers/footers, page layout and high-fidelity image/document round trips.
- Broader spreadsheet features: charts, pivots, data validation, protected ranges, advanced sort behavior with formulas, Excel formula compatibility and high-fidelity workbook formatting.
- Survey execution: evaluate and validate relevance, constraints, calculations, repeat groups, translations and media capture in respondent preview. Current preview shows controls and explicitly discloses this limit.
- Research-platform synchronization: ODK/Kobo scheduled refresh, incremental reconciliation and writeback; REDCap, SurveyCTO and SurveyMonkey API connections. The implemented ODK/Kobo API import needs real study-account verification. CSPro dictionary/form/logic support is not implemented; exported CSV/XLSX data can be edited.
- Stronger review: comments with replies/resolution/mentions, richer tracked suggestions, granular permissions, notifications and shared review of another person's draft.
- Versioning refinements: offline browser recovery, persistent exclusion rules, branch-to-branch review, more detailed side-by-side rich-content comparison, metadata/permission audit events and long-lived retry/idempotency handling for every new workspace endpoint.
- Production operation: off-site backups, monitoring/alerts, recovery objectives, resource/load tests beyond ten synthetic concurrent edits, complete keyboard/screen-reader audit and an external security review.
- Public consultation stages 22–31, civic identity, moderation, citizen frontend, deliberation and ML services remain future work. The CRDT live-typing layer is also deferred; overlapping block edits require review instead of character-level co-editing.

## Verification evidence

The development verification log is recorded in `SETUP.md`. Automated tests cover membership isolation, atomic batches, stale writes, ten simultaneous independent edits, draft revisions/conflicts, stable-ID restoration and hash integrity. Browser tests exercise real local API sessions, documents, spreadsheets, sharing, version history, draft merging and file imports. Google/Resend provider tests cannot be marked complete until the owner's credentials are configured.

See [format compatibility](research-compatibility.md) for specific import/export limits. No unsupported tool should be represented as a working live integration.
