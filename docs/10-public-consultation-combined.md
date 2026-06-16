# Public Consultation at Scale — A System Design for Dynodoc Protocol Applied to Policy Drafts

### From a small expert team to thousands of citizen contributors on a government document

**Status:** Design exploration, follow-on to the two-volume versioning protocol
**Companion documents:** `dynodoc_versioning_complete.md` (the underlying engine — Volumes I & II)
**Reading time:** sequential — Part I sets the problem, Parts II–VI build the answer

---

## 0. The problem, stated precisely

We have an engine designed for a small team of expert collaborators editing a structured research instrument: stable-identity nodes, semantic events, CRDT convergence within a draft, governance over proposed events, content-addressed hash-linked history. The previous documents argued that engine is correct *for that profile* — a handful of named, capable, accountable authors whose changes are valuable and whose attention is the scarce resource.

You are now asking: what happens when we open the same protocol to **thousands of public contributors on a government policy draft** — the consultation analog of a public GitHub repository on a national document? Imagine the Bangladesh National AI Policy 2026, or the National Startup Policy, posted for thirty days, and twenty thousand citizens, journalists, lawyers, industry associations, NGOs, opposition figures, foreign actors, and bots all hitting it at once.

The honest answer that this document defends, in one sentence: **the core engine is correct and re-used unchanged; almost everything that has to be added sits *above* the engine — in the public-participation, identity, moderation, deliberation, and presentation layers — and most of it has nothing to do with version control at all.** The interesting design work is in five new sub-problems that the original document did not have, and they dominate the system. Naming them precisely matters because they are different problems with different solutions, and a system that conflates them fails in ways the engine alone cannot rescue.

---

## 1. The five sub-problems the original design does not solve

The original design assumed a closed authoring team. Opening the same protocol to public contributors introduces five categorically new problems. They are not engine problems; they are *layer-above-the-engine* problems, and each has its own substantial literature.

**1.1 The Sybil problem.** The original system trusts that "author Aluk" is one human Aluk. In public consultation that assumption collapses: a single actor can spin up thousands of accounts to amplify a position, simulate consensus, drown out dissent, or just exhaust moderator attention. This is the *Sybil attack* (Douceur, 2002), and it is the single most consequential failure mode of any public-input system. Without an answer to it, every other quality of the platform is undermined — clustering shows fake consensus, voting reflects fake support, the "popular" suggestions are bot-amplified, and the consultation's output is uninterpretable as a signal of public opinion. Solving it without demanding intrusive identification from every citizen is the central design tension of the participation layer.

**1.2 The signal-to-noise problem (the funnel).** With twenty thousand contributors there will be tens of thousands of comments, suggestions, alternative versions, edits to identical clauses, duplicate ideas expressed differently, vehement disagreement on the same line, and far more low-quality input than high-quality input. The engine *can* hold all of it — every contribution is a proposed event in the log — but a sponsor reading the result needs something distilled: where is the public consensus, where is the genuine disagreement, what are the orthogonal positions, what is the agenda-setting voice. This is the *deliberation* problem (rough consensus, opinion clustering, bridge statements), not a version-control problem, and it has been worked on extensively (Polis/vTaiwan, Decidim, Consul). Naive ranking by upvotes is *exactly* the failure mode Polis was created to fix.

**1.3 The moderation problem.** Public participation attracts hate speech, defamation, doxing, off-topic material, advertising, coordinated harassment of specific contributors, and content that creates legal exposure for the sponsoring institution. At twenty thousand contributors and tens of thousands of items this *cannot* be done by hand at the rate it arrives. It also cannot be left undone, because one piece of unmoderated illegal content is a regulator's headline. Moderation at this scale requires a layered pipeline (automated filtering → community signals → human review of the residue) with appeal, transparency, and audit — and most of that pipeline is well outside what version control does.

**1.4 The accessibility problem.** The previous design's data model is a typed tree of nodes with `varName`, `relevant`, `constraint` — concepts that are second nature to a researcher and Greek to a citizen. A retiree commenting on a pension reform clause should not have to learn what a "relevance expression" is, or pick between a "suggested rewording" and an "alternative draft," or understand that their comment "this is unfair to widows" is being recorded as a `CommentAdded` event on `item_3.4.2`. The original document already made the engine produce researcher-friendly surfaces (the §III.5 translation table in Volume I); the public surface must go a step further and present the *document* in a form a citizen can read and engage with on a phone in five minutes. This is partly UX, partly content presentation, partly the question of *what units of the document are even contributable* — sentence? paragraph? section?

**1.5 The legitimacy problem.** A consultation is not just an input pipeline; it is a public act with political consequences. A sponsor who runs a consultation, then ignores it, harms trust more than not having run it at all. A consultation whose results are *misrepresented* — "the public supports clause 7" when in fact the bot-amplified portion supports it — does worse. A consultation whose process is opaque ("we considered all your input and decided…") cannot be defended in parliament or in court. Legitimacy demands public process: published rules of acceptance and rejection, attributable decisions, an audit trail of what was kept and what was discarded with reasons, machine-verifiable counts, a closeout report that the public can challenge. The *audit* part of this is exactly what the engine was built for; the *process* and *closeout* parts are new.

These five — Sybil, signal, moderation, accessibility, legitimacy — are the new design surface. They are largely orthogonal to one another, they have separate literatures, and the architecture below addresses them as separate layers atop the unchanged engine.

---

## 2. The scale shift, quantified

A back-of-envelope to make the architecture decisions defensible rather than asserted. Take a concrete instance: a national policy draft of about 40 pages, opened for public consultation for 30 days, in a country with a digitally engaged population of a few million.

- **Document:** ~40 pages, ~120 sections/clauses, ~800 paragraphs. Roughly 1,200 addressable nodes in our tree model.
- **Participants:** generous projection, 20,000–50,000 distinct contributors over 30 days (Decide Madrid hit ~400,000 registered users over years; vTaiwan engaged 200,000 across many consultations; the BD National AI Policy realistically draws a smaller absolute number but proportionally similar). Let us use **30,000** as a working figure.
- **Engagement intensity:** a power-law distribution holds in every empirical study of public consultations. Roughly 1% of participants leave 50% of the input, 10% leave 90%, the remaining 90% read or vote without contributing text. Translating: **~300 heavy contributors, ~3,000 moderate, ~27,000 light/passive**.
- **Total contributions (events on the log) over 30 days:** estimate ~100,000 suggestions/comments (heavy 300×100 + moderate 3,000×20 + light 27,000×1 ≈ 87,000, round up) plus ~500,000 votes/upvotes/agree-disagrees on others' contributions. Call it **~600,000 governance events**.
- **Peak rate during a news-driven spike** (the policy hits the front page, a celebrity tweets about it): 10× the average for an hour. Average rate is ~600,000 / (30 × 86,400) ≈ 0.23 events/sec; peak ~2–3 events/sec sustained, with bursts to 10+. Modest by web standards.
- **Read load:** much higher — a single news cycle can drive 100k–1M page reads over a few days. This is the dominant infrastructure pressure, and it is *cacheable* (the read model is mostly static between sponsor pushes).

The scale numbers are encouraging in one respect: **write rates are well within what a single small Postgres instance handles**, and the engine's per-event cost ($O(1)$ append, semantic-event sized) holds easily. The hard scaling problems are not in the storage engine. They are in the *participant management* (30,000 Sybil-checked identities), the *signal extraction* from 100,000 suggestions (clustering, ranking, summarization), and the *moderation pipeline* (filtering 100,000 items, of which perhaps 2–5% need human review — call it 2,000–5,000 human-moderation decisions, or ~70–170 per day spread across moderators). Those are the bottlenecks; the architecture is shaped around them.

Two further observations that drive design:

The **engagement is bursty and front-loaded**: typically 50–60% of contributions arrive in the first week (often the first 72 hours after launch and any news cycle), then a long tail. The system must survive the burst without back-pressuring legitimate participation, and the moderation pipeline must keep up or the consultation visibly degrades.

The **non-contributor majority is the silent supermajority** whose perspective the consultation is *supposed to surface* — the 27,000 who read but did not write. A successful design extracts signal from their *votes*, *agree/disagrees*, and *read-time patterns*, not only from the 300 loudest voices. This is the central insight of Polis (§4), and it inverts the GitHub model entirely: most participants in a healthy consultation are *not contributors at all*.

---

## 3. What stays exactly the same: the engine is correct, reused unchanged

Before designing what is new, it is worth stating clearly what is *not* new, because the temptation will be to rebuild the engine to "scale" and that is wrong.

The four-layer architecture of Volume II §D.3 — governance, audit/history, convergence, merge engine — applies *unchanged* to the public consultation case, with one re-mapping: **the "draft" is now the public consultation document, the "suggestions" are now public submissions, the "reviewers" are now citizens, and the "PI / accept-reject authority" is now the sponsoring institution's consultation team plus the legitimacy process of §6 below.** Every claim proven in Volume II about that engine still holds:

- Disjoint-node operations still commute (Vol II §D.2); the public submission stream is still a CmRDT on disjoint nodes, so 30,000 people commenting on 1,200 different paragraphs converge with no fuss.
- The append-only hash-linked log (Vol II §E.2) gives the tamper-evidence the legitimacy problem (§1.5) requires; this is now *load-bearing politically*, not just methodologically.
- Stable-identity nodes (Vol II §D.1) mean that when the sponsor publishes "draft v2 incorporating consultation input," every accepted citizen suggestion remains attributable to its individual author by node identity, forever and traceably.
- The semantic operation vocabulary (Vol II §D.2) gives us a public-facing change log in researcher-legible terms ("123 citizens proposed reword of §4.2; sponsor accepted the rewording of 14 January 2026 by Citizen #4471").

The engine does the *right* thing here. It is the *layers above and around it* that change. Everything that follows — Parts II through VI — is the design of those new layers; the engine is treated as a stable substrate they call into.

This is the most important architectural commitment of this document: **do not modify the engine to accommodate scale or publicness. Build new layers that translate public participation into engine operations, and present engine state back to the public.** That commitment is what lets the same protocol serve a 5-person research team and a 30,000-citizen consultation without forking the codebase.
---

# Part II — The new layer stack, and identity (the Sybil problem)

The engine occupies the bottom of the stack. Above it sit five new layers, each addressing one of the §1 sub-problems. The stack reads bottom-up the way the original four-layer engine read top-down: the engine is the substrate; participation, identity, moderation, deliberation, presentation, and legitimacy build on it.

## 4. The full layer architecture

```
┌──────────────────────────────────────────────────────────────────────────────┐
│  L8  PRESENTATION & CITIZEN UX                                                 │
│      readable document; phone-first; bilingual; one-tap participation;         │
│      "what changed in this draft since v1?" public diff; closeout report       │
├──────────────────────────────────────────────────────────────────────────────┤
│  L7  DELIBERATION & SIGNAL EXTRACTION                                          │
│      opinion clustering (Polis-style); bridge statement detection;             │
│      duplicate-collapse; rough-consensus computation; sponsor dashboards       │
├──────────────────────────────────────────────────────────────────────────────┤
│  L6  MODERATION PIPELINE                                                       │
│      automated filters → community flags → human review → appeal;              │
│      visible / shadow-hidden / removed states; audit log of all decisions      │
├──────────────────────────────────────────────────────────────────────────────┤
│  L5  PARTICIPATION & SUBMISSION                                                │
│      submit comment / suggestion / vote / agree-disagree / propose alternative │
│      rate-limit per identity; abuse-pattern detection; first-time gentling     │
├──────────────────────────────────────────────────────────────────────────────┤
│  L4  IDENTITY & SYBIL RESISTANCE                                               │
│      tiered identity (anonymous → verified citizen);                           │
│      proof-of-personhood without surveillance; civic-ID integration optional   │
│      capability tiers gate which operations a participant can perform          │
├──────────────────────────────────────────────────────────────────────────────┤
│  L1–L3  THE ENGINE  (the original four-layer architecture, unchanged)          │
│      governance · audit/history · convergence · merge                          │
│      semantic event log, content-addressed, hash-linked, SEC within drafts     │
└──────────────────────────────────────────────────────────────────────────────┘

Cross-cutting:  legitimacy/audit (§6), telemetry, transparency reports, accessibility
```

Each new layer translates a public-participation concept into engine operations (downward) and presents engine state in a citizen-comprehensible form (upward). The downward translations create *governance events* in the engine's vocabulary; the upward translations are *projections* of the event log, materialized as cached views (Vol II §D.5's snapshot mechanism, generalized).

Two design properties of this stack are worth stating explicitly:

**Each layer can be replaced independently.** The identity layer can run on civic-ID, on email-only, on biometric proof-of-personhood, or in a hybrid tiered model (§5), without touching anything above or below. The moderation layer can use whatever classifier/policy the sponsoring institution requires (some jurisdictions will demand on-premise classifiers; others can use managed services). The deliberation layer can use Polis-style clustering, simple agree/disagree, or a more ambitious system, swapped in based on sophistication and budget. *This pluggability is essential* because public consultation tools that hard-wire one identity scheme or one deliberation algorithm cannot be deployed across jurisdictions with different legal and political requirements — and the very same engine should serve a Bangladesh National AI Policy consultation, an icddr,b protocol review, and a municipal participatory budget in Cox's Bazar.

**Engine operations remain semantic.** The downward translation is a thin adapter — a citizen comment becomes `CommentAdded(node_id, text, author_id, lang)`, an upvote becomes `VoteCast(target_event_id, +1, author_id)`, a suggested edit becomes `SuggestionProposed{wrappedEvent: PromptEdited{...}}`. These are the same event types the engine already knows. The audit guarantee (Vol II §E) and the governance gate (Vol II §D.3 L4) apply uniformly. *The engine doesn't know it is being talked to by 30,000 people instead of 5.*

---

## 5. Layer 4 — Identity and the Sybil problem

The single hardest design question in this entire stack. Get it wrong and everything above is corrupted; get it too heavy-handed and citizens won't participate; get it ethnographically wrong (e.g. require smartphones in a country where rural women's phone access is family-mediated) and the consultation systematically excludes the very population it claims to engage. This section walks the decision space rigorously.

### 5.1 The Sybil attack, formally

Following Douceur's original formulation (*The Sybil Attack*, IPTPS 2002): in any open network where participation is unrestricted and identity is cheap, a single adversary can create an arbitrarily large number of pseudonymous identities and use them to disproportionately influence any process that treats identities as equally weighted. The attack is *cheap to mount and expensive to defend against*, which is its asymmetry.

For a public consultation, the attack surface includes — and most of these have been observed in real deployments:

- **Astroturf consensus:** flood agree-votes on a position to make it appear popular.
- **Drown-out:** flood disagree-votes on opposing positions to bury them.
- **Anchor injection:** post a position early under many identities so it dominates the deliberation layer's clustering and downstream attention.
- **Exhaustion:** generate enough low-quality contributions to overwhelm moderation, causing the platform to either choke (degrade) or relax filtering (legitimize the noise).
- **Targeted harassment:** create disposable accounts to attack named contributors and force them out.

The defensive primitive is **proof of personhood (PoP)** — a credential asserting "this account is operated by one and only one distinct human, with reasonable assurance and without giving up that human's other privacy." The literature here is rich (Borge et al. 2017; Siddarth et al. 2020; Buterin's "decentralized society" writing; the Civic / WorldID / BrightID lineage), and the conclusion the literature reaches, which we adopt, is: **no single PoP mechanism is sufficient or appropriate alone; the practical answer is a tiered identity system in which capability scales with assurance.**

### 5.2 The tiered-identity design

The model we adopt has four tiers. A participant can move between tiers freely, and the platform reveals only the tier (not the underlying credential) for any given action.

| Tier | What it requires | What it grants | Sybil cost to attacker |
|---|---|---|---|
| **T0 · Observer** | nothing | read the document, read others' input, read the consultation's evolving state | n/a (read-only) |
| **T1 · Reactor** | email or SMS (rate-limited; CAPTCHA on entry) | vote agree/disagree on others' statements; minimal write | low — minutes per fake identity, but votes are weighted at the lowest level |
| **T2 · Contributor** | T1 + a light proof-of-personhood signal: phone number + behavioral (time on site, interaction pattern), or a third-party social proof (Google/civic-login attestation) | submit comments, suggestions; vote at standard weight | medium — costs phone numbers (≈ $0.05–1 each at scale) plus behavioral plausibility |
| **T3 · Verified citizen** | T2 + a strong personhood credential: government ID (national ID, election commission roll), or biometric attestation by an accredited registrar, or in-person verification at a civic site | weighted vote in official tallies; ability to formally co-sign a position; appears in the consultation's verifiable count of citizen support | high — requires identity fraud per fake account |

Two design subtleties make this tractable rather than dystopian.

**Subtlety A: T3 is optional, not gating.** A citizen can participate fully at T2 — comment, suggest, vote, see their input reflected in the deliberation layer — without ever submitting government ID. T3 only unlocks the *additional* property of *counting in the verifiable-citizen tally*, which the sponsor uses for the legitimacy claim "X verified citizens of N total weighed in." This means a privacy-conscious citizen, an undocumented person, a member of a community with reason to distrust ID systems, a minor, or a foreigner with legitimate interest, can all participate meaningfully. The Sybil defense at T3 is strong; the participation gate at T2 is light. *This is the central trade-off, and it is calibrated so that legitimacy and inclusion both survive.*

**Subtlety B: the verification is held by the identity layer alone and not exposed to the engine or to other participants.** The engine sees `author_id` (a stable opaque identifier) and `author_tier` (T1/T2/T3); it does not see the underlying credential. The participant's name, ID number, biometric data — if any — live in the identity layer's separate store, with separate access control, separate retention policy, and (ideally) separate hosting in a jurisdiction the sponsor's data-protection law mandates. This is the standard *separation of identification from action* pattern, and it is what lets the consultation be both attributable enough to be legitimate and private enough to be safe.

### 5.3 Proof-of-personhood mechanisms — the actual menu

Within this tiered design, T2 and T3 must each pick one or more concrete PoP mechanisms. The honest menu, with trade-offs:

**Civic-ID / national-ID integration (strongest for T3 in democracies with rolls).** A government's existing identity infrastructure — Bangladesh's National ID (NID) and the Election Commission roll, India's Aadhaar, Estonia's eID, Taiwan's MOI — already does the hard work of one-person-one-identity. The platform integrates as a *relying party*, gets a signed assertion "this is a verified resident over 18," and stores only a zero-knowledge attestation that *some* valid ID was used (not which). This is the cleanest design where it is available; it inherits the trust and the abuses of the underlying system, so a deployment in a jurisdiction with a *politicized* ID system needs the T2 escape hatch (5.2 Subtlety A) all the more.

**Phone-number-based attestation (the practical T2 default).** A unique phone number per identity, with rate limits, network-provider data flagging known SIM-farms, and behavioral signals. Cheap, friction-light, but defeated by an attacker with a SIM farm — well-documented in election-misinformation literature. Effective against amateur Sybils; insufficient against a determined state-level adversary. Acceptable for *most* of what T2 needs because T2's actions are individually low-impact (comments and suggestions don't bind anything until the sponsor or community lifts them).

**Biometric proof-of-personhood (controversial, sometimes appropriate).** Worldcoin/World ID, Civic's facial-liveness check, FaceTec attestations. These produce strong unique-human guarantees by capturing biometric data and proving "this is a previously-unseen human" via privacy-preserving (sometimes) cryptography. The technical strength is real; the political and ethical cost is also real (biometric capture is irreversible if leaked, raises serious concerns about coercion and surveillance, and is illegal in some jurisdictions). **Not recommended as a default; offer it only as an optional T3 path in contexts where it is clearly proportionate and lawful, and never make it a participation requirement.**

**Web-of-trust attestations (social PoP).** BrightID-style: a user is verified by being attested-to by N already-verified users, often through video calls or in-person meetings, building a graph that is hard for Sybils to embed in. Strong privacy properties; scales poorly to new entrants; concentrates power in the initial-trust-seed group. Useful as an alternative path *into* T2 for those without phones, but not a primary mechanism.

**Behavioral / temporal signals (not PoP, but Sybil-mitigation).** Even without strong PoP, the platform can detect Sybils statistically: account creation bursts from the same network range, voting patterns too coordinated to be independent, content too similar across "different" accounts. These are *defensive* signals, not credentials; they feed the moderation layer (§6) and the deliberation layer's clustering (§7), where they downweight (not exclude) suspicious activity.

**A note on Audrey Tang's framing.** The Taiwan / vTaiwan tradition makes a principled argument that the right defense is *not primarily authentication* but *deliberation design that resists Sybil amplification structurally* — Polis's clustering algorithm, for instance, surfaces *bridge statements that win agreement across clusters*, which is exactly the signal a Sybil flood cannot fake (a single actor cannot manufacture cross-cluster agreement, only same-cluster amplification). We adopt this view in §7: the deliberation layer is designed to be Sybil-resistant *in its outputs*, not only in its inputs. This is defense in depth — strong identity *and* Sybil-resistant aggregation.

### 5.4 The capability matrix, formally

The identity tier determines what engine operations a participant may perform:

| Engine operation | T0 | T1 | T2 | T3 |
|---|:-:|:-:|:-:|:-:|
| Read document, log, derived views | ✓ | ✓ | ✓ | ✓ |
| `VoteCast` (agree/disagree on others' positions) |  | ✓ | ✓ | ✓ |
| `CommentAdded` |  |  | ✓ | ✓ |
| `SuggestionProposed` (wrapping any content event) |  |  | ✓ | ✓ |
| `AlternativeCreated` (propose alternative wording of a clause) |  |  | ✓ | ✓ |
| Vote in the *verified-citizen tally* used in the sponsor's legitimacy claim |  |  |  | ✓ |
| Formally co-sign a position (for advocacy NGOs / civil-society groupings) |  |  |  | ✓ |
| Sponsor / moderator / accept-reject |  |  |  | sponsor role (separate from T3) |

A participant may freely upgrade tiers. Downgrade is also possible (deleting one's credential reverts to a lower tier but does *not* retract prior contributions — those remain attributable to the pseudonymous `author_id`, an important property: *the system is irreversibly attributable from the moment of contribution, which is the audit guarantee, even if the participant later wishes to be less visible*).

### 5.5 Cost and rate-limiting as part of identity

A complement to identity assurance: per-identity *rate limits* and *cost gradients*. T1 votes are rate-limited (e.g. one vote per statement, ~100 statements/day), as are T2 comments (e.g. 20 comments/day, 5 suggestions/day, with longer cooldowns for repeated submissions on the same node). These limits don't stop a determined Sybil but they *raise the cost per act of influence*, which combined with identity tiers raises total attack cost into the range where it can be detected by behavioral signals and pursued legally.

A finer technique borrowed from anti-spam literature: **first-time gentling** — a brand-new T2 account's first three contributions are shadowed pending light moderation (visible only to the author for ~30 minutes) so that an attacker who creates 1000 throwaway accounts cannot use them simultaneously. This costs legitimate participants a small delay on their first post; it costs a Sybil attacker a 30-minute multiplier on every wave.

### 5.6 The honest residual risk

After all of the above, a sophisticated, well-funded adversary — a state intelligence service, a corporate actor with millions to spend, a coordinated political campaign — *can still* mount Sybil attacks of substantial scale. No identity system stops this completely. The honest design posture is:

- T3 raises the cost to a level where attacking with verified citizens requires actual identity fraud at scale, which is criminally prosecutable.
- T2 with rate limits and behavioral signals makes mass low-effort attacks expensive and detectable.
- The deliberation layer (§7) is structured so that even moderate Sybil contamination of T1/T2 cannot *manufacture cross-cluster consensus* — the signal sponsors most care about.
- Transparency reports (§9) publish suspected-Sybil-activity statistics so that any *adversarial influence on the consultation* is itself visible and debatable, not silently absorbed.

This is the same posture security engineers take everywhere: not "make attack impossible" (which is impossible itself) but "raise the cost above the value, make attacks detectable, and ensure detection is itself part of the public record." The consultation that gets attacked and *says so* is more trustworthy than one that claims it wasn't.
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
