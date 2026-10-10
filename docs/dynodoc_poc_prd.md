# Dynodoc — Proof of Concept

> Historical research-instrument reference. The 10 October 2026 [product specification](PRODUCT-SPEC.md) and [engine evolution plan](ENGINE-EVOLUTION-PLAN.md) supersede this product scope/build order. Preserve existing history compatibility; proposed algorithms are not implementation proofs.

### Product Requirements Document

**Document status:** Draft v1.0
**Owner:** Verne (Arefin Mizan), Verne Ltd.
**Last updated:** June 2026
**Target ship:** 8–10 weeks from kickoff

---

## 1. Summary

Dynodoc is an authoring environment for structured research instruments (ODK/Kobo/REDCap-style questionnaires) with version control, auditability, and collaborative review built into the data model rather than bolted on. This document specifies the **Proof of Concept**: a working system that a small expert research team can use to co-author a real questionnaire, with rigorous audit guarantees, but without the public consultation layers, AI-assisted features, or multi-tenant deployment hardening that come later.

The PoC's purpose is to **prove the engine** — the append-only hash-chained event log with semantic operations on stable-identity nodes. If this foundation works for a five-person team, the same engine extends without modification to the national-scale public consultation surface that is the longer-term product vision. The PoC is therefore both an immediately useful tool for friendly research teams and a de-risking exercise for the larger architecture.

---

## 2. Problem and motivation

Survey methodologists, regulated-trial researchers, and policy drafters share a common pain point: their working artifacts are *structured* (every item has a varName, a type, a constraint expression, a relevance condition, a translation set, choice options) but their tools treat them as *text*. The result is a chronic mismatch.

XLSForm spreadsheets are edited in Excel. Diff-and-merge in Excel is hostile. Git on XML exports produces unreadable diffs and silently merges across semantic boundaries. The standard authoring environments (KoboToolbox's form designer, ODK Build, REDCap's online designer) have no version control at all — saves overwrite, history is at best a few daily snapshots, and there is no concept of a suggested change that one collaborator proposes and another accepts.

For low-stakes work this is annoying. For high-stakes work — a regulated clinical trial protocol, a national policy draft, a multi-country impact evaluation — it is disqualifying. The teams doing this work currently mitigate by routing every change through one person (a bottleneck), or by working in disconnected forks that are reconciled by hand (an error magnet), or by accepting that institutional memory of *why* a change was made disappears within months (an audit failure).

Dynodoc addresses this by making the version control and audit trail **part of the data model**, not a layer on top of a serialization. Every change is a typed semantic event. Every event is attributable, hash-linked, and replayable. Concurrent edits to different fields of different items merge automatically with no conflicts. Same-field edits surface as a structured four-card conflict, not a textual diff. A reviewer's suggestion is a first-class proposal that the author accepts or rejects with a recorded reason. The audit trail is not a feature; it is the substrate.

---

## 3. Goals and non-goals

### 3.1 Goals for the PoC

- **G1.** A research team of 2–6 expert authors can collaboratively edit a structured questionnaire (sections, items, choices, expressions) in a web browser.
- **G2.** Concurrent edits by different authors merge cleanly when they touch different fields; conflicts are surfaced clearly when they touch the same field.
- **G3.** A reviewer (or another author) can suggest a change without applying it; the change exists as a proposal until the author accepts or rejects it.
- **G4.** The complete history of every change is preserved, attributable, hash-linked, and verifiable — a third party with read access can confirm the log has not been tampered with.
- **G5.** Referential integrity is enforced — the system refuses to deploy a draft whose expressions reference variables that no longer exist or form cycles.
- **G6.** Two browsers logged in as different authors see each other's edits within 100ms via Server-Sent Events.
- **G7.** The system runs on commodity infrastructure (one small VM, one Postgres) with no exotic dependencies and no managed-service lock-in.

### 3.2 Non-goals for the PoC

The following are *intentionally deferred* — they belong to the next phase, not this one. Stating them explicitly prevents scope creep:

- **NG1.** Public consultation surface (the read-only citizen view, citizen voting, suggestions from the public).
- **NG2.** Identity tiers beyond `Author` and `Reviewer`. No T1 reactor tier, no phone-verified T2 contributor tier, no T3 civic-ID verified tier.
- **NG3.** Moderation pipeline (automated classifiers, community flagging, human review queue, appeals).
- **NG4.** Deliberation layer (Polis-style clustering, bridge statements, opinion groups).
- **NG5.** LLM-assisted features (summarization, cluster validation, summary generation).
- **NG6.** Civic-ID / NID integration for verified citizen identity.
- **NG7.** Closeout reports and verifiable counts (no public consultation to close out).
- **NG8.** Real-time collaborative typing within a single field (the CRDT layer). Same-field concurrent edits surface as a four-card conflict, which is acceptable for a small expert team.
- **NG9.** Multi-tenant hosting with Helm charts. The PoC is deployed as a single instance for a single team.
- **NG10.** Offline-first / service worker support. The PoC requires an internet connection.
- **NG11.** Bangla localization of the researcher interface. English-only for the PoC; localization comes with the public consultation surface.
- **NG12.** Export to runnable XLSForm or ODK XML. The PoC stores and renders questionnaires natively but does not yet export to ODK Collect-compatible formats. (Add in phase 2.)

---

## 4. Users and personas

### 4.1 Primary users

**The Author** (lead methodologist). A survey methodologist or principal investigator who owns the instrument. They create the document, write items, define expressions, and decide which suggestions to accept. Typically holds a PhD, has 5–15 years of fieldwork experience, knows ODK syntax fluently. Two to four authors per project.

**The Reviewer** (peer collaborator, junior researcher, content-area expert). Reviews drafts and proposes changes via the suggesting workflow. Does not have direct write access to the canonical state. May be a content-area subject expert (a clinician reviewing a health questionnaire, a labor economist reviewing a livelihood questionnaire) rather than a survey methodologist. Two to ten reviewers per project.

### 4.2 Secondary users

**The Auditor** (compliance officer, IRB representative, future-self). Has read-only access to the full event log and the verification tools. Can confirm that a deployed version's history is intact and that every change is attributable. Rarely active during authoring; consulted at deployment time and during audits.

### 4.3 Out of scope for the PoC

Citizens, the general public, moderators, panel members, and sponsors are users of the public consultation surface (NG1) and are not addressed by the PoC.

---

## 5. User stories

The PoC must support these user journeys end-to-end. Each is testable as a Playwright E2E scenario.

### 5.1 Document creation and authoring (Author)

> *As an author, I want to create a new questionnaire document, add sections and items with typed fields, and edit content in a web browser, so that I can author my instrument without leaving the system.*

- Log in via magic-link email.
- Create a new document with a title and a sponsor organization.
- Open the document in the four-mode editor.
- In Document mode: edit prose, item labels, choice labels, and translations as a respondent would see them.
- In Outline mode: see a tree view of sections and items; drag to reorder.
- In Spec mode: edit technical fields (`varName`, `type`, `constraint`, `relevance`, `calculation`, `hint`).
- In Preview mode: render the questionnaire as ODK Collect would render it; tab through as a respondent.
- Switch modes without losing focus or unsaved work; state is shared across modes.

### 5.2 Collaborative editing (Author + Author)

> *As two authors working on the same document, we want to see each other's changes as they happen, so that we can coordinate without out-of-band communication.*

- Both authors log in simultaneously from different browsers.
- Author A adds an item to §3; Author B sees it appear within 100ms.
- Author B edits the prompt of an existing item; Author A sees the new prompt within 100ms.
- Both authors edit *different* items concurrently; both edits land; no conflict.
- Both authors edit the *same field* of the *same item* concurrently; the system flags this as a four-card conflict for resolution by one of them.

### 5.3 Suggesting a change (Reviewer → Author)

> *As a reviewer, I want to propose a specific change to wording without applying it myself, so that the author retains editorial control while still getting concrete suggestions.*

- Reviewer opens the document; UI is in suggesting mode (read-only to canonical state, but proposals are write-enabled).
- Reviewer edits an item's prompt; the change appears inline with strikethrough/highlight overlay; an "Add rationale" field appears.
- Reviewer writes a rationale and submits; the suggestion appears in the author's proposals queue.
- Author opens the proposals queue, sees the suggestion with the rationale and a side-by-side diff against the canonical state.
- Author accepts; the suggested change applies to canonical state, the event log records the acceptance, and the reviewer is notified.
- Alternatively, author rejects with a reason; the suggestion is closed without applying; the reviewer is notified.

### 5.4 Conflict resolution (Author)

> *As an author, when my changes and another author's changes conflict on the same field, I want to see both versions side-by-side with the original and choose which to keep, so that I never accidentally clobber my collaborator's work.*

- Two authors edit the same item's `prompt` field concurrently.
- On the second commit, the system detects the conflict and creates a `FourCardConflict` entry.
- A persistent indicator in the UI shows "1 conflict pending."
- Clicking the indicator opens the conflict resolution view: four cards showing (1) the common ancestor value, (2) Author A's value, (3) Author B's value, and (4) a custom blank for a third option.
- One author chooses; the chosen value becomes canonical; the conflict is resolved with attribution.

### 5.5 Referential integrity (Author)

> *As an author, when I rename a variable that another item's expression references, I want the system to either update both atomically or warn me before I deploy, so that my form does not silently break in the field.*

- Author renames the `varName` of item Q5 from `hours_outdoors` to `time_outdoors`.
- Item Q7 has a `relevance` expression `${hours_outdoors} > 4`.
- The author attempts to mark the document `Deployed`.
- The system runs the referential integrity pass and refuses, citing: "Q7's relevance expression references `${hours_outdoors}` which no longer exists. Did you mean `${time_outdoors}`?"
- Author updates Q7's expression; the integrity check passes; deployment succeeds; a `Deployed` event lands in the log with a clean Merkle root.

### 5.6 Audit and verification (Auditor)

> *As an auditor, I want to verify that the document's history has not been tampered with, so that I can attest to its integrity in a regulatory review.*

- Auditor runs the CLI: `audit verify-chain <document_id>`.
- The tool walks every event in the log, recomputes each `chain_hash`, and reports OK with the number of events verified.
- Auditor runs `audit replay <document_id> --to-seq N` to reproduce the document's state at any historical moment.
- Auditor runs `audit history <document_id> --node <node_id>` to see every event that touched a specific item.

---

## 6. Functional requirements

### 6.1 Document model

- **FR-1.** A document is a tree of typed nodes: `section`, `item`, `choice`, `paragraph`. Sections contain items; items contain choices (for multiple-choice) or have leaf fields (for text/number/date).
- **FR-2.** Every node has a stable identity (ULID) that is assigned at creation and never changes for the node's lifetime. References to the node use this ID, not its position or its `varName`.
- **FR-3.** Every node has typed fields appropriate to its type. Items have `varName`, `type`, `prompt`, `constraint`, `relevance`, `calculation`, `hint`, `required`. Sections have `label`, `relevance`. Choices have `value`, `label`.
- **FR-4.** Documents have a status lifecycle: `Draft` → `Deployed` → `Archived`. Transitions are events in the log.

### 6.2 Event log

- **FR-5.** Every state-changing action produces a typed semantic event. The vocabulary is closed and includes at minimum: `NodeCreated`, `FieldEdited`, `NodeMoved`, `NodeDeleted`, `NodeRestored`, `CommentAdded`, `SuggestionProposed`, `SuggestionAccepted`, `SuggestionRejected`, `ConflictResolved`, `Deployed`.
- **FR-6.** Events are append-only. No update, no delete. Enforced by Postgres trigger.
- **FR-7.** Every event is hash-linked: each event's `chain_hash` is `SHA256(content_hash || prev_chain_hash)`, forming a chain from the genesis event to the latest.
- **FR-8.** Every event is attributable to an identified actor (`author_id`) and carries an immutable timestamp.

### 6.3 Materialization and snapshots

- **FR-9.** The current state of a document is derived from the event log by folding events into a materialized state.
- **FR-10.** Snapshots are taken automatically: every 500 events, and mandatorily on `Deployed` events. Snapshots include a Merkle root over all node hashes.
- **FR-11.** Reading current state takes at most `(snapshot fold) + (latest tail of events)`, bounded by snapshot cadence.

### 6.4 Concurrent edits and merge

- **FR-12.** Edits to different fields of different nodes commute trivially and never conflict.
- **FR-13.** Edits to different fields of the same node commute and apply both.
- **FR-14.** Edits to the same field of the same node by two actors produce a `FourCardConflict` requiring explicit resolution.
- **FR-15.** Move operations that would create a cycle in the tree are detected and refused.

### 6.5 Suggestions and review

- **FR-16.** A reviewer can submit a `SuggestionProposed` wrapping any content event (`FieldEdited`, `NodeCreated`, etc.).
- **FR-17.** A suggestion is appended to the event log immediately but does not affect canonical state until accepted.
- **FR-18.** An author can accept or reject any pending suggestion, with an optional rationale.
- **FR-19.** Accepting a suggestion atomically marks it accepted and applies the wrapped event to canonical state.

### 6.6 Referential integrity

- **FR-20.** The system parses every `relevance`, `constraint`, and `calculation` expression and extracts variable references.
- **FR-21.** Before any `Deployed` event, the system runs an integrity pass: every referenced variable must resolve to an existing item's `varName`; no cycles in the dependency graph; type compatibility between operators and operands.
- **FR-22.** Integrity violations block the `Deployed` event with a list of all problems (not just the first).

### 6.7 Live updates

- **FR-23.** When a client opens a document, it subscribes to an SSE stream and receives every subsequent event for that document within 100ms.
- **FR-24.** On reconnect, the client resumes from the last known sequence number using the SSE `Last-Event-ID` header — no events are lost.

### 6.8 Authentication

- **FR-25.** Users register with email and confirm via magic link.
- **FR-26.** Authenticated sessions use Paseto v4 local tokens with a 24-hour expiry and refresh-token rotation.
- **FR-27.** Each document has an access list mapping identities to roles (`Author`, `Reviewer`, `Auditor`).

### 6.9 Audit tools

- **FR-28.** A CLI tool can verify the hash chain of any document from a local Postgres connection, independent of the web application.
- **FR-29.** The CLI tool can replay the log to any historical sequence number and emit the materialized state as JSON.
- **FR-30.** The CLI tool can produce per-node history (every event that touched a given node).

---

## 7. Non-functional requirements

### 7.1 Performance

- **NFR-1.** Event append latency P99 ≤ 5ms on a modest dev VM (4 vCPU, 8 GB RAM) with local Postgres.
- **NFR-2.** Document read (current state) P99 ≤ 50ms for a 500-node document.
- **NFR-3.** Chain verification of 100,000 events completes in ≤ 1 second.
- **NFR-4.** Live update latency from event commit to SSE delivery on a connected client ≤ 100ms.

### 7.2 Reliability

- **NFR-5.** Single instance, no replication required for PoC. Backups via `pg_dump` on a nightly cron.
- **NFR-6.** RPO (recovery point objective): 24 hours for the PoC. RTO (recovery time): 4 hours.

### 7.3 Security

- **NFR-7.** All traffic over HTTPS in deployed environments; HTTP-only acceptable in local development.
- **NFR-8.** Auth tokens are Paseto v4 (not JWT) to avoid the JWT family's known design flaws.
- **NFR-9.** Passwords are not used; authentication is via magic link only. No password reset flow needed.
- **NFR-10.** Secrets (DB credentials, token signing keys) are environment variables, never committed.

### 7.4 Accessibility

- **NFR-11.** WCAG 2.2 AA compliance on the researcher frontend: keyboard navigation, screen reader semantics, contrast ratios ≥ 4.5:1 on body text.
- **NFR-12.** Lighthouse score ≥ 95 on Performance, ≥ 100 on Accessibility, on a desktop simulated profile.

### 7.5 Browser support

- **NFR-13.** Chrome, Firefox, Safari on their two most recent major versions. No IE, no Edge Legacy. Mobile browsers are not required to work for the PoC (the researcher app is desktop-first).

### 7.6 Code quality

- **NFR-14.** All Rust code passes `cargo clippy` with no warnings.
- **NFR-15.** Property-based tests for the event log (`proptest`) and materializer covering at minimum: tampering detection, concurrent append correctness, materializer-snapshot equivalence.
- **NFR-16.** Playwright end-to-end tests cover all six user stories from §5.

---

## 8. Out-of-scope items, restated

To prevent ambiguity, the PoC explicitly does **not** include:

- Any public-facing surface; the system is accessible only to authenticated authors, reviewers, and auditors with an access-list entry per document.
- ODK Collect / XLSForm export. Documents are stored and rendered natively in the PoC.
- Real-time character-by-character co-typing in the same field (no CRDT layer).
- Localization of the researcher UI to non-English languages.
- Mobile-optimized interface.
- AI / LLM features of any kind.
- Identity tiers beyond `Author` / `Reviewer` / `Auditor`.
- Multi-tenant deployment, Helm charts, observability stack.

These are deferred to later phases per the build-reference series (files 22 through 31 in the full reference, plus stage 20 within the PoC scope).

---

## 9. Success criteria

The PoC is successful when **all** of the following are true:

- **SC-1.** A friendly research team (icddr,b, a2i, or equivalent) successfully authors at least one real questionnaire on the system, end-to-end, and reports that the tool is usable for ongoing work.
- **SC-2.** Two collaborators using the system simultaneously produce a clean merged document without out-of-band coordination over a multi-session authoring sprint of at least one week.
- **SC-3.** The suggesting workflow is used at least 20 times across the pilot project, with reviewers reporting that proposing changes is easier than pasting suggested wording into Slack.
- **SC-4.** The referential integrity pass catches at least one real bug before deployment that the team would otherwise have shipped.
- **SC-5.** An auditor with no prior context can verify the document's history using the CLI tools and confirm it is intact.
- **SC-6.** All performance NFRs are met under realistic load (200-node document, 4 concurrent users).
- **SC-7.** All E2E scenarios from §5 pass in CI continuously for two weeks before launch.

If three or more of these are missed, the PoC has failed and the architecture needs reconsideration before extending to public consultation.

---

## 10. Timeline and resourcing

### 10.1 Team

- **1 backend engineer** (Rust). Primary owner of the engine: event log, materializer, governance, API.
- **1 frontend engineer** (SvelteKit + TypeScript). Primary owner of the researcher app, porting the existing prototype.
- **Verne** (project lead): integration, schema, build reference compliance, validation against design documents.

For a leaner version, one full-stack engineer plus Verne can ship the PoC in ~12–14 weeks instead of 8–10. Either is viable.

### 10.2 Phases

| Week | Backend | Frontend | Deliverable |
|---|---|---|---|
| 1 | Repo, Postgres schema, migrations | Project scaffold, design tokens | `make dev` works |
| 2 | Event log core, hash chain | Routing, auth UI | Append + verify works in tests |
| 3 | Materializer, snapshots | Document list, document open | Round-trip read works |
| 4 | Semantic ops, governance | Outline mode, Spec mode | Edit one field end-to-end |
| 5 | API: REST + SSE | Document mode (prose) | Live update visible across browsers |
| 6 | Auth: magic link, sessions | Preview mode | All four modes complete |
| 7 | Integrity pass, expression parser | Suggesting workflow UI | Suggest + accept end-to-end |
| 8 | Semantic merge, conflict detection | Four-card conflict UI | Concurrent edits handled |
| 9 | Audit CLI tools | Polish, accessibility, Lighthouse | Verification works |
| 10 | E2E tests, hardening, docs | E2E tests, hardening, docs | PoC ready for pilot |

### 10.3 Risks

- **R1.** The Rust learning curve slows the backend if the engineer is unfamiliar. *Mitigation:* the build reference files 12–21 specify the engine in detail with code sketches; pair-program the first week.
- **R2.** The expression parser (stage 21) is the trickiest piece; ODK's XLSForm expression language has corners. *Mitigation:* implement the 80% subset (variable references, comparison, arithmetic, boolean, `selected()`) for the PoC; defer rare functions.
- **R3.** The four-mode editor (stage 19) is substantial frontend work. *Mitigation:* the prototype HTML files already exist and can be ported almost directly.
- **R4.** Schema decisions made in week 1 are expensive to change later. *Mitigation:* the build reference file 13 has the schema worked out in detail; don't improvise.

---

## 11. Open questions

These should be resolved with the sponsor before week 1:

- **OQ-1.** Hosting: managed cloud (which region?), self-hosted on Verne Ltd. infrastructure, or sovereign on a Bangladesh government provider? Affects deploy and ops.
- **OQ-2.** First pilot team: which real team's questionnaire will be the PoC's validation workload?
- **OQ-3.** Audit access: who counts as an Auditor for the PoC? IRB representative? In-house compliance? Verne directly?
- **OQ-4.** Identity provider integration: pure email + magic link sufficient, or integrate with an existing institutional SSO (Google Workspace, Microsoft 365) for the pilot team?
- **OQ-5.** Roadmap beyond PoC: is the next phase the public consultation surface (large jump) or ODK export + multilingual researcher UI (smaller jumps)? Affects what we optimize the engine API for in this PoC.

---

## 12. References

- **Engine design and mathematics:** `docs/04-versioning-combined.md` (Volumes I + II of the versioning protocol). Read this before coding.
- **UI/UX theory:** `docs/01-uiux-theory.md`. Read before frontend work.
- **Build sequence:** `docs/11-build-index-and-stack.md` through `docs/21-build-integrity-and-merge.md`. The PoC corresponds to these stages with the simplifications noted in §3.2.
- **Prototype to port:** `docs/dynodoc_interactive.html` and `docs/dynodoc_interface.html` — the four-mode editor's visual design and interaction patterns.

---

*End of PRD.*
