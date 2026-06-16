# 19 · Transparency Reports and Audit Tools

### Claude Code prompt to start this stage

> Build the transparency reporting and external audit tools that make the platform's behavior inspectable by anyone. Reference: §6.4 (moderation transparency) and §9.3 (knowing capture is more useful than not knowing). Implementation across `tools/audit/` (a CLI auditor tool) and `services/transparency/` (the periodic report generator):
>
> 1. **The auditor CLI** (`tools/audit/`, Rust):
>    - `audit verify-chain <document_id>` — walks the event log, recomputes every chain_hash, reports OK or first inconsistency. Same as the API endpoint but from the local DB, runnable by any operator.
>    - `audit replay <document_id> --to-seq N` — materializes the document state at seq N from the event log; outputs the JSON. Lets an auditor reproduce historical state.
>    - `audit count-votes <document_id> <statement_id> --tier T3` — independent implementation of the verifiable count, replayable to match the closeout's number.
>    - `audit merkle-proof <document_id> <event_id>` — produces the authentication path for an event's inclusion in the latest snapshot (Vol II §E.3.2).
>    - `audit moderation-stats <document_id>` — aggregates moderation decisions: per-stage counts, per-reason counts, appeal rate.
>    - `audit sybil-signals <document_id>` — lists detected Sybil patterns with severity.
>
> 2. **The platform transparency report** (quarterly):
>    - Total contributions across all consultations.
>    - Moderation breakdown: blocks at stage 1, holds at stage 2, community flags resulting in escalation, human decisions in stage 4, appeals, overturn rate.
>    - Classifier evaluation: precision/recall on a held-out human-labeled set (~500 items per language per quarter). Published methodology.
>    - Sybil signal counts: detected patterns, actions taken, T3 verifications denied.
>    - Service health: uptime, P99 latency, incidents.
>    - Hosted on `/transparency` with full historical archive.
>    - Itself published as a content-addressed signed document.
>
> 3. **Per-consultation transparency report** (at consultation close, embedded in the closeout):
>    - Specific to that consultation; same structure as the platform-level but scoped.
>    - Highlights anomalies: e.g. "this consultation showed a 3x baseline Sybil signal rate during week 2"; "the moderation overturn rate was 12%, above the platform average of 7%."
>
> 4. **Public read API for audit data**:
>    - `GET /audit/document/:id/events` — paginated event stream including all moderation decisions, with the same data the moderator UI sees (modulo any author-PII fields).
>    - `GET /audit/document/:id/moderation-stats` — aggregated stats.
>    - `GET /audit/document/:id/sybil-signals` — detected patterns.
>    - Rate-limited but open; no auth required. A journalist or researcher can fetch.
>
> 5. **Reproducibility kit**:
>    - For any closeout report, a kit can be exported: the full event log up to closeout, the snapshots, the deliberation cache, the moderator decisions, and the auditor CLI binary.
>    - The kit is self-contained: a journalist can run the auditor offline against the kit and verify everything.
>    - Distributed as a single tarball with a manifest hash. Sponsors and oversight bodies have copies.
>
> 6. **The "challenge" workflow**:
>    - A citizen or journalist who suspects the closeout misrepresents the consultation can file a public challenge.
>    - The challenge is itself an event in the engine log: `ClosedoutChallenged{report_id, challenger_id, basis, evidence}`.
>    - The sponsor must respond within a defined window with either an amended closeout or a public defense.
>    - The challenge and response are part of the permanent record.
>
> Tests:
>    - End-to-end: corrupt one byte of one event's payload in a forked database; verify the auditor CLI detects it and reports the seq.
>    - Verifiable count: auditor's count matches closeout's count for a synthetic consultation.
>    - Merkle proof: auditor produces a proof; an independent verifier checks it without access to the rest of the log.
>    - Transparency report: synthetic consultation produces expected stats.

### Why a separate auditor CLI matters

A platform that publishes "trust us, the numbers are correct" is asking the public to take its word. A platform that publishes "here is a binary you can download, here is the event log; replay it yourself and verify" is making a different kind of claim: *verifiability, not trust*. This is the audit tier's contribution to legitimacy.

The CLI must be:
- Open source. The code is on the public repo.
- Reproducible. Same input, same output, every time. Pin all dependencies.
- Sovereign. A journalist in Bangladesh should be able to run it locally without depending on the platform's servers.

### The platform's own transparency

§6.4 committed the platform to publishing its own behavior. Concretely:

- Quarterly report with the metrics above.
- An "incidents" log listing every confirmed Sybil attack, every classifier failure, every moderator dispute that required panel escalation.
- A red-team commitment: invite security researchers to find flaws; publish their findings (after responsible disclosure) and the fixes.

A platform that *publishes its own failures* is more credible than one that hides them.

### Watch out for

**Auditor CLI distribution.** Make it easy to run. Single static binary (Rust). Hosted with checksums on the public repo + a mirror on a sovereign-cloud or IPFS pin. A user who wants to verify shouldn't need to install a dev environment.

**Avoid creating de-anonymization risks.** Transparency stats are aggregates. Per-clause moderation counts are fine; "moderator X has overturned more decisions on author Y than baseline" is dangerous (re-identifies). Scrub.

**Reproducibility kit storage cost.** A national consultation kit can be 1–2 GB. Don't generate them eagerly for every consultation; generate on request. Keep the source data (event log) and the auditor; reconstruct the kit as needed.

### Verification checklist

- [ ] Auditor CLI verifies a clean chain, detects corruption.
- [ ] Verifiable count matches across CLI and API.
- [ ] Merkle proof verifies independently.
- [ ] Transparency report renders with real stats.
- [ ] Reproducibility kit can be exported and replayed.
- [ ] Challenge workflow records and notifies correctly.
