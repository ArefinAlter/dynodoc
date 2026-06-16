# Research IDE + Public Consultation Platform — Full Reference Series

This is the complete numbered sequence of documents for designing and building the platform. Read them in order.

## Layout

The series has three groups, in increasing concreteness:

**Files 00–04 · Engine design and mathematics (the foundation).** What the system is, why it has the shape it does, and the formal mathematics that proves the core engine correct. Read these to understand *why* the build decisions later were taken.

**Files 05–10 · Public consultation extension (the scale-up).** How the same engine extends from a small expert team to thousands of public contributors on a government policy draft. The new layers (identity, moderation, deliberation, presentation, legitimacy) that sit above the unchanged engine.

**Files 11–31 · Build reference (the implementation).** A numbered, sequential reference for actually building the platform end-to-end. Each file represents one stage of work (~1–3 weeks for a small team) with a Claude Code prompt at the top, specific decisions to make, code sketches where helpful, watch-out points, and a verification checklist.

## Full sequence

```
── Foundation: engine design ──────────────────────────────────────────────
00 · Feasibility assessment
01 · UI/UX theory
02 · Versioning protocol Volume I (design narrative)
03 · Versioning protocol Volume II (rigorous mathematics)
04 · Combined volumes I + II with reading guide

── Extension: public consultation ─────────────────────────────────────────
05 · Public consultation part 1 (framing, sub-problems, scale)
06 · Public consultation part 2 (layer stack, Sybil & identity)
07 · Public consultation part 3 (moderation & deliberation)
08 · Public consultation part 4 (citizen UX, legitimacy, prior art)
09 · Public consultation part 5 (infrastructure, governance, roadmap, risks)
10 · Public consultation combined (parts 1–5 in one document)

── Implementation: build reference ────────────────────────────────────────
11 · Build index and stack decisions
12 · Repository and development environment setup
13 · Database schema and migrations
14 · Event log core (the irreducible foundation)
15 · Snapshot engine and state materialization
16 · Semantic operation vocabulary and governance gate
17 · Authentication and identity tiers (T1–T2)
18 · Web API surface (REST + Server-Sent Events)
19 · Research IDE frontend (the Phase-0 deliverable)
20 · CRDT convergence layer (for live co-edit)
21 · Referential integrity and semantic merge engine
22 · Public consultation surface (read-only public view)
23 · Citizen participation (vote / comment / suggest)
24 · Moderation pipeline (Stages 1–3 automated)
25 · Moderation pipeline (Stage 4 human review + appeals)
26 · Deliberation layer (Polis-style clustering)
27 · AI summary with validation loop
28 · T3 verified citizen identity (civic-ID integration)
29 · Closeout reports and verifiable counts
30 · Transparency reports and audit tools
31 · Multi-tenant hardening and deployment
```

## How to read these

If you are **designing**: read 00 → 04 in order. The combined Volume I + II (file 04) duplicates 02 + 03; pick one form depending on your reading style.

If you are **scoping public consultation specifically**: read 05 → 09. File 10 duplicates 05–09 in one document.

If you are **building**: read 11 (stack decisions, repo layout) first. Then proceed through 12 onward in order. Each subsequent file assumes the prior ones are done.

If you are **briefing a sponsor or funder**: read 00 (feasibility) and 09 (risks + roadmap) — these are the lay-readable framings.

If you are **doing the math review**: file 03 alone (Volume II) is the proof-bearing companion.

## What is *not* in this series

- The dark-themed PDF render of the engine documents (built via the puppeteer-core pipeline; file lives separately as `research_ide_versioning_complete_dark.pdf`).
- The interactive UX prototype HTML files (`research_ide_interactive.html`, `research_ide_interface.html`) — these are reference artifacts for the frontend stage (file 19), not part of the reading sequence.

## Companion artifacts

The dark PDF and the prototype HTML files sit in the same outputs directory but outside this numbered sequence. Reference them where stage 19 (research IDE frontend) directs you to port the prototype's visual design.

---

Total: ~80,000 words across 32 files. The first 11 files are roughly half the volume (the design carries the weight of the system); the 21 build files are densely actionable rather than expository.
