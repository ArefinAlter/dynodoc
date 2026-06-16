# 18 · Closeout Reports and Verifiable Counts

### Claude Code prompt to start this stage

> Implement the mandatory closeout report — the political output of the consultation. Reference: §9 in full. This is what the sponsor publishes when the consultation closes, what the public can challenge, what parliament can examine. In `services/closeout/` (a new Python service for the report-generation pipeline) plus engine support in `crates/engine-core/src/closeout.rs`:
>
> 1. **The closeout report structure** (JSON document, then rendered to PDF + HTML):
>    - Header: consultation name, sponsor, dates, document version, total participants, identity tier breakdown.
>    - Participation statistics: per tier, geographic where lawful, demographic where lawful, language distribution.
>    - Moderation statistics: per-stage counts, reason-code breakdown, appeal rate, overturn rate, identified Sybil signals.
>    - The diff: clause-by-clause comparison of pre- and post-consultation drafts. Each changed clause cites:
>      - The bridge statements that motivated the change.
>      - The contributors (by `author_id`) whose suggestions are reflected.
>      - The cluster constituencies that supported the change.
>    - The disposition of each cluster position: "your group's view was [accepted | partially accepted | not accepted], reasoning: [...]"
>    - The verifiable T3 tally: for each bridge consensus statement, the exact count of T3 agreement/disagreement votes.
>    - The signed, hash-linked record: closeout report content_hash + the consultation's final Merkle root + sponsor's Ed25519 signature.
>
> 2. **The sponsor's closeout workflow**:
>    - At consultation close (the `ConsultationClosed` event), the system generates a *draft* closeout report by aggregating data automatically.
>    - The sponsor's editorial team reviews and adds the per-clause reasoning text. This is the manual labor: writing 30–80 short paragraphs explaining the disposition of each cluster's position on each substantively-debated clause.
>    - The platform refuses to mark the consultation `Archived` until the closeout is filed (a strong enforcement of §9.1's "mandatory" requirement).
>    - The closeout itself is content-addressed and chained into the engine's log as `ClosedoutPublished{report_id, content_hash, signature}`.
>
> 3. **The verifiable count function**:
>    - In `engine-core/src/closeout.rs`: `verifiable_count(document_id, statement_id) -> (t3_agree, t3_disagree, t3_pass, t3_total_eligible)`.
>    - This function replays the event log, counts only `VoteCast` events where `author_tier = 3`, and produces an exact number.
>    - The closeout report includes this count for every bridge consensus statement.
>    - Anyone with read access to the event log can independently reproduce the count and verify the report's number.
>
> 4. **External anchoring** (optional but recommended for high-stakes consultations):
>    - The closeout report's Merkle root is published to a separate transparency log: either a government-operated append-only log (RFC 6962-style Certificate Transparency, or its variant for non-PKI) or a public timestamping service (RFC 3161 TSA).
>    - The anchor URI is recorded in the closeout report.
>    - This gives §E.6's "prevention" property in addition to evidence.
>
> 5. **Public rendering**:
>    - HTML version at `/c/[slug]/closeout` — long-form, accessible, searchable, with permalinks to each clause's section.
>    - PDF version downloadable: same content, formal layout, government-standard typography. Use the same dark-PDF pipeline we built for the design documents.
>    - Both versions link back to the raw audit data: the event log, the snapshot, the verification endpoint.
>
> 6. **Closeout-driven diff visualization**:
>    - For each substantively-changed clause, show: original text, post-consultation text, the change inline-diffed.
>    - For each new clause added: marked as additions; rationale.
>    - For each clause removed: marked as removals; rationale.
>    - Citizens can search "what changed in §4" and see exactly that.
>
> 7. Tests:
>    - End-to-end: run a synthetic consultation, mark closed, generate the report; verify all counts match the event log replay.
>    - Tamper: alter a Postgres row in the closeout body; verify the content_hash and signature both fail.
>    - Verifiable count: independent replay tool produces identical numbers.

### Why this stage is the legitimacy keystone

A consultation without closeout is performative. With closeout — mandatory, structured, cryptographically signed, externally anchored, citing per-clause reasoning, distinguishing T3 verified counts from raw totals — the consultation is a *public artifact that can be defended or contested on the record*. This is what makes the platform's political claim defensible.

Three properties matter especially:

**Each closeout cites its source events.** A claim "we changed §4.2 because 9,847 verified citizens supported it" must be checkable by anyone with the event log. The closeout includes the query that produces 9,847; running it against the log either confirms or refutes.

**The closeout is itself an event.** It's hashed into the chain, signed by the sponsor's key, anchored externally if requested. A sponsor can't later quietly edit the closeout without leaving forensic evidence.

**The closeout is mandatory.** The platform refuses to archive without it. A sponsor who wanted to walk away from a consultation finds the platform won't let them — they must publish a closeout, even one that says "we did not change the draft and here is our reasoning."

### The diff visualization is more important than it sounds

Citizens want to know "what did my input change?" The diff view gives that answer directly: for each clause, here's the before, here's the after, here are the bridge statements that motivated the change. This is the loop §8.1 promised — engagement rewarded with information, not gamification.

### Watch out for

**Sponsor editorial fatigue.** Writing 60 per-clause reasoning paragraphs is real work. The platform helps: pre-populates with templates citing bridge statements, the constituency analysis, the moderator-surfaced suggestions. The sponsor edits, doesn't write from scratch. But: don't let templates produce bland boilerplate. A formulaic closeout is worse than a brief honest one.

**Don't let the closeout be the only output.** Many citizens won't read 60 pages. The platform also generates: a one-page executive summary (auto-generated then sponsor-edited), a short video script for press release, a social-media thread, a clause-level "what changed" view. Multiple surfaces for the same content.

**Closeout deadline pressure.** §9.1 recommended 60 days. Build a reminder cadence; show the public a "closeout due in X days" countdown after consultation close. Sponsors that miss the deadline are publicly visible as overdue.

**Privacy in citing contributors.** The closeout cites contributors by `author_id`. Per §6.3, the underlying identity may or may not be public depending on the user's display preferences. Default: pseudonymous (`Citizen #4471`); opt-in to display name. Never the underlying real name unless the user explicitly publishes it.

### Verification checklist

- [ ] Closeout report generates from a synthetic consultation correctly.
- [ ] Verifiable count matches event log replay exactly.
- [ ] Content hash and signature verify; tamper detected.
- [ ] HTML and PDF render correctly with same content.
- [ ] Diff visualization renders for changed clauses.
- [ ] Platform refuses to archive without closeout filed.
