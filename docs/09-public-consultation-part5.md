---

# Part V — Infrastructure, operating model, roadmap, risks

The engine is correct, the layers are designed; this part covers what it costs to actually run, who operates it, how it phases, and what can still go wrong.

## 11. Infrastructure scaling

The numbers from §2 (~30,000 participants, ~600,000 events over 30 days, ~0.23 events/sec average and ~10/sec peak, read-dominated traffic) determine the deployment shape. Three components, with concrete sizing.

### 11.1 The engine tier (write path)

The semantic event log is an append-only store. PostgreSQL with `INSERT`-only tables, `BIGSERIAL` primary keys, partitioning by month, and a covering index on `(node_id, created_at)` handles tens of millions of events on a single modest instance (4 vCPU, 16 GB RAM, SSD storage). At 600,000 events per consultation, a busy platform hosting *100 simultaneous consultations* would write ~60M events over 30 days — comfortably within single-Postgres territory. Read scaling is a separate concern (§11.3) but the *write path* needs no exotic database choice. The conservative engineering call: **standard managed Postgres (RDS, Cloud SQL, or a sovereign-cloud equivalent) for the engine; do not adopt a distributed/blockchain/exotic store for an unfounded reason.** The hash-linked content-addressed log is implemented *in the application layer over standard tables*, exactly as Git implements its DAG over a filesystem.

The CRDT convergence layer (Vol II §B) is needed only for *within-draft real-time co-editing* — which is a Tier-1 (small expert team) feature. The public consultation, with 30,000 participants making independent comments on different clauses, is essentially the disjoint-node case of the commutativity theorem (Vol II §D.2): edits commute trivially because they touch different fields of different nodes. **The CRDT layer is not strictly needed for the public consultation flow**; events can be accepted directly into the log in order of arrival, with conflict resolution handled at the engine's L4 governance gate. This is a genuine simplification at scale: the public consultation is *easier* for the engine than the small-team co-edit, because there is essentially no within-node concurrency.

### 11.2 The deliberation tier (computational work)

Polis clustering on a 30,000 × 100 vote matrix is small by modern ML standards. The PCA + clustering pipeline runs in under a second on a single CPU core; the *embedding computation* for statement clustering (§7.4) is the heavier piece, requiring GPU acceleration if turnaround needs to be near-real-time. A single mid-range GPU instance (e.g. an A10 or T4) handles batch embedding of all comments in a major consultation in under an hour, and incremental embedding of new comments in tens of milliseconds.

The cluster summary generation (§7.5) calls an LLM, with the cluster-validation loop ensuring quality. Costs per consultation: a few hundred summaries, each a few thousand tokens, totalling tens of dollars of model cost — trivial.

**Cadence and freshness.** The clustering does not need to be live to the second. Recompute every 15 minutes during active consultation, every hour in quieter periods, on-demand for any node receiving sudden burst input. This is well within budget for a small CPU instance running in the background.

### 11.3 The presentation tier (read path)

This is where most of the load is, and it is *aggressively cacheable*. The consultation's public face — clauses, bridge statements, comment counts, cluster summaries — changes on the order of minutes, not seconds. A CDN with 60-second TTLs absorbs essentially all read traffic; the origin sees only contributions and the moderation team's actions. Standard web hosting (Vercel, Cloudflare Pages, a sovereign equivalent, or a self-hosted nginx in front of the application) handles read scale of millions per day at low cost.

The *interactive participation* surface (submit, vote) hits the application server, but at the rates of §2 — a few events per second average, ten per second peak — a single application server is sufficient with substantial headroom. Multiple application servers behind a load balancer for redundancy, not for capacity.

### 11.4 Self-hosting for sovereignty

A government consultation, especially in a country with active data-sovereignty politics, will likely require *on-premise* deployment. The architecture supports this: every component (Postgres, the embedding model, the LLM for summaries, the CDN substitute) has a self-hostable variant. The LLM piece is the one with material vendor lock-in risk if a closed model is chosen — the design must accept that and either run an open model on-prem (Llama family, Mistral, etc., adequate for summarization) or accept the political risk of routing political-speech summaries through a commercial cloud. For Bangladesh and similar contexts, **on-prem open-model deployment is the safer default** even at modest quality cost; the cluster-validation loop (§7.5) compensates for model weakness.

### 11.5 Cost summary, for a 30-day consultation at the §2 scale

Rough order of magnitude for a single major consultation, on commodity managed cloud:

- Postgres (small, even with healthy headroom): ~$200/month
- Application servers (3 instances, redundant): ~$300/month
- GPU instance for embeddings (on-demand, perhaps 50 hours/month): ~$100/month
- LLM API costs (summaries): ~$50 (one-time per consultation)
- CDN: ~$50/month at the read volumes implied
- Storage (event log, snapshots, moderated content): negligible (~$20/month)
- Observability, logging, backups: ~$100/month

**Total: ~$800–1,000 per month per major consultation.** A national platform running 5–10 simultaneous consultations spends a few thousand dollars a month on infrastructure. This is well within institutional budgets and an order of magnitude cheaper than the moderation team's salaries — which is the right shape: *human judgment, not compute, is the binding constraint*.

For sovereignty deployments, the cost is higher (self-hosted GPU, redundant on-prem Postgres, on-prem LLM hosting) but still bounded — a single rack in a government data center handles many simultaneous consultations.

---

## 12. Governance and operating model

A consultation platform is not a product the way an app is a product; it is a piece of civic infrastructure. The operating model differs accordingly.

### 12.1 The actors

A working consultation platform involves at least five categorically different actors. The architecture cleanly separates their roles.

**The platform operator** — an institution (could be a government IT body, an arms-length non-profit, a civil society consortium) that runs the codebase, the infrastructure, the moderation team, and the identity verification process. *Critically, the platform operator should not be the policy sponsor*: separation of platform from policy is what makes citizens trust that consultations are run fairly across administrations. The Decidim precedent — operated as an open-source consortium, deployed by individual institutions — is the right model.

**The policy sponsor** — the government ministry or institution drafting the policy under consultation. They author the initial draft, set the consultation timeline, publish the closeout report. They cannot modify others' input; they can only respond to it through the closeout report's per-clause reasoning.

**The moderation team** — separate from both platform operator and policy sponsor, ideally including representatives of civil society. Moderates content per the published policy (§6.2). The platform operator's role is to *resource and accredit* the moderation team; the moderation team's day-to-day decisions are not subject to platform-operator override.

**The participants** — citizens at any of the four identity tiers.

**The oversight body** — for high-stakes consultations (e.g. constitutional consultations), a separate parliamentary or judicial body has access to the full audit trail and can demand the platform operator produce records under defined process. This is the institutional analog of judicial subpoena power over a platform.

### 12.2 The institutional architecture parallels the technical architecture

The five-actor model is itself a *separation of concerns*: each actor has a defined scope, defined accountability, and a defined relationship to the engine's audit trail. Just as the engine separates concurrency from governance from audit (Vol II §III.1), the institution separates platform from sponsor from moderator. This is not coincidence; it is the same architectural principle applied at a different layer.

### 12.3 Open source as legitimacy

The codebase running a national consultation should be open source (AGPL or similar), publicly hosted, with public issue tracking and public deployment process. This is *not* a technical preference; it is a legitimacy requirement. The Decidim precedent — and the broader civic-tech consensus — is that closed-source platforms running citizen input cannot be trusted because their behavior cannot be verified. The codebase, the deployment configuration (sans secrets), the moderation policy, the identity policy, and the closeout report templates are all public artifacts. Reproducible builds of the deployed code allow technically-capable citizens to verify that the running system matches the published source.

---

## 13. Phased roadmap

The full system is large; a credible build path phases it. The recommended phasing, with each phase delivering deployable value:

**Phase 0 — Engine on a real document.** Take the existing engine (Volumes I & II) and use it to host one consultation, with a small invited expert panel (10–50 contributors at T2 only, no T1 reactor tier, no Polis clustering, no Sybil defense beyond basic email + rate limit). Outcome: validate that the engine works for a structured document at this small scale, build the per-clause UX. ~3 months.

**Phase 1 — Public T1+T2 launch on a constrained consultation.** Add the T1 reactor tier (vote-only), the moderation pipeline (Stages 1, 2, 3 — defer Stage 4 human-review at scale), the citizen UX (§8). Launch on a single, lower-stakes consultation (e.g. a municipal-level policy in a friendly jurisdiction). Target: ~1,000–3,000 participants. Outcome: real-world validation of moderation, UX, and the basic legitimacy claim. ~4 months.

**Phase 2 — Deliberation layer.** Add Polis-style clustering (§7.1), statement deduplication (§7.4), cluster summary with validation loop (§7.5). This is the phase where the platform becomes *categorically more useful* than a comment-stream platform. Outcome: bridge statements emerging from the data and visibly surfaced to participants and the sponsor. ~3 months.

**Phase 3 — T3 verification and the closeout discipline.** Integrate one civic-ID system (e.g. Bangladesh NID via the Election Commission API); build the closeout-report template and the diff-against-draft generation; deploy on a real national-scale consultation with the sponsor committed to closeout. Target: 10,000+ participants. Outcome: the full legitimacy stack proven on a real policy. ~4 months.

**Phase 4 — Multi-tenant, multi-jurisdiction, hardening.** Multi-tenancy (one platform install, many institutions), additional civic-ID integrations, transparency-report tooling, internationalization beyond Bangla/English, accessibility audit by certified body. ~ongoing.

Total time-to-credible-national-consultation: ~14 months from Phase 0 start. This is realistic for a focused team of 6–10 engineers + 2–3 deliberation/civic-tech specialists + the standing moderation team.

---

## 14. Risks and honest failure modes

A serious treatment of a serious idea ends with what can go wrong.

**The platform is captured by the operator.** If the platform operator is also the policy sponsor (despite §12.1's recommendation), the consultation is structurally untrustworthy. *Mitigation:* the open-source codebase and the audit trail mean even a hostile operator cannot silently alter the record, but they can decline to act on it. The structural defense is institutional, not technical: separation of operator from sponsor must be policy.

**The Sybil defense is broken by a state-level adversary.** §5.6 was honest about this. The platform's transparency reports will surface unusual patterns, and the deliberation layer's cross-cluster resistance limits damage, but a truly determined attack will compromise the input. *Mitigation:* report it visibly, distinguish T3 verified counts from raw totals, and accept that the consultation's outcome includes "what was the attempted manipulation" as a finding, not a hidden contamination.

**The moderation team is overwhelmed or biased.** Public attention spikes (a news cycle hitting the consultation) can overwhelm moderation. A moderation team drawn entirely from the sponsoring institution's perspective will bias the visible discourse. *Mitigation:* moderation team includes civil-society representatives (§12.1); peak-load contingency plans (e.g. on-call augmented moderation in coordination with digital-rights organizations); transparency report makes moderation rates visible.

**The deliberation layer's clustering is gamed.** A clever adversary, knowing how Polis clusters, can manufacture votes that *do* produce cross-cluster agreement (by puppeting multiple "clusters" with coordinated agreement on the adversary's preferred statements). *Mitigation:* this requires high-cost coordination across many fake identities at T3 (where verification costs are real), which moves the attack into the realm of criminal-prosecutable identity fraud rather than mere internet abuse.

**The closeout is performative but ignored in practice.** A sponsor publishes a beautifully-formatted closeout that does not actually engage with the input. *Mitigation:* the closeout template *requires* per-clause reasoning citing the bridge statements; the public, the press, and civil society can compare claimed engagement to actual diff. The defense is political, not technical; the platform provides the receipts that make the defense possible.

**The platform fails accessibility for excluded populations.** A phone-app-first design excludes those without smartphones; a Bangla-only design excludes minority-language speakers; a written-input design excludes those with low literacy. *Mitigation:* phone-line/IVR fallbacks for verbal input (converted to text downstream); in-person consultation sessions for digitally-excluded populations, with operators submitting on participants' behalf using a higher tier of attribution care; multilingual support beyond the two official languages where the consultation affects minority communities.

**The platform itself becomes the bottleneck.** The platform operator's reluctance, capacity, or political situation can become the constraint on consultation. *Mitigation:* the open-source codebase means consultations can be run by other capable operators if the primary fails or is captured. This is exactly the "multiple deployers, one codebase" property §12 emphasized.

---

## 15. What this design is, and what it is not

A brief, honest framing for the close.

**What it is.** A complete system architecture for taking the structured-document version-control engine of the original Dynodoc engine and applying it, with new layers above and around, to public consultation at the scale of tens of thousands of participants — keeping the engine's correctness guarantees (audit, attribution, semantic merge, content-addressed history) and adding the participation, identity, moderation, deliberation, presentation, and legitimacy layers that public participation actually requires. It assembles existing primitives — Polis clustering, proof-of-personhood, the Decidim modular philosophy, hash-linked audit trails — into a coherent stack whose components have been independently validated at scale.

**What it is not.** It is not a panacea for legitimacy. It cannot make a government act on its public's input; it can only make the failure to act visible. It cannot eliminate Sybil attacks; it can raise their cost and surface them in the public record. It cannot ensure participation by digitally-excluded populations alone; it must be combined with offline outreach that uses the platform as a back-end, not a front-end. It cannot replace political will; it can replace the *excuse* that gathering structured public input on a policy draft is too hard.

**The technical claim.** The same engine that serves a 5-person research team editing a survey instrument can — with the layers above it described here, none of which modify the engine — serve a 30,000-person consultation on a national policy. The core architectural insight of Volumes I and II — that *concurrency, audit, and governance are separable, and that semantic operations on stable-identity nodes are the right substrate* — survives the scale shift unchanged.

**The political claim.** This kind of platform, deployed with discipline, makes the failure modes of public consultation *legible* in a way that the alternatives (no consultation, paper consultation, free-form web comment) do not. That legibility is itself a public good, even when individual consultations disappoint. A society in which government drafts can be publicly scrutinized at clause-level, with attributable input, deliberation-extracted bridge consensus, mandatory closeout, and a tamper-evident audit trail — even imperfectly, even with adversarial pressure — is one with a stronger democratic infrastructure than one without that capability.

The engine was built for a research team. It turns out to also be the right substrate for a national consultation. That this is true is not coincidence — it follows from getting the foundations (Volume II Parts A–E) right. The same primitives that defend a survey instrument from accidental corruption defend a public consultation from adversarial corruption: stable identity, semantic operations, hash-linked audit, separation of governance from concurrency. The work this document does is to wrap those primitives in the additional layers that the *public* context demands. The engine remains, throughout, what it was.
