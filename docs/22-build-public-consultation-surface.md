# 11 · Public Consultation Surface (read-only public view)

### Claude Code prompt to start this stage

> Start the public consultation work. This stage builds the read-only citizen-facing surface for a document under consultation, with no participation features yet — just clean, fast, accessible viewing. The engine stays unchanged; this is a new frontend that consumes the existing API. Reference: `dynodoc_public_consultation_design.md` §8 (citizen UX). In `frontend/citizen/`:
>
> 1. **Routing**:
>    - `/` — landing page listing all open consultations, each with a one-paragraph summary, deadline, and participant count.
>    - `/c/[slug]` — consultation home. One-paragraph problem statement, draft preview, time left, "read the draft" CTA.
>    - `/c/[slug]/read` — the document itself in citizen-readable format.
>    - `/c/[slug]/clause/[node_id]` — per-clause permalink (the only granular page; everything else lives within `/read`).
>    - `/c/[slug]/about` — process information: who is the sponsor, the moderation policy, the closeout commitments, the platform's transparency reports.
>
> 2. **Visual style**: distinct from the researcher app. The citizen app reads as official government infrastructure (the sponsor's identity, not the platform's): clean sans-serif, generous spacing, high contrast, no decorative flourishes. Use the sponsor's color palette (configured per-tenant); fall back to a neutral civic blue-gray. Bangla typography correct: Noto Sans Bengali / Solaiman Lipi as the Bangla face, with Inter for English.
>
> 3. **Bilingual rendering**: every text content node has optional `text_en` and `text_bn` fields. The page is presented in one primary language with a one-tap toggle. The URL preserves the language choice (`?lang=bn`).
>
> 4. **Reading view design**:
>    - Long-form document layout: ~640px max content width, 18px body text, 1.7 line height, generous paragraph spacing.
>    - The document tree is rendered with section headings, subsection headings, and clause text. Numbering is automatic from the tree position.
>    - Each clause has a small permalink anchor (the `node_id`) but the URL-bar reflects the section, not the clause.
>    - A floating "table of contents" sidebar on desktop, a collapsible drawer on mobile. Updates highlight as the reader scrolls.
>
> 5. **Per-clause panel** (the right rail on desktop, expandable below clause on mobile): shows the aggregate state of citizen input for that clause:
>    - "Read by X citizens · Discussed by Y · Suggested edits: Z"
>    - The bridge-consensus statements (rendered as quotes, ≤140 chars each, with vote counts).
>    - The divisive issues (rendered as "citizens are split on…" with cluster counts).
>    - Buttons "see all input" (expands to a stream view) and "add your view" (links to stage 12 participation).
>
> 6. **Server-side rendering**: SvelteKit's SSR mode. First paint is ≤200 KB total payload, ≤500 ms time-to-interactive on a simulated 3G connection. The reading view should work without JavaScript at a degraded but usable level (the citizen can read, navigate clauses, and click through to participation pages that prompt for full-app load).
>
> 7. **Accessibility**: WCAG 2.2 AAA where feasible:
>    - Reading view text contrast ≥7:1.
>    - Skip-to-clause anchor at the top.
>    - Heading hierarchy clean (only one h1 per page; clauses use h3/h4).
>    - Live regions announce when the per-clause panel updates.
>    - All interactive elements reachable by keyboard with visible focus.
>
> 8. **Cache strategy**: aggressive CDN caching with revalidation:
>    - The document body is cached for 5 minutes (stale-while-revalidate).
>    - Per-clause aggregate counts are cached for 60 seconds with `Cache-Control: public, s-maxage=60, stale-while-revalidate=300`.
>    - The CDN serves nearly every read; the origin sees only writes and uncacheable interactive pages.
>
> Tests:
>    - Lighthouse score on slow 3G simulation: Performance 90+, Accessibility 100.
>    - SSR-only path: load with JavaScript disabled in headless Chrome → page renders, clauses readable, navigation usable.
>    - Visual regression tests via Playwright on three viewports (mobile 360px, tablet 768px, desktop 1280px) in both languages.

### Why this is a clean stage break

The participation features (vote/comment/suggest) come in stage 12. By landing this stage first, you can deploy a read-only consultation surface — useful in its own right: a government can publish a policy draft with no input features and still get the audit-trail, hash-linked record, transparency benefits. Some early Decidim deployments do exactly this.

It also lets you stress-test the read-path scaling separately from the write path. CDN caching, SSR rendering, accessibility, bilingual layout — these are all things you want working before you add participation traffic on top.

### The bilingual rendering decision

Some choices to make at this stage that ripple:

**One canonical language per consultation, optional translations of clauses.** The sponsor authors the draft in Bangla; English is an optional translation per clause provided by the sponsor or translation contractors. The reading view shows the canonical with a toggle to the translation. *Not* parallel parallel-rendering side-by-side (cluttered on phones).

**Citizen contributions in either language.** A T2 user can comment in English on a Bangla clause; the platform displays the comment in its language with a "translate" affordance (machine translation via the LLM service). The original is preserved; the translation is marked as machine-translated.

**Language detection for incoming text.** Use a small language-id library (`fasttext` or `lingua`) to tag every incoming text with its language. This drives both translation suggestions and the moderation classifiers (which are language-specific).

### Watch out for

**Bangla font sizing.** Bangla glyphs are taller than Latin; same point size renders larger. Test layout with real Bangla text, not lorem ipsum. Account for line height inflation.

**SSR + auth.** The reading view is public (no auth needed). But the per-clause aggregates that depend on identity context (e.g. "you have already voted on this") need user state. Resolve by hydrating client-side on the second pass — SSR delivers the aggregate counts without personalization; the JS hydration adds the personal layer.

**Right-to-left readiness.** Bangla is LTR but if you ever extend to Urdu or Arabic, the layout must flip. Use CSS logical properties (`margin-inline-start`, not `margin-left`) throughout. Costs nothing now, saves work later.

### Verification checklist

- [ ] First paint ≤200 KB on the reading view.
- [ ] Lighthouse 90+ Performance, 100 Accessibility on slow 3G.
- [ ] SSR-only path produces a usable page.
- [ ] Bilingual toggle works without page reload.
- [ ] All clauses are permalinkable.
- [ ] Visual regression suite green.
