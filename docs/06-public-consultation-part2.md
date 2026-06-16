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
