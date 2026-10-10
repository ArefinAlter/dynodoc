# Rich-text engine: algorithm and architecture research review

> Implementation update, 10 October 2026: [shared checkpoints v1](SHARED-CHECKPOINTS.md)
> implement object reuse with stable-key radix maps and normal mixed-format reads.
> Part 2c batches database writes and index reads, preserving v1 roots and validation.
> Part 2d permits content writes during checkpoints with pinned-history/permission
> checks; [the lock contract](CHECKPOINT-CONCURRENCY.md) records its limits.
> Shared periodic writes are gated off by default pending database/lock acceptance;
> named versions and draft bases still use full JSON. The dated assessment below
> predates these increments. The [evolution plan](ENGINE-EVOLUTION-PLAN.md) controls
> current product priorities; the checkpoint contract records measured limitations.

> **Status note (23 September 2026).** This was the first research pass.
> [BLOCK-VERSIONING-LITERATURE-REVIEW.md](BLOCK-VERSIONING-LITERATURE-REVIEW.md)
> is the current and better-sourced treatment of the same ground: it verifies
> publication venues, adds the multi-level-editing lineage (Ignat & Norrie),
> OpenDocument/OOXML state-based merge (Roennau & Borghoff), and the competitive
> analysis (Lix, Juro, Ironclad, Fluid Framework SharedTree). **Where the two
> disagree, that document wins.** This one is retained for the material it covers
> in more depth: Eg-walker's measured results, the layered compression stack,
> content-defined chunking, and the machine-learning boundary argument.
> Platform and strategy questions are in
> [PLATFORM-AND-STRATEGY-REVIEW.md](PLATFORM-AND-STRATEGY-REVIEW.md).

Compiled 23 September 2026. This is a literature and prior-art review of the
Dynodoc engine's block model, segmentation strategy, storage/compression plan and
the question of whether machine learning should replace any of it.

It checks a proposed direction against published work. It is **not** a claim that
any of it is implemented, benchmarked end to end, or novel. Where a number is
quoted it comes from someone else's paper or benchmark, on their hardware and their
corpus, and is a reason to run our own measurement — not a result we can advertise.

Open questions, experiments, facts to re-verify and unvalidated assumptions raised by this review are tracked in the [open research register](OPEN-RESEARCH-REGISTER.md).

Companion documents: [rich-text versioning research and implementation record](RICH-TEXT-VERSIONING-RESEARCH.md),
[versioning design Vol. I](02-versioning-design-vol1.md),
[versioning mathematics Vol. II](03-versioning-mathematics-vol2.md),
[capability map](VERSIONING-CONFORMANCE.md),
[office editor limits](OFFICE-EDITOR-CAPABILITIES.md).

---

## 0. Summary verdict

**The direction is sound, and it is the direction the field converged on.** Blocks
with stable identity, formatting runs inside them, operation-based deltas and a
content-addressed history are what Word, ProseMirror, Notion, Quill, Automerge,
Yjs, Loro, Dolt and Peritext all independently arrived at. That is reassurance,
not novelty: the research contribution has to sit somewhere else — provenance,
review semantics, research-instrument meaning — which
[RICH-TEXT-VERSIONING-RESEARCH.md](RICH-TEXT-VERSIONING-RESEARCH.md) already says.

Five corrections and additions come out of the review:

1. **"Merkle tree" is the wrong name for the history.** Three different tree
   structures are being conflated (§1.2). The history is a **DAG of events**, the
   integrity layer is a **hash chain plus Merkle commitments**, and the
   storage-sharing layer — if we build it — is a **prolly tree**, which is not the
   same thing as a Merkle tree.
2. **The segmentation problem is already solved and is not a search problem.**
   "Largest area with the same formatting" is *maximal-run canonicalisation*: the
   answer is unique, greedy computation is provably optimal, and it is one linear
   pass (§3.2). There is no dynamic program to write. The real work is making the
   pass incremental instead of re-running it (§3.4), and not letting
   canonicalisation destroy the authorship seam (§3.3).
3. **Formatting is not a tree, and that is the hard problem we have not costed.**
   Overlapping and cross-block annotations — a bold span crossing a paragraph
   boundary, a comment spanning three blocks, a tracked change over a list — do not
   nest. This is the classic overlapping-markup problem and it bites every
   block-tree model eventually (§2, item 1).
4. **The compression win is columnar encoding of the operation log, not
   compressing documents.** Automerge, Yjs, Loro and Eg-walker all get their size
   from run-length and delta-coded columns over a repetitive history; Eg-walker
   additionally shows you can discard the expensive in-memory metadata entirely
   between merges (§4.7). That last idea maps unusually well onto our
   server-authoritative design.
5. **Machine learning must stay out of the identity path.** Anything that
   determines a hash, a merge result or an audit answer has to be deterministic and
   reproducible years later. ML belongs in ingestion, advisory semantics, conflict
   *suggestions* and search — the derived plane, never the canonical plane (§6).

---

## 1. Are we on the right track?

### 1.1 Blocks with runs inside them: confirmed

The proposed unit — a stable block containing an ordered sequence of text runs,
each carrying complete formatting metadata — is not a guess. It is the same model
in every serious system:

| System | Its name for the block | Its name for the run |
|---|---|---|
| [WordprocessingML](https://learn.microsoft.com/en-us/office/open-xml/word/structure-of-a-wordprocessingml-document) | `w:p` paragraph | `w:r` run with `w:rPr` properties |
| [ProseMirror](https://github.com/ProseMirror/prosemirror-model) / Tiptap | block node | text node + marks, coalesced in a `Fragment` |
| [Quill Delta](https://quilljs.com/docs/delta/) | newline-attributed line | `insert` op with `attributes` |
| [Notion](https://www.notion.com/blog/data-model-behind-notion) | block with a UUIDv4 id | property value with inline annotations |
| [BlockNote](https://www.blocknotejs.org/docs/foundations/document-structure) | `Block { id, type, props, content, children }` | styled inline content |
| [Peritext](https://www.inkandswitch.com/peritext/static/cscw-publication.pdf) | paragraph marker inside the character sequence | formatting span anchored to character IDs |

So the model is right, and the correct framing of our contribution is the one
already in our docs: runs are not the research idea; *versioning them with
identity, provenance and review semantics* is.

Two points in the current model are specifically endorsed by this prior art and
should not be traded away:

- **Stable block IDs, assigned once, never reused.** BlockNote states the
  invariant plainly — an id lives from creation to removal and is never shared.
  Our ULID node IDs do the same. Attribution, comment anchoring, merge alignment
  and the provenance projection all depend on it.
- **Run boundaries are *not* identity.** We already say this. It bears restating
  because the Quill/Delta lineage makes the opposite tempting: an offset is a
  coordinate in one specific base revision, not a durable anchor.

### 1.2 The three trees we must not conflate

Most of the ambiguity in "merkle tree or maybe some other appropriate algorithm"
dissolves once these are separated. They are different structures with different
algorithms, and we need all three.

```
+-- Tree 1: the DOCUMENT tree --------------------------------------------+
| doc -> section -> paragraph -> runs. Ordered, shallow, edited constantly|
| Algorithms: maximal-run canonicalisation, path-based patches,           |
|             structural three-way merge, tree diff only at the margins.  |
+-------------------------------------------------------------------------+

+-- Tree 2: the HISTORY ---------------------------------------------------+
| NOT a tree. A DAG of events: branches fork, merges join.                |
| Algorithms: causal ordering, three-way merge against a common ancestor, |
|             (later) Eg-walker-style replay over the event graph.        |
| Integrity: a hash chain over the append order, plus Merkle commitments  |
|            for inclusion proofs. This is what "merkle" belongs to.      |
+-------------------------------------------------------------------------+

+-- Tree 3: the STORAGE tree ---------------------------------------------+
| Content-addressed objects; equal subtrees stored once.                  |
| Algorithms: prolly tree / content-defined chunking, structural sharing, |
|             skip-equal-hash diffing, delta chains for cold objects.     |
+-------------------------------------------------------------------------+
```

Vol. II already handles the hash-chain and Merkle mathematics correctly (Part E,
including where a chain beats a tree beats a DAG). The contribution here is mostly
terminological discipline: when someone says "Merkle tree" in a design meeting, we
should ask which of the three. The wrong answer to "how do we diff two versions
fast?" is *walk a Merkle tree of the document*; the right answer is *compare
per-block content hashes and descend only where they differ*.

### 1.3 Merkle is an integrity structure; prolly trees are the diffing structure

A plain Merkle tree over a document gives tamper-evidence and O(log n) inclusion
proofs. What it does *not* give is stable chunk boundaries: insert one character at
the start of a Merkle-chunked document and every downstream boundary shifts, so
every hash changes and structural sharing collapses.

[Prolly trees](https://docs.dolthub.com/architecture/storage-engine/prolly-tree)
(probabilistic B-trees, from Noms, used by Dolt) fix exactly this by choosing node
boundaries with a rolling hash over the content —
[content-defined chunking](https://www.usenix.org/system/files/conference/atc16/atc16-paper-xia.pdf) —
rather than by insertion order. The consequence is *history independence*: the
tree's shape depends only on content, so two versions that share data share chunks
however each was built, and a diff can skip any subtree whose hash matches. Dolt's
[scaling write-up](https://www.dolthub.com/blog/2025-05-16-millions-of-versions/)
is the clearest argument for why this matters at history scale, and their
[JSON prolly trees post](https://www.dolthub.com/blog/2024-07-15-json-prolly-trees/)
is the closest analogue to our own payloads.

For us this is a **storage decision for later** — it is already increment 3 on our
roadmap — and it should be evaluated against the far simpler baseline of per-block
content hashing, which we already compute. A 400-page document is on the order of
8,000 blocks; per-block hashes plus a flat manifest may well be enough. Prolly
trees earn their complexity mainly when an individual block is itself large, or
when spreadsheet/tabular data gets versioned the same way.

[Merkle Search Trees](https://zenodo.org/records/3935655) (Auvolat & Taïani, 2019)
are the related structure for ordered-key state with anti-entropy between replicas.
Relevant only if we ever do peer-to-peer replication; not for the current engine.

### 1.4 Where the design is genuinely under-specified

Honest list, from comparing our model against the papers:

- **Nested identity.** We have stable top-level block IDs. We do not have stable
  IDs for inner paragraphs, table cells, list items or inline atoms. Attribution,
  comment anchoring and move detection all eventually need them. This is already
  increment 4 on our roadmap and it is the highest-leverage gap we have.
- **Split and join semantics.** When a paragraph splits in two, which half keeps
  the ID? That answer determines what "who changed this?" returns forever after.
  No paper hands us a correct answer; Peritext and WikiWho each pick one and
  document it. We must pick one and document it too.
- **Move.** Reordering blocks is currently a conflict. The
  [move-operation CRDT](https://martin.kleppmann.com/papers/move-op.pdf)
  (Kleppmann, Mulligan, Gomes & Beresford, IEEE TPDS 2022, proved in Isabelle/HOL)
  is the known-correct treatment, and Vol. II §C.2 already reproduces it. It is
  cheap *for us* precisely because we serialize appends server-side.
- **Cross-block ranges.** See §2, item 1 — the largest unpriced item in the design.

---

## 2. Challenges the brief did not list

The brief named three: detecting formatting, preserving it, and reconciling
conflicting concurrent edits with provenance. All real. Here are the others the
literature says will arrive, roughly ordered by how much they will hurt.

| # | Challenge | Why it bites | Where it is treated |
|---|---|---|---|
| 1 | **Formatting is not a tree** — bold spans, comments, tracked changes and citations cross block boundaries and overlap each other | A block tree can only express nesting. The moment a reviewer comments across two paragraphs, or a bold span must survive a paragraph split, the annotation cannot live *inside* a block | [Peritext](https://www.inkandswitch.com/peritext/static/cscw-publication.pdf) stores spans *beside* the text, anchored to character IDs, instead of inside the tree |
| 2 | **Mark boundary intent** — type at the end of a bold word: is the new text bold? And concurrently someone un-bolds that word | Every editor answers differently; the answer must be *per mark type* (links usually do not expand, bold usually does) and must survive merging | Peritext's expand/shrink rules; [Loro's crdt-richtext](https://github.com/loro-dev/crdt-richtext) makes expand behaviour a per-mark configuration |
| 3 | **Interleaving** — two authors insert at the same position and the merge interleaves their sentences character by character | Affects OT *and* CRDTs, went unnoticed for decades, and produces unreadable text rather than an honest conflict | [The Art of the Fugue](https://arxiv.org/abs/2305.00583) (Weidner, Gentle & Kleppmann) defines *maximal non-interleaving* and gives FugueMax |
| 4 | **Concurrent move / cycles** — A moved under B while B is moved under A | Naively produces a cycle and corrupts the tree; real bugs shipped in Google Drive and Dropbox | [A highly-available move operation for replicated trees](https://martin.kleppmann.com/papers/move-op.pdf) |
| 5 | **Attribution granularity vs. cost** — "who wrote this sentence?" | Per-character attribution is the obvious model and is ruinously expensive. It is also *derived*, and derivation is ambiguous when text is copied, moved, deleted and reinserted | [WikiWho](https://archives.iw3c2.org/www2014/proceedings/proceedings/p843.pdf) (Flöck & Acosta, WWW 2014) does token-level provenance over full revision history at ~95% attribution accuracy, using paragraph → sentence → token matching with a diff fallback |
| 6 | **Canonicalisation determinism** — mark order, JSON key order, number formatting, Unicode normalisation, `12pt` vs `16px` | In a hash-chained system one non-deterministic serialization detail silently forks history. Highest-severity, lowest-glamour risk we carry | Our existing rule — conservative JSON equality, canonical key order, no NFC rewriting — is correct and must never be "improved" casually |
| 7 | **History growth and tombstones** | Append-only history grows without bound; deleted content cannot be dropped until it is causally stable | Vol. II §B.4; snapshot generations plus compaction |
| 8 | **Erasure vs. immutability** | A consultation participant requesting deletion collides with an append-only hash chain. The resolution — content-addressed indirection, or key-shredding — must be decided before launch, not after | Not covered in our docs; flagged here |
| 9 | **Binary embeds** | Images inlined into block JSON destroy every compression and diff property we are trying to build | Content-addressed blobs out of line; the block holds only a hash |
| 10 | **Semantic vs. presentational equivalence** | `<em>` vs italic, theme fonts vs literal fonts, Word style inheritance vs direct formatting: same appearance, different metadata, therefore different hashes and spurious diffs | Handled at import/normalisation time with explicit rules, never silently inside the merge |
| 11 | **Diff alignment ambiguity on repeated text** | Myers admits many valid alignments over repeated strings; the chosen one decides what a reviewer sees attributed to whom | Ours already picks one and applies conservative overlap checks. This is a documented limitation, not a bug |
| 12 | **Per-container time budgets compose badly** | A 20 ms diff deadline per container is fine; 8,000 containers × 20 ms is 160 s. We need a request-level budget, not only a per-diff one | See §3.6 |

---

## 3. Problem A — block segmentation and run metadata

### 3.1 Naming: do not call it "paradata"

In survey methodology — the exact field our users work in — **paradata** already
means *data about the data-collection process*: interview timings, GPS traces,
keystroke logs, device metadata. Reusing the word for "formatting metadata on a
text run" will collide with the meaning our researchers already hold, in a product
whose whole point is fidelity to research practice.

The settled vocabulary in the systems literature is:

- **Formatting run** (or *styled run*, *attributed text span*): a maximal adjacent
  range of text sharing identical metadata. Word calls it a run; ProseMirror calls
  it a text node with marks.
- **Style descriptor**: the reusable metadata value itself, interned and shared.
- **Mark / annotation / span**: an interval carrying meaning, which may overlap
  others and may cross block boundaries.

Our existing model document already uses *structural block*, *formatting run*,
*style descriptor* and *semantic role*. That vocabulary is correct — keep it, and
reserve "paradata" for the survey sense.

### 3.2 "Largest area with the same formatting" is maximal-run canonicalisation, and greedy is already optimal

The intuition in the brief — segment by the biggest region sharing the same
formatting — is right, and it is better than the brief assumes, because it is not a
search problem at all.

Define equivalence on adjacent positions: positions *i* and *i+1* are in the same
run iff their complete style descriptors are equal. Runs are then the equivalence
classes of the transitive closure of that relation. Two consequences:

1. **The maximal-run decomposition is unique.** There is no choice to make and
   therefore nothing to optimise. Any two implementations that agree on descriptor
   equality produce identical run boundaries.
2. **A single left-to-right pass computes it optimally.** Extend the current run
   while the descriptor matches; start a new one when it does not. This is O(n) in
   scalars plus the cost of descriptor comparison. No dynamic program, no
   minimum-description-length criterion, no learned model can do better than a
   unique answer.

This matters strategically: **it makes the segmentation deterministic**, which is
the precondition for hashing it, and hashing it is the precondition for dedup,
skip-equal diffing, and the audit chain. Any "smarter" segmentation that produced a
different answer for the same content would break the property we actually need.

Dynamic-programming and MDL segmentation — [Kay & Han's optimal signal
segmentation](https://www.ele.uri.edu/faculty/kay/New%20web/downloadable%20files/imagesegment.pdf),
[optimal changepoint algorithms](https://link.springer.com/article/10.1007/s11222-016-9636-3)
such as PELT/FPOP, [optimal multi-paragraph text segmentation](https://arxiv.org/pdf/cs/9812005) —
are genuinely useful, but for a *different* problem: choosing boundaries when the
criterion is a cost trade-off rather than an equality relation. That describes
storage chunking (§3.5), not formatting runs.

### 3.3 What canonicalisation costs us: the provenance seam

There is a real trap here, and it is the one thing in the segmentation design worth
arguing about.

Coalescing adjacent equal-descriptor runs is correct for rendering and required for
hashing. But if Alice writes "the consent form" and Bob later appends "must be
signed" in the same style, canonicalisation merges them into one run — and the seam
between two authors disappears from the structure.

The resolution is the same one Peritext uses for formatting and WikiWho uses for
authorship: **attribution is a separate interval layer over the text sequence, not
a property of the run**. Runs canonicalise freely; the provenance layer keeps its
own boundaries, and the two are allowed to disagree.

Concretely, for our engine:

- Keep style runs canonical and hashable. Do not add author IDs to style
  descriptors — that would fragment runs per author, inflate storage, and make two
  visually identical documents hash differently.
- Derive attribution from the event log on demand (which is what
  `GET /documents/{id}/structure` already begins to do), or materialise it
  asynchronously into the proposed `run_span` / `operation_span` projections.
- Accept that derived attribution is a *best reconstruction*, and say so in the UI.
  WikiWho's ~95% on Wikipedia is the realistic ceiling for reconstruction from
  diffs; we can beat it only where we recorded intent at edit time.

### 3.4 Making it extremely fast: three levers, in order of payoff

Our measured baseline ([benchmarks/richtext-2026-09-21.json](benchmarks/richtext-2026-09-21.json))
on a synthetic 8,000-block, 3.94M-scalar, 6.03 MB-JSON fixture:

| Stage | Median |
|---|---:|
| Parse already-available JSON | 53.68 ms |
| Classify parsed tree and intern style metadata | 32.86 ms |
| Build a formatting patch for a long paragraph (with verification) | 0.156 ms |
| Replay that patch (with hashes) | 0.060 ms |

**Lever 1 — stop re-parsing and re-scanning (the biggest win by far).**
Classification is already linear; the problem is that it runs over the *whole*
document. The technique to adopt is Raph Levien's
[rope science](https://xi-editor.io/docs/rope_science_01.html): treat the
interesting quantities as **monoid homomorphisms** over text — scalar count, run
count, distinct-style set, content hash — and cache each one at every node of a
balanced tree. Then an edit updates O(log n) cached summaries instead of forcing an
O(n) re-scan, and whole-document classification becomes a root lookup.
[Part 2 on metrics](https://xi-editor.io/docs/rope_science_02.html) is the precise
statement; note that the planned part 7, on spans and interval trees, was never
written, so the span side has to come from Peritext instead.

This is the structural answer to "extremely fast". Everything else is constant
factors.

**Lever 2 — parse less, and parse faster.** Parsing costs more than classifying
(53.68 ms vs 32.86 ms), so the classifier is not the bottleneck today.
[simdjson](https://arxiv.org/abs/1902.08318) (Langdale & Lemire, VLDB Journal 2019)
parses JSON at gigabytes per second using SIMD and roughly a quarter of RapidJSON's
instructions; `simd-json` and `sonic-rs` bring that to Rust. Better still is to
parse *nothing*: if the client sends compact patches rather than whole documents
(increment 2 on our roadmap), the 6 MB parse disappears from the hot path entirely.

**Lever 3 — cheaper style interning.** Today
[`Styles::intern`](../crates/engine-core/src/richtext.rs#L63-L78) canonicalises each
text node's metadata to JSON and allocates a `String` key into a `BTreeMap`:

```rust
let key = String::from_utf8(crate::log::canonical_json(&value)).unwrap();
```

That is one canonical serialization plus one heap allocation per text node, plus
O(log d) string comparisons. Three incremental improvements:

- Key on a 128-bit hash of the canonical bytes in a `HashMap` instead of the string
  itself, keeping the full value only in the interning vector. Removes the
  allocation and reduces comparison to integer equality. (Retain a full comparison
  on hash match if we want collision safety in the audit path.)
- Cache the descriptor id on the node when the editor already knows it, so
  unchanged nodes skip interning entirely — the same reuse trick we already apply
  to unchanged ProseMirror nodes during serialization.
- Note the structural parallel to [FSST](https://www.vldb.org/pvldb/vol13/p2649-boncz.pdf):
  interning is dictionary coding at the object level, and FSST's key property —
  equal values are equal in compressed form, so comparison needs no
  decompression — is exactly what makes interned style ids cheap to compare.

**What we should not do:** give every character a row, a per-word object, or a
database identity. Our docs already reject this and the rejection is correct;
Vol. II §C.3 has the cost mathematics.

### 3.5 Where segmentation *is* a real search problem

Two places, both in storage rather than formatting:

- **Chunk boundaries for deduplication.** Here the criterion is a trade-off
  (smaller chunks find more sharing, larger chunks cost less metadata), boundaries
  must be stable under insertion, and the right tool is content-defined chunking —
  [FastCDC](https://www.usenix.org/system/files/conference/atc16/atc16-paper-xia.pdf)
  (Xia et al., USENIX ATC 2016) is 3–10× faster than Rabin-based CDC at comparable
  or better dedup ratios, via a gear-based rolling hash, cut-point skipping and
  normalised chunk-size distribution.
- **Retrieval chunking for search and AI features.** Different problem, different
  unit, ML-appropriate (§6.2e). A *block* is a versioning unit; a *chunk* is a
  retrieval unit. Conflating them is a common and expensive mistake.

### 3.6 Specific risks in the current implementation

From reading [crates/engine-core/src/richtext.rs](../crates/engine-core/src/richtext.rs):

| Risk | Detail | Suggested response |
|---|---|---|
| Per-container deadline does not bound the request | 20 ms per text diff × many containers has no global ceiling | Add a request-level budget; degrade to `FieldEdited` fallback once exhausted |
| Full-tree `analyze` on every call | `analyze` pushes the whole tree onto a stack and walks it each time | Cache monoid summaries per block; recompute only dirty subtrees (§3.4, lever 1) |
| String-keyed interning | One canonical-JSON `String` allocation per text node | Hash-keyed interning (§3.4, lever 3) |
| `MAX_SCALARS = 100_000` per container | A pathological single paragraph is rejected rather than degraded | Confirm the fallback path is a clean `FieldEdited`, and that the limit is surfaced to the user, not just the log |
| Alignment ambiguity on repeated text | Myers picks one of several valid alignments | Already documented; keep it documented in user-facing review copy too |

---

## 4. Problem B — storage and compute cost

### 4.1 The five-layer stack

These are independent and compose multiplicatively. Most systems get most of their
win from layers 1 and 2.

| Layer | Technique | What it exploits | Status for us |
|---|---|---|---|
| 1. Operation | retain/insert/delete with style references | Unchanged text is never retransmitted | **Implemented** (`RichTextPatch` v1) |
| 2. Encoding | columnar layout, RLE, delta coding, varints | Histories are extremely repetitive across events | **Not started** — biggest available win |
| 3. Dictionary | style interning; trained zstd dictionaries | Small payloads share vocabulary | Interning done per event; cross-event dictionaries not started |
| 4. Content addressing | immutable block objects, structural sharing, CDC/prolly trees | Most blocks are unchanged between versions | Roadmap increment 3 |
| 5. Cold storage | delta chains + general-purpose compression | Old versions are rarely read | Not started |

### 4.2 Layer 2 is where the field actually wins

Every high-performing system in this space uses the same trick, and it is worth
being precise about it, because "columnar" sounds like a database thing and is
actually a serialization thing.

Instead of serializing each event as a record with its fields together, store each
*field* of all events in its own byte stream, then compress each stream with the
codec that suits it. From the
[Automerge binary format specification](https://automerge.org/automerge-binary-format-spec/):
RLE columns encode `(length, value)` pairs with negative lengths marking literal
runs; delta columns store signed differences from the predecessor; boolean-run
columns encode alternating true/false lengths; actor columns RLE-encode indices
into an actor table; and whole columns or chunks may additionally be DEFLATE'd.
The specification's own rationale is that Automerge "stores the full history of
changes to the document: this is a large amount of data but in practice it is very
repetitive and amenable to compression."

Two properties matter for us beyond size:

- **Decoding is fast** because each column is a homogeneous stream — that is the
  "speed of parsing" half of the design goal, and it is why Eg-walker can afford to
  load and replay history.
- **Some operations run directly on compressed bytes.** A million identical values
  costs three bytes at rest, and splice-type operations can be done in
  O(log n + affected bytes) without decompressing the column.

Loro reaches the same place through
[`serde-columnar`](https://github.com/loro-dev/crdt-richtext), explicitly crediting
Automerge for bringing columnar encoding to CRDTs; Yjs does its own integer/varint
encoding via lib0; Eg-walker stores its event graph column-wise with RLE'd event
types, UTF-8-compressed inserted characters, and separately coded parent references
and replica/sequence pairs.

**Recommendation:** define a columnar container format for our event payloads
before the history gets large. Retrofitting an encoding onto a live append-only log
is painful, because old readers must keep working — a constraint our rollout notes
already flag.

### 4.3 Layer 3: trained dictionaries are the cheap win for small events

Most of our events are small, and small data compresses badly because there is no
preceding context to learn from. Zstandard's answer is a trained dictionary:
reported gains are **2–5× better than dictionary-less compression on small
samples**, with training via the COVER algorithm (`zstd --train`), and the benefit
concentrated in the first few kilobytes — precisely our event-size regime.

This is low-risk and independently deployable: train a dictionary on a
representative corpus of our event payloads, version it, store the dictionary id
alongside the compressed bytes so old events stay readable, and never mutate a
published dictionary.

### 4.4 Layers 4 and 5: content addressing and delta chains

Content addressing is already half-present: we compute per-block content hashes.
The step to take is to make blocks *immutable objects addressed by those hashes*,
so a snapshot becomes a manifest of hashes and unchanged blocks are stored once
across every version that contains them. Diffing two versions then means comparing
manifests, and equal hashes are skipped without reading the content at all.

For cold storage, Git's packfile design is the reference: store one full object,
then instruction-encoded deltas against it, and zlib the result. The trade-off is
explicit and worth stating in our own design — delta chains reduce storage but make
reads do reconstruction work proportional to chain depth, which is why Git bounds
chain length. Our audit and restore paths care about read latency on old versions,
so any chain depth we pick is a product decision, not just a storage one.

### 4.5 Compute: the Eg-walker idea is the one to steal

[Collaborative Text Editing with Eg-walker: Better, Faster, Smaller](https://arxiv.org/abs/2409.14252)
(Gentle & Kleppmann, EuroSys 2025) is the most directly relevant recent paper to
our situation, and its central insight transfers even though we are not building a
CRDT.

The observation: CRDTs pay a permanent memory tax to keep per-character metadata
resident, so that a concurrent operation can be merged at any moment. But most
editing is not concurrent. Eg-walker therefore stores only the **event graph**, and
materialises CRDT-like internal state *only while merging*, discarding it
afterwards. The enabling concept is a **critical version**: a point in the graph
such that no event at or before it can affect how any event after it is
transformed. At a critical version the entire internal state can be dropped.

Reported results (their traces, normalised to ~500k characters; treat as directional):

| Claim | Reported |
|---|---|
| Steady-state memory vs. best CRDTs | 1–2 orders of magnitude less |
| Sequential traces (S1–S3) vs. reference CRDTs | 7–10× faster, from clearing state at critical versions |
| Asynchronous trace A2 vs. OT | milliseconds vs. approximately one hour |
| OT peak memory, Git Makefile trace | 6.8 GiB |
| Merging branches of k and m concurrent events | O(n log n), vs. O(n²) or worse for OT |
| File size vs. Automerge | competitive or smaller on most traces |
| File size vs. Yjs | smaller on sequential and asynchronous traces; 20–300% larger on highly concurrent traces (event-graph edges) |

Why this matters for Dynodoc specifically: our engine is **server-authoritative and
append-serialized**, so our event log is overwhelmingly sequential, with
concurrency confined to explicit branch merges. That is exactly the shape where
critical versions are dense and the optimisation pays 7–10×. We can adopt the
storage and replay discipline — keep the event graph as the system of record,
build expensive indices only during a merge, then throw them away — without
adopting a CRDT at all. It also sharpens the argument in Vol. II §C.3.5 against
storing fat CRDT state as the system of record.

### 4.6 Reference numbers from other implementations

Directional only; different corpora, machines and versions.

| Source | Measurement |
|---|---|
| Yjs, B4 real-world trace (260k characters) | ~227 KB on disk; ~1,074 ms to process 260k operations |
| Loro, same trace | ~231 KB on disk; Loro claims 2–5× smaller documents than Yjs/Automerge on its own corpus |
| [crdt-richtext](https://github.com/loro-dev/crdt-richtext), B4 with N=6000, M1 MacBook Pro | apply 176 ± 10 ms; encode 8 ± 1 ms; document 127,639 bytes |
| Automerge 2.0 Rust rewrite | memory on the same trace reported down from ~1.1 GB to ~44.5 MB |
| Diamond Types | fastest single-threaded plain-text CRDT on the paper trace; plain text only, explicitly not production |
| Our engine, formatting-heavy long paragraph | full content 9,924 JSON bytes vs. 302-byte patch (~97% smaller) — one example, not a ratio to generalise |

Caveat carried forward from our own benchmark file: these exclude DOCX conversion,
network, database writes, hash-chain insertion, rendering and concurrent users.

### 4.7 Benchmark against the standard traces, not only our synthetic fixture

Our 8,000-block synthetic fixture is useful for regression but is not comparable to
anything published. The field has shared editing traces —
[automerge-perf](https://github.com/automerge/automerge-perf),
[crdt-benchmarks](https://github.com/dmonad/crdt-benchmarks) (and
[zxch3n's fork](https://github.com/zxch3n/crdt-benchmarks)), and the
[Eg-walker paper artifact](https://zenodo.org/records/13823409) with its S1–S3,
C1–C2 and A1–A2 traces. Running our codec against those makes our numbers legible
to anyone who knows this literature, and gives us the adversarial cases (repeated
text, concurrent boundary formatting, Unicode, long-running branches) that our own
fixture does not contain. This directly serves roadmap increment 5, which already
calls for a shared adversarial corpus.

### 4.8 Supporting structures worth knowing

- [Roaring bitmaps](https://arxiv.org/abs/1402.6407) (Chambi, Lemire, Kaser &
  Godin) for set membership — which blocks changed, which carry a given style,
  which are visible to a reviewer. Reported up to ~900× faster intersections than
  RLE-based alternatives while compressing better; adopted in Lucene, Spark and
  Druid. Useful once "diff two versions" becomes "intersect two block-id sets".
- [Succinct trees](https://cs.uwaterloo.ca/~imunro/cs840/ProjectPapers/SODA10_013_sadakanek.pdf)
  (LOUDS, balanced parentheses, DFUDS) store an n-node tree in 2n + o(n) bits.
  Worth knowing, probably not worth adopting: our trees are shallow and small
  relative to the text, so the win is marginal and the complexity is not.
- [truediff](https://andrepacak.de/paper/truediff_pldi2021.pdf) (Erdweg, Szabó &
  Pacak, PLDI 2021) for incremental, type-safe structural diffing — linear time,
  patch sizes on par with GumTree, reported an order of magnitude faster. Relevant
  if we ever need real tree diffing rather than the path-based patches plus
  ID-matching we use now.
- [X-Diff](https://research.cs.wisc.edu/niagara/papers/xdiff.pdf) (Wang & DeWitt)
  and XyDiff as the XML-era prior art on node signatures and subtree hashing —
  mainly useful as a reminder of the cost of *not* having stable IDs, which is what
  forces those algorithms to do expensive matching in the first place.

---

## 5. Open-source components: adopt, benchmark, or just read

| Project | What it is | Our recommended posture |
|---|---|---|
| [Loro](https://github.com/loro-dev/loro) | Rust CRDT with rich text (Peritext + Fugue), movable tree, columnar encoding | **Benchmark seriously** for increment 5. Closest single match to what we need; Rust, so it fits the engine |
| [Automerge](https://github.com/automerge/automerge) | Rust CRDT, Peritext rich text, documented [binary format](https://automerge.org/automerge-binary-format-spec/) | **Read the format spec and copy the encoding ideas now**, independent of whether we adopt the library |
| [Yjs](https://github.com/yjs/yjs) | Mature JS CRDT, best editor-integration ecosystem | Candidate for a future live-collaboration layer only; still needs an adapter to our reviewed semantic history |
| [Eg-walker artifact](https://github.com/josephg/egwalker-paper) | Paper code, experiments and traces | **Read and reuse the traces.** Its architecture idea is the one to steal (§4.5) |
| [Peritext prototype](https://github.com/inkandswitch/peritext) | TS reference for rich-text CRDT semantics | Use as the **acceptance oracle** for boundary cases, as our docs already propose |
| [crdt-richtext](https://github.com/loro-dev/crdt-richtext) | Rust Peritext+Fugue on `generic-btree` + `serde-columnar` | Study the data-structure choices. Authors state the interface is unstable and not for production |
| [zstd](https://github.com/facebook/zstd) | Compression with dictionary training | **Adopt for layer 3.** Low risk, independently deployable |
| [FSST](https://github.com/cwida/fsst) (and [Rust port](https://github.com/spiraldb/fsst)) | Random-access string compression, LZ4-class speed | Evaluate for a shared style/string dictionary if interning outgrows a simple map |
| [simd-json](https://arxiv.org/abs/1902.08318) / sonic-rs | SIMD JSON parsing | Evaluate against the 53.68 ms parse; prefer eliminating the parse first |
| [roaring-rs](https://arxiv.org/abs/1402.6407) | Compressed bitmaps | Adopt when set-difference queries over blocks appear |
| [merkle-search-tree](https://docs.rs/merkle-search-tree) | Rust MST for anti-entropy | Only if peer-to-peer replication is ever in scope |
| [Dolt](https://docs.dolthub.com/architecture/storage-engine/prolly-tree) | Prolly-tree versioned SQL storage | Read the design; do not adopt the database. Relevant for increment 3 and for versioned spreadsheets |
| [similar](https://docs.rs/similar/2.7.0/similar/) | Myers diff with deadline | Already pinned and in use |
| [Docling](https://github.com/docling-project/docling) | MIT-licensed document conversion with ML layout analysis | Candidate for **ingestion only** (§6.2a) |

Licensing must be checked per project before any adoption; this table is a research
shortlist, not a legal clearance.

One negative result worth recording so nobody re-reads it:
[Differentially Processed Optimized Collaborative Rich Text Editor](https://arxiv.org/abs/2407.03027)
(Jatana et al., 2024) sounds directly on-topic but is about multiplexing several
collaborative applications over a single WebSocket connection. Its headline "96%
improvement in access time" is about connection handling, not about rich-text
representation, diffing or merging. Not relevant to this design.

---

## 6. Machine learning and deep learning: where it belongs

The brief asks whether ML or DL models could be used "instead of strict
algorithms". The answer is a firm split rather than a yes or no.

### 6.1 The determinism boundary

Dynodoc's value proposition is a verifiable history: the same inputs must produce
the same hash, the same merge result and the same audit answer — today, and on
different hardware in five years, after the model vendor has deprecated the version
we used. Neural networks do not offer that. Floating-point non-determinism across
hardware and library versions, model updates, and sampling all break bit-exact
reproducibility, and a hash chain turns any such divergence into a silent fork.

So the rule is:

> **Canonical plane (deterministic only):** segmentation into runs, canonical
> serialization, hashing, patch construction and replay, merge outcomes, conflict
> detection, the event log, access control.
>
> **Derived plane (ML permitted):** ingestion of unstructured input, advisory
> semantic labels, conflict *suggestions* offered to a human, diff summaries,
> search and retrieval.

Our existing docs already assert a version of this ("no LLM or graph database in
the classification path", "semantic enrichment must be separate from deterministic
parsing"). This review endorses that and asks for it to be stated as an
architectural invariant with a test, not just a comment in `analyze`.

### 6.2 Legitimate, high-value uses

**(a) Ingestion — the strongest case.** Turning PDFs, scans and messy files into
blocks is genuinely an ML problem, and it sits *upstream* of versioning: the model
proposes a structure, a human or a deterministic validator accepts it, and only the
accepted result enters the log. [Docling](https://arxiv.org/abs/2501.17887)
(IBM Research, MIT-licensed, LF AI & Data) is the strongest open option: an RT-DETR
layout model trained on [DocLayNet](https://dl.acm.org/doi/10.1145/3534678.3539043)
(80,863 human-annotated pages, 11 element classes) plus TableFormer for table
structure recovery, running in a small resource budget on commodity hardware.
Alternatives include LayoutLMv3 and DiT; note that a
[low-resource-language comparison](https://dl.acm.org/doi/10.1145/3603287.3651184)
found YOLOv8 beating both on IoU, so model choice should be driven by our own
document mix rather than by leaderboard reputation.

This does not help our DOCX path, which is already structured and is correctly
handled by deterministic OOXML parsing. It helps the PDF and scan paths we do not
support yet.

**(b) Semantic role labelling.** Classifying a block as a consent clause, an
eligibility criterion, a question stem or a citation is a natural classification
task, and our model already has exactly the right hook: `semantic role` is defined
as optional, evidence-based, and explicitly *not* proven by formatting. Keep the
label advisory and out of the hash; store it as a derived annotation with a model
identifier and version so a later relabelling is auditable.

**(c) Conflict resolution suggestions.**
[MergeBERT](https://arxiv.org/abs/2109.00084) (Svyatkovskiy et al., ESEC/FSE 2022)
reformulates merge resolution as classification over primitive merge patterns
extracted from real merge commits, reporting 64–69% precision — roughly 2× prior
structured and neural tools; DeepMerge reported 55.0% top-1 and could only produce
predictions for 63.8% of test conflicts. Read those numbers carefully: **a third of
suggestions are wrong.** That is useful as a ranked suggestion a reviewer accepts
or rejects, and unacceptable as an automatic resolution in a system whose promise
is that nobody's wording is silently lost. Our four-card conflict UI is already the
right surface for this.

**(d) Change summarisation.** "What changed in this version?" in prose is a
straightforward generative task over a deterministic diff. The diff stays
authoritative; the summary is a convenience with a visible provenance note.

**(e) Retrieval segmentation.** For search and any AI assistance over documents,
semantic chunking and
[late chunking](https://arxiv.org/pdf/2504.19754) — embedding the whole document
first, then pooling into chunks so each retains global context — outperform
fixed-size splitting. Large comparative evaluations now exist across dozens of
segmentation strategies. Again: retrieval chunks are a *separate index*, derived
from blocks, never the versioning unit.

Adjacent and worth knowing for (b) and (e): supervised topic segmentation from
[Koshorek et al.](https://arxiv.org/pdf/1803.09337) (NAACL 2018, WIKI-727K) and
[cross-segment BERT](https://arxiv.org/pdf/2004.14535) (Lukasik et al., 2020).

### 6.3 Learned compression: impressive, and wrong for us

[Language Modeling Is Compression](https://arxiv.org/abs/2309.10668) (Delétang et
al.) makes the equivalence precise — log-loss minimisation *is* a lossless
compression objective — and demonstrates it dramatically: Chinchilla 70B compresses
ImageNet patches to 43.4% (PNG: 58.5%) and LibriSpeech to 16.4% (FLAC: 30.3%),
despite being trained on text.

It is still the wrong choice for our object store, for one decisive reason:
**decompression requires the exact model, with bit-exact inference, forever.** A
system whose entire proposition is that a research instrument's history remains
verifiable for years cannot make its archive unreadable without a specific GPU, a
specific library version and a specific checkpoint. The failure mode is total and
silent. Zstandard with a trained dictionary captures a large share of the available
win (§4.3) at a tiny fraction of the archival risk, and its format is stable,
auditable and implementable from a specification.

If learned compression is ever revisited, it should be for a *cache or transport*
layer where the original remains recoverable by deterministic means — never for the
system of record.

### 6.4 If a model ever does enter the edit path

Preconditions, all of them: the model artefact is content-addressed and archived
alongside the data; inference is pinned and reproducible; the model's output is
recorded as a *proposal event* with its own provenance, distinct from the human
acceptance event that follows it; and the deterministic path can reconstruct the
document without the model. In practice these constraints mean "the model proposes,
the log records what a human accepted" — which is exactly (a) through (d) above.

---

## 7. Recommendations, in dependency order

Mapped onto the increments already listed in
[RICH-TEXT-VERSIONING-RESEARCH.md](RICH-TEXT-VERSIONING-RESEARCH.md).

| # | Action | Why now | Maps to |
|---|---|---|---|
| 1 | Write down the **canonical vs. derived plane** invariant and add a test that the classification and merge paths take no model, network or clock input | Cheap, prevents the most expensive possible mistake | cross-cutting |
| 2 | Adopt the **standard editing traces** (automerge-perf, crdt-benchmarks, Eg-walker artifact) as a second benchmark corpus | Makes our numbers comparable and supplies adversarial cases we lack | increment 5, 6 |
| 3 | Add a **request-level time budget** above the per-diff 20 ms deadline | Current bound does not compose across 8,000 containers | increment 6 |
| 4 | Switch style interning to **hash-keyed**, and cache descriptor ids on unchanged nodes | Removes per-node allocation and canonicalisation from the hot path | increment 2 |
| 5 | Specify the **columnar event encoding** (RLE + delta + varint columns, DEFLATE/zstd per column) before the log grows | Retrofitting an encoding onto a live append-only log is painful | increment 3 |
| 6 | Train and version a **zstd dictionary** for event payloads | 2–5× on small payloads, low risk, independently shippable | increment 3 |
| 7 | Decide and document **split/join identity semantics** | Everything about attribution depends on it; it is currently undefined | increment 4 |
| 8 | Design **cross-block annotation storage** (spans beside the tree, Peritext-style), even if only comments use it at first | The block tree cannot represent it, and the need is certain | increment 4 |
| 9 | Make blocks **immutable content-addressed objects** with manifest snapshots; measure against per-block hashing before reaching for prolly trees | Structural sharing is the layer-4 win; the simple version may suffice | increment 3 |
| 10 | Prototype **monoid-cached metrics** over the block tree so classification is incremental | The structural answer to "extremely fast"; everything else is constant factors | increment 2, 6 |
| 11 | Evaluate **Loro** (and Automerge) against a shared adversarial corpus before writing any convergence code | Build-vs-buy decision should be made on measurement | increment 5 |
| 12 | Decide the **erasure vs. immutability** policy before public consultation launches | Legal exposure, and it constrains the storage design | consultation stages |

---

## 8. What this review did not establish

- No benchmark was run for this document. Every number in §4.6 is someone else's,
  on their corpus and hardware.
- No licence review was performed on any listed project.
- Adoption of Loro, Automerge or Yjs is **not** recommended here; measurement
  against a shared corpus is.
- Nothing here establishes that 300–400 page documents import, classify and save
  within a product-acceptable budget. That remains an open item in our own
  roadmap (increment 6) and is unchanged by this review.
- Peritext's guarantees are not our guarantees. Our centralized merger
  deliberately reports ambiguous boundaries for review; that remains true.
- Loro's own rich-text design blog was not readable during this review (HTTP 403);
  the Loro material here comes from the `crdt-richtext` repository and Loro's
  published documentation instead.

---

## 9. Sources

**Rich-text and collaborative editing**

- Litt, Lim, Kleppmann & van Hardenberg, [Peritext: A CRDT for Collaborative Rich Text Editing](https://www.inkandswitch.com/peritext/static/cscw-publication.pdf), PACM HCI 6 (CSCW2), art. 531, 2022 · [ACM](https://dl.acm.org/doi/10.1145/3555644) · [prototype](https://github.com/inkandswitch/peritext)
- Gentle & Kleppmann, [Collaborative Text Editing with Eg-walker: Better, Faster, Smaller](https://arxiv.org/abs/2409.14252), EuroSys 2025 · [artifact](https://zenodo.org/records/13823409) · [code](https://github.com/josephg/egwalker-paper)
- Weidner, Gentle & Kleppmann, [The Art of the Fugue: Minimizing Interleaving in Collaborative Text Editing](https://arxiv.org/abs/2305.00583), 2023
- Kleppmann, Mulligan, Gomes & Beresford, [A Highly-Available Move Operation for Replicated Trees](https://martin.kleppmann.com/papers/move-op.pdf), IEEE TPDS, 2022
- Nicolaescu, Jahns, Derntl & Klamma, Yjs / YATA, ICWE 2015 · [internals](https://github.com/yjs/yjs/blob/main/INTERNALS.md)
- [Quill Delta format](https://quilljs.com/docs/delta/) · [ProseMirror model](https://github.com/ProseMirror/prosemirror-model)
- Jatana et al., [Differentially Processed Optimized Collaborative Rich Text Editor](https://arxiv.org/abs/2407.03027), 2024 — *not relevant; see §5*

**Version control, provenance and writing**

- Ink & Switch, [Upwelling: Combining real-time collaboration with version control for writers](https://www.inkandswitch.com/upwelling/), 2023
- Ink & Switch, [Patchwork: towards universal version control](https://www.inkandswitch.com/patchwork/notebook/)
- Ink & Switch, [Jacquard: Version control and provenance for empirical research](https://www.inkandswitch.com/jacquard/notebook/) — closest external project to Dynodoc's positioning; uses implicit build specs and fine-grained provenance linking source passages to outputs
- Flöck & Acosta, [WikiWho: Precise and Efficient Attribution of Authorship of Revisioned Content](https://archives.iw3c2.org/www2014/proceedings/proceedings/p843.pdf), WWW 2014 · [code](https://github.com/wikiwho/WikiWho) · [TokTrack dataset](https://arxiv.org/pdf/1703.08244)
- [W3C PROV-O](https://www.w3.org/TR/prov-o/)

**Storage, encoding and compression**

- [Automerge binary document format specification](https://automerge.org/automerge-binary-format-spec/)
- Xia et al., [FastCDC: a Fast and Efficient Content-Defined Chunking Approach for Data Deduplication](https://www.usenix.org/system/files/conference/atc16/atc16-paper-xia.pdf), USENIX ATC 2016
- Boncz, Neumann & Leis, [FSST: Fast Random Access String Compression](https://www.vldb.org/pvldb/vol13/p2649-boncz.pdf), VLDB 2020
- Chambi, Lemire, Kaser & Godin, [Better bitmap performance with Roaring bitmaps](https://arxiv.org/abs/1402.6407), 2014/2016
- Langdale & Lemire, [Parsing Gigabytes of JSON per Second](https://arxiv.org/abs/1902.08318), VLDB Journal 2019
- Auvolat & Taïani, [Merkle Search Trees: Efficient State-Based CRDTs in Open Networks](https://zenodo.org/records/3935655), 2019
- Dolt, [Prolly Trees](https://www.dolthub.com/blog/2024-03-03-prolly-trees/) · [JSON prolly trees](https://www.dolthub.com/blog/2024-07-15-json-prolly-trees/) · [scaling to millions of versions](https://www.dolthub.com/blog/2025-05-16-millions-of-versions/)
- [Git packfiles and delta compression](https://git-scm.com/book/en/v2/Git-Internals-Packfiles)
- [Zstandard dictionary training](https://github.com/facebook/zstd/)
- Sadakane & Navarro, [Fully-Functional Succinct Trees](https://cs.uwaterloo.ca/~imunro/cs840/ProjectPapers/SODA10_013_sadakanek.pdf), SODA 2010

**Segmentation, diffing and trees**

- Levien, [Rope science part 1: MapReduce for text](https://xi-editor.io/docs/rope_science_01.html) · [part 2: metrics](https://xi-editor.io/docs/rope_science_02.html)
- Erdweg, Szabó & Pacak, [truediff: Concise, Type-Safe, and Efficient Structural Diffing](https://andrepacak.de/paper/truediff_pldi2021.pdf), PLDI 2021
- Wang, DeWitt & Cai, [X-Diff: An Effective Change Detection Algorithm for XML Documents](https://research.cs.wisc.edu/niagara/papers/xdiff.pdf), ICDE 2003
- Maidstone et al., [On optimal multiple changepoint algorithms for large data](https://link.springer.com/article/10.1007/s11222-016-9636-3), Statistics and Computing, 2017
- Kay & Han, [Optimal Segmentation of Signals Based on Dynamic Programming](https://www.ele.uri.edu/faculty/kay/New%20web/downloadable%20files/imagesegment.pdf)
- Heinonen, [Optimal Multi-Paragraph Text Segmentation by Dynamic Programming](https://arxiv.org/pdf/cs/9812005), 1998

**Machine learning**

- Auer et al., [Docling: An Efficient Open-Source Toolkit for AI-driven Document Conversion](https://arxiv.org/abs/2501.17887), 2025 · [technical report](https://arxiv.org/abs/2408.09869) · [code](https://github.com/docling-project/docling)
- Pfitzmann et al., [DocLayNet: A Large Human-Annotated Dataset for Document-Layout Segmentation](https://dl.acm.org/doi/10.1145/3534678.3539043), KDD 2022
- Svyatkovskiy et al., [MergeBERT: Program Merge Conflict Resolution via Neural Transformers](https://arxiv.org/abs/2109.00084), ESEC/FSE 2022
- Delétang et al., [Language Modeling Is Compression](https://arxiv.org/abs/2309.10668), ICLR 2024
- Koshorek et al., [Text Segmentation as a Supervised Learning Task](https://arxiv.org/pdf/1803.09337), NAACL 2018
- Lukasik et al., [Text Segmentation by Cross Segment Attention](https://arxiv.org/pdf/2004.14535), EMNLP 2020
- [Reconstructing Context: Evaluating Advanced Chunking Strategies for Retrieval-Augmented Generation](https://arxiv.org/pdf/2504.19754), 2025
