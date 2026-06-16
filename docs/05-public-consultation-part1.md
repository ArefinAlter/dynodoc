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
