# Design: installed-host acceptance run plan (Word, Excel, PowerPoint, Docs, Sheets, Slides)

> **Status: design, not implemented.** 14 October 2026. This is a run plan for the
> owner. No installed-host result exists for any of the six add-ins; nothing here
> certifies a host, platform or listing. Part of the
> [remaining-engine-designs index](ENGINE-REMAINING-DESIGNS.md). Item: `E-HOSTS`.
> Derived from [`extensions/ACCEPTANCE.md`](../extensions/ACCEPTANCE.md),
> [NATIVE-CONNECTOR-ACCEPTANCE.md](NATIVE-CONNECTOR-ACCEPTANCE.md),
> [ADDIN-PUBLICATION-AND-ACCEPTANCE.md](ADDIN-PUBLICATION-AND-ACCEPTANCE.md) and the
> [submission checklist](listings/SUBMISSION-CHECKLIST.md).

## 0. Summary

Dynodoc ships six editor add-ins as test packages (three Office task-pane manifests,
three Apps Script editor add-ons). Source tests, manifest validation and simulated
bridges exist; **no real-host run has happened** and the locally installed
PowerPoint 2021 fails the declared API prerequisite
([ADDIN-PUBLICATION-AND-ACCEPTANCE.md](ADDIN-PUBLICATION-AND-ACCEPTANCE.md)). The
roadmap ledger lists "installed-host results supplied by the owner" as the
remaining acceptance for native adapters
([ROADMAP-ACCEPTANCE.md](ROADMAP-ACCEPTANCE.md)).

This document turns the worksheet into an executable programme:

1. a **platform matrix** with the minimum builds each declared API set implies
   (verified against Microsoft's requirement-set tables on 14 October 2026);
2. **environments** and identities to prepare, with versions and channels;
3. **test scripts** with IDs, priorities and automation classes, derived from the
   existing worksheet and extended where the worksheet is silent;
4. **evidence capture** (what, where, how named, how redacted) and a result
   record schema;
5. **defect triage** and a **regression policy**;
6. **exit criteria** for marketplace submission, expressed as checkable conditions;
7. an **automation analysis**: what Playwright and an Office.js probe can do, and
   what only a person on real hardware can;
8. an **effort and schedule** estimate for the owner and for engineering.

### 0.1 Order and dependencies

| Item | Relationship |
|---|---|
| Engine designs in this set | Independent. Add-ins capture and push existing paragraph text, cell values/formulas/date-only values and shape text; relocation, pruning and witnesses do not change connector behaviour. |
| [Large-file acceptance](DESIGN-LARGE-FILE-ACCEPTANCE.md) | Egress fidelity of large exports must pass this plan's host-open checks. |
| Owner decisions | Publisher accounts, support mailbox, adopting legal pages (checklist section 0) are prerequisites of *submission*, not of *testing*. |

Recommended order: dry-run on Office on the web (cheapest, most automatable) ->
Windows desktop -> Mac desktop -> Google editors -> regression and evidence freeze
-> submission.

## 1. Purpose, goals, non-goals, personas, metrics

### 1.1 Problem statement

Manifests that validate and unit tests that pass do not show that a pane loads in a
real host, that sign-in works in the host's embedded browser, that capture survives
close and reopen, or that apply refuses to write over local edits. Microsoft's
certification policy requires add-ins to "work across all platforms that support
methods defined in the Requirements element", so every platform implied by the
declared requirement sets is exposed to certification testing whether or not the
owner exercised it (policy section 1120.3, read 14 October 2026).

### 1.2 Goals

| ID | Goal |
|---|---|
| G1 | A pass/fail/blocked record for each (host, platform, build) the owner intends to list. |
| G2 | A defect list with severity and reproduction, sufficient for engineering to fix. |
| G3 | A supported-platform statement for listings that is exactly the passed set. |
| G4 | Evidence that survives the run (what was installed, tested, observed). |
| G5 | The least owner effort that still produces honest evidence: automate what is automatable, script what is not. |

### 1.3 Non-goals

- Running Microsoft or Google certification (store reviewers do that).
- Performance or load testing (see large-file acceptance).
- Penetration testing of the pane or sign-in flow (separate security review).
- Testing unsupported structural editing; the add-ins explicitly stop for it and the
  test verifies that stop.
- Validating localized UI.

### 1.4 Personas

| Persona | Role in the run |
|---|---|
| Owner-tester | executes scripts on real devices; records results |
| Second tester | a second identity on a second copy; may be the owner on another machine |
| Engineer | fixes defects, supplies builds, maintains automation |
| Store reviewer (later) | follows the reviewer script in `docs/listings/<host>.md`; the run should have executed that script |

### 1.5 Success metrics

| Metric | Target |
|---|---|
| P0 cases passing on every platform in the listing | 100 % |
| Open S0/S1 defects at submission | 0 |
| Platforms claimed that were not exercised | 0 |
| Evidence completeness (every executed case has a result record and artifacts) | 100 % |
| Owner time for a full first pass | within the estimate in section 12 |

## 2. Current state against target

| Capability | Tag | Evidence / path |
|---|---|---|
| Three Office manifests (version 1.1.0.0) and three Apps Script packages | EXISTS | `extensions/word|excel|powerpoint/manifest.xml`, `extensions/google-docs|sheets|slides/` |
| Worksheet with 13 cases and a run record template | EXISTS | `extensions/ACCEPTANCE.md` |
| Native acceptance checklist (common workflow, format checks, date-only matrix) | EXISTS | `docs/NATIVE-CONNECTOR-ACCEPTANCE.md` |
| Submission checklist and per-host listing copy and reviewer scripts | EXISTS | `docs/listings/` |
| Offline invariants for manifests, scopes, assets | EXISTS | `frontend/researcher/src/lib/connectors/*.test.ts` |
| Manifest validation (online) | EXISTS | `pnpm validate:manifests` (reported valid 12 October 2026) |
| Simulated Office/Google bridges in unit and browser tests | EXISTS | `frontend/researcher/src/lib/provenance/*` tests (development evidence only) |
| Real-host results for any host or platform | MISSING | [ADDIN-PUBLICATION-AND-ACCEPTANCE.md](ADDIN-PUBLICATION-AND-ACCEPTANCE.md) acceptance matrix: "Not run" |
| PowerPoint-capable build available to the owner | MISSING | installed 2021 volume build `16.0.14326.20348` lacks PowerPointApi 1.4 |
| Mac and iPad environments | MISSING | not recorded |
| Second Microsoft test tenant, Google test organization | MISSING | not recorded |
| Run records, evidence layout, defect tracker | MISSING | worksheet record is a text block |
| Automation of any real-host step | MISSING | none |
| Acceptance probe (test-only manifest) | MISSING | none |

## 3. Platform matrix

### 3.1 Minimum builds implied by the declared API sets

From Microsoft's requirement-set tables (pages dated December 2025 to September
2026, read 14 October 2026). "Retail/365" means Microsoft 365 subscription or retail
perpetual; "Volume" means volume-licensed perpetual/LTSC.

| Host | Declared set | Web | Windows retail/365 (minimum) | Windows volume (minimum) | Mac | iPad |
|---|---|---|---|---|---|---|
| Word | WordApi 1.3 | supported | 1612 (7668.1000) | Office 2019: 1612 (7668.1000) | 15.32 (17030901) | 2.22 |
| Excel | ExcelApi 1.13 | supported | 2102 (13801.20738) | Office 2021: 2102 (13801.20738) | 16.50 (21061301) | 16.50 |
| PowerPoint | PowerPointApi 1.4 | supported | 2207 (15330.20122) | Office 2024: 2207 (15330.20122); **Office 2021 is not listed** | 16.62 (22061100) | **not available** |

Consequences:

1. The owner's installed Office 2021 volume build `16.0.14326.20348` is above the
   Excel 1.13 minimum (13801) and above the Word 1.3 minimum, but is **not** listed
   for PowerPointApi 1.4, which needs Office 2024 volume (2207, 15330.20122) or a
   Microsoft 365 build (matches the repository's finding).
2. Word and Excel declare sets that iPad supports. Under policy 1120.3 the store
   tests "all platforms that support methods defined in the Requirements element".
   Either iPad is exercised, or the manifest requirement is raised or restricted so
   iPad is excluded, or the risk is accepted. Raising the requirement set to exclude
   iPad also excludes older desktop builds; the trade-off is a product decision
   (open question IH-Q1). How to restrict form factors in the add-in-only manifest
   was not verified in this pass: unverified, check at build time.
3. The floor builds are old. A practical *claim* is "tested on build X and later in
   the same channel"; claim only what was tested.

### 3.2 Hosts and browsers

| Surface | Webview or browser | Source |
|---|---|---|
| Office on Windows | Microsoft Edge (Chromium) WebView2 | Microsoft "Browsers and webview controls used by Office Add-ins", updated 19 August 2026 |
| Office on Mac | Safari with WKWebView | same |
| Office on iPad | Safari with WKWebView | same |
| Office on the web | the browser used to open Office; Microsoft says add-ins are tested with Edge (Chromium), Chrome and Safari; policy 1120.3 also names Firefox | "Test Office Add-ins", updated 17 June 2026; certification policies |
| Google Docs/Sheets/Slides add-on sidebar | Google-hosted sidebar iframe inside the editor in the user's browser | Google editor add-on docs |

### 3.3 Planned environment set

| ID | Environment | Hosts | Purpose |
|---|---|---|---|
| E1 | Windows 11, Microsoft 365 Apps, Current Channel | Word, Excel, PowerPoint | primary desktop path |
| E2 | Windows 11, Microsoft 365 Apps, Monthly Enterprise Channel (or Semi-Annual) | Word, Excel, PowerPoint | lagging build; surfaces API availability lag |
| E3 | Windows, Office LTSC 2021 (volume) | Word, Excel | oldest supported volume perpetual; PowerPoint expected to show the unsupported-host response (negative test) |
| E4 | Windows, Office LTSC 2024 (volume) | Word, Excel, PowerPoint | oldest PowerPoint-capable volume build |
| E5 | macOS (current) with Office for Mac (current) | Word, Excel, PowerPoint | Safari WKWebView path |
| E6 | macOS (previous major) with Office for Mac (supported build) | Word, Excel, PowerPoint | only if a Mac claim older than current is wanted |
| E7 | Office on the web in Edge | Word, Excel, PowerPoint | most automatable |
| E8 | Office on the web in Chrome | same | |
| E9 | Office on the web in Safari (on Mac) | same | |
| E10 | Office on the web in Firefox | same | policy names Firefox; confirm basic load only (P1) |
| E11 | iPad with current Office for iPad | Word, Excel | only if iPad is listed or a decision requires evidence |
| G1 | Google Docs/Sheets/Slides in Chrome (desktop) | Docs, Sheets, Slides | primary |
| G2 | Google editors in Edge (Chromium) | same | |
| G3 | Google editors in Safari (macOS) | same | |
| G4 | Google editors in Firefox | same | |

Count: Word 9-10 environments, Excel 9-10, PowerPoint 7-8 (E3 negative), Google 3 x
up to 4. A minimal credible first listing is E1, E4 (PowerPoint), E5, E7 or E8, and
G1 (about 15 combinations); each added environment is a claim to be paid for.

### 3.4 Identities and tenants

| Item | Requirement |
|---|---|
| Dynodoc accounts | two (owner, editor); each needs a mailbox the tester can read (sign-in by email link) or Google sign-in; a third "reviewer" mailbox for dry-running the reviewer script |
| Microsoft sign-in to Office | any account entitled to the Office edition under test; **keep separate from Dynodoc identities** (the add-ins do not use Microsoft SSO) |
| Microsoft 365 tenant for admin-deployment test | one test tenant with Integrated apps access for the organization-distribution test (P1) |
| Google accounts | two consumer or Workspace accounts; a Workspace organization only for the private-listing test (P2) |
| Test data | synthetic files from the worksheet; no real project data |

## 4. Test scripts

### 4.1 Conventions

- **Case ID**: `AC-nn` (worksheet), `NC-n` (native acceptance), `FS-W/X/P` (format
  specific), `DM-n` (date matrix), `X-nn` (proposed additions).
- **Priority**: P0 blocks submission for that platform; P1 should pass or have a
  documented limitation; P2 informational.
- **Automation class**: **A** automatable without an installed host (already
  exists or can run in CI); **B** automatable against a real host with prepared
  credentials and a dedicated runner; **C** manual only.
- **Result**: `pass`, `fail`, `blocked`, `not run` (worksheet vocabulary). A failed
  or blocked case remains open.
- **Hosts**: W = Word, X = Excel, P = PowerPoint, D = Docs, S = Sheets, L = Slides.

### 4.2 Master case list

| ID | Title | Hosts | Priority | Class |
|---|---|---|---|---|
| AC-01 | Install and load; store-reviewer naming and wording; required API detection | W X P D S L | P0 | C (B on web) |
| AC-02 | Authorize: pair through browser approval; comparison code; cancellation; expiry | all | P0 | C (B on web) |
| AC-03 | Capture supported work and push; remote diff contains intended change only | all | P0 | B/C |
| AC-04 | Review and pull; preview and apply in second copy; save, reopen, compare | all | P0 | B/C |
| AC-05 | Independent edits in two copies; comparison | all | P0 | B/C |
| AC-06 | Conflicting edits stay visible; chosen result and attribution after merge | all | P0 | B/C |
| AC-07 | Stale application stops; local work preserved | all | P0 | B/C |
| AC-08 | Retry and reconnect: no duplicate canonical change, no lost pending work | all | P0 | C |
| AC-09 | Restart: save, close, reopen file and pane; binding and queue recovery; Office re-authorization | all | P0 | C |
| AC-10 | Revoke access; stop without deleting recoverable local edits; owner-only merge check | all | P0 | B/C |
| AC-11 | Unsupported change produces a clear stop or the documented file workflow | all | P0 | C |
| AC-12 | Historical recovery proposal through Projects, review, apply to a disposable copy | all | P1 | B/C |
| AC-13 | Clean up test registrations and fixtures only | all | P0 | C |
| NC-1 | Wrong-file and wrong-host key fails | all | P0 | B |
| NC-2 | Capture disabled by default; enabling; queue survives close/reopen; stable IDs retained | all | P0 | C |
| NC-3 | Push during interruption; export sidecar; one immutable receipt after retry; frozen upload intent | all | P0 | C |
| NC-4 | Typing after upload attempt: later observations survive | all | P0 | C |
| NC-5 | Receipt alone does not advance canonical checkpoint or overwrite file | all | P0 | B |
| NC-6 | Same-block edit conflicts; type after preview stops apply | all | P0 | B/C |
| NC-7 | Rename/Save As/copy: connection stops or demands recovery; sidecar restore only into a matching fresh copy | all | P1 | C |
| NC-8 | Revoke key, remove access, force sign-out (separate runs) | all | P0 | B/C |
| FS-W | Word/Docs: three plain paragraphs; both directions; content-control/named-range IDs; lists/tables/new/moved paragraphs require file push | W D | P0 | C |
| FS-X | Excel/Sheets: two sheets, scalar types, blank, `=A1*2`, date-only; merged range stops writes; new rows/columns require file push | X S | P0 | C |
| FS-P | PowerPoint/Slides: text shapes, image, blank slide; native tags; competing text conflicts; new/moved/grouped shapes require file push | P L | P0 | C |
| DM-1..DM-4 | Date-only matrix: 1900 and 1904 systems; Sheets time zones (Asia/Dhaka, America/New_York, Pacific/Kiritimati); DST; invalid dates stop | X S | P0 | C |
| X-01 | Bengali and mixed RTL text through capture/push/apply (grapheme integrity) | W D P L | P1 | C |
| X-02 | Co-authoring / AutoSave on OneDrive or SharePoint (Office); shared Drive editing (Google) | all | P1 | C |
| X-03 | Protected View, read-only and locked documents | W X P | P1 | C |
| X-04 | Keyboard-only operation and basic screen-reader labelling of the pane | all | P1 | C |
| X-05 | Task pane lifecycle: pin, resize, close/reopen, multiple documents open | W X P | P1 | C |
| X-06 | Manifest version bump re-sideload; cache refresh | W X P | P2 | C |
| X-07 | Network variance: captive proxy / TLS inspection / offline load of the pane | all | P2 | C |
| X-08 | Microsoft 365 admin deployment (Integrated apps) to a test user | W X P | P2 | C |
| X-09 | Google quota boundary: saved-queue store near the 500 KB per-user limit | D S L | P1 | C |
| X-10 | Large-ish file within the 3,000,000-byte cap: pane responsiveness | all | P2 | C |

X-01 to X-10 are proposed additions beyond the existing worksheet.

### 4.3 Host adaptation table

| Step | Word / Docs | Excel / Sheets | PowerPoint / Slides |
|---|---|---|---|
| Fixture | three paragraphs `Acceptance alpha`, `Acceptance beta`, `বাংলা sample` | sheet `Acceptance`: A1 Label, B1 Value, A2 Alpha, B2 10, A3 Beta, B3 20, B4 `=SUM(B2:B3)`; a date-only cell | two slides, text shapes `Acceptance alpha`, `Acceptance beta` |
| "Make a supported change" | alpha -> `Acceptance alpha revised` | B2 -> 11 | first slide's existing text shape -> revised |
| "Independent edit" | edit beta in copy B, alpha in copy A | edit B3 in B, B2 in A | edit slide 2 shape in B, slide 1 shape in A |
| "Conflicting edit" | alpha -> `X` in A and `Y` in B | B2 -> 11 in A and 12 in B | same shape text different |
| "Unsupported change" | insert a table or new paragraph | insert a column, merge cells | add a new shape or group |
| Verify | paragraph text and IDs | cell values, formula source, date value | shape text and recognized IDs |

### 4.4 Detailed scripts (representative)

Each script lists preconditions, steps, expected result and evidence. Host
placeholders use section 4.3.

**AC-01 Install and load (P0)**

| | |
|---|---|
| Preconditions | clean profile or documented prior state; test build deployed to the web server named in the manifest; manifest or Apps Script package under test with its version and hash noted |
| Steps | 1 Install per platform instructions in `extensions/INSTALL.md`. 2 Open the host, locate the ribbon group **Dynodoc** with a **Dynodoc** button (Office) or **Extensions > Dynodoc > Open Dynodoc** (Google). 3 Open the pane. 4 Read the add-in name (**Dynodoc for Word/Excel/PowerPoint**). 5 Check **Sign in to Dynodoc** is the first action and manual key fields are collapsed under **Advanced: connect with a key**. 6 Check no "development" or "test" wording is visible. 7 On a host below the requirement set (E3 PowerPoint), open the pane. |
| Expected | pane loads over HTTPS inside the host; names and wording as in listing copy; below-requirement hosts show a useful unsupported-host response and do not offer capture |
| Evidence | screenshot of ribbon and pane; build string from the host's About dialog; manifest `Version`; browser console log excerpt without secrets |

**AC-02 Authorize (P0)**

| | |
|---|---|
| Steps | 1 Select **Sign in to Dynodoc**; note the comparison code. 2 Open the browser link; sign in as the owner identity; compare the code; choose the intended Project file; approve. 3 Return; **I approved this connection**. 4 Repeat with cancel; repeat letting the request expire. 5 Confirm unrelated projects are not selectable and no access exists before approval. |
| Expected | connection bound to exactly one file; cancel and expiry leave no connection; the sign-in page does not replace the working copy; Office keeps the key in pane memory only |
| Evidence | screenshots at each step; Dynodoc connection list showing one grant; time stamps |

**AC-03 Capture and push (P0)**

| | |
|---|---|
| Steps | 1 Enable capture explicitly with the pane open. 2 Make the supported change. 3 Wait for the saved-observation notice. 4 Inspect the queued change; push for review. 5 Open the remote diff. |
| Expected | the remote diff contains only the intended change; other paragraphs/cells/shapes and Unicode text unchanged; formula source preserved separately from displayed result (Excel/Sheets) |
| Evidence | queue screenshot; remote diff screenshot; sidecar export (redacted) |

**AC-06 Conflicting edits (P0)**

| | |
|---|---|
| Steps | 1 Use two copies bound to the same remote file. 2 Make the conflicting change in both. 3 Push both. 4 Review in Projects; resolve; merge as owner. 5 Pull in both copies. |
| Expected | a conflict is visible and must be resolved; neither local version disappears before resolution; attribution after merge names both authors' contributions as designed |
| Evidence | conflict screenshot; resolution record; final content in both copies |

**AC-07 Stale application (P0)**

| | |
|---|---|
| Steps | preview a remote pull; edit the same local content; try to apply |
| Expected | application stops; local work and previous base preserved; message explains |

**AC-09 Restart (P0)**

| | |
|---|---|
| Steps | save; close file and pane; reopen; observe binding and queue; confirm Office requires sign-in again (secret in pane memory); Google reopens with stored approved key in user properties |
| Expected | binding and queue recover; edits made while the pane was closed are **not** represented as observed capture; any rescan is shown as an unobserved interval |

**AC-10 / NC-8 Revoke (P0)**

| | |
|---|---|
| Steps | owner revokes the connection; old pane attempts preview and push; separately remove membership; separately force sign-out; editor attempts an owner-only merge |
| Expected | access stops in each case; locally exportable observations remain; no silent deletion of recoverable local edits |

**AC-11 / FS-* Unsupported change (P0)**

| | |
|---|---|
| Steps | in a disposable copy make the unsupported change per section 4.3 |
| Expected | a clear unsupported outcome or the documented edited-file workflow; a text-only successful push is **not** evidence of layout/style/object/animation/formula-engine fidelity |

**DM matrix (P0 for X/S)**

Follow `docs/NATIVE-CONNECTOR-ACCEPTANCE.md` "Date-only acceptance matrix" exactly:
workbook date systems 1900 and 1904 (including 1904-01-01), three Sheets time zones
with a different Apps Script time zone, New York DST dates, a zone whose local
midnight is skipped (sync must stop), invalid and ambiguous values must stop sync,
formula source preserved without claiming calculation equality.

**X-04 Accessibility (P1, proposed)**

Keyboard-only: reach every pane control with Tab, activate with Enter/Space,
visible focus, no focus trap; screen reader announces control names and roles
(Narrator on Windows, VoiceOver on Mac); contrast per the site design tokens.
Result is qualitative; log defects with the control and the assistive technology.

### 4.5 Detailed scripts (continued)

**NC-1 Wrong-file and wrong-host keys (P0)**

| | |
|---|---|
| Preconditions | project with a Word file, an Excel file and a PowerPoint file (and Google equivalents); one file-scoped key per file (manual-key route, `Advanced: connect with a key`) |
| Steps | 1 In the Word pane, paste the Excel file's key. 2 Paste the Word file's key for a different Word file. 3 Paste the correct key. 4 Repeat with a Google host key in an Office pane. |
| Expected | each wrong key is refused with an explicit message; no partial binding; the correct key binds; nothing is written to the document |
| Evidence | screenshot of each refusal; connection list shows one grant |

**NC-3 Interrupted push, sidecar, one receipt (P0)**

| | |
|---|---|
| Steps | 1 Enable capture; make a change; open the push dialog. 2 Disconnect the network (airplane mode or disable adapter) and press push. 3 Export the sidecar. 4 Change the request name in the dialog. 5 Reconnect; retry. 6 Check the project for requests. |
| Expected | exactly one immutable request and one receipt; the frozen upload intent is unchanged by the renamed request; exported sidecar parses and contains the observation; no duplicate canonical change |
| Evidence | project request list; sidecar file (redacted); network state screenshots |

**NC-4 / NC-6 Typing during upload and after preview (P0)**

| | |
|---|---|
| Steps | A: start a push, keep typing in the same paragraph/cell/shape during the attempt; after completion inspect the queue. B: preview a remote pull; type; try apply. |
| Expected | A: later observations survive the retry. B: apply stops, queue and previous base retained |

**NC-7 Rename, Save As and copy (P1)**

| | |
|---|---|
| Steps | rename the native file; Save As a copy; open both; attempt capture and push from each; restore a sidecar into (a) a fresh matching copy in the same host and (b) a different file |
| Expected | the original connection stops or demands explicit recovery; sidecar restores only into a matching fresh copy, preserves the historical base and exact retry IDs; an existing queue, foreign IDs or different content are never overwritten; a similarly named unrelated file never attaches automatically |

**FS-W Word / Docs (P0)**

| Check | Steps | Expected |
|---|---|---|
| Both directions | change alpha in Word, apply in Docs; change beta in Docs, apply in Word | text equal on both sides; `বাংলা sample` unchanged |
| IDs | save the DOCX; inspect content-control tags `dynodoc.block.<ULID>` (Word) or named-range IDs (Docs) after reopen | IDs retained |
| Structural | add a new paragraph; add a list; add a table; move a paragraph | each requires the edited-file push workflow, with a clear message |
| Replacement formatting | replace text inside a bold run | behaviour recorded (host may inherit formatting); not claimed as formatting sync |
| Docs tabs | use a document with several tabs | unsupported tabs require file push; the selected tab stays selected throughout the sidebar session |

**FS-X Excel / Sheets (P0)**

| Check | Steps | Expected |
|---|---|---|
| Scalars and blanks | change numbers, text, booleans; blank a cell | values round-trip; blank distinct from zero |
| Formulas | edit `=A1*2` source; compare source and displayed result separately | formula source preserved; no claim that two engines compute equal results |
| Different cells in one row | edit B2 in copy A and B3 in copy B | both survive pull |
| Merged range | merge cells locally | binding/capture/apply stops before any write |
| Structure | add a row, a column, a sheet, sort, paste a block | new rows/columns/sheets require file push; sort/paste may appear as value observations, not as the original command |
| Names | move or rename a named row | stops with a clear message |

**FS-P PowerPoint / Slides (P0)**

| Check | Steps | Expected |
|---|---|---|
| Shape text | edit existing text shapes in both directions | applied; other shapes unchanged |
| IDs | reopen; inspect recognized shape IDs against the pre-edit list | stable IDs or private native-ID mappings, including alt text |
| Competing text | same shape, different text | conflict visible |
| Structural | add, move, group, table, notes, media, theme changes | file push required, clear message |
| Layout | apply native text replacement | layout and formatting effects recorded separately |
| Requirement | open the pane on a host without PowerPointApi 1.4 | unsupported-host response; no capture |

**DM Date-only matrix (P0 for Excel/Sheets), concrete cases**

| Case | Setup | Check |
|---|---|---|
| DM-1 | workbook date system 1900: `2024-02-29`, `2026-10-06`, blank date cell, formula cell; Excel -> review -> Sheets -> review -> Excel | calendar days unchanged; blank stays blank |
| DM-2 | workbook date system 1904: include `1904-01-01` (serial 0) | not confused with blank; day unchanged |
| DM-3 | Sheets time zones Asia/Dhaka, America/New_York, Pacific/Kiritimati with a different Apps Script time zone; New York DST dates | no shift to previous/next day; where local midnight cannot round-trip the connector stops |
| DM-4 | invalid dates, Excel's fictional 1900-02-29, hidden time parts, date-to-text, `06/10/2026` strings, formatting changed to General/text/time/ambiguous masks | sync stops in each; common date-only masks are supported and format changes are not synchronized |
| DM-5 | change a cell or the sheet time zone between preview and apply | apply stops before its first write |
| DM-6 | repeat offline retry, sidecar recovery, separate-cell merge and same-date conflict with date observations | behaves as AC-08, NC-7, AC-05, AC-06 |

**AC-12 Historical recovery (P1)**

| | |
|---|---|
| Steps | from Projects, choose an earlier retained supported revision, download or propose recovery, review it, apply in a disposable copy |
| Expected | recovery appears as new reviewed work; newer history preserved; operations are per file (do not report project-wide branch acceptance) |

### 4.6 Defect record template

```markdown
# DEF-014: <one-line symptom>

- Severity: S1   Class: connector   Status: open
- Found: ACC-2026Q4 / word-mac-<build> / AC-09 / 2026-11-04
- Environment: macOS <version>, Word <build>, manifest 1.1.0.0 (sha256 …), workspace <commit>/schema 40
- Reproduction (numbered, exact):
  1. …
- Expected:
- Observed:
- Frequency: always | n of m runs
- Reproduced on: <second environment or "not yet">
- Evidence: AC-09/04-queue.png, console.txt
- Workaround:
- Fix: <commit>, web build <id>, manifest <version>
- Verified in: <run id / date>
```

### 4.7 Example calendar (illustrative)

| Week | Block |
|---|---|
| 1 | environments, accounts, fixtures; install E7 (web) and G1; run AC-01..AC-04 on web as dry run; decision G-A |
| 2 | Windows E1 and E4: Word, Excel, PowerPoint P0 cases; PowerPoint negative on E3 |
| 3 | Mac E5 and Safari web E9; write up defects; first engineering fix batch |
| 4 | Google Docs, Sheets, Slides on G1; date matrix DM-1..DM-6 on Excel/Sheets |
| 5 | targeted reruns; second browser on Google; X-04, X-02 |
| 6 | full P0 rerun on the release candidate; freeze; decision G-B |
| 7 - 8 | reviewer script by a third person; screenshots; decision G-C and submission preparation |

## 5. Evidence capture

### 5.1 Layout

```
acceptance-evidence/
  ACC-2026Q4/
    run.json                         # environment inventory, build ids, manifest versions/hashes
    word-win-365cc-<build>/
      AC-01/
        result.json
        01-ribbon.png
        02-pane.png
        console.txt                  # redacted
      AC-02/ ...
    excel-mac-<build>/ ...
    defects/
      DEF-001.md
    SHA256SUMS                       # hashes of every file; generated at freeze
```

Kept in a private location (encrypted disk or private repository); **not** in the
public engine repository.

### 5.2 `result.json` (proposed `dynodoc.host-acceptance-result.v1`)

```json
{
  "schema": "dynodoc.host-acceptance-result.v1",
  "run": "ACC-2026Q4",
  "case": "AC-03",
  "host": "word",
  "platform": "windows",
  "channel": "current",
  "host_build": "16.0.0000.00000",
  "webview": "WebView2 0.0.0.0",
  "manifest": { "version": "1.1.0.0", "sha256": "…" },
  "workspace": { "commit": "…", "schema": 40 },
  "identities": ["owner", "editor"],
  "result": "pass",
  "expected": "Remote diff shows only the paragraph change",
  "observed": "As expected",
  "evidence": [ { "file": "03-diff.png", "sha256": "…" } ],
  "defects": [],
  "tester": "initials",
  "started_at": "2026-11-02T09:10:00+06:00",
  "duration_min": 22,
  "cleanup_done": true
}
```

### 5.3 What to capture

| Evidence | How | Redaction |
|---|---|---|
| Host and build | Windows: File > Account > About (Office); Mac: application menu > About; web: the browser version and the Office web build if shown; Google: browser version and Apps Script version | none |
| Manifest/package identity | `Version` in manifest or Apps Script version; SHA-256 of the file | none |
| Pane state | screenshot | blur email addresses, pairing codes and URLs |
| Console and network | Windows: Edge DevTools via the add-in debugging route; Mac: Safari Web Inspector; web: browser DevTools; attach-debugger-from-task-pane for staging builds (Microsoft documents these routes) | strip tokens, keys, file IDs if sensitive; never paste connection keys |
| Server-side corroboration | request IDs, project activity entries, connection records from the Dynodoc admin/activity API | remove email addresses |
| Screen recording | optional for hard-to-describe flows; required for the first OAuth verification video later | keep private |

Never store credentials, connection keys, pairing URLs or codes, email addresses or
private files (worksheet rule).

### 5.4 Freeze

At the end of the run: lock the directory, generate `SHA256SUMS`, sign the checksum
file with a detached Ed25519 signature (for example OpenSSL with the owner's
key) and record the freeze date in `run.json`. Evidence
after a freeze is a new run.

## 6. Defect triage

### 6.1 Severity

| Severity | Definition | Examples |
|---|---|---|
| S0 | unauthorized access, credential or document-content exposure, or corruption/loss of the user's native document | pane writes over unsaved local edits; key written to a shared location |
| S1 | a core flow fails or silently produces wrong content | cannot sign in; capture drops edits; apply writes stale state; revocation does not stop access |
| S2 | degraded or misleading but with a workaround | unsupported change not detected until push; unclear error; Office requires sign-in more often than documented |
| S3 | cosmetic or copy | wording, spacing, icon |

### 6.2 Classification and flow

```
 observe -> reproduce on a second environment -> classify -> fix -> verify -> close
```

| Class | Owner | Typical fix |
|---|---|---|
| Host bug or documented host limitation | listing text and workaround | document; narrow platform claim |
| Connector bug (pane/Apps Script) | engineering | code fix; new manifest/script version |
| Server bug (API) | engineering | API fix; deploy web build first (manifests load the live pane) |
| Fixture or script bug | owner | correct the script |
| Environment bug | owner | rebuild environment |

### 6.3 Regression policy

| Change | Re-run |
|---|---|
| Fix in pane code with web deploy | all P0 cases on one Office web environment, plus the failing case on every platform where it failed |
| Manifest change (version bump) | AC-01, AC-02, AC-09 on every Office platform |
| Apps Script change (new versioned deployment) | AC-01, AC-02, AC-03, AC-09 on each Google host |
| API change affecting connectors | NC-1..NC-8 on one platform per host family |
| Host update during the run | record the new build; re-run AC-01..AC-03 before continuing |

Panes update the moment the web build deploys, while an installed Google script
stays on its published version until updated; compatibility with every published
script version must be kept ([checklist](listings/SUBMISSION-CHECKLIST.md) section 4).

## 7. Exit criteria for marketplace submission

All must be true and recorded.

| # | Criterion | Check |
|---|---|---|
| 1 | Every listed (host, platform) has a frozen run with all P0 cases `pass` | evidence index |
| 2 | Zero open S0 or S1 defects; each open S2 has a documented workaround that appears in the listing or support page | defect list |
| 3 | The supported-platform text in each listing equals the set of passed platforms (no more) | diff of listing copy against run matrix |
| 4 | Platforms the validator lists as eligible but not exercised are either excluded by manifest or explicitly accepted as certification risk in writing | decision record (IH-Q1) |
| 5 | `pnpm validate:manifests` passes for all three Office manifests and Partner Center validation passes at upload | log |
| 6 | PowerPoint listed only for builds with PowerPointApi 1.4 | matrix |
| 7 | Store screenshots are real captures from the installed hosts of the passed platforms | files |
| 8 | Reviewer script in `docs/listings/<host>.md` executed end to end by someone who did not write it, with a reviewer mailbox or Google sign-in | record |
| 9 | Privacy policy and terms adopted and consistent with `docs/listings/addin-data-handling.md`; support mailbox confirmed monitored | owner |
| 10 | Google: standard Cloud project, consent screen scopes equal manifest scopes, OAuth verification submitted or granted, versioned deployment id recorded | console |
| 11 | No unsupported claims (checklist section 6): no "approved", no background capture, no formatting sync, no proof of authorship | text review |

Three decision gates:

| Gate | When | Decision |
|---|---|---|
| G-A | after Office-on-the-web dry run | continue to desktops, or fix first |
| G-B | after desktop and Mac passes | fix the platform set to list |
| G-C | before submission | go/no-go against the 11 criteria |

## 8. Automation analysis

### 8.1 What exists

| Layer | Mechanism | Real host? |
|---|---|---|
| Manifest/scope/icon invariants | vitest suites in `src/lib/connectors/` | no |
| Protocol and bridge logic | unit tests with simulated bridges (the add-in test mock library `office-addin-mock` is Microsoft's tool for mocking Office.js objects in Node tests) | no |
| API behaviour | Rust integration tests; Playwright against a local stack | no |

Microsoft's documentation states the Office JavaScript APIs "must initialize in a
webview control in the context of an Office application", so they cannot load in a
unit-test process; the mock library emulates `load`/`sync` semantics. Mock-based
tests are not host acceptance.

### 8.2 What can be automated with real hosts (class B)

| Target | Approach | Feasibility | Notes |
|---|---|---|---|
| Office on the web (Edge/Chrome) | Playwright drives the browser: sign in to a dedicated test account, open the fixture, sideload with *Upload My Add-in*, open the pane iframe, interact with it | medium | Microsoft sign-in may require MFA or Conditional Access; use a dedicated test tenant account with relaxed policy. Reliability of selectors across Office UI updates is poor; budget maintenance |
| Native document oracle | a **test-only probe** manifest (separate `<Id>`, separate path on the staging origin, never in the production manifest) exposing `readParagraphs`, `setParagraph`, `readCells`, `setCell`, `listShapes`, `setShapeText`, `isSetSupported`; automation calls them through the pane frame | medium; needs a spike | gives deterministic local edits and readback without UI typing; probe code must not ship in the production pane |
| Windows desktop | UI Automation (for example FlaUI or pywinauto) to launch Office, open the file and invoke the ribbon; WebView2 debugging to reach the pane DOM | low to medium | Microsoft documents Edge DevTools for WebView2 add-in debugging; attaching Playwright over the Chrome DevTools Protocol to the Office WebView2 instance is plausible but **not verified here**: spike required |
| Mac desktop | AppleScript/Accessibility, Safari Web Inspector (manual) | low | WKWebView has no Chrome DevTools Protocol; treat as manual |
| Google editors | Playwright with a persisted, pre-authenticated browser profile; Apps Script test deployment to open the add-on; sidebar iframe interaction | low | Google's sign-in detects automation in many configurations; practitioners reuse a logged-in profile or attach to a normally launched Chrome. Treat as fragile and run only on a dedicated runner |
| Server-side oracle | after each manual step, a script asserts API state (requests, receipts, connection records) | high | reduces human judgement and evidence effort |

### 8.3 What cannot be automated

| Item | Why |
|---|---|
| Mac desktop and iPad flows | no supported automation path to the pane; UI variance |
| Visual fidelity of the pane and of native content | judgement; screenshots only |
| Accessibility with a screen reader | human assessment |
| Microsoft certification and Google Marketplace/OAuth review | performed by the vendors |
| Real co-authoring behaviour (X-02) | needs multiple real clients and cloud storage timing |
| Office/Google update-channel behaviour over time | observed, not scripted |
| MFA-protected business accounts | cannot be automated without weakening the account |
| Store screenshots | must be real captures |

### 8.4 Proposed automation deliverables (design only)

| ID | Deliverable | Effort | Value |
|---|---|---|---|
| H-A1 | **Acceptance recorder** CLI in the application repository: prints the next script step, records results to `result.json`, hashes evidence, runs server-side assertions | 2 - 3 pw | halves owner bookkeeping; consistent evidence |
| H-A2 | **Probe manifests** (staging only) and probe page | 1.5 - 2 pw | enables class B and deterministic edits |
| H-A3 | **Office-on-the-web smoke** (AC-01..AC-04, NC-1) with Playwright and the probe | 2 - 3 pw | daily regression of the pane in a real Office web host |
| H-A4 | Windows desktop spike (UIA + WebView2 attach) | 1 - 2 pw | decide feasibility |
| H-A5 | Google smoke with persisted profile | 1.5 - 2 pw | fragile; optional |
| Total | | 8 - 12 | |

Automation reduces repeated effort across regressions; it does not replace the
first manual pass on each platform.

## 9. Architecture of the run

```
  +-------------------+     +---------------------+     +-------------------+
  | build + deploy    | --> | environments E1..G4 | --> | scripts (AC/NC/..)|
  | web, manifests,   |     | identities, fixtures|     | owner/automation  |
  | Apps Script ver.  |     +---------------------+     +---------+---------+
  +-------------------+                                           |
                                                                  v
                  +-----------------------+            +-------------------+
                  | defects (S0..S3)      | <--------- | results + evidence|
                  +----------+------------+            | (result.json,     |
                             |                         |  artifacts, sums) |
                             v                         +---------+---------+
                  fix -> redeploy -> targeted re-run              |
                                                                  v
                                                        exit criteria + gates
                                                                  |
                                                                  v
                                            submission kit (docs/listings/...)
```

Trust boundaries: the run touches real accounts and real hosts; Dynodoc test
accounts must hold only synthetic data; evidence stores must not be public.

## 10. Security and privacy

| # | Risk | Control |
|---|---|---|
| T1 | Secrets in evidence (connection keys, pairing codes, tokens) | redaction checklist; recorder refuses files with patterns of known secret formats; review before freeze |
| T2 | Sideloaded test manifests left installed | AC-13 cleanup; inventory of registrations in `run.json`; staging origin separate from production |
| T3 | Probe manifest exposed in production | separate `<Id>` and path; CI check that production manifests contain no probe URLs; probe endpoints disabled on production origin |
| T4 | Personal accounts or tenants mixed with test data | dedicated accounts; no real documents |
| T5 | Tenant admin test (X-08) alters a real organization | use only a disposable tenant |
| T6 | Screenshots reveal personal information | synthetic users (Maya Chen, Sam Okoye, Priya Nair per checklist section 5) |
| T7 | Evidence repository leak | private repository or encrypted disk; access list |
| T8 | Test with real customer file | not allowed; synthetic only |

Not claimed: that a pass on one build implies later builds; that a pass on a
platform implies another channel; that mock-based tests substitute.

## 11. Operations

| Topic | Design |
|---|---|
| Environment lifecycle | create from checklist; snapshot VMs after clean install; destroy after freeze; record patches applied |
| Build freeze | the web build and manifests under test are tagged; no deploys during a case unless the case is restarted |
| Scheduling | quarterly re-run of P0 on E1/E5/E7/G1, and after any connector or manifest release |
| Observability | server-side connection and request logs correlated by connection ID; request IDs noted in results |
| Failure modes of the run | sign-in blocked by account policy (record `blocked`); host auto-updates mid-run (re-record build, re-run basics); corporate proxy interference; time zone mistakes in DM cases; fixture drift |

## 12. Delivery plan

### 12.1 Owner effort (first full pass)

Assumptions: two identities, two machines or two copies, 13 core cases + 8 native
workflow items + format checks per (host, platform), about 3.5 to 5 hours per
combination including setup of the copies and evidence; reruns about 40 % of that.

| Block | Combinations | Hours |
|---|---:|---:|
| Word: E1, E3, E4 (or E2), E5, E7, E8, (E9) | 6 - 7 | 21 - 35 |
| Excel: same | 6 - 7 | 21 - 35 |
| PowerPoint: E1, E4, E5, E7 (+E3 negative) | 4 - 5 | 14 - 25 |
| Google: Docs, Sheets, Slides on G1 (+G2 or G3) | 3 - 6 | 11 - 30 |
| Setup (accounts, tenants, machines, fixtures, manifests) | | 24 - 40 |
| Defect write-ups and verification | | 16 - 32 |
| Total first pass | | about 107 - 197 h, i.e. 14 - 25 working days |
| Targeted reruns after fixes (two cycles) | | 12 - 20 working days |

Calendar: 6 - 10 weeks including fix cycles. These are planning figures, not
measurements; first runs will correct them.

### 12.2 Engineering effort

| Item | Effort (person-weeks) |
|---|---|
| Defect fixing | not estimated: depends on findings; budget 4 - 8 pw as a placeholder for planning |
| Recorder, probe, web smoke (H-A1..H-A3) | 5.5 - 8 |
| Windows spike and Google smoke (H-A4, H-A5) | 2.5 - 4 |
| Docs and script maintenance | 1 |

**Excludes**: store account setup, legal text adoption, OAuth verification effort,
translation, performance work, security review.

### 12.3 Phases

| Phase | Content | Exit |
|---|---|---|
| H0 | Prepare: environments E1, E7, G1; accounts; fixtures; recorder (optional); probe (optional) | environment inventory in `run.json` |
| H1 | Dry run on Office on the web (Word, Excel, PowerPoint) | G-A decision |
| H2 | Windows desktop (E1, E4; E3 negative; E2 optional) | defect list |
| H3 | Mac (E5) and Safari web (E9) | defect list |
| H4 | Google (G1, plus second browser) | defect list |
| H5 | Fix cycles and targeted reruns | zero S0/S1 |
| H6 | Full P0 re-run on the final build; freeze evidence | G-B, G-C |
| H7 | Submission (owner), reviewer script execution | records |

### 12.4 Rollout and migration

No product migration. The plan changes documentation status: after each frozen
run update `ADDIN-PUBLICATION-AND-ACCEPTANCE.md` (acceptance matrix) and the
project status pages with exact builds and dates, and never mark a host accepted
without a frozen run.

## 13. Risks and open questions

### 13.1 Risks

| Risk | Mitigation |
|---|---|
| Owner lacks Mac or recent Windows build | decide platform claims accordingly; cloud Mac rental is an option (provider and terms unverified: check at build time) |
| Host behaviour differs by channel and changes monthly | claim "build X and later"; quarterly re-run |
| Findings are numerous late | dry run on the web first; stage by host |
| Automation brittle | keep manual path authoritative; automation only for regression |
| Certification tests platforms not exercised (iPad) | decide IH-Q1 before submission |
| Google OAuth verification timing | start in parallel; it is not part of host acceptance |

### 13.2 Open questions

| # | Question | Recommended default | Decided by |
|---|---|---|---|
| IH-Q1 | List iPad for Word/Excel, or exclude by manifest, or accept risk? | exclude if the manifest mechanism is confirmed; otherwise test | Product owner |
| IH-Q2 | Minimum claimed Office builds | the lowest build actually tested in each channel | Product owner |
| IH-Q3 | Is Office LTSC 2021 for Word/Excel worth a claim? | yes if an environment exists; else omit | Product owner |
| IH-Q4 | Firefox for Office on the web in claims | basic load (P1) only | Product owner |
| IH-Q5 | Build the probe manifests? | yes, staging only | Engineering lead |
| IH-Q6 | Third tester for the reviewer script | yes | Owner |
| IH-Q7 | Organization-only distribution (X-08) before public listing? | optional rehearsal | Product owner |
| IH-Q8 | Where to keep evidence | private repository with LFS or an encrypted drive | Owner |

## Appendix A. Glossary

| Term | Meaning |
|---|---|
| Requirement set | named group of Office.js APIs (for example WordApi 1.3) |
| Sideload | install an add-in manifest without a store |
| Probe | test-only add-in page that reads/writes the document deterministically |
| Frozen run | evidence directory with checksums, not modified afterwards |
| Eligible platform | platform that supports the declared requirement sets |
| Sidecar | exported local queue/ledger of observations |

## Appendix B. References

| Source | Used for | Checked |
|---|---|---|
| Microsoft, "Word JavaScript API requirement sets", https://learn.microsoft.com/en-us/javascript/api/requirement-sets/word/word-api-requirement-sets (page dated 10 September 2026) | WordApi 1.3 minimum builds | 14 October 2026 |
| Microsoft, "Excel JavaScript API requirement sets", https://learn.microsoft.com/en-us/javascript/api/requirement-sets/excel/excel-api-requirement-sets (page dated 10 September 2026) | ExcelApi 1.13 minimum builds | 14 October 2026 |
| Microsoft, "PowerPoint JavaScript API requirement sets", https://learn.microsoft.com/en-us/javascript/api/requirement-sets/powerpoint/powerpoint-api-requirement-sets (page dated 16 December 2025) | PowerPointApi 1.4 minimum builds; iPad not available | 14 October 2026 |
| Microsoft, "Browsers and webview controls used by Office Add-ins", https://learn.microsoft.com/en-us/office/dev/add-ins/concepts/browsers-used-by-office-web-add-ins (19 August 2026) | WebView2 / WKWebView by platform | 14 October 2026 |
| Microsoft, "Test Office Add-ins", https://learn.microsoft.com/en-us/office/dev/add-ins/testing/test-debug-office-add-ins (17 June 2026) | sideload routes; web browsers tested | 14 October 2026 |
| Microsoft, "Unit testing in Office Add-ins", https://learn.microsoft.com/en-us/office/dev/add-ins/testing/unit-testing (updated 17 April 2026) | office-addin-mock; APIs cannot load in the unit-test process | 14 October 2026 |
| Microsoft, "Debug Office Add-ins", https://learn.microsoft.com/en-us/office/dev/add-ins/testing/debug-add-ins-overview (10 September 2026) | Edge DevTools on Windows, Safari Web Inspector on Mac, debugging staging/production add-ins | 14 October 2026 |
| Microsoft commercial marketplace certification policies, section 1120, https://learn.microsoft.com/en-us/legal/marketplace/certification-policies | all-platforms-in-Requirements rule; browsers; latest Office.js; version increments | 14 October 2026 (first 100,000 characters read) |
| Google, "Test your editor add-on", https://developers.google.com/workspace/add-ons/how-tos/testing-editor-addons | test deployments; no automated testing guidance; installable triggers unsupported in tests | 14 October 2026 |
| Practitioner reports on Google sign-in detecting automated browsers (several vendor and forum posts from a web search) | automation fragility; persisted-profile workaround | 14 October 2026 (secondary sources; unverified against Google documentation) |

Not verified in this pass (check at build time): manifest mechanism to exclude
iPad; attaching Playwright to the Office WebView2 over the DevTools protocol;
macOS version support policy for Office for Mac; Microsoft 365 developer sandbox
availability; Google add-on availability on mobile editors.
