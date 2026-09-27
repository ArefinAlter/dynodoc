# Platform build decision and product strategy review

Compiled 23 September 2026 from an owner discussion covering three questions the
research reviews do not answer:

1. Should the editor be built on a fork of LibreOffice or OpenOffice?
2. How long does the Docs/Sheets "80/20" take, and what does agentic AI change?
3. If the moat is version control and the goal is acquisition or partnership,
   what should actually be built?
4. Does the "hosting and review platform for rich documents" framing work as a
   proving surface for the engine? (Part E, added later the same day.)
5. How does Dynodoc differ from Lix, and how far can the same discipline extend
   to vector, raster, CAD and other structured file types? (Part F.)

This is analysis and recommendation. It changes no code and adds no capability.
Facts are marked where they were verified from vendor or publisher documentation;
everything else is judgement and is labelled as such.

Open questions, experiments, facts to re-verify and unvalidated assumptions raised by this review are tracked in the [open research register](OPEN-RESEARCH-REGISTER.md).

Companion documents:
[block versioning literature review](BLOCK-VERSIONING-LITERATURE-REVIEW.md) (the
technical verdict and the 15-step sequence),
[rich-text algorithm research review](RICH-TEXT-ALGORITHM-RESEARCH-REVIEW.md) (an
earlier pass on the same ground),
[office editor capabilities](OFFICE-EDITOR-CAPABILITIES.md),
[project status](PROJECT-STATUS.md),
[versioning conformance](VERSIONING-CONFORMANCE.md).

---

## Part A. Should we fork LibreOffice or OpenOffice?

### A.1 Recommendation

**No.** Not Collabora/LibreOffice Online, not ONLYOFFICE, not Apache OpenOffice.
The reason is architectural, not effort: in every case the document model belongs
to the forked product, and Dynodoc's entire premise is owning that model.

### A.2 Collabora Online / LibreOffice Online

*(verified: vendor and project documentation)*

Collabora Online is headless LibreOffice rendering the document server-side and
**streaming image tiles over a WebSocket**. The browser client is built on Leaflet,
a map library, because what it displays is binary tiles. Every keystroke is
processed on the server and the changed region is re-rendered and pushed back.
The server side is a multi-process architecture (WSD, ForKit, Kit) with documents
loaded into chroot-jailed Kit processes.

Consequences for Dynodoc:

| Consequence | Detail |
|---|---|
| No model to attach identity to | The document lives in LibreOffice's C++ core. There is no node tree in the browser, only pictures. ULIDs have nowhere to go |
| Distillation becomes the whole project | Extracting semantic operations would mean writing C++ inside LibreOffice. Volume I §10 already names distillation fidelity as "the main technical risk of the hybrid" |
| Competing collaboration model | Collabora ships its own collaborative editing, persistence and WOPI integration. These would have to be displaced, not extended |
| Breaks G7 | The PoC target of "one small VM, one Postgres" becomes per-user LibreOffice processes |
| Fork maintenance | LibreOffice is reported at ~9.5M lines across ~200 modules, with the codebase "deeply entangled in VCL" *(reported; community sources)* |

Licence: MPL-2.0 is anticipated for new Collabora Online code, in line with The
Document Foundation's model. *(reported; confirm current terms before any use.)*

### A.3 ONLYOFFICE

*(verified: project repository and vendor pages)*

Better suited architecturally — HTML5 canvas editors with an OOXML-native core and
a real client-side document model. Two blockers:

1. **AGPL-3.0.** The network clause is triggered by providing the software as a
   service. For a proprietary hosted product this is normally disqualifying
   without a commercial licence from Ascensio. **Obtain legal advice before
   spending engineering time on this path.**
2. **Still not our model.** Its own document core, its own collaboration. The
   distillation problem is smaller than Collabora's but not absent.

The AGPL point has a second implication that matters for Part C: an AGPL component
in the core would be a serious diligence problem in a technology acquisition.

### A.4 LibreOffice WASM, and Apache OpenOffice

LibreOffice supports Emscripten cross-compilation, producing either a Qt5-GUI WASM
build or *LibreOffice core with no UI* for embedding in software that supplies its
own interface. *(verified: `static/README.wasm.md` in the LibreOffice repository.)*
Public demonstrations (LOWA) have been Writer-only and experimental. The headless-core
variant is the interesting one long-term; it is not a 2026 foundation.

Apache OpenOffice has struggled to maintain enough active developers to ship timely
security fixes since 2015. *(reported.)* It is not a candidate.

### A.5 The decomposition that dissolves the question

"Build a Word/Excel clone" is the wrong framing. What is actually missing splits
into four capabilities with different answers:

| Capability | Needed? | Cheapest source | Fork required? |
|---|---|---|---|
| Format conversion (DOCX/XLSX in and out) | Yes, critically | **LibreOffice headless** — already used for `.doc` | No |
| Layout/pagination fidelity, print, PDF | Yes, for output | **LibreOffice headless**, server-side | No |
| Editing feature breadth | Partially | Incremental Tiptap work, corpus-driven | No |
| Spreadsheet calculation | Mostly present | HyperFormula, extended | No |

Two of the four are already solved by a tool the project already runs. LibreOffice's
value to Dynodoc is as a **format and layout engine**, not as an editor.

### A.6 Recommended architecture

1. **Keep Tiptap/ProseMirror as the editing surface and system of record.** It is
   the only option in which blocks are first-class objects that can carry identity.
   That property is the product.
2. **Promote LibreOffice headless from a `.doc` converter to a first-class service**
   with three jobs: format conversion, high-fidelity export, and print/pagination
   rendering. MPL-2.0 is weak file-level copyleft and suits service use; still
   confirm obligations.
3. **Treat Collabora, if ever offered, as an optional external editing surface**
   reconciled through the export manifest of
   [literature review §19.6](BLOCK-VERSIONING-LITERATURE-REVIEW.md). "Edit in
   Collabora" is structurally identical to "edit in Word": an outside editor that
   returns a DOCX. This buys fidelity as an option without the engine depending
   on it, and it can be dropped without consequence.
4. **Extend the non-goals discipline to editor features.** A research and policy
   document editor does not need mail merge, SmartArt or WordArt. Decide scope
   from a real document corpus, not a feature matrix.

---

## Part B. How long, and what agentic AI changes

### B.1 The starting point is much further along than the question assumes

Per [OFFICE-EDITOR-CAPABILITIES.md](OFFICE-EDITOR-CAPABILITIES.md) and
[PROJECT-STATUS.md](PROJECT-STATUS.md), the following already ship: full ribbon,
headings 1–6, font family/size/colour/highlight, lists and checklists, links,
alignment, indentation, spacing, tables with spans, equations, comments, document
tabs, search/replace, page size/orientation/margins, headers/footers, page numbers,
live page reflow with Paged.js print and PDF, tracked insertions and deletions with
accept/reject, footnotes and endnotes with a notes panel, table of contents,
IME-aware input, RTL with bundled Noto fonts, custom font upload, accessibility
checks, and DOCX import/export with style inheritance. The spreadsheet has
HyperFormula, formatting, conditional formatting, charts, pivot summaries, named
ranges, data tools and protection.

**That is most of the Docs/Sheets 80/20 already.** The open question is not "how
long to build it" but "what is left, and is it needed".

### B.2 What AI accelerates, and what it does not

| Speedup | Work | Why |
|---|---|---|
| 5–10× | Feature breadth of known shape, OOXML element handlers, UI components, tests, refactors, documentation | Mechanical, wide surface, well specified |
| 1.5–3× | Novel algorithms (merge engine), performance work, integration debugging | Requires measurement loops and judgement |
| ~1× | Layout and pagination correctness, adversarial input handling, IME/browser/accessibility edge cases, **defect discovery** | Learned empirically from real files and real users |

**The binding constraint is verification and real-document exposure, not code
production.** A feature can be generated in a day and be wrong for three weeks
before a user opens the file that breaks it. AI does not shorten the interval
between shipping a defect and someone finding it.

**The specific hazard.** Cheap breadth produces a false sense of completion: the
demo works, the researcher's real document does not. This project's documentation
is unusually disciplined about stated limits; that discipline becomes harder to
maintain when features arrive faster than evidence.

### B.3 Estimates

Assumptions: 1–3 engineers driving agentic coding; the capability baseline in B.1;
no fork. These are judgement, not commitments.

| Target | Estimate | What dominates the time |
|---|---|---|
| **Threshold** — ordinary research and policy documents open, round-trip and edit without reaching for Word | 2–4 months | Real-corpus testing. Named styles, numbering fidelity, citations, spell check, outline pane |
| **Comfortable** — preferred for the whole workflow | 6–12 months | Scale limits, performance at real document sizes, mobile |
| **Word/Sheets parity** | Not achievable, and not a goal | Infinite regress with no user at the end |

**The most under-rated risk is the scale caps, not the features.** The current
limits — 10 MB upload, 3,000 spreadsheet rows, 100 columns, 5,000 operations per
batch — are product-visible and will be hit by a real research dataset immediately.
Raising them is streaming, pagination and memory work, which is precisely the
category AI accelerates least.

### B.4 The measurement that replaces the estimate

Take **20 real documents and 5 real spreadsheets** from target users and count
which features actually appear. This single exercise converts the open-ended
question into a finite ordered list, locates the threshold, and produces the DOCX
corpus the round-trip work needs anyway. A few days of work, and it should precede
further editor features.

---

## Part C. Product strategy and the acquisition thesis

### C.1 The owner's thesis, as stated

The moat is the version-control mechanism for documents and spreadsheets, not
research-instrument features. Google and Microsoft cannot be beaten on editing.
The goal is acquisition or partnership: an almost-identical platform that solves
real pain for Docs/Sheets users (draft proliferation, no record of who changed
what, painful collaboration). Few users in the first years is acceptable, but a
platform is still required, hence the 80/20 editor.

### C.2 Where the thesis is right

The moat is the engine. A demonstration surface is required. Low early user counts
are compatible with a technology acquisition. All correct.

### C.3 The structural problem: "almost identical" is dilutive

*(judgement)*

An acquirer does not buy a worse copy of its own product. Every hour spent on
editor parity produces something Google and Microsoft already have, better. That
work weakens the acquisition case rather than strengthening it.

The two pitches land very differently:

> "We are almost identical to Docs, with version control."
> → *"We have Docs. We will build version control."*

> "Here is version control running on **your** documents, with N users, and here
> is their retention."
> → a different conversation.

Acquisition value is concentrated entirely in what the acquirer does not have.

### C.4 The precedent points away from building a competitor

*(verified: contemporaneous reporting and vendor announcements)*

| Acquisition | Year | What it was |
|---|---|---|
| **DocVerse → Google, ~$25M** | 2010 | **A plug-in for Microsoft Office** adding online collaboration. Bought for Office interoperability, not as a Docs competitor |
| Quickoffice → Google | 2012 | Mobile suite reading and writing Office formats — again, attach to the incumbent's formats |
| Writely → Google | 2006 | The exception, and it became Google Docs — acquired when the category was empty |
| Intentional Software → Microsoft | 2017 | Team and technology for future productivity scenarios, not a competing product |

**The pattern: these companies buy products that attach to the document surface,
not products that replace it.** Writely is not a usable precedent in 2026.

### C.5 The uncomfortable question the thesis must answer

Microsoft owns GitHub — the world's version-control platform — plus twenty years
of Office engineering, and Word still has no branching. That is not a capability
gap. It is either a deliberate product decision (branching may genuinely confuse
mainstream users, which is the open research question in Ink & Switch's Patchwork)
or a priority decision.

Either way, **"we built it better" does not by itself create an acquisition.
Evidence that their users want it does.**

The counter-evidence is real, and it locates the demand precisely: Simul Docs
sells (vendor reports 7,000+ users); Litera, iManage, NetDocuments and Workshare
are a substantial market; Veeva is a multi-billion-dollar company built on
regulated document control; Juro and Ironclad ship contract redline round trips.
Demand for document version control is genuine but **segmented** — legal,
regulated, academic — not mainstream Docs.

### C.6 A revised acquirer set

*(judgement)*

| Candidate | Why |
|---|---|
| **Litera, iManage, NetDocuments** | Legal document management; acute version pain; active consolidators; "who changed what and can I prove it" is billable |
| Veeva, MasterControl | Regulated document control; audit chain is directly on-thesis |
| Atlassian, Notion, Coda | Would value differentiated versioning in a content platform |
| Box, Dropbox | Document platforms with weak editing depth |
| European sovereign-office and govtech buyers | Adjacent to the LEOS and public-consultation roadmap |
| Google, Microsoft | Possible, but on the DocVerse pattern — via an add-in with users, not a parallel suite |

### C.7 The API asymmetry that decides the attachment point

*(verified: Microsoft Learn and Google for Developers documentation)*

| Surface | Viability | Evidence |
|---|---|---|
| **Word add-in** | **Strong** | Office.js exposes `TrackedChangeCollection` with author, date, text and type, plus OOXML get and set on ranges. Real authorship signal and real document access. Known limit: track revisions cannot be temporarily disabled, so OOXML replacement marks the whole text as a revision |
| **Google Docs add-on** | **Weak** | Structural elements exist but paragraphs carry no stable IDs; the Drive revisions API is documented as returning incomplete history for frequently edited documents |

The Word add-in is the viable attachment point, and it reuses the OOXML expertise
and the reconciliation design already specified in
[literature review §§18–19](BLOCK-VERSIONING-LITERATURE-REVIEW.md).

### C.8 Recommendation

**Keep the platform, change what it is for.**

Dynodoc becomes the **system of record**; the Word add-in becomes the
**distribution surface**. Users author where they already are, and come to Dynodoc
to branch, review diffs, resolve and audit. This is the architecture already
designed for the DOCX round trip — the add-in is simply a better-instrumented
client than a file upload.

The consequence for Part B is large:

> **The editor bar drops.** If users author in Word and come to Dynodoc to review,
> what is needed is excellent *rendering and diffing*, not excellent *authoring*.

The revised 80/20 becomes:

1. **Open any real document faithfully** — hard, necessary, non-negotiable
2. **Render diffs beautifully** — this is the product, and nobody does it well
3. **Review, resolve, merge** — the moat made visible
4. Enough editing to resolve conflicts and make small corrections

**Not** a daily-driver authoring environment. This removes months from the plan and
sharpens the acquisition story instead of diluting it.

### C.9 Two assets to protect under this thesis

- **IP cleanliness.** The decision not to fork ONLYOFFICE was strategically correct
  beyond engineering: an AGPL component in the core would be a serious diligence
  problem. Keep the engine's dependency graph clean, licence-audited and
  transferable. The MIT engine extraction already helps here.
- **Patent position.** For a *technology* acquisition specifically, a patent on the
  hash-chained semantic document versioning method is a meaningful lever and a
  defensive necessity — much of what makes "buy" cheaper than "build". Worth a
  patent attorney's time before more of the design is published. Note the tension
  with the publication opportunities in literature review §13; sequence matters.

### C.10 What would falsify this recommendation

Stated plainly so it can be tested rather than believed:

- If the Word add-in cannot recover block identity well enough to produce useful
  proposals (literature review §19.7 step 0a fails), the attachment strategy
  weakens and the standalone platform becomes the only route.
- If real users reject reviewing changes outside their authoring tool, the
  "system of record plus add-in" split is wrong and authoring must be first-class.
- If a legal or regulated buyer engages early, the acquirer set in C.6 should
  displace the Google/Microsoft framing entirely, and the roadmap should follow
  that vertical's requirements instead.

---

## Part D. How this fits the existing sequence

The [literature review's 15-step sequence](BLOCK-VERSIONING-LITERATURE-REVIEW.md)
stands. This review changes emphasis, not content:

| Change | Effect on the sequence |
|---|---|
| No fork; LibreOffice headless as a service | Removes an unlisted alternative; no new steps |
| Editor scope reframed to render/diff/review | Lowers priority of broad editor features in PROJECT-STATUS "remaining work" |
| Add-in promoted from optional to strategic | Step 15 moves earlier, after step 12 proves the round trip |
| Auditor export tied to the product claim | Step 14 becomes a product feature, not a compliance nicety |
| Real-corpus measurement precedes feature work | Reinforces steps 0a, 11 and the B.4 corpus exercise |

---

## Sources

**Office suite architecture and licensing**

- [Collabora Online — LibreOffice Online questions answered](https://www.collaboraonline.com/blog/libreoffice-online-questions-answered-what-who-how-and-when/)
- [ONLYOFFICE DocumentServer repository](https://github.com/onlyoffice/documentserver) (AGPL-3.0)
- [LibreOffice Emscripten/WASM build notes](https://github.com/LibreOffice/core/blob/master/static/README.wasm.md)

**Add-in and API surfaces**

- [Word.TrackedChangeCollection (Office.js)](https://learn.microsoft.com/en-us/javascript/api/word/word.trackedchangecollection?view=word-js-preview)
- [Get, edit and set OOXML in a Word add-in](https://learn.microsoft.com/en-us/samples/officedev/office-add-in-samples/word-get-set-edit-ooxml/)
- [Structure of a Google Docs document](https://developers.google.com/workspace/docs/api/concepts/structure)
- [Google Drive revisions API](https://googleapis.github.io/google-api-python-client/docs/dyn/drive_v3.revisions.html)

**Acquisition precedents**

- [Google acquires DocVerse](https://venturebeat.com/technology/google-docverse)
- [Microsoft to acquire Intentional Software](https://blogs.microsoft.com/blog/2017/04/18/microsoft-acquire-intentional-software-expand-future-productivity-capabilities/)

**Market comparators**

- [Simul Docs](https://www.simuldocs.com/how-it-works) · [LEOS](https://interoperable-europe.ec.europa.eu/collection/justice-law-and-security/solution/leos-open-source-software-editing-legislation) · [Ink & Switch Patchwork](https://www.inkandswitch.com/patchwork/notebook/)

---

## Part E. Positioning: a hosting and review platform for rich documents

Added 23 September 2026 after the owner proposed framing the platform as "a GitHub
for rich documents" — explicitly as a surface that proves the engine's value, not
as the pitch itself. This section records the analysis and the design consequences.

### E.1 Why this frame resolves the problem in Part C

Part C's objection to the earlier plan was that "almost identical to Google Docs"
makes Dynodoc a **substitute** for the incumbent's product, and an acquirer does
not buy a worse copy of what it already has.

The hosting-and-review frame makes Dynodoc a **complement** instead.

The precedent is exact: **GitHub never built a better text editor.** It did not
compete with vim, Emacs or any IDE. It hosted, versioned, reviewed and discussed.
VS Code arrived later and separately. Because GitHub was not an editor, it was
never a threat to anyone's editor, and it became the place work was coordinated
rather than the place work was typed.

Applied here: people write in Word, Excel or Google Docs. Dynodoc holds the
document, records who changed what, shows the differences, and gates acceptance.

The demonstration changes character completely:

> Not *"look, our editor works"* but *"watch this document go out to three
> reviewers in Word, come back, and see exactly who changed what — provably."*

That demonstration is impossible in Google Docs or Word, and it exercises the
engine directly rather than incidentally. For the owner's stated purpose —
proving the engine's value — it is materially better than editor parity.

### E.2 What transfers from GitHub, and what does not

This is the central strategic fact of the frame and it must not be glossed over.

| What made GitHub work | Transfers to documents? | Note |
|---|---|---|
| The pull request | **Yes** | Already implemented as personal drafts, selected sharing and review |
| Discussion attached to the artifact | **Yes** | Threaded comments with resolve and reopen already ship |
| Automated checks on every change (CI) | **Yes, and under-exploited** | See E.6 |
| Free public hosting and the open-source network effect | **No** | Documents are private. This was GitHub's growth engine |
| Everyone already used git | **No** | Document authors have no version-control habit to build on |
| Profile and reputation graph | Mostly no | Some value in institutional settings, little elsewhere |

**The review primitive transfers. The growth engine does not.** GitHub grew on
public repositories doing its marketing. Nothing does that for private documents.
A deliberate substitute is required.

### E.3 The growth mechanism: the review invitation

*(judgement)*

When a document owner sends a draft for review, the reviewer **must** come to
Dynodoc to return it. The reviewer is exposed to the product by the artifact
itself. This is the DocuSign, Figma and Calendly pattern: the work being shared
carries the tool to the next person.

This is already implicit in the round-trip design of
[literature review §18](BLOCK-VERSIONING-LITERATURE-REVIEW.md). It should be made
explicit and designed for:

- A reviewer must be able to **see the diff and return changes with no account
  setup**. Any signup wall placed before the value is seen destroys the mechanism.
- The invitation is a first-class product surface, not an email template.
- **Reviewer-to-owner conversion becomes the primary growth metric**, ahead of
  document count or event count.
- Because it is the only growth mechanism in the plan, it must be tested early
  and deliberately rather than assumed.

### E.4 The category has a graveyard, and its causes are instructive

*(verified for status; causes are inference and marked as such)*

| Attempt | Framing | Status |
|---|---|---|
| Penflip | "GitHub for writers" | Closed |
| Authorea | "GitHub for scientists" | Raised funding; absorbed into a publisher; platform migration in 2026 transferring only DOI-bearing preprints |
| GitBook | Began as where books are written | Pivoted to technical documentation software |
| Draft | Version control for writing | Defunct |

Published postmortems are thin, so the following causes are inference from the
products' own design, not confirmed founder accounts:

1. **They required Markdown or plain text.** Writers do not write in Markdown.
   → **Dynodoc has genuinely solved this.** Real rich text, real Office formats,
   real formatting preservation. This is the substantive difference from every
   member of the graveyard.
2. **They exposed Git vocabulary** — commit, branch, merge, rebase.
   → **Dynodoc has avoided this.** The vocabulary map in
   [VERSIONING-CONFORMANCE.md](VERSIONING-CONFORMANCE.md) is already disciplined
   and should stay that way (see literature review §16.4).
3. **They tried to be the authoring destination** and therefore competed with
   Word on editing, which they lost.
   → This is precisely what the E.1 frame steps away from.
4. **They had no distribution mechanism.**
   → The review invitation (E.3) is the proposed answer, and it is unproven.

Two of the four causes are already addressed by work that is done. That is a real
position rather than optimism, but the remaining two are the ones that kill
companies, and the fourth is entirely unvalidated.

### E.5 Design consequences

**1. Never require a commit action.** Developers arrived at GitHub with a commit
discipline. Document authors have only *save*. Every save must therefore produce a
recoverable version, with a named version as an optional, human-meaningful marker.
The engine already works this way (Save version is opt-in over continuous event
capture); Simul Docs independently made the same choice by versioning on each Word
save. This is correct and should not be revisited.

**2. Decide where the canonical file lives.** This is the significant open fork:

| Model | Description | Trade-off |
|---|---|---|
| **Canonical store** (GitHub model) | Dynodoc holds the document; upload, download and round trip are the on-ramps | Strongest position; highest adoption friction |
| **Watcher** (CI-bot model) | Dynodoc observes Drive or SharePoint and never owns the file | Lower friction; weaker position; inherits the host's API limits, which for the Google Docs API are documented as poor (Part C.7) |

**Recommendation: canonical store**, because that is where GitHub's leverage came
from, with the round trip and upload as the low-friction on-ramps. The watcher
model should be reconsidered only if adoption evidence demands it.

**3. The editor's job becomes diff viewing plus light editing.** GitHub's web
editor is deliberately unremarkable and no one minds. Dynodoc's must be better
than that — documents are genuinely edited in place during review — but it does
not need to approach Word. This is the same conclusion as Part B.3 and C.8,
now with an independent reason.

**4. Lead with the pain, not the analogy.** "Final_v3_REVISED_JK_final2.docx" is
universally recognised; "version control" sounds technical and "GitHub for X" is a
frame investors have heard many times. The analogy is an excellent internal design
compass and weak external positioning. Keep it out of customer-facing copy.

**5. Treat the audit chain as a product feature, not compliance plumbing.** In this
frame, "who changed what, provably" is the headline capability. That makes the
signed tree heads and auditor export of literature review §17 and step 14 a
**product** deliverable rather than a later hardening task.

### E.6 Document CI: the most under-exploited idea in the frame

GitHub's third pillar, after hosting and the pull request, is **automated checks
that run on every proposed change**. Dynodoc already has the seed of this: the
referential-integrity deployment gate that blocks a questionnaire with duplicate
names, missing references, dependency cycles, incompatible numeric operands or
undefined choices.

Generalised, this becomes a check suite that runs on every proposed change:

- Broken cross-references, missing or malformed citations
- Accessibility violations (heading order, image descriptions, table headers,
  empty links — all already implemented as authoring checks)
- Required sections present; template conformance
- House style and terminology rules
- For instruments: the existing referential-integrity and expression checks
- For spreadsheets: formula errors, broken names, out-of-range references

No document platform found in this review offers this. It is what makes the
hosting-and-review analogy substantive rather than cosmetic, and it is largely
assembled from checks that already exist in the codebase but currently run at
different moments and for different reasons.

### E.7 Risks

- **Two-sided adoption.** The owner must adopt *and* reviewers must participate.
  A higher bar than single-player tools, and a slower one.
- **No compounding public surface.** Private documents generate no discovery.
  Growth rests entirely on E.3, which is unproven.
- **The frame invites feature creep toward a social network** — profiles, follows,
  stars — none of which have a demonstrated purpose for private documents.
- **Reviewer friction is fatal, not merely costly.** If returning a review is
  harder than replying to an email with an attachment, the mechanism fails.

### E.8 Effect on the build sequence

Almost none, which is evidence the frame is compatible with the existing plan
rather than a redirection. The
[15-step sequence](BLOCK-VERSIONING-LITERATURE-REVIEW.md) stands, with the round
trip becoming *more* central because it is now the product rather than a feature.

Two additions:

| Addition | Where | Why |
|---|---|---|
| Review invitation as a first-class surface, with no-signup diff viewing | Alongside step 12 | It is the only growth mechanism in the plan |
| Generalise the validation gate into document CI | After real-user exposure | Scope must be set by what real documents actually break on |

---

## Part F. Lix, and the ambition of "all file types"

### F.1 What Lix is

*(verified from the project's own repository and site, September 2026)*

Lix repositioned in September 2026 as a "universal version control system": an
embeddable repository combining files, a database and version control, designed to
run in-process on pluggable storage with ACID guarantees. Plugins map file formats
to SQL rows — "a paragraph, cell, or property becomes a row" — so history, blame,
branching and rollback are database queries rather than bespoke features. Changes
are recorded automatically on every write rather than by an explicit commit.
Claimed format coverage includes DOCX, XLSX, JSON, PDF and Markdown.

Stated status: **alpha**. Permissions are documented as **planned**, not
implemented. The repository does not document stable entity identity mechanisms,
per-format merge algorithms, or any cryptographic integrity or audit capability.

### F.1a Verified plugin inventory, and the gap between claims and code

*(verified 23 September 2026 via the GitHub API against `opral/lix`)*

The `plugins/` directory of the Lix repository contains exactly five plugins:

```
csv    excalidraw    json    markdown    text
```

There is **no DOCX plugin, no XLSX plugin and no PSD plugin**, although the
landing page displays PSD among its example formats and states that "plugins diff
and merge DOCX, XLSX, or JSON by clause, cell, or row." Of those three, only JSON
exists. The official plugin directory page reports itself as "being rebuilt".

Two consequences:

1. **PSD support is opaque blob versioning**, under the site's separate "text,
   binaries, large blobs" category — the file changed, both versions are retained,
   either can be restored. No parsing, no layer identity, no layer diff, no merge.
   Functionally comparable to Git-LFS or Dropbox with content-addressed dedup.
2. **The five shipped plugins are all already-structured text formats** where
   obligation 1 (decompose) is trivial and obligation 2 (identify) is largely free.
   Excalidraw is JSON whose elements already carry IDs.

Mapped onto the five obligations of F.3: Lix has built the **format-agnostic half**
well — storage engine, plugin API, SQL-queryable history — and has not yet
completed the **format-specific half** for any difficult format. Its claims
currently run ahead of its plugins.

This materially improves Dynodoc's competitive position on the formats that matter
to it: the DOCX and XLSX work already shipped is ahead of Lix on exactly the
formats Lix advertises. It also confirms the F.6 thesis — the agnostic half is the
tractable half, and breadth claims are cheap until a hard format is attempted.

*Caveat: a same-day snapshot. Plugins can be published independently of the
repository and the project moves quickly. Re-verify before relying on this.*

### F.1b What layer-level PSD would actually require

Recorded because the analysis is instructive, not because PSD is recommended.

| Obligation | PSD assessment |
|---|---|
| Decompose | Parse to layers; `ag-psd`, `@webtoon/psd` and `psd-tools` exist. Non-trivial but solved |
| **Identify** | **PSD carries native layer IDs** (the `lyid` additional layer information block) |
| Diff | Layer properties — name, opacity, blend mode, visibility, position, mask — diff cleanly. Pixel content supports only a changed/unchanged hash |
| Merge | Disjoint layers merge; concurrent same-layer pixel edits go to review |
| Render | Render each layer and present side by side or overlaid |

The notable asymmetry: **PSD has better native identity than DOCX.** Photoshop
supplies stable layer IDs; Word requires identity to be reconstructed from an
export manifest (§19.6). The real constraints are storage volume — mitigated by
content-addressed dedup of unchanged layers — and the fact that a pixel diff can
never say more than "this region changed". Layer granularity is the correct unit
regardless, for the same reason blocks are the correct unit in a document.

Feasible; not strategic. The recommendation in F.6 is unchanged: PPTX first,
PDF comparison second.

### F.2 Differences from Dynodoc

| Dimension | Lix | Dynodoc |
|---|---|---|
| **Layer** | Embeddable SDK — the *git* layer | Hosted platform — the *GitHub* layer |
| **Integrity** | No cryptographic audit chain documented | Append-only log, SHA-256 hash chain, Merkle commitments, verification endpoint |
| **Identity** | Not documented; plugins map file content to rows | ULIDs assigned at creation and carried through semantic events |
| **Change capture** | Recorded on write, plugin-derived from file content | Typed semantic events with authenticated actor, governance-gated |
| **Governance** | Permissions planned | Roles, org/team/folder inheritance, review gates, proposal lifecycle, shipped |
| **Breadth vs depth** | Many formats via a plugin ecosystem | Office and research formats, deeply |
| **Maturity** | Alpha | Deployed workspace with CI and live checks |
| **Business shape** | Open-source infrastructure | Product, with an MIT engine published separately |

**The most important observation:** Lix competes more directly with
`engine-core` — the separately published MIT engine — than with the Dynodoc
platform. Its existence **validates the category** while leaving the platform
position open. It is best treated as a benchmark and a possible interoperability
target, and watched closely, rather than as an immediate threat.

**Where Dynodoc is genuinely differentiated:** the tamper-evident chain and the
review-gated governance model. Those are what make the claim "provably who changed
what" true rather than asserted, and they are exactly what regulated, legal and
policy buyers pay for. They are also the parts Lix does not currently have.

### F.3 The plugin contract: what any format must supply

The engine splits cleanly into two halves, and this is the key to the ambition
question:

**Format-agnostic (already built, works for any payload):** the event log, the
hash chain, stable identity, the review gate, permissions, three-way merge over
identified units, snapshots, and the audit projection. None of this cares what
the payload is.

**Format-specific (a project per format):** five obligations.

| # | Obligation | What it means |
|---|---|---|
| 1 | **Decompose** | Split the file into ordered, addressable units |
| 2 | **Identify** | Assign or recover stable IDs for those units across versions |
| 3 | **Diff** | Produce a meaningful change description between two states of a unit |
| 4 | **Merge** | Define behaviour for concurrent changes, including what must go to review |
| 5 | **Render** | Present a visual difference a human can actually adjudicate |

Obligation 2 is almost always the hard one, and obligation 5 is the one most often
forgotten. A format is only viable when all five are achievable.

### F.4 Feasibility by format

*(assessment; CAD and design findings verified from the sources listed)*

| Format | Decompose | Identify | Diff | Merge | Render | Verdict |
|---|---|---|---|---|---|---|
| DOCX / XLSX | Done | Hard — addressed by the export manifest | Done | Done | Done | **Shipping** |
| PPTX | Easy (slides, shapes) | Moderate — shape IDs exist in OOXML | Moderate | Moderate | Moderate | **Best next candidate** |
| SVG and other open vector | Easy — elements carry `id` | Good | Good — path data, transforms, styles | Good — disjoint objects merge | Easy — render and overlay | **Feasible** |
| Adobe `.ai` | Hard — PDF-based, proprietary conventions | Unclear | Unclear | Unclear | Moderate | Avoid |
| PSD | Easy — layers carry IDs | Good | **Pixels: only changed or unchanged** | Layer-level only | Easy | Layer-level feasible; pixel-level is not |
| CAD | Moderate — feature tree | **Open research problem** | Moderate | Hard | Hard (3D) | See F.5 |
| GeoJSON / GIS | Easy — features carry ids | Good | Good | Good | Moderate — map view | Feasible |
| Jupyter notebooks | Solved — cell IDs, nbdime | Good | Good | Good | Good | Easy, low value here |
| PDF | Hard — no reliable semantic structure | Hard | Hard | Not meaningful | Easy | Annotations and comparison only |

### F.5 Two structural observations

**Parametric CAD is already an event log — and that is not enough.** A CAD model's
feature tree (extrude, fillet, cut) *is* an operation history, which is a closer
structural match to Dynodoc's engine than documents are. But CAD has a named, open
research problem that is the same problem as Dynodoc's split-and-join identity,
several orders of magnitude harder: the **topological naming problem**. References
to a face or edge are evaluation-local, so an upstream sketch edit can renumber
`Edge8` to `Edge2` and silently invalidate every dependent feature. *(verified: a
ScienceDirect review of persistent identification mechanisms; FreeCAD's own
documentation and a decade of unresolved work.)* Proposed solutions track directed
acyclic graphs of topological entity histories.

Meanwhile **Onshape already ships branching and merging for CAD**, and did it the
same way Dynodoc succeeds: by owning the model in a cloud database rather than
versioning files. *(verified: vendor documentation.)* Structurally attractive,
practically occupied.

**Raster degrades gracefully to layer granularity, and that is the right answer.**
Pixels cannot be merged meaningfully, but layers can be identified, versioned,
diffed as changed-or-unchanged, and merged when disjoint. That is the same insight
as blocks in a document: pick the unit a human would argue about. Content-addressed
storage makes it affordable, because unchanged layers deduplicate across versions.

**Design files are occupied.** Figma ships branching with submit-for-review and
merge on Organization and Enterprise plans. *(verified.)* Abstract and InVision
have diminished. That door is closed.

### F.6 Recommendation: architect for generality, ship depth

The discipline generalises. The ambition is a trap at this stage, for five reasons:

1. **Each plugin is a deep domain project**, not a weekend. A credible vector or
   CAD plugin requires format expertise that takes months to years to acquire.
2. **Lix chose the SDK shape precisely so that others write the plugins.**
   Competing on breadth means competing on ecosystem — a network-effects contest
   started from zero against an open-source project that began earlier.
3. **The differentiator is trust in domains where trust is worth money.** Nobody
   pays a premium for "version control for everything"; buyers pay for "prove who
   changed this clause." Breadth dilutes exactly the claim that commands a price.
4. **Every new format multiplies the verification burden**, which Part B.2
   identifies as the binding constraint already. Breadth worsens the weakest link.
5. **The attractive adjacent markets are occupied.** Figma owns design branching;
   Onshape owns CAD.

The recommended posture:

- **Keep the engine payload-agnostic.** It largely is — blocks, events and hashes
  do not care about the payload. Resist any Office-specific assumption leaking
  into `engine-core`.
- **Write the five-obligation plugin contract down formally** (F.3), and require
  any new format to satisfy all five before work starts.
- **Ship only formats where excellence is achievable.** PPTX is the natural next
  one; PDF comparison and annotation is the natural second.
- **Let customer pull choose the format**, not technical interest. For the research
  and policy trajectory that means presentations and PDFs, not PSD or CAD.
- **Note that payload-agnosticism is itself an asset in the Part C thesis**, even
  for formats never shipped. "The engine is format-agnostic and proven on Office"
  is a credible acquisition story. "We support fifteen formats badly" is not.

---

## Sources added for Parts E and F

**Category history**

- [Penflip company profile](https://www.crunchbase.com/organization/penflip) (closed)
- Authorea and GitBook status from vendor and community pages

**Lix**

- [Lix repository README](https://github.com/opral/lix/blob/main/README.md) · [lix.dev](https://lix.dev/) · [repositioning pull request #1840](https://github.com/opral/lix/pull/1840)

**Adjacent markets**

- [Onshape branch and merge](https://www.onshape.com/en/features/branch-merge-cad)
- [Mechanisms of Persistent Identification of Topological Entities in CAD Systems: A Review](https://www.sciencedirect.com/science/article/pii/S1110016818300814)
- [FreeCAD topological naming problem documentation](https://github.com/FreeCAD/FreeCAD-documentation/blob/main/wiki/Topological_naming_problem.md)
- [Figma branching](https://www.figma.com/) (Organization and Enterprise plans; per vendor documentation and Forrester coverage)
