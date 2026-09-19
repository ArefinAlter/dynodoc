# 001 · Hosted research workspace

Date: 2026-09-19. Authority: the project owner's explicit request to expand the PoC.

This decision supersedes the local-only, magic-link-only, and no-import/export
restrictions in 000-initial.md for the researcher product. Existing append-only
events, stable node identities, immutable migrations, and reproducible history remain
mandatory. The initial hosted audience is ten researchers. Existing VPS applications
must be preserved. The owner will deploy the public homepage to Vercel manually,
now using `dynodoc.online`. Google and email are the first login
methods; LinkedIn and GitHub follow later.

## Product contract

Researchers upload, share, edit, review, compare, and publish structured documents,
questionnaires, and tables through familiar document and spreadsheet interfaces.
Common formatting and table tools must perform real operations; unfinished tools
must not be represented as working. Do not claim complete Google Docs/Sheets or
third-party format parity without tested fixtures. Preserve original uploads and
report unsupported conversion features rather than silently losing them.

| Technical operation | Researcher-facing language |
| --- | --- |
| init | New document |
| clone | Make a copy |
| local / remote | Personal draft / Team version |
| stage / unstage | Include in next version / Keep for later |
| commit | Save version |
| push / pull | Share changes / Get latest |
| branch | Create a draft |
| switch / checkout | Open draft / View version |
| merge | Combine drafts |
| conflict | Review overlapping changes |
| blame | Who changed this? |
| diff | Compare versions |
| revert | Restore as a new version |
| grep | Find in workspace |
| ignore | Exclude from shared version |

## Delivery checklist

These are acceptance criteria, not claims of completed work.

- [x] Repair write serialization, optimistic concurrency, client event ordering,
  integrity semantics, and deployed snapshots; add regression tests.
- [x] Private document membership, collaboration roles, sharing, and secure sessions.
- [ ] Google OAuth and email delivery configured and verified using owner accounts.
- [x] One-page homepage with interactive workflow explanation and reduced-motion support.
- [x] Document workspace: create, upload, organize, search, share, duplicate, and archive.
- [x] Document editor: headings, text formatting, lists, links, tables, comments,
  suggestions, find/replace, keyboard shortcuts, and undo/redo.
- [x] Spreadsheet editor: cell/range editing, paste, rows/columns, formatting,
  sorting/filtering, formulas, sheet tabs, and export.
- [x] Named versions and drafts: compare, review selected changes, synchronize,
  merge, resolve conflicts, attribution, and restore without rewriting history.
- [x] DOCX, XLSX, CSV, and legacy DOC uploads with explicit conversion reports.
- [ ] XLSForm interchange for ODK/Kobo/SurveyCTO; validated REDCap and CSPro mappings;
  SurveyMonkey interchange/API compatibility documented against tested capabilities.
- [x] Questionnaire authoring and respondent preview with supported logic validation.
- [x] Isolated VPS deployment, HTTPS, durable storage, backups and restore exercise.
- [x] Vercel homepage build and exact deployment instructions.
- [ ] Ten-user integration exercise, full browser workflows, accessibility and latency checks.

The broader public-consultation roadmap remains recorded in stages 22–31. Complete
the researcher workflows and their acceptance checks before building those layers;
do not label the full platform complete merely because the team demo is deployed.

As of 19 September 2026, eleven of the fourteen pilot criteria above are implemented
within the documented format/editor limits. Three remain open: real provider login and delivery, external research-tool
validation, and complete researcher/accessibility/load acceptance. Google and
Resend credentials are installed; the owner confirmed public Google publishing.
The questionnaire preview executes the documented subset and reports unsupported
constructs; it does not implement the complete runtime of every survey vendor. Checked editor criteria describe common
working controls, not complete Google Docs/Sheets compatibility. See `SETUP.md`
for verification evidence and `../research-compatibility.md` for limitations.

## External configuration

The owner must configure Google OAuth client credentials and an email delivery
account. Credentials belong in deployment environment files or a secret manager,
never source files, chat transcripts copied into docs, or browser bundles. Vercel
deployment is manual by owner request. Use separate ports, Compose project names,
volumes, and Nginx configuration on the existing VPS.
