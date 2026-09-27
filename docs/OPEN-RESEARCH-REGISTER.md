# Open research register

Opened 23 September 2026. A single place for the questions the research reviews
raised but did not settle, so they are not lost across six documents.

**How to use this.** Each item states the question, why it matters, and what would
settle it. Items are referenced by ID (for example `E3`, `V2`) in commits, decision
records and future reviews. When an item is settled, record the answer and the date
in place rather than deleting it — a closed question with evidence is worth more
than a blank line.

Sections:
[A. Experiments to run](#a-experiments-to-run) ·
[V. Facts to re-verify](#v-facts-to-re-verify) ·
[P. Prior art to read properly](#p-prior-art-to-read-properly) ·
[L. Legal and IP](#l-legal-and-ip) ·
[S. Strategic assumptions](#s-strategic-assumptions) ·
[T. Deferred by tripwire](#t-deferred-by-tripwire)

Sources: [block versioning literature review](BLOCK-VERSIONING-LITERATURE-REVIEW.md),
[platform and strategy review](PLATFORM-AND-STRATEGY-REVIEW.md),
[rich-text algorithm research review](RICH-TEXT-ALGORITHM-RESEARCH-REVIEW.md).

---

## A. Experiments to run

Ordered by value. `A1` gates most of the round-trip work and costs hours.

| ID | Question | Why it matters | What settles it | From |
|---|---|---|---|---|
| **A1** | After a Word desktop save and a Word-for-the-web save with **no edits**, what share of untouched blocks fail to match on the reconciliation hash? | If untouched paragraphs do not match, manifest-first reconciliation fails and no carrier engineering compensates. Everything downstream depends on this number | Export a real document, save untouched in both, re-import, count false negatives. Tune the lossy projection until near zero | Lit review §19.7 step 0a |
| **A2** | What is the reconciliation-hash collision profile on real corpora, especially questionnaires with repeated choice labels ("Yes", "No", "Don't know")? | Repeated text must never serve as an alignment anchor. Too loose a hash produces false block matches, which is worse than no match | Compute hashes across real corpora; confirm collisions are confined to genuinely repeated text | Lit review §19.7 step 0b |
| **A3** | Full edit matrix: recovery rate per tier, false matches, moves detected, per editor (Word desktop, Word for the web, LibreOffice, Google Docs export) | Determines whether the round trip is a tier 1–2 system with graceful fallback or a tier 3 system relying on luck | The experiment in lit review §19.5, run after A1 and A2 pass | Lit review §19.5 |
| **A4** | Which Word and Excel features actually appear in **20 real documents and 5 real spreadsheets** from target users? | Converts the open-ended "how long to build the 80/20" into a finite ordered list, locates the editor threshold, and produces the DOCX corpus A3 needs | Collect and count. Days of work | Strategy §B.4 |
| **A5** | How does Word behave with **thousands of locked content controls** in one document? | Layer B of the identity design assumes locked controls are viable at document scale. 8,000 controls is an unusual configuration | Generate a realistic document with per-block `sdtLocked` controls; measure open time, edit latency, save behaviour, and whether controls survive | Lit review §19.7 |
| **A6** | Storage breakdown by table and column (events, snapshots, TOAST, indexes, WAL) on a representative corpus | Compression effort is currently aimed by assumption. The review argues the waste is in snapshots and full-state work, not events — unverified | Measure and record next to the existing benchmarks | Lit review §14 step 1 |
| **A7** | Does the engine's storage and load profile hold up against the **standard editing traces** (automerge-perf, crdt-benchmarks, Eg-walker artifact S1–S3 / C1–C2 / A1–A2)? | Makes Dynodoc's numbers comparable to published work and supplies adversarial cases the synthetic fixture lacks | Run the codec against the public traces; publish the comparison | Algorithm review §4.7 |
| **A8** | What do real documents actually **fail** on, once users are on the system? | Sets the scope of document CI. Guessing the check list wastes the feature | Instrument failures during the 10-user pilot; let the check suite follow the evidence | Strategy §E.6 |

---

## V. Facts to re-verify

Competitive and vendor facts with a short shelf life. **Re-verify before relying on
any of these in a decision or a pitch.**

| ID | Fact as recorded | Recorded | Re-check |
|---|---|---|---|
| **V1** | Lix ships exactly five plugins (`csv`, `excalidraw`, `json`, `markdown`, `text`). No DOCX, XLSX or PSD plugin. PSD support is opaque blob versioning | 23 Sep 2026, GitHub API | **Quarterly.** The project moves fast and publishes plugins independently of the repository |
| **V2** | Lix has no documented cryptographic audit chain; permissions are "planned"; status is alpha | 23 Sep 2026 | Quarterly. This is the core differentiation claim and must not go stale |
| **V3** | Word for the web preserves existing content controls but cannot create them; reports on rich-text support conflict | 23 Sep 2026, Microsoft Q&A | **Test directly** as part of A1 — do not rely on forum reports |
| **V4** | Word replaces `w14:paraId` values written by other tools on first save | Reported | Confirm empirically in A3. It determines whether paraId is usable as a secondary anchor |
| **V5** | Custom XML parts are stripped by Word for the web and removable by Document Inspector | 23 Sep 2026 | Confirm in A1/A3 |
| **V6** | Collabora Online new code is MPL-2.0; Collabora Online is tile-based server rendering | 23 Sep 2026, vendor docs | Confirm current licence terms before any use |
| **V7** | ONLYOFFICE is AGPL-3.0; commercial licence available from Ascensio | 23 Sep 2026 | Confirm with counsel before any evaluation work — see `L1` |
| **V8** | LibreOffice WASM: Writer-only experimental builds; a headless-core-for-embedding variant exists | 23 Sep 2026 | Annually. The headless-core variant is the one that could change the Part A answer |
| **V9** | Figma branching is Organization/Enterprise only, with submit-for-review and merge | 23 Sep 2026 | Annually. Relevant only if design formats are reconsidered |
| **V10** | Simul Docs reports 7,000+ users; compares saved Word files and merges into tracked changes | Vendor claim, not verified | Treat as vendor marketing unless independently confirmed |

---

## P. Prior art to read properly

Identified as relevant but not yet read in depth.

| ID | Source | Why it is worth the time |
|---|---|---|
| **P1** | [XLIFF 2 Extraction and Merging Best Practice](https://galaglobal.github.io/TAPICC/T1/WG3/prd01/XLIFF-EM-BP-V1.0-prd01.xhtml) (GALA TAPICC, 2019) | The localization industry's formalisation of exactly the round-trip problem. Likely contains failure modes not yet anticipated |
| **P2** | Trados bilingual review export, and its community-reported failures (segments changed since export, segments now containing paragraphs, unmergeable segments) | Those are the stale-base, split and join cases under other names, with two decades of user feedback attached |
| **P3** | Juro and Ironclad contract round-trip UX | The closest shipping commercial analogues of the reviewed-proposal flow. Worth a hands-on trial, not just documentation reading |
| **P4** | Simul Docs behaviour under the identity question | How does a shipping product handle block identity across a Word round trip? Reverse-engineering its output would be informative |
| **P5** | [Mechanisms of Persistent Identification of Topological Entities in CAD Systems: A Review](https://www.sciencedirect.com/science/article/pii/S1110016818300814) | The DAG-of-entity-histories approach is the most mature treatment anywhere of the split/join identity problem Dynodoc has in easier form |
| **P6** | Figma branching mechanics — how merges and conflicts are actually presented | Prior art for block-level merge UX in a non-text medium, from a product people like |
| **P7** | Ignat & Norrie, multi-level editing (ETH Zurich, 2006–2008) | Named in the literature review as the closest academic statement of the approach. Read in full before publishing anything |
| **P8** | Rönnau & Borghoff, state-based diff and merge for ODF and OOXML (2008–2012) | Directly on the DOCX merge problem; context fingerprints may inform the reconciliation hash |
| **P9** | Lix plugin API and schema | If it becomes a de facto interchange format, interoperability may be cheaper than competition |

---

## L. Legal and IP

| ID | Question | Why it matters |
|---|---|---|
| **L1** | Confirm licence obligations for **every** adopted or evaluated component; several are marked "verify" in the literature review §15 table | An AGPL or unexpected copyleft component in the core would be a serious diligence problem under the strategy thesis |
| **L2** | Is the hash-chained semantic document versioning method patentable, and should it be filed? | For a *technology* acquisition specifically, a patent is a meaningful lever and a defensive necessity — much of what makes "buy" cheaper than "build" |
| **L3** | **Sequencing conflict:** the publication opportunities in lit review §13 versus any patent filing in `L2` | Publishing first may foreclose filing. Resolve with a patent attorney **before** further public disclosure of the design |
| **L4** | Erasure versus immutability policy, decided before any public consultation launch | Append-only hash chains collide with deletion requests. The resolution — content-addressed indirection or key shredding — constrains the storage design, so it cannot be retrofitted |
| **L5** | Confirm the engine's dependency graph stays clean, licence-audited and transferable | The MIT engine extraction helps; it needs to stay true as dependencies are added |

---

## S. Strategic assumptions

Unvalidated beliefs the plan rests on. Each should be tested rather than defended.

| ID | Assumption | How it fails | How to test |
|---|---|---|---|
| **S1** | **The review invitation works as a growth mechanism** — reviewers who receive a document convert into owners | It is the *only* growth mechanism in the plan. Private documents generate no public surface, so if this fails there is no distribution | Instrument reviewer→owner conversion from the first pilot. Make it the primary metric ahead of document or event counts |
| **S2** | Reviewers will tolerate returning a review through Dynodoc rather than replying to an email with an attachment | If returning a review is harder than the email it replaces, adoption stops at the owner | Measure reviewer completion rate and time-to-return in the pilot. Any signup wall before the diff is visible is a likely cause of failure |
| **S3** | Users accept authoring in Word and reviewing in Dynodoc as a split workflow | If they reject the split, authoring must be first-class and the editor scope reverts to the larger estimate in strategy §B.3 | Direct observation in the 10-user pilot |
| **S4** | "Draft proliferation" is a pain users will change tools to solve | Widely assumed, rarely tested. Pain that people complain about is not always pain they will act on | Interview target users before building further. Ask what they did last time, not what they would like |
| **S5** | The likely acquirer set is legal/regulated document management rather than Google or Microsoft | Determines the roadmap's vertical requirements and the demo's framing | Early exploratory conversations. One engaged buyer beats a year of speculation |
| **S6** | Branching is wanted by document authors at all | Microsoft owns GitHub and twenty years of Office engineering, and Word still has no branching. That may be a demand signal, not a capability gap. Ink & Switch's Patchwork treats it as an open question | Observe whether pilot users create drafts unprompted, or only when instructed |

---

## T. Deferred by tripwire

Decisions deliberately closed. Each lists what would reopen it, so nobody relitigates without cause.

| ID | Decision | Reopens if |
|---|---|---|
| **T1** | No CRDT runtime | Offline editing with local divergence, true intra-paragraph multi-cursor typing, or peer-to-peer/federated deployment becomes a requirement. Note this is the **same** tripwire as `T2` |
| **T2** | No distributed/remote protocol; Dynodoc is the single authority | As `T1`. Offline clients are replicas, and replicas need sync. One decision, not two |
| **T3** | No fork of LibreOffice, ONLYOFFICE or OpenOffice | The LibreOffice headless-core WASM variant (`V8`) matures enough to supply a model rather than tiles |
| **T4** | No desktop client | The Word round trip proves insufficient for offline needs *and* `A3` shows identity recovery is the blocker rather than workflow |
| **T5** | No breadth beyond Office and research formats | Customer pull for a specific format, satisfying all five obligations of strategy §F.3. PPTX is the expected first; PDF comparison the second |
| **T6** | No public consultation stages (22–31) | The researcher pilot is validated and a consultation customer is engaged |
| **T7** | Watcher model (observing Drive/SharePoint) rejected in favour of canonical storage | Adoption evidence shows canonical storage is the blocking friction |

---

## Settled

Record answers here as items close, with the date and the evidence.

| ID | Answer | Date |
|---|---|---|
| — | *(none yet)* | — |
