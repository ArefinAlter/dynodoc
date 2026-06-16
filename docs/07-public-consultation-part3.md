---

# Part III — Moderation and deliberation: turning noise into signal

The identity layer governs *who* can speak. The next two layers govern *what is shown* and *what counts as the public's view*. They are the layers that turn a flood of input into a defensible reading of public opinion.

## 6. Layer 6 — The moderation pipeline

At 30,000 contributors generating ~100,000 textual contributions, hand-moderation is not feasible. Pure automation is not acceptable either — automated takedowns of political speech are legally dangerous and politically toxic. The architecture is a **staged funnel** in which automated filters handle volume and humans handle judgment, with every decision logged and appealable.

### 6.1 The four-stage funnel

```
incoming contribution
        │
        ▼
┌──────────────────┐    BLOCK (illegal, CSAM, malware links)
│ Stage 1: hard    │ ──────────────────────────► event recorded; not shown;
│ filters          │                              forwarded to law-enforcement
│ (categorical)    │                              channel where applicable
└──────────────────┘
        │ PASS
        ▼
┌──────────────────┐    HOLD (high-confidence
│ Stage 2: soft    │     hate/harass/spam)
│ classifiers      │ ──────────────────────────► event recorded; shadow-hidden
│ (probabilistic)  │                              pending human review (target: 4h)
└──────────────────┘
        │ PASS / UNSURE
        ▼
┌──────────────────┐    publish; subject to:
│ Stage 3: publish │ ──────────────────────────► community flagging (any reader)
│                  │                              behavioral pattern detectors
└──────────────────┘                              periodic sponsor audit
        │
        ▼ FLAGGED (by community or pattern)
┌──────────────────┐    moderator decides:
│ Stage 4: human   │      keep | hide | remove | escalate
│ review           │ ──────────────────────────► decision logged; appeal available
└──────────────────┘                              author notified of action + reason
```

Each stage is independent, and each writes to the engine's event log:

- `ContributionFiltered{contribution_id, stage, action, classifier_version, confidence}`
- `ContributionHeld{contribution_id, reason, reviewer_target_deadline}`
- `ContributionFlagged{contribution_id, flagger_id, reason_code}`
- `ModerationDecided{contribution_id, decision, moderator_id, rationale, prior_state}`
- `ModerationAppealed{contribution_id, appellant_id, basis}`

Every moderation action is itself a governance event in the audit log (Vol II §D.3 L4). This is non-negotiable for legitimacy: the public can later see *exactly* how much was filtered and on what grounds, and individual authors can appeal individual decisions with a record. A consultation that quietly removes content and cannot produce the record is — politically and often legally — broken.

### 6.2 Stage costs and recommended thresholds

The numbers from §2 drive the calibration:

- **Stage 1 (hard filters):** ~100,000 incoming, perhaps 0.5–2% caught (500–2,000 items). Throughput needs ~1 item/sec average sustained, ~10/sec at peak — trivial. *Use rule-based filters for the categorical bans* (URL block-lists, hash matches for known CSAM via the standard NCMEC PhotoDNA / Microsoft API for image content, regex for the narrow set of unambiguously illegal patterns). False positives at this stage are politically costly, so the rules are kept *narrow and conservative* — "anything that's a borderline judgment goes to Stage 2."
- **Stage 2 (soft classifiers):** the residual ~98,000–99,500. Use a hate-speech / harassment / spam classifier at a confidence threshold that catches the clear cases (~5–10%, so 5,000–10,000 items held for human review). Use a model that is *publicly documented* (the public has a right to know what classifier is making decisions on their political speech); on-prem deployment for jurisdictions where data sovereignty requires it. Bangla-language classifiers are notoriously weaker than English; budget for this — Stage 2's recall on Bangla hate speech is *worse* than on English, so the *Stage 3 community-flagging channel matters more* for non-English consultations.
- **Stage 3 (publish + community flagging):** the bulk of contributions reach the public. Community flags are weighted by flagger tier (T3 flags > T2 flags > T1 flags, and a single T3 flag triggers moderator attention faster than ten T1 flags — a cheap defense against flag-brigading). Flagging is itself rate-limited.
- **Stage 4 (human moderation):** the residue. ~5,000–10,000 Stage-2 holds + ~2,000–5,000 community flags = ~7,000–15,000 human decisions over 30 days, or 230–500/day. **A team of 5–8 moderators working full time, in shifts, with a structured queue, is enough.** Smaller teams can be augmented with cross-jurisdictional civil-society moderators (Bangladesh has serious capacity in this area through digital-rights organizations).

Two critical properties of the human-moderation tier:

**Moderation policy is published, before the consultation opens.** The categories ("incites violence against a person or group," "personal attacks on contributors," "off-topic to the draft," etc.) are defined, with examples, in a public document. Moderators are trained on it. Decisions cite the category. This is what makes Stage 4 defensible in court and in parliament: the rules existed before the speech did.

**Appeal is real and structurally separate.** An author whose contribution is removed can appeal once. The appeal goes to a *different* moderator (rotation enforced by the queue), and on second removal it can be escalated to a *moderation review panel* that includes someone outside the sponsoring institution (e.g. a representative from a digital-rights organization). The appeal record is itself public (anonymized for the appellant if requested). The cost of this — a small fraction of decisions are appealed, perhaps 5% — is well worth the legitimacy it buys.

### 6.3 What gets moderated, and what doesn't

A surprisingly important distinction. The moderation pipeline filters **what is publicly displayed and what feeds the deliberation layer's signal**. It does **not** erase from the audit log. A removed comment still exists as a `CommentAdded` event followed by a `ModerationDecided{decision: remove, reason: ...}` event; the original content is retained (encrypted-at-rest, accessible only via lawful process and the sponsor's audit team), the public display is null, the closeout report can truthfully say "47 contributions were removed for reason A, 81 for reason B." This is the audit-vs-display distinction, and it is what lets the system be both responsive to genuinely harmful content *and* incorruptible: a future challenger to the consultation cannot claim "they removed contributions opposing the policy" because the log can be audited under appropriate process to falsify that claim.

This split — *publicly hide, internally preserve* — is also what lets the platform comply with takedown-required jurisdictions without becoming an instrument of political censorship. The data is held; only the display is changed.

### 6.4 Open-source moderation transparency

The pipeline's transparency is itself a feature. Quarterly (or per-consultation), the platform publishes:

- Total contributions; total moderated at each stage.
- Breakdown by category, with counts.
- Median time-to-decision; appeal volume; appeal-overturn rate.
- The current classifier versions and their published evaluation metrics.
- Anomalies (Sybil patterns detected, coordinated brigading flagged).

This transparency report is itself a hash-linked, signed document — built using the same engine. The consultation system audits itself with its own machinery.

---

## 7. Layer 7 — Deliberation and signal extraction

Even after moderation removes the abuse, the consultation team faces 80,000+ legitimate contributions. The naive view "rank by upvotes, present top 50" is *exactly* the failure mode the deliberation literature warns against — it produces the most polarizing, in-group-amplified content, not the public's considered view. This section presents the alternative.

### 7.1 The Polis insight, formally

The Polis algorithm (Computational Democracy Project; deployed in vTaiwan since 2014) inverts the usual social-media aggregation. Instead of ranking statements by total agreement, it does the following:

**Step 1 — Construct a participant × statement vote matrix.** For each participant $p$ and each statement $s$, record a vote $v_{p,s} \in \{-1, 0, +1\}$ (disagree, pass/unsure, agree); zero for unvoted.

**Step 2 — Reduce dimensionality.** Apply PCA (principal component analysis) or a related embedding to the vote matrix, yielding a low-dimensional (typically 2D) embedding of *participants* in opinion space.

**Step 3 — Cluster participants.** Apply k-means (or HDBSCAN, more recent) to the embedded participants, yielding $k$ opinion clusters. Typically $k = 2$–$6$ emerges naturally — these are the *opinion camps* in the consultation.

**Step 4 — For each statement, compute group-informed agreement.** Within each cluster, compute the fraction agreeing. The statement-level outputs are:

- **Group-divisive statements:** high agreement in one cluster, low in another. These reveal where genuine disagreement lies — exactly the issues a sponsor most needs to know about.
- **Group-informed consensus statements:** high agreement *across all clusters*. These are the *bridge statements* — propositions that survive cross-cluster scrutiny. They are what a Sybil flood cannot fake, because manufacturing cross-cluster agreement requires controlling all clusters, which is much harder than controlling one.

**Step 5 — Surface bridge statements first.** The dashboard and the closeout report lead with the consensus statements, ranked by cross-cluster agreement breadth. The divisive ones are surfaced separately, as "issues requiring further deliberation."

### 7.2 Why this is the right primitive for public consultation

The bridge-statement output answers the question a policy sponsor actually has: *what does the public agree on, across its divisions, about my draft?* It does not answer "what is the most popular position" (which a polarized public almost never agrees on). It does not collapse the public into a single number. It tells you *what survives scrutiny from everyone* and *where the genuine controversies lie* — which is what a sponsor must understand to revise the draft responsibly.

Three subtler properties make it Sybil-robust in ways naive ranking is not:

- **Sybils cluster.** A coordinated attack typically produces participants with highly correlated voting patterns, which the clustering picks up as a *single tight cluster*. A statement that is wildly popular in *only that cluster* is not a bridge statement; it is downweighted automatically.
- **The output is qualitative, not numeric.** "Bridge statements" is a discrete set, typically 10–30 in a major consultation, not a percentage. There is no single number for a Sybil to inflate.
- **Statements that survive across clusters are over-determined.** Each cluster votes independently; getting agreement from all of them is exponentially harder than getting agreement from one. This is the algorithmic version of "consensus across opposing camps is rare and therefore meaningful."

### 7.3 Adapting Polis to a structured document

vTaiwan ran Polis on *free-form opinion statements* — participants submitted whole sentences. Our setting is different: contributions are *attached to specific nodes in a structured document*. The adaptation:

- **Cluster per node, not globally.** For each clause of the policy that receives substantial input, run the Polis pipeline on contributions to *that clause*. This is essential because a citizen may agree with the document on §2 and disagree on §4 — global clustering would collapse this nuance and produce meaningless "average citizens."
- **Two types of contribution feed clustering:** explicit comments/positions, and votes on those positions. The vote matrix is per-clause.
- **Cross-clause meta-clustering:** once per-clause clusters are computed, a second-order analysis identifies participants whose clause-level positions correlate — surfacing coherent constituencies ("citizens who prioritized data sovereignty across all clauses," "citizens who prioritized innovation speed across all clauses"). These constituencies are *useful information for the sponsor*; they are not "factions" in any voting sense.

### 7.4 Duplicate detection and statement-clustering

A second deliberation problem: a thousand citizens may say "this clause is unfair to small businesses" in a thousand slightly different ways. Treating these as a thousand distinct contributions buries the signal. The solution is **semantic clustering of statements**:

- Compute embeddings for each contribution (multilingual sentence transformer; on-prem deployment if the consultation requires data sovereignty).
- Cluster contributions within a node by cosine similarity above a threshold; treat each cluster as a *single position* with $N$ supporters.
- Present to readers (and to the sponsor) the *positions*, with the contributor count and a representative quote, rather than the raw list. Individual contributions remain attributable and accessible, but the *summary surface* is at the position level.

The threshold is tuned empirically; getting it wrong in either direction is bad (too tight → 900 of the 1000 are listed separately; too loose → genuinely distinct positions are conflated). The platform exposes the cluster boundaries to moderators and to interested citizens, who can flag "these two should not be one position" or vice versa — a *human-in-the-loop* check on the clustering.

### 7.5 The role of AI-generated summaries (the vTaiwan caution)

vTaiwan's recent practice — and a published caution worth heeding — is to use LLMs to summarize what each cluster of opinion *says* in natural language, then **submit the summary back to that cluster for validation before publishing**. The flow:

```
cluster of 800 participants who agreed on a set of statements
    │
    ▼
LLM produces a 2-sentence summary:
"This group is concerned that …"
    │
    ▼
summary shown back to the 800 participants
    │
    ▼
they vote: does this summary accurately represent your view?
    │
    ▼  (≥70% accept)
summary is published as the cluster's position
```

This is the *human validation of AI summarization* that vTaiwan insists on, and it is what prevents AI mis-summary from putting words in citizens' mouths. The summary becomes part of the consultation's record only after the cluster confirms it. The labor cost is real but bounded — one validation pass per cluster per clause, not per contribution.

### 7.6 What the deliberation layer outputs

The deliberation layer's output, fed up to the presentation layer and to the sponsor:

- **Per clause:** the opinion clusters, bridge statements, divisive statements, cluster summaries (validated), participant counts per cluster.
- **Per consultation:** a map of cross-clause constituencies; a list of clauses ranked by participation intensity; a list of clauses where bridge consensus was found vs. where divisions remained.
- **Per participant (privately, to themselves):** "your cluster on §3 was X; the bridge statements you also agreed with were Y; here are the bridge statements that emerged across clusters." This is the loop that *teaches* citizens about the shape of the consultation and is one of vTaiwan's distinctive properties — participants leave knowing what the public agreed on, which is a public good even when the consultation does not change the draft.

The deliberation layer does *not* output a single ranking, a single recommendation, or a single "voice of the people." Those are politically presumptuous and statistically meaningless. The layer outputs the structure of opinion, and the sponsor — accountable, named — decides what to do with it.
