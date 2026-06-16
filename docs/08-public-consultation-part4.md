---

# Part IV — Citizen UX, legitimacy, and prior art

The remaining two layers — presentation and legitimacy/closeout — are where most public consultations actually fail, irrespective of the engine behind them. They are also where the difference between a tool that gets used and one that gets ignored lies.

## 8. Layer 8 — Citizen UX

A consultation that requires citizens to understand the engine — to think about events, suggestions, alternative versions, or even "the document tree" — has already lost most of its audience. The presentation layer is a translation in the same spirit as Volume I §III.5's developer-to-researcher table, but more aggressive, because the audience is broader and the median attention is shorter.

### 8.1 What a citizen sees, in order

A successful consultation interface progresses through the same flow a citizen's attention would take naturally:

**Land on the consultation.** The home page is a single screen: a one-paragraph plain-language summary of what the policy is and what the government wants to know, the deadline, the participation count so far, and a single primary CTA — *Read the draft and tell us what you think*. No login required to read.

**Read the document in the citizen's language.** The policy is presented in the official language(s), with a clear, navigable structure (clauses, sub-clauses, examples). For Bangladesh, this means Bangla as the primary surface with English secondary; for any consultation, side-by-side bilingual where applicable. Vocabulary is normalized: "this clause," "this section," "comment," "suggest a change," "see what others think." The engine's vocabulary (events, nodes, deltas) never surfaces.

**See what others have said per clause.** When a citizen taps a clause, they see (a) the clause text, (b) the bridge-consensus statements emerging from the deliberation layer ("80% of citizens across all opinion groups agree: …"), (c) the divisive issues ("there is disagreement on …"), and (d) a "see all comments" expander. The bridge statements are surfaced first because they are the signal; the controversies second; the raw stream last. *This is the inversion of social media's ranking, and it is the single most important UX decision.*

**Participate, in three modes of increasing commitment.**

- **Vote (T1).** Tap "agree" or "disagree" on any statement someone else has made. Zero typing. Designed for the silent supermajority — the 27,000 of 30,000 from §2 who will not write but will react.
- **Comment (T2).** Write a sentence about the clause. The interface gently suggests reading existing comments first and offers "agree with this existing comment" as an alternative to writing yet another — *aggregation, not duplication*. When the citizen does write, the comment is shown to them as a position; if a near-duplicate cluster exists, the platform suggests "your view is similar to this group of 240 citizens; do you want to join their position or start a new one?" The default action is *join*, which is what makes the platform's signal coherent rather than fragmented.
- **Suggest a change (T2 also).** "Propose specific wording for this clause." This produces a `SuggestionProposed` event in the engine, with the new wording shown beside the original. The interaction is the same one Dynodoc prototype already demonstrated — track-changes style — but the *vocabulary* is "suggested change," never "branch" or "alternative draft."

**See your contribution in context.** After submitting, the citizen sees: *where their contribution sits* (the cluster it joined or started), *the cluster's overall position*, and *the bridge statements they happened to also agree with*. This is the loop that converts a one-shot input into a moment of learning about the public's range of views.

**Optional upgrade to T3.** A subtle but visible "verify as a citizen" path explains the difference clearly: *unverified contributions count in the deliberation analysis; verified contributions also count in the formal citizen tally the government must report on.* No pressure, no gating; the trade-off is explicit and the choice belongs to the citizen.

### 8.2 Three UX principles that follow from the design

**Principle A: aggregate by default, individuate on demand.** When a clause has 1,400 comments, no one reads 1,400 comments. The default presentation is the cluster summary ("4 opinion groups; bridge agreement on X, Y; division on Z") with counts; expanding shows representative comments; expanding further shows the full stream, paginated. This is the duplicate-detection of §7.4 made visible.

**Principle B: every action is reversible.** A citizen can edit their own contribution, retract it, change their cluster vote. The engine handles this trivially (events are emitted that supersede prior events; the audit trail retains the history but the *current state* reflects the latest). Reversibility lowers the cost of participation — citizens who fear "saying the wrong thing permanently" do not participate.

**Principle C: no engagement gamification.** No streaks, no badges, no points for contribution count, no leaderboards. The goal is *considered participation*, not maximum participation. This is what makes the platform not become Twitter. The closest thing to gamification is the post-contribution loop in §8.1 — "here's the public structure of opinion on this clause" — which rewards engagement with *information*, not with score.

### 8.3 Phone-first, low-bandwidth, accessible

Most citizens reach the platform on phones, often on slow connections. The interface is server-rendered HTML for first paint, with progressive enhancement for interactivity. Total initial payload under 200KB per page. Works without JavaScript at a reduced level (read, vote, comment all work; only the live clustering visualization degrades to a static snapshot). WCAG 2.2 AA compliance is non-negotiable for a government-sponsored platform — keyboard navigation, screen-reader semantics, sufficient contrast, no time-limited interactions for participation. Bangla typography handled correctly (Noto Sans Bengali; mixed-script line breaking; Bangla numerals option). All of this is craft, but it is the craft that determines whether the rural participant exists in the data or doesn't.

### 8.4 The mobile interaction model in concrete terms

```
┌─────────────────────────────────┐
│ National AI Policy 2026 (draft) │  ← bilingual header
│ Public consultation · 18 days   │
│ left · 12,431 citizens so far   │
├─────────────────────────────────┤
│ § 4 · Data sovereignty          │  ← swipeable clauses
│                                 │
│ "All training data collected    │
│  from Bangladesh citizens must  │
│  be stored in Bangladesh…"      │  ← the clause itself
│                                 │
│ ── what citizens agree on ──    │
│  ✓ data sovereignty matters     │  ← bridge statements
│    (94% across all groups)      │    leading
│  ✓ enforcement is unclear       │
│    (81% across all groups)      │
│                                 │
│ ── where citizens disagree ──   │
│  ⚡ should exceptions be allowed │  ← divisive issues
│    for foreign research?        │    next
│   (4 opinion groups, see all)   │
│                                 │
│ [ Agree with bridge statements ]│  ← T1 one-tap path
│ [ Add your view ]               │  ← T2 path
│ [ Suggest specific wording ]    │  ← T2 path
└─────────────────────────────────┘
```

The screen is the entire interaction. No menus, no profile setup, no introduction to "version control." The citizen reads, taps, and goes; the engine logs three semantic events that participate in the same audit trail as the sponsor's decisions.

---

## 9. The legitimacy layer — closeout, audit, and what gets published

A consultation that takes input and produces no public reckoning has wasted everyone's time and *eroded* trust. The legitimacy layer specifies what the sponsor must do after the consultation closes, and what infrastructure makes that defensible.

### 9.1 The closeout report, formally

Within a fixed period after the consultation closes (we recommend 60 days), the sponsor publishes a structured closeout report. It is mandatory in this design — *the platform refuses to mark the consultation "closed and archived" until the report is filed.* The report contains:

- **Participation statistics:** total participants by tier; total contributions; total votes; cluster structure; bridge consensus statements; divisive statements; geographic and demographic breakdowns where ethical and lawful.
- **Moderation statistics:** total moderated by stage; reasons; appeal counts; appeal-overturn rate; identified Sybil patterns and what was done about them. This includes the platform's own self-audit (the §6.4 transparency report for this consultation).
- **The diff:** a clause-by-clause comparison between the consultation draft and the post-consultation draft. For each clause that changed, the sponsor states the *bridge statements that motivated the change* and the *contributors whose suggestions are reflected* (attributable by `author_id`). For each clause that did *not* change despite substantive input, the sponsor states the reasoning, *citing the bridge statements that did not prevail and explaining why.*
- **The disposition of each cluster's positions:** "your group's view was reflected/partially reflected/not reflected; here is the reasoning." This is the loop that makes participants feel heard even when they did not prevail.
- **The signed, hash-linked record:** the entire report is itself an event in the engine's audit log, content-addressed, hash-linked, signed by the sponsoring institution's key (Vol II §E.6). A future challenger can verify the report is the one originally published.

The closeout report is the *political* output of the consultation. It is the thing the sponsor stands behind in parliament, in the press, in a court challenge. The engine's audit trail (Vol II §E) is the *forensic* substrate that makes the report defensible.

### 9.2 The verifiable count

A specific element of the closeout report deserves emphasis: the **verifiable count of citizen positions**. For each bridge consensus statement, the report states:

> *"On §4: Data sovereignty matters: 9,847 verified citizens (T3) agreed; 1,203 disagreed; 14,231 verified-citizens did not vote on this statement. Across all participation tiers including unverified, 28,433 agreed; 4,108 disagreed."*

The verifiable count uses only T3 contributions and is *cryptographically auditable*: anyone can replay the event log, count the T3 votes for each statement, and verify the report's number to within zero error. This is the audit guarantee (Vol II §E) put to political work. A press challenge to the count is resolved by re-running the count from the public log.

### 9.3 Adverse outcomes and ungentle truths

Two outcomes the design must accept honestly:

**A sponsor can run a perfect consultation and then ignore it.** No architecture forces a government to act on its public's input. What the architecture *can* do is force the sponsor to *explain themselves*: the closeout report requires per-clause reasoning, and the political cost of "we considered 28,000 contributions and changed nothing without justification" is high enough that most institutions will not do it. The platform's contribution to legitimacy is to make non-engagement *visible and accountable*, not to prevent it.

**The consultation can be captured.** A sufficiently determined adversary, with money and patience, can mount a Sybil campaign that *partially* contaminates the input. The defense in depth (§5.6 + §7.2) raises the cost, but does not eliminate the risk. The honest design posture: the platform's transparency reports surface suspected capture, the deliberation algorithm structurally resists cross-cluster amplification, and the closeout report distinguishes T3 verified-citizen counts from raw totals — so a reader can judge how much weight to put on each. *Knowing the consultation was partially captured is more useful than not knowing.*

---

## 10. Prior art: how this compares to existing public-consultation platforms

The deepest design check on a system like this is to position it precisely relative to what exists. Three matter most.

### 10.1 Polis / vTaiwan (the deliberation precedent)

Polis is the algorithmic deliberation engine, deployed since 2014 in Taiwan, since 2024 used in multiple jurisdictions, and the primary inspiration for our §7. It is *not* a version-control system; it has no concept of a structured document being edited. Participants submit statements and vote agree/disagree/pass on others; the clustering and bridge-statement extraction is its central output. vTaiwan has used it as a *front-end* to legislative drafting: the consultation surfaces bridge consensus, and that consensus is then incorporated into legislation through a separate (sometimes manual, sometimes formal) process. Over a decade it has contributed to 26 pieces of legislation in Taiwan.

**What we adopt:** the clustering and bridge-statement algorithm (§7.1), the inversion of social-media ranking, the human-validation of AI cluster summaries (§7.5), the focus on cross-cluster agreement as the durable signal.

**What we add:** the consultation is *on a specific structured document* with stable-identity nodes, so input is per-clause and the closeout includes a specific clause-by-clause diff (§9.1). This binds the consultation to the legislative artifact in a way Polis alone does not. The engine's audit trail (Vol II §E) gives a stronger legitimacy substrate than vTaiwan's process produces.

### 10.2 Decidim (the platform precedent)

Decidim is the Barcelona-originated, AGPL-licensed participatory democracy platform, used by ~400 institutions across ~30 countries (city governments, EU bodies, universities, cooperatives). Its architecture is modular and multi-tenant (one Decidim install can host many consultations). It supports proposals, debates, participatory budgeting, citizen initiatives, and consultations. It is *not* designed around editing a structured document; proposals and comments are typically free-form items, voted on for inclusion in a budget or a recommendation set.

**What we adopt:** the multi-tenant, modular philosophy (one engine, many consultations); the AGPL-license precedent for institutional trust; the published governance/moderation policy practice; the integration with existing civic-ID systems.

**What we add:** the structured-document model (per-clause input, semantic merge, attributable diff) — Decidim does not do this. The four-tiered identity model (Decidim is generally email-based with optional ID verification; we make the trade-off more explicit). The deliberation-layer clustering (Decidim has voting and comment-ranking but not Polis-style clustering by default; one could integrate them).

### 10.3 Consul / Decide Madrid (the cautionary tale)

Consul is the platform that powered Madrid's *Decide Madrid* from 2015. Its architecture was monolithic (not modular), which became the constraint that drove Barcelona to fork it into Decidim. Decide Madrid achieved scale — hundreds of thousands of registered users, multiple participatory budget cycles, formal citizen initiatives — but ran into well-documented problems with low conversion of input to political outcome and the brittleness of its architecture as feature requirements grew.

**What we learn from it:** modularity and pluggability are not nice-to-haves; they are what determines whether the platform survives the second jurisdiction's different legal requirements (§4's per-layer pluggability is directly motivated by this). The platform's legitimacy is determined as much by the *closeout* (§9) as by the *input* — a high-scale consultation without strong closeout discipline becomes the cautionary tale, not the success story.

### 10.4 The differentiator, in one paragraph

The platform we are designing combines three things that no existing tool combines: **a stable-identity structured-document engine** (Vol II's contribution), **Polis-style deliberation extraction** (the vTaiwan contribution), and **the four-tiered identity / mandatory closeout legitimacy stack** (this document's contribution). The result is a consultation tool that produces an *attributable, defensible, semantically-merged* draft revision with a *defensible reading of public opinion* attached, rather than a separate input gathering process and a separate (manual) drafting process. The engine and the deliberation are bound to the same audit trail.

This is not "GitHub for laws" — that framing is wrong because GitHub assumes contributors who can read diffs and resolve conflicts, and treats popularity (stars, contributor count) as the signal. It is closer to **"vTaiwan attached to a versioned document, with a real identity and moderation stack."** That phrasing locates what is new and what is borrowed honestly.
