# A Versioning, Review & Collaboration Protocol for Dynodoc

> Historical research-instrument reference. The 10 October 2026 [product specification](PRODUCT-SPEC.md) and [engine evolution plan](ENGINE-EVOLUTION-PLAN.md) supersede this product scope/build order. Preserve existing history compatibility; proposed algorithms are not implementation proofs.

### Design compendium — every approach, end-to-end, with the science, mathematics, engineering, and economics behind each

**Project:** Dynodoc (agentic questionnaire authoring for ODK / Kobo / REDCap workflows)
**Document type:** Architecture & protocol design brief
**Status:** Exploratory — intended to precede an architectural decision record (ADR)
**Companion documents:** *Feasibility & Market Assessment*; *UI/UX Theory*

---

## 0. How to read this document

This is a design compendium, not a specification. Its job is to lay out the full solution space for "Git-like" editing, appending, and reviewing inside Dynodoc, so that the eventual decision is made with the whole landscape visible rather than by defaulting to the first plausible library.

The argument runs in four movements. **Part I** (§1–§2) establishes the problem and the underlying science: what a version-control system actually *is* once you strip away Git's command-line surface, and the mathematics of diffing, merging, causality, and convergence that any honest design must reckon with. **Part II** (§3) surveys prior art — the systems we are standing on the shoulders of, and where each one's ideas transfer or fail to transfer to our domain. **Part III** (§4–§6) builds the semantic model specific to survey instruments and then presents five complete, end-to-end candidate architectures, mapping the strongest one back onto the two-tier collaboration UX already designed. **Part IV** (§7–§10) covers engineering, economics, and a concrete phased recommendation, followed by formal appendices.

A reader who only wants the conclusion can skip to §9. A reader who wants to challenge the conclusion should read §2 and §5. A reader implementing it needs §4, §6, §7, and the appendices.

The central claim, stated once up front so the rest can argue for it: **a survey instrument is not a text file, and the single most consequential design decision is not which merge algorithm to use but how stable identity is assigned to items and fields.** Get identity right and most merge problems become tractable; get it wrong and no algorithm rescues you.

---

# PART I — THE PROBLEM AND THE SCIENCE

## 1. Why versioning a survey instrument is its own problem

### 1.1 The impedance mismatch

Git versions bytes arranged in lines. Its diff, its merge, its conflict markers, its storage — all of it assumes the atom of change is a line of text and the structure of a document is the sequence of its lines. This assumption is spectacularly successful for source code and spectacularly wrong for structured documents.

A survey instrument, in our system, is a typed tree: an instrument contains sections; sections contain items and nested groups; an item carries a type, a variable name, a label (often in several languages), a choice list, skip-logic and constraint expressions, and metadata such as which validated module it belongs to. The meaningful units of change are *these* — "the third choice of question 4 was relabelled," "a skip condition was added to question 7," "the PHQ-9 module was inserted as a block" — not "line 142 changed."

If you store this tree as serialized text (XLSForm rows, XML, JSON) and version the text, every tool you build inherits the impedance mismatch. Reordering two questions looks like a large deletion and a large insertion. Reformatting whitespace produces phantom diffs. A three-way merge can produce a syntactically corrupt instrument — a dangling skip reference, a duplicated variable name — because the line-merger has no idea what a variable name *is*. Worst of all, the conflict presented to the researcher is a wall of `<<<<<<<` markers in a format they never chose to read. The UX theory document already identified this as the core failure mode; this document explains why it is structural, not cosmetic, and what to do instead.

The correct atom of change is the *typed tree node*. The correct diff is a *tree diff*. The correct merge is a *structural, type-aware merge*. The correct conflict is a *semantic conflict surfaced at item granularity*. Each of these is a harder problem than its text-based cousin, and each has its own literature, which §2 develops.

### 1.2 What we actually want from "Git-like"

"Git-like" is a feeling more than a specification. When researchers and engineers ask for it, they want some subset of: a complete and trustworthy history (nothing is ever silently lost); the ability to see what changed, when, and by whom; the ability to work in parallel without overwriting each other; the ability to develop a variant in isolation and bring it back; and the ability to review changes before they land. They emphatically do *not* want: a staging area, a detached HEAD, rebasing, the reflog, or a mental model that takes a week to learn. The engineering challenge is to deliver the first list without importing the second.

It helps to name the capabilities precisely, because different architectures deliver different subsets cheaply:

| Capability | Plain-language meaning | Git's name for it |
|---|---|---|
| **History** | Every prior state is recoverable | commit log |
| **Attribution** | Who changed what, when, and why | blame / author |
| **Diff** | A legible description of a change | diff |
| **Concurrent editing** | Two people edit at once, no clobber | (Git punts to merge) |
| **Isolation** | Develop a variant without disturbing the main line | branch |
| **Integration** | Bring a variant's changes back | merge |
| **Review** | Approve changes before they land | pull request (external to Git) |
| **Provenance** | Tamper-evident record for audit/IRB | signed commits |

The last row is unusual and worth flagging early: a research instrument used for human-subjects data collection has compliance weight. An ethics board, a journal reviewer, or a replication auditor may need to know exactly which version of the instrument a given respondent saw, and that the recorded history has not been edited after the fact. This pushes us toward append-only, tamper-evident history as a *requirement*, not a nicety — a constraint that meaningfully narrows the architecture space (§5).

### 1.3 Requirements specific to research instruments

Five domain requirements shape everything downstream.

**Referential integrity is sacred.** Skip logic and constraints reference other items by variable name (`${hours_outdoors} >= 4`). The instrument is therefore not just a tree but a tree with a *dependency graph* laid over it. A version-control operation that breaks a reference — renaming a variable that something depends on, deleting an item another item's logic needs — must be either cascaded or refused. Text merge cannot do this; the system must understand the graph.

**Validated modules are atomic.** A scored instrument like the PHQ-9 is only valid if its items are unaltered and complete. The versioning system must be able to treat a module as a locked, indivisible unit — editable only by detaching it (and thereby flagging that the score is no longer comparable to the literature). This is a type of node with special merge rules: essentially "no merge, replace-or-reject."

**Field deployment is offline-first and low-bandwidth.** The intended users — icddr,b units, IPA, J-PAL South Asia, enumerator teams across LMIC settings — work in conditions where connectivity is intermittent and metered. This makes *offline editing with later reconciliation* a first-class scenario and makes the *byte cost of synchronization* an economic variable, not an afterthought (§7.2, §8.3). Architectures that require a live server to mediate every keystroke are disqualified for the field tier even if they are fine for the design tier.

**Multiple languages are structural, not metadata.** A label exists in English and Bangla (and possibly more). Translations have their own review lifecycle, can diverge, and must be merged independently. The data model must treat a label as a small map of language→string with its own per-language history, which interacts with merge.

**The reviewers include an AI agent.** The methodologist agent produces suggestions that must enter the same review pipeline as a human's, with the same attribution and the same accept/reject lifecycle. This is less a storage constraint than a uniformity constraint: the protocol must not have a privileged "machine edit" path that bypasses review. Whatever represents a human suggestion must also represent an agent suggestion.

These five, together with the compliance requirement from §1.2, are the rubric against which §5's architectures are scored.

## 2. Theoretical foundations

This section develops the science any serious design must reckon with. It is deliberately mathematical where the mathematics is load-bearing, because several of the candidate architectures in §5 live or die on properties proven (or disproven) here.

### 2.1 What a version-control system is, formally

Strip away the surface and a VCS is a structure for talking about three things: **state** (what the document is now), **history** (the lineage of states that produced it), and **identity** (which parts of one state correspond to which parts of another). Almost every hard problem in the field is really a problem of identity in disguise — diffing is "find the correspondence between two states," merging is "reconcile two states that share an ancestor," and the quality of both depends entirely on getting correspondence right.

Formally, it is useful to model history as a directed acyclic graph (DAG) of versions. Each node is a complete state (or enough information to reconstruct one); each edge points from a version to the version(s) it derived from. A linear history is a path; a branch is a fork; a merge is a node with two parents. This DAG-of-versions model is shared by Git, Mercurial, Darcs, and most modern systems, and it is the right mental model for our review workflow regardless of which storage mechanism we choose underneath.

### 2.2 Snapshots, deltas, and content-addressed storage

There are two ways to store a history: keep every full state (snapshots) or keep a base state plus the changes between states (deltas). Snapshots are simple and fast to read but storage-heavy; deltas are compact but require replaying a chain to reconstruct a state. Real systems blend the two: Git conceptually stores snapshots but physically compresses them into delta-packed files; RCS and older systems stored reverse deltas.

Git's deeper idea is **content-addressed storage**. Every object — a file blob, a tree, a commit — is named by the cryptographic hash of its content. Two identical files anywhere in history are stored once. A tree's hash depends on the hashes of its children, so a commit's hash transitively fixes the entire state of the project. This makes the version graph a **Merkle DAG**: a directed acyclic graph in which every node's identifier is a hash over its content and its children's identifiers.

Two consequences matter enormously for us. First, the structure is **tamper-evident for free**: you cannot alter any historical state without changing its hash and therefore the hashes of everything that descends from it, which immediately satisfies the IRB/audit provenance requirement of §1.2. Second, it gives **cheap structural sharing**: an instrument that differs from its predecessor by one choice label shares every unchanged item, section, and module by hash, so history storage grows with the *size of changes*, not the *size of the document times the number of versions*. Any architecture we choose should keep this property; it is one of the few pieces of Git we want almost unaltered.

The cost is that content addressing fixes identity to *content*. If a node is named by its content hash, then editing the node creates a new node with a new name — which is exactly what we want for storage but exactly what we must *not* conflate with the *logical* identity of a survey item. Question 4 is still question 4 after you relabel its third choice; its content hash changed but its item identity did not. Holding these two notions of identity apart (content identity for storage, logical identity for the domain) is a recurring theme and a frequent source of bugs in naïve designs.

### 2.3 The sequence-diff problem

Before tree diffing, understand sequence diffing, because it is a building block and because its mathematics recurs.

Given two sequences, the canonical formalization of "what changed" is the **shortest edit script**: the minimum number of insertions and deletions that transform one into the other. This is dual to the **longest common subsequence** (LCS): the longest sequence of elements appearing in both, in order, not necessarily contiguously. The elements not in the LCS are precisely what was inserted or deleted.

The LCS has a clean dynamic-programming solution. For sequences `A` of length `m` and `B` of length `n`, let `L(i,j)` be the LCS length of the first `i` of `A` and first `j` of `B`:

```
L(i,j) = 0                              if i = 0 or j = 0
       = L(i-1, j-1) + 1                if A[i] = B[j]
       = max(L(i-1, j), L(i, j-1))      otherwise
```

This is `O(mn)` time and space — fine for small inputs, too slow for large ones. The breakthrough is **Myers' algorithm** (Eugene Myers, *An O(ND) Difference Algorithm and Its Variations*, 1986), which finds the shortest edit script in `O(ND)` time where `D` is the size of the edit script — the length of the *difference* itself. When two versions are similar (small `D`), this is nearly linear. Myers' insight was to reframe the problem as finding the shortest path through an "edit graph" using a greedy frontier indexed by diagonals; it is the algorithm Git and most diff tools actually run.

The **Levenshtein distance** generalizes the edit script to allow substitutions as well as insertions and deletions, giving the familiar edit-distance metric. The recurrence is the same shape as LCS with a substitution term added. This matters to us mainly at the level of *word-level diffing inside a single label* — when we show a researcher that "hottest months" became "April to September," we are running a small Levenshtein-style diff on the label text to render the highlight. The interactive prototype already does a crude version of this.

The lesson sequence diffing teaches, which we carry into trees: **diff quality is correspondence quality**. The algorithm's job is to find the *best* alignment, and "best" is defined by a cost model. Different cost models produce different, equally valid diffs. There is no canonical "the diff"; there is only the diff under a chosen cost function. We will get to choose that cost function for our domain, which is an advantage text tools don't exploit.

### 2.4 The tree-diff problem

Now the real problem: our documents are trees, and we need to diff and merge trees.

The classical formalization is **tree edit distance**: the minimum-cost sequence of node insertions, deletions, and relabellings that transforms one ordered, labelled tree into another. The seminal algorithm is **Zhang–Shasha** (1989), which solves ordered tree edit distance in roughly `O(n³)` time for trees of `n` nodes (more precisely, bounded by the product of tree sizes and the squares of their depths/leaf counts). Tai (1979) gave the first algorithm at much worse complexity; later work (Demaine et al., 2007) brought the worst case down to `O(n³)` with a proof of optimality for the general class of decomposition algorithms.

Two facts about tree edit distance are decisive for our design.

**Fact one: ordered tree edit distance is polynomial; unordered is NP-complete.** If the children of a node have a meaningful order (as the items in a section do — question order matters), the problem is tractable. If the children are an unordered set (as the choices in some analyses might be treated, or tags), then computing the minimal edit distance is NP-complete (Zhang, Statman, Shasha, 1992). This tells us to *keep order meaningful wherever we can* and to treat genuinely unordered collections (like a set of choices where order is cosmetic) with set-specific logic rather than throwing them at a general tree-diff.

**Fact two: naïve tree edit distance does not detect moves.** The classical model has insert, delete, relabel — but not *move*. Moving a subtree (dragging question 5 above question 3) is modelled as deleting it in one place and inserting it in another, which both inflates the diff and destroys the correspondence we care about ("this is the same question, relocated"). Detecting moves turns the problem NP-hard in general, so practical systems use heuristics. **GumTree** (Falleri, Morandat, Blanc, Martinez, Monperrus, 2014), the most influential practical AST-differ, runs in two phases: a top-down phase that greedily matches the largest identical subtrees, then a bottom-up phase that matches "container" nodes when enough of their descendants are already matched, recovering moves and renames that the pure edit-distance model misses. Its lesson — *match first, then derive edits from the matching* — is the right architecture for our differ.

But here is the pivotal observation, and the bridge back to §2.2: **all of this heuristic matching is only necessary because the differ has to reconstruct identity that the editor threw away.** GumTree works hard to guess that "this node in tree A is the same logical node as that node in tree B" because source-code ASTs carry no stable IDs. *We do not have to throw identity away.* If every item and field in our instrument carries a stable, persistent identifier that survives edits (assigned at creation, never reused, never changed by relabelling or moving), then the matching phase is trivial — match by ID — and the diff collapses to a near-linear comparison of correspondingly-identified nodes. This is why §1's central claim holds: **stable identity converts an NP-hard-ish heuristic problem into bookkeeping.** Architectures that assign and preserve node IDs (§5, Approaches B, C, D, E) get good diffs almost for free; an architecture that versions serialized text (a degenerate version of Approach A) is condemned to GumTree-style guessing forever.

### 2.5 Three-way merge and its failure modes

Merging is reconciling two versions that diverged from a common ancestor. The dominant technique is the **three-way merge**: given the two divergent versions and their most recent common ancestor (the *merge base*, found as the lowest common ancestor in the version DAG), classify each region as changed-on-neither (keep), changed-on-one-side (take that side), or changed-on-both (conflict). The classical text implementation is `diff3`.

Three-way merge has a precise, limited correctness guarantee and a large informal failure surface. Its guarantee is roughly: if a region was changed on at most one side, the merge is unambiguous. Its failures are everything else:

**Textual false conflicts.** Two people edit adjacent lines; `diff3` flags a conflict though the changes are independent. Conversely, **textual false agreements**: two people make textually identical but semantically incompatible edits and the merger silently accepts. Both stem from the merger reasoning about lines, not meaning.

**Semantic conflicts that are textually invisible.** This is the dangerous class for us. Reviewer A deletes question 3. Reviewer B, on their branch, adds skip logic to question 7 that reads `${q3_answer} == 'yes'`. Neither edit touches the other's text; a text merge produces a clean result that is a *broken instrument* — question 7 references a question that no longer exists. No line-based system can catch this, because the conflict lives in the dependency graph, not in the text. A type-and-graph-aware merger can and must (§4.5). The existence of this class is, by itself, a strong argument against any text-based approach.

**The merge base may be wrong or absent.** Octopus histories, criss-cross merges, and cherry-picks can make the lowest common ancestor ambiguous; Git uses a recursive merge strategy that synthesizes a virtual merge base from multiple candidates. We can largely avoid this by keeping our history topology simple (the UX deliberately limits branching to "alternative drafts"), which is a case where a UX constraint buys an engineering simplification.

The takeaway: three-way merge is the right *skeleton*, but it must operate over the typed tree and the dependency graph, with a domain-specific classification of what counts as a conflict, not over text. §4.4 specifies those semantics.

### 2.6 Causality and logical time

To merge correctly — indeed, to even know whether two edits are concurrent or one happened after the other — the system needs a notion of time that survives offline work and clock skew. Wall-clock timestamps are insufficient: two enumerators' tablets disagree on the time, and "later timestamp wins" silently discards edits. The field's answer is **logical clocks**.

**Lamport timestamps** (Leslie Lamport, *Time, Clocks, and the Ordering of Events in a Distributed System*, 1978) assign each event a counter such that if event `a` causally precedes `b`, then `L(a) < L(b)`. They give a consistent total order but cannot, by themselves, distinguish "concurrent" from "ordered" — the converse does not hold.

**Vector clocks** (Fidge and Mattern, independently, 1988) fix this. Each replica keeps a vector of counters, one per replica; the partial order on vectors exactly captures causality, so two versions are *concurrent* precisely when neither vector dominates the other. This is what lets a merge engine ask the crucial question: "did these two edits know about each other, or were they made independently?" — which determines whether they merge cleanly or conflict.

Vector clocks have a scaling wart: the vector grows with the number of replicas ever seen, which is awkward when "replicas" are dozens of field tablets. **Version vectors** are the same idea applied to whole-object versions; **dotted version vectors** (Preguiça, Baquero, et al.) refine them to handle the server-mediated case efficiently, attaching a precise "dot" to each write so the system can represent concurrent values without vector bloat. **Hybrid logical clocks** (Kulkarni, Demirbas, et al., 2014) combine a logical counter with physical time so that timestamps are both causally correct *and* close to wall-clock (useful for human-readable history and for "show me everything from last Tuesday").

For us: any architecture with offline editing (the field tier) needs real causality tracking, and the choice among vector/version/dotted variants is an engineering optimization, not a fundamental one. An architecture that is always server-mediated (the design tier, online) can sometimes use a simple server-assigned sequence number and sidestep the whole apparatus — a reason the hybrid design in §5E uses different machinery in different tiers.

### 2.7 Conflict-free replicated data types (CRDTs)

CRDTs are the most important idea of the last fifteen years for our problem, so this subsection is the longest. A CRDT is a data type engineered so that concurrent replicas can be merged *automatically and deterministically* into a consistent result, with no coordination and no conflicts — by construction, not by heuristics.

**The algebra.** The foundational case is the state-based CRDT (a *convergent* replicated data type, CvRDT). Its set of possible states forms a **join-semilattice**: a partially ordered set in which any two elements have a least upper bound (a *join*, written ⊔). The merge function *is* the join. Because the join operation is, by the lattice axioms, **commutative** (`a ⊔ b = b ⊔ a`), **associative** (`(a ⊔ b) ⊔ c = a ⊔ (b ⊔ c)`), and **idempotent** (`a ⊔ a = a`), it does not matter in what order, how many times, or via what path replicas exchange and merge their states — they provably converge to the same value, namely the join of everything they have all seen. This property is **Strong Eventual Consistency** (SEC): replicas that have received the same set of updates are in the same state, with no consensus protocol required. The result is due to Shapiro, Preguiça, Baquero, and Zawirski (2011), who unified earlier scattered constructions and proved the general theorem.

There are two dual formulations. **State-based (CvRDT):** replicas ship their whole state and merge by join; robust against lost and reordered messages, but shipping whole states is expensive (deltas mitigate this). **Operation-based (CmRDT):** replicas ship operations, which must be designed to *commute* when concurrent; cheaper on the wire but requires a reliable causal-delivery channel. The two are theoretically equivalent in power. Our field tier, with its lossy connectivity, leans toward state/delta-based robustness; our design tier, with a reliable channel, can use op-based efficiency.

**The catalogue.** Simple CRDTs compose into complex documents. Counters (grow-only and positive-negative); registers (last-writer-wins, using a logical clock to pick a winner — note this *does* discard a concurrent value, a deliberate loss; or multi-value, which keeps all concurrent values for the application to resolve); grow-only sets; two-phase sets; observed-remove sets (OR-Set, which correctly handles concurrent add/remove by tagging each addition with a unique token); and maps that contain other CRDTs as values. A survey item maps neatly onto a CRDT map: the label is a register or a text CRDT, the choice list is an ordered-set CRDT, the type is a register, and so on. This compositionality is why CRDTs are attractive for structured documents.

**The hard part: sequences.** The deep difficulty is ordered sequences — the list of items in a section, or the characters in a label being co-typed. The problem is assigning stable positions to elements such that any two replicas can independently insert "between" two positions and agree on the result. Naïve integer indices fail immediately under concurrency. The literature is a genealogy of solutions:

- **WOOT** (Oster et al., 2006): the first correct character-level CRDT, using "between" relationships among unique character IDs; correct but heavy.
- **Treedoc** (Preguiça et al., 2009): positions as paths in a binary tree, so "between" is always expressible by descending one level; needs periodic rebalancing.
- **Logoot** (Weiss et al., 2009) and **LSEQ** (Nédelec et al., 2013): positions as variable-length identifiers from a dense identifier space, with allocation strategies to keep IDs from growing too fast.
- **RGA** (Roh et al., 2011): replicated growable array, using timestamped insertion points; widely implemented.
- **YATA** (Nicolaescu et al., 2016): the algorithm behind **Yjs**, optimized for the common editing patterns and for performance at scale.

**The interleaving anomaly and Fugue.** A subtle but real defect haunted many sequence CRDTs: when two replicas concurrently insert *runs* of elements at the same position (Alice types "abc," Bob types "xyz," at the same spot), some algorithms interleave them into "axbycz" — garbage no human intended. Characterizing and fixing this is recent work; **Fugue** (Weidner and Kleppmann, 2023) defines "maximal non-interleaving" and gives an algorithm achieving it. For us this is mostly relevant to real-time co-typing of a single label, a comparatively rare event, but it is a reason to adopt a mature library rather than hand-roll a sequence CRDT.

**Rich text: Peritext.** Plain sequences are not enough for formatted text; concurrently applying "bold" to a range while someone edits inside that range is its own problem. **Peritext** (Litt, Lim, Kleppmann, Henry; Ink & Switch, 2021–2022) is a CRDT for rich text that keeps formatting spans consistent under concurrency. We need only a thin slice of this (labels are lightly formatted), but it is the reference if rich labels become a requirement.

**The catch for us.** CRDTs guarantee *convergence* — all replicas reach the same state — but convergence is not the same as *correctness* or *intent preservation*. A CRDT will happily converge two replicas to a state that is a perfectly consistent, perfectly broken instrument (the deleted-question-3 / dependent-question-7 scenario of §2.5 converges fine; it is just wrong). CRDTs eliminate the *low-level* merge conflict but not the *semantic* one. And critically, **CRDTs by design avoid surfacing conflicts to the user** — their whole point is automatic resolution — whereas our review workflow sometimes *wants* a conflict surfaced for a human to adjudicate (the four-card resolution UX). So CRDTs are an excellent substrate for *real-time co-editing within a single draft*, where automatic convergence is exactly right, and a poor fit for *cross-draft review*, where deliberate human adjudication is the point. This split is the seed of the hybrid recommendation.

### 2.8 Operational Transformation (OT)

OT is the older rival to CRDTs and the technology that originally powered Google Docs. The idea: replicas exchange *operations* (insert at position 5, delete at position 9), and when an operation arrives that was generated against a different prior state, the receiver *transforms* it against the operations it has since applied, adjusting indices so the intent is preserved. "Insert at 5" becomes "insert at 7" if two characters were inserted before position 5 in the meantime.

The correctness conditions are precise and famously hard to satisfy. **TP1** (transformation property 1) requires that transforming operation `a` against `b` and then applying, versus transforming `b` against `a` and then applying, yield the same state — necessary for two replicas exchanging one op each. **TP2** requires consistency when transforming an operation against a sequence of others regardless of the order in which the sequence was assembled — necessary for three or more concurrent sites. Many published OT algorithms were later found to violate TP2 in corner cases; getting it provably right is genuinely difficult, which is much of why the field drifted toward CRDTs.

The pragmatic exception is the **Jupiter** model (Nichols et al., 1995), which sidesteps TP2 by routing all operations through a central server that imposes a total order — each client only ever transforms against a linear server history, so TP2 never arises. This is exactly Google Docs' architecture: a central authority makes a hard problem easy. For our *design tier*, which is naturally server-mediated, a Jupiter-style OT is a legitimate and proven option. For the *offline field tier*, where there is no live central server during editing, OT's central-server crutch is unavailable and CRDTs are the better fit. Another input to the hybrid.

### 2.9 Patch theory

The most mathematically ambitious lineage treats *patches* — not states — as the primary objects, and asks what algebra they obey. This is the world of **Darcs** and its successor **Pijul**.

The motivating idea: a patch is a function transforming one state into another, so patches should *compose* like functions, and the interesting question is when two patches **commute** — when patch `p` then patch `q` produces the same result as `q` then `p`. If two patches commute, they are independent and can be reordered freely; merging is then "reorder the patches into a common sequence." Conflicts are precisely the patches that *don't* commute. Darcs formalized a "theory of patches" around commutation but suffered from exponential-time merges in pathological cases (the notorious "merge hell").

**Pijul** rebuilds this on firmer ground. Its theory (drawing on work by Mimram and Di Giusto, *A Categorical Theory of Patches*, 2013) models states and patches in the language of category theory: states are objects, patches are morphisms, and a merge is a **pushout** — the universal way to combine two divergent morphisms from a common source. When the naïve pushout doesn't exist (a genuine conflict), the construction is completed in a larger category that represents the conflict explicitly as a first-class state rather than as textual markers. The practical payoff is that, unlike Git, Pijul makes patches that commute *actually* commute (so cherry-picking and reordering are clean and associative), and conflicts are well-defined objects with a sound algebra rather than ad-hoc `<<<<<<<` text.

For us, patch theory is more *influence* than *implementation candidate*. Its insistence that **a conflict is a first-class state, not an error**, directly informs our four-card resolution UX — the conflict is something the system represents and carries until a human resolves it, which is precisely the Pijul stance. And its commutation lens is the right way to *think about* when two researcher edits are independent. But a full categorical patch engine is a large research-grade investment (§8.4), and the same conflict-as-state benefit is obtainable more cheaply by combining a CRDT substrate with an explicit semantic-conflict layer (§5E). We borrow the philosophy, not the machinery.

---

# PART II — PRIOR ART AND INFLUENCES

## 3. A survey of systems we are standing on

The point of this survey is not completeness but transfer: for each system, what is the idea, and does it carry into a survey-instrument IDE or not?

### 3.1 The Git family: Git, Mercurial, Fossil

**Git** contributes the Merkle DAG, content-addressed storage, and the branch/merge/three-way mental model. We adopt the storage idea (§2.2) almost wholesale and the DAG-of-versions model for the review layer. We reject its text-line atom, its exposed plumbing, and its vocabulary. Git's most relevant *negative* lesson is its learning curve: a tool whose data model is beautiful but whose interface leaks that model onto every user is a tool researchers will not adopt. The UX theory document's vocabulary-translation discipline is a direct response.

**Mercurial** is architecturally close to Git but historically friendlier, with a more uniform command surface and first-class extensibility. Its relevant lesson is that the *same* underlying DAG model can be presented far more humanely; the data model does not dictate the interface. Its *revlog* storage (interleaved snapshots and deltas with a guaranteed reconstruction bound) is a clean reference if we build custom delta storage.

**Fossil** (from the SQLite author) is instructive because it bundles version control with issue tracking, a wiki, and a forum in a single self-contained, self-hostable binary backed by a SQLite database. For our self-hosted institutional tier, Fossil's philosophy — *one auditable file, no server dependencies, batteries included* — is a strong model for the deployment story (§7, §8). It also demonstrates that storing a version DAG inside a relational database is entirely viable, which de-risks Approach A and B's storage layer.

### 3.2 The patch-theory family: Darcs, Pijul

Covered theoretically in §2.9. As prior art, they matter because they are existence proofs that a sound, commutation-based alternative to Git's model can be built and shipped, and because Pijul's conflicts-as-first-class-objects is the cleanest articulation of the stance our review UX takes. Darcs is also a cautionary tale about performance: an elegant theory with exponential worst-case merges taught the field that *algorithmic complexity of merge is a product requirement, not an academic footnote*.

### 3.3 Version control for data: Dolt

**Dolt** is "Git for data" — a SQL database with branch, diff, and merge at the level of *table rows and schemas* rather than text lines. This is the closest prior art to our problem, because a survey instrument is semi-tabular (the XLSForm survey and choices sheets are literally tables) and Dolt has already solved "diff and merge structured tabular data with a Git-like workflow."

Its key engineering idea is the **prolly tree** (probabilistic B-tree, a content-addressed search tree / Merkle search tree). A prolly tree is a B-tree whose node boundaries are determined by content-defined chunking, so that two structurally similar datasets share most of their internal nodes by hash. This gives simultaneously: efficient diff (compare trees, skip shared subtrees by hash), structural sharing (storage proportional to change size), *and* fast indexed lookup (it is still a search tree). For a system that wants both Git-like history *and* spreadsheet-like random access to a large instrument or a bank of reusable items, prolly trees are the most directly applicable storage structure in the literature. If we build Approach A or B at scale, this is the storage design to study first.

### 3.4 Immutable fact logs: Datomic

**Datomic** inverts the usual model: the database is an append-only log of immutable **datoms**, each an `(entity, attribute, value, transaction, added?)` tuple. Nothing is ever updated in place; a "change" is a new datom asserting a new value (and optionally retracting the old). The current state is a *view* derived by reducing the log; any past state is recoverable by reducing the log up to a point in time. History is not a feature bolted on — it is the substrate.

This is profoundly aligned with two of our requirements. The append-only log is *inherently* a tamper-evident audit trail (§1.2 compliance), and "what did the instrument look like when respondent #4271 answered it?" becomes a first-class query ("as-of" a transaction), not an archaeology project. Datomic is the strongest influence on Approach B. Its cost — the log grows without bound and must be indexed and occasionally compacted — is the economic theme of §8.1.

### 3.5 Real-time collaborative editors: Google Docs, Figma, Notion, Linear

**Google Docs** is the proof that Jupiter-style server-mediated OT (§2.8) scales to mass adoption. Its relevant lesson beyond the algorithm is the **suggesting mode** interaction model, which the UX theory document already adopts as Tier 1: edits as reviewable proposals attributed to an author, accepted or rejected individually. We are, in effect, rebuilding Google Docs' suggesting mode over a typed tree instead of a text rope.

**Figma** is the most instructive, because it solved real-time multiplayer over a *structured document* (a scene graph of design objects), not text — exactly our shape of problem. Figma uses a custom CRDT-flavoured system with a central server assigning authority, and — crucially for us — it later shipped **branching and merging** for design files, presenting it to non-engineer designers without Git vocabulary. Figma is the closest existing answer to "what does Git-like branching look like for a structured document aimed at non-programmers," and its branching UI is worth direct study before we design ours.

**Notion** models every document as a tree of typed "blocks," each with a stable ID — precisely the stable-identity discipline §2.4 argues for. It demonstrates that a block tree with per-block identity is a viable, popular substrate for structured documents, and that users readily accept "everything is a block" when the blocks render as ordinary prose by default (again echoing the UX theory).

**Linear** popularized the **sync engine** architecture: a local in-memory store on the client, a compact operation log synced to the server, optimistic local application, and background reconciliation. This is the modern pattern for "feels instant, works offline-ish, syncs reliably," and its shape informs our client architecture (§7.2) regardless of which merge core we choose.

### 3.6 The CRDT toolkits and the local-first movement

**Yjs** (YATA, §2.7) and **Automerge** (a JSON-CRDT) are the two production-grade CRDT libraries. Yjs is performance-optimized and battle-tested in many editors; Automerge offers a richer document model (arbitrary JSON with nested maps, lists, text, and full change history) and a notable **columnar binary encoding** (Kleppmann, 2020) that compresses operation logs dramatically by storing many operations' fields in column-major order with run-length and delta encoding — directly relevant to our bandwidth economics (§8.3). **Collabs** and **ElectricSQL** (CRDT-backed sync over Postgres) and **Replicache** (a sync-engine SDK) round out the toolkit landscape.

The intellectual umbrella is **local-first software** (Kleppmann, Wiggins, van Hardenberg, McGranaghan; Ink & Switch, 2019), whose seven ideals — fast, multi-device, offline-capable, collaborative, long-lived, private, and user-controlled — read almost as a requirements list for a field-research tool used in low-connectivity settings by institutions that care about data sovereignty. The local-first thesis is the philosophical backbone of our field tier and self-hosted tier, and CRDTs are its enabling technology. The honest counterpoint, which §5 takes seriously, is that local-first's automatic convergence is sometimes *not* what a review workflow wants.

### 3.7 Domain prior art: REDCap, Qualtrics, SurveyCTO, ODK/Kobo

The most important survey of all, because it defines the whitespace. Across the incumbent survey platforms, version control is conspicuously weak.

**REDCap** versions a project's data dictionary largely through a draft-and-commit cycle: you enter "draft mode," make changes, and commit them, sometimes with administrator approval for projects already collecting data. There is change logging and a field-level audit trail, but there is no real branching, no diff between arbitrary versions presented structurally, and no collaborative suggesting — and, as the UX theory document noted, researchers resort to encoding structure by hand in field-label HTML for want of better primitives. **Qualtrics** offers revision history and the ability to restore prior versions of a survey, but again no branch/merge and limited structural diff. **SurveyCTO** and the broader **ODK/Kobo** ecosystem version the *form* as a published artifact with a version string; the authoring-time history typically lives in whatever the author used to edit the XLSForm (often a spreadsheet with no versioning at all, or a manually-versioned file).

The pattern is clear: incumbents treat the instrument as a document to be drafted and published, with history as a safety net, not as a collaborative artifact with a first-class branching, reviewing, and merging workflow. *No incumbent offers structural diff, semantic merge, item-level suggesting, or AI-reviewer integration over a typed instrument model.* That gap — already identified commercially in the feasibility assessment — is, from the versioning angle, the specific technical territory this protocol stakes out.

### 3.8 What transfers — a synthesis

| Source | Idea we take | Idea we leave |
|---|---|---|
| Git | Merkle DAG storage, version-DAG model, three-way skeleton | text-line atom, plumbing, vocabulary |
| Mercurial / Fossil | humane presentation; DAG-in-a-database; self-contained deployment | — |
| Dolt | prolly trees; structured-data diff/merge | SQL-first interface |
| Datomic | append-only immutable fact log; as-of queries | full Datalog engine |
| Google Docs | suggesting mode; Jupiter OT for the online tier | text-only model |
| Figma | structured-document multiplayer; branching for non-engineers | proprietary internals |
| Notion | per-block stable identity; blocks-as-prose | — |
| Linear | sync-engine client architecture | — |
| Yjs / Automerge | production CRDT substrate; columnar encoding | hand-rolling our own |
| Pijul | conflict-as-first-class-state; commutation lens | full categorical engine |
| Local-first | offline-first, sovereignty, longevity ideals | dogmatic auto-merge everywhere |

---

# PART III — THE DESIGN

## 4. The semantic model for survey instruments

Before choosing an architecture we must pin down *what is being versioned*. Every architecture in §5 operates over this model; they differ only in the machinery of history, sync, and merge.

### 4.1 The canonical object model

The instrument is a typed tree with a dependency graph overlaid. Recapitulating and formalizing the model the prototype already uses:

- An **Instrument** has metadata (title, languages, settings) and an ordered list of **Sections**.
- A **Section** has a title, optional **Module** binding, and an ordered list of **Items** (and may nest sub-groups).
- An **Item** has: a stable **id**; a **type** (`integer`, `select_one`, `select_multiple`, `text`, `geopoint`, …); a **variable name** (the analysis-facing identifier, e.g. `hours_outdoors`); a **label** as a language→string map; an optional ordered **choice list**; optional **relevance** (skip-logic) and **constraint** expressions; and provenance metadata.
- A **Choice** has a stable **key**, a language→label map, and a position.
- A **Module** is a binding to a registered validated instrument (PHQ-9, GAD-7, …) with a lock flag and a citation.

Two cross-cutting structures sit on top of this tree. The **dependency graph** has an edge from item X to item Y whenever X's relevance or constraint expression references Y's variable name. The **module membership** relation marks subtrees as belonging to a locked validated instrument. Both are derived from the tree but must be consulted by the merge engine, because they are where semantic conflicts live (§4.5).

### 4.2 Stable identity — the keystone decision

§2.4 argued that stable identity is what makes diffing and merging tractable. Here is the discipline, stated as rules:

1. **Every Item, Choice, Section, and Module binding receives a globally unique, persistent id at creation** (a UUID or a collision-resistant random string). The id is assigned once and never changes.
2. **The id is independent of all content.** Relabelling an item, changing its type, moving it to another section, translating its label — none of these change its id. The id answers "is this the same question?", which is a different question from "does this question have the same content?"
3. **The id is distinct from the variable name.** The variable name (`hours_outdoors`) is *content* — analysis-facing, human-chosen, and changeable (with care, because the dependency graph references it). The id is *identity* — internal, opaque, immutable. Conflating the two is a classic and costly error: it makes renaming a variable look like deleting one question and creating another, destroying history and breaking diffs.
4. **Ids are never reused.** A deleted item's id is retired forever, so that a later "undelete" or a merge that resurrects it refers to the same logical thing.

With these rules, diffing two versions is: match nodes by id (trivial), then for each matched pair compare content fields, and classify ids present on one side only as inserts/deletes, and ids whose parent or position changed as moves. The NP-hard heuristic matching of §2.4 evaporates. This is the highest-leverage paragraph in the document: **the identity rules are more important than the choice of architecture.**

### 4.3 Granularity: what is the unit of change, history, and review?

Different operations want different granularity, and the protocol should support a *hierarchy* of grains rather than a single one.

- **Field grain** (a single label, a single expression, one choice's text): the unit of *real-time co-editing* and of the finest diffs. Two people editing different fields of the same item never conflict.
- **Item grain**: the unit of *review and suggestion* (the UX theory's Tier 1 — you accept or reject a change to question 4 as a unit) and of *semantic conflict resolution* (the four-card UX picks among whole-item versions).
- **Section / module grain**: the unit of *restructuring* (reordering, moving groups) and of *locking* (a module is locked as a unit).
- **Instrument grain**: the unit of *branching* (an "alternative draft" is a whole-instrument variant) and of *publication* (a released version that respondents see).

A well-designed protocol lets history and attribution be queried at any grain ("show me everything that ever happened to question 4" vs "show me everything in this section last week" vs "diff these two whole drafts"). The data model supports this naturally if every change record carries the id of the node it touched and the ids of that node's ancestors.

### 4.4 Typed merge semantics

The merge engine dispatches on node type. Each type has its own rule for combining two concurrent changes that share an ancestor. This is the type-aware analogue of three-way merge (§2.5).

**Scalar fields (type, variable name, required flag):** last-writer-wins by logical clock if we want automatic resolution, or *surface as conflict* if both sides changed it differently and we want human adjudication. The choice is policy, set per field: variable-name collisions should *conflict* (they have analysis consequences and dependency-graph implications), whereas the `required` flag can safely last-writer-win.

**Label maps (language→string):** merge per language independently. English changed on side A, Bangla changed on side B → clean merge, both taken. The same language changed on both sides → either a field-grain text merge (if the changes are to different parts of the string, mergeable like a tiny CRDT/diff3) or an item-grain conflict surfaced to the four-card UX.

**Choice lists:** the interesting case. Treat the choice list as an *ordered observed-remove set* keyed by stable choice id. Concurrent additions both survive (set union by id). A concurrent add on one side and reorder on the other merge cleanly (position is a separate, mergeable attribute). An add-on-one-side / remove-the-same-choice-on-the-other is resolved by OR-set semantics (the remove only removes the adds it observed, so a concurrent add survives — usually the safe choice for a survey, since silently dropping an answer option is dangerous). Relabelling a choice on both sides → field-grain conflict on that choice's label, not on the whole list.

**Expression fields (relevance, constraint):** parse to an expression tree and, ideally, merge structurally; at minimum, treat any concurrent change to an expression as an item-grain conflict, because expressions are where silent breakage hides. Never text-merge an expression — a half-merged boolean condition is worse than a conflict.

**Modules:** locked modules do not merge internally at all. If either side edited inside a locked module, that is not a merge — it is a *module detachment* event that must be surfaced ("Side A modified the PHQ-9; this breaks score comparability — keep the standard module, or accept the modified non-standard version?"). This is a domain-specific conflict class with no analogue in general VCS.

**Tree structure (insertions, deletions, moves):** match by id (§4.2). Insert on one side → take it. Delete on one side, no other change → take the delete *unless* the dependency graph forbids it (§4.5). Move (reparent or reposition) on one side → apply it; concurrent moves of the same node to different places → conflict.

### 4.5 Referential integrity as a constraint system

This is the class of conflict that text systems cannot see and that justifies the entire type-aware approach. The dependency graph (§4.1) must be checked at merge time and, ideally, continuously during editing.

The canonical hazard, restated: side A deletes item Y; side B adds or keeps a relevance expression on item X that references Y's variable name. A naïve merge produces a clean tree with a dangling reference — a broken instrument. The engine must detect that the merged state contains an edge `X → Y` with `Y` absent and **surface this as a semantic conflict**, offering the resolutions the domain allows: restore Y, remove the dependency on X, or redirect the reference. The same logic guards variable renames (rename Y's variable on one side, add a dependency on the old name on the other) and reorderings that move a referenced item *after* its referrer (a forward reference some engines forbid).

The clean way to implement this is to treat the merged candidate state as a *proposal* that must pass a **validation pass** — a set of invariants (no dangling references, no duplicate variable names, no forward references if disallowed, no broken modules, all required translations present) — before it can be committed. A proposal that fails validation does not silently commit; it produces a typed list of violations, each mapped to the items involved, which the review UX renders as resolvable conflicts. This validation pass is the same machinery whether the underlying engine is snapshot-based, log-based, or CRDT-based, which is why it can be specified once here and reused across all of §5's architectures.

### 4.6 Edit, append, review — three distinct operations

The request named three operations; the model distinguishes them precisely.

**Editing** mutates an existing node (relabel, retype, change an expression). Under stable identity, an edit is a new value for a field of a node whose id persists. In review mode, an edit becomes a *proposed* new value awaiting accept/reject.

**Appending** adds a new node (a new item, a new choice, a new section, attaching a module). Appends are the *least* conflict-prone operation — two people appending different items rarely interfere — and most architectures handle them almost for free, provided the new node gets a fresh unique id (never a reused one) and a well-defined position (the sequence-CRDT or fractional-index machinery of §2.7 gives concurrent appends a deterministic order).

**Reviewing** is the workflow of proposing, inspecting, and accepting/rejecting changes (edits or appends), possibly across a divergent draft, with attribution and an audit trail. Review is not a storage operation; it is a *protocol over* the storage operations — a state machine on change-sets (proposed → under-review → accepted/rejected/superseded) with comments and approvals attached. Critically, the protocol treats a human's proposal and the AI agent's proposal identically (§1.3): both are change-sets entering the same state machine with an author field that happens to be a person or an agent.

## 5. Five candidate architectures, end to end

Each architecture is specified along the same axes so they can be compared: **storage** (how state and history are kept), **edit/append**, **branch (alternative draft)**, **merge**, **conflict**, **review & attribution**, **offline/sync**, and a verdict against the §1 requirements. All five assume the §4 semantic model, especially stable identity and the validation pass.

### 5.1 Approach A — Snapshot + structural three-way merge ("Git-lite, server-authoritative")

**Storage.** A content-addressed object store of immutable instrument states, exactly Git's Merkle DAG (§2.2) but over the typed tree: each Item, Section, and Instrument serializes to an object named by its content hash; unchanged subtrees are shared by hash. A version is a commit object pointing at a root tree and at parent commit(s), carrying author, timestamp, and message. Branches ("alternative drafts") are named pointers to commits. Optionally back this with prolly trees (Dolt-style, §3.3) for indexed access at scale.

**Edit / append.** The client mutates an in-memory tree and, on save, writes a new commit. Because nodes carry stable ids (§4.2), the diff against the parent is computed by id-matching, not heuristics.

**Branch.** Create a named pointer at the current commit. The alternative draft evolves as its own commit chain. Cheap — a branch is one pointer.

**Merge.** Classic three-way (§2.5) but type-aware (§4.4): find the merge base (lowest common ancestor commit), compute structural diffs base→A and base→B by id, combine per the typed rules, run the §4.5 validation pass, and produce either a clean merge commit or a list of conflicts.

**Conflict.** Surfaced at item grain and fed to the four-card UX. Because merges are explicit and infrequent (only when integrating an alternative draft), conflict handling is a deliberate, reviewable moment — which suits the workflow.

**Review & attribution.** A proposal is a branch (or a lightweight change-set) reviewed before merging — a pull-request-shaped flow, deliberately not called that. Suggesting mode (Tier 1) can be modelled as a change-set of per-item proposed edits against the main commit. Attribution is the commit author; tamper-evidence is the Merkle hashing.

**Offline / sync.** Works offline at the granularity of "edit locally, commit, sync commits later." Two offline editors of the *same* branch produce divergent commit chains that must be *merged* on reconnect — which can surface conflicts for edits that real-time systems would have resolved silently. Acceptable for the design tier (where co-editing is occasional and review-mediated); coarse for heavy simultaneous field editing.

**Verdict.** Simplest to reason about, strongest audit story, reuses the most proven ideas, and maps cleanly onto the review UX. Its weakness is real-time co-editing: it has none natively, falling back to merge-on-save. Excellent backbone for the *review and branching* layer; insufficient by itself for *live multi-cursor editing*.

### 5.2 Approach B — Operation log / event sourcing ("Datomic-style append-only")

**Storage.** The source of truth is an append-only, immutable log of typed **operations**: `SetLabel(itemId, lang, text)`, `AddChoice(itemId, choiceId, …)`, `InsertItem(sectionId, itemId, position)`, `SetRelevance(itemId, expr)`, `DeleteItem(itemId)`, `AttachModule(...)`, and so on. Each operation carries an id, author, logical timestamp (§2.6), and causal dependencies. Current state is a left-fold (reduce) over the log; any historical state is the fold up to a point (Datomic's "as-of", §3.4). Snapshots are a cache/optimization (periodic materialized states so you needn't replay from genesis).

**Edit / append.** Every change *is* an appended operation. Appending is literally the native verb of this architecture, which makes the "append" requirement first-class. Editing and appending differ only in operation type.

**Branch.** A branch is an alternate head over the same shared log prefix — a named marker plus the set of operations unique to that branch. Divergence is just two sets of operations sharing a common ancestor in the causal graph.

**Merge.** Reconcile two operation sets sharing a base: take the union of operations, order them by causal/logical clock, fold them, and run the §4.5 validation pass. Where two concurrent operations target the same field with incompatible intent, either apply a type-specific resolution (§4.4) or emit a conflict. Because operations are typed and fine-grained, merges are often cleaner than snapshot diffs (you are merging *intentions*, e.g. "A relabelled, B added a choice," which obviously compose) — this is the event-sourcing dividend.

**Conflict.** Detected when concurrent operations on the same field carry incompatible intent; surfaced at item grain. The log makes "what exactly did each side do" perfectly legible, which produces unusually clear conflict descriptions for the four-card UX.

**Review & attribution.** Superb. Attribution is intrinsic — every operation names its author and time, so blame, activity feeds, and audit trails are queries over the log, not bolted-on metadata. The agent's suggestions are just operations with an agent author, entering the same review state machine. Tamper-evidence comes from hash-chaining the log (each entry includes the hash of the prior, a lightweight blockchain-of-edits).

**Offline / sync.** Strong. Offline clients accumulate operations and ship them on reconnect; operation logs are naturally mergeable given causal metadata; the wire cost is just the new operations, which is bandwidth-frugal (§8.3) and amenable to columnar compression (§3.6). This is close to Linear's sync engine (§3.5).

**Verdict.** The best *audit and provenance* story (the compliance requirement is satisfied by construction), the most natural "append," excellent offline/bandwidth behaviour, and very clear conflicts. Its costs are the unbounded log (needs compaction/GC, §8.1) and the engineering discipline of designing a clean, stable operation vocabulary that you must support forever (operations are a public schema once written to history). Does not, by itself, give real-time multi-cursor convergence — but composes beautifully with a CRDT layer that does (see E).

### 5.3 Approach C — CRDT-native ("local-first, real-time")

**Storage.** The instrument *is* a composed CRDT (§2.7): a CRDT map per item (label as text-CRDT or register, choices as an ordered-set CRDT, expressions as registers), items within a section as a sequence CRDT, all backed by a mature library (Yjs or Automerge). History is the CRDT's internal change log (Automerge retains full history and supports time-travel; Yjs keeps a compact update log).

**Edit / append.** Local mutations apply instantly and optimistically; the library emits updates that propagate to peers/server. Concurrent edits *converge automatically* by the SEC guarantee — no merge step, no conflict, for the low-level data.

**Branch.** Two ways. Either fork the CRDT document (clone it; edit independently; later "merge" by feeding one's updates into the other — automatic convergence does the rest) or model an alternative draft as a separate CRDT document with a recorded common ancestor. Automerge's full-history model makes the former clean; the latter is closer to the review UX.

**Merge.** For real-time co-editing within a draft: *there is no merge step* — convergence is automatic and that is the whole point. For integrating an alternative draft back into main: feed updates across; the CRDT converges; then run the §4.5 validation pass to catch the semantic breakages convergence cannot see (§2.7's catch). The validation pass is doing the *semantic* work; the CRDT is doing the *structural* work.

**Conflict.** Here is the tension. CRDTs are *designed to not have conflicts* — they auto-resolve. That is exactly right for live co-typing and exactly wrong for review, where we sometimes *want* to stop and ask a human. So in CRDT-native designs we must *manufacture* conflict surfacing on top: detect, via the validation pass and via multi-value registers (which retain concurrent values rather than auto-picking), the cases we want adjudicated, and route them to the four-card UX. The CRDT won't hand us conflicts; we synthesize the ones the domain cares about.

**Review & attribution.** The weakest axis natively. CRDTs optimize for convergence, not for "these twelve changes are a reviewable proposal by Aluk awaiting your accept/reject." Attribution is per-operation in principle but libraries vary in how cleanly they expose it, and the *suggesting* model (a change held in proposed state, not yet applied) cuts against the CRDT grain of applying-immediately. Achievable, but you are building the review layer largely yourself, against the substrate's current.

**Offline / sync.** Best in class. This is precisely what CRDTs and local-first were built for: edit offline indefinitely, sync opportunistically, converge guaranteed. The field tier's dream.

**Verdict.** Unbeatable for real-time co-editing and offline field use; weakest for the deliberate, attributable, hold-for-approval review workflow that is the *other* half of our product. Choosing C alone would optimize the co-editing experience at the expense of the review experience. Its natural role is the *editing substrate within a draft*, paired with something else for cross-draft review.

### 5.4 Approach D — Patch-theory / commutation ("Pijul-style")

**Storage.** Patches as first-class objects (§2.9), each with explicit dependencies on the patches it requires, stored in a structure that records which patches commute. State is the result of applying a set of patches; history is the partial order of patches.

**Edit / append.** Each change is a patch with declared dependencies (an edit to item X's label depends on the patch that created X). Append is a dependency-free patch; edits depend on prior patches to the same node.

**Branch / merge.** Branches are different sets of applied patches. Merging is taking the union of patch sets and applying them in any order consistent with their dependencies — sound *because* commuting patches genuinely commute (the categorical guarantee), so cherry-picking a single fix from one draft into another is clean and associative in a way Git's cherry-pick is not. Conflicts are the patches that do not commute, represented as first-class conflict states (Pijul's pushout-completion, §2.9) rather than text markers.

**Conflict.** The most theoretically satisfying treatment: a conflict is an explicit object the system carries until resolved, which is *exactly* the stance our four-card UX takes. Philosophically, D and our UX are the same idea.

**Review & attribution.** Patches are naturally authored and reviewable; cherry-picking a reviewer's single suggestion is a first-class, clean operation. Good fit in principle.

**Offline / sync.** Patches carry their dependencies, so they sync and apply out of order gracefully — comparable to the operation log of B.

**Verdict.** The most elegant theory and the best conceptual match to "conflict as a state to resolve." But it is the highest-risk to *build*: a sound patch-commutation engine is research-grade work (Darcs' performance history is the warning, §3.2), the talent to build and maintain it is scarce and expensive (§8.4), and the same conflict-as-state benefit is reachable more cheaply by B-or-C plus the explicit conflict layer. We adopt D's *philosophy* (conflicts are states; think in terms of commutation) and decline D's *implementation*. Revisit only if a future requirement demands clean, associative cherry-picking across many divergent drafts as a core workflow.

### 5.5 Approach E — Hybrid layered ("the recommended synthesis")

No single mechanism is best on all axes: C wins real-time and offline, B wins audit and append and bandwidth, A wins simplicity and review-clarity, D wins conceptual cleanliness. The product has two genuinely different collaboration modes — *real-time co-editing within a draft* and *deliberate review across drafts* — and the §2.7 analysis shows these *want different machinery*. So the right architecture is layered, assigning each layer the mechanism that fits.

**Layer 1 — Real-time co-editing substrate (CRDT, from C).** Within a single draft, the live document is a composed CRDT (Yjs or Automerge). Multiple cursors, instant local edits, automatic convergence, full offline capability. This layer owns the "two people (or a person and an agent typing a suggestion) editing the same draft right now" experience and the entire field-tablet offline story. It does *not* deal with review or cross-draft merge.

**Layer 2 — Durable operation/commit history (from B and A).** Edits are also recorded as a hash-chained, append-only log of typed semantic operations (B), periodically materialized into content-addressed snapshot commits (A). This layer owns history, attribution, audit/provenance (satisfying the compliance requirement by construction), as-of queries ("what did respondent #4271 see"), and branching (alternative drafts as named commit lineages). The CRDT's low-level updates are *distilled* into semantic operations for this log — "label of item X changed to …" rather than a thousand keystroke deltas — so the history is legible and the log stays compact.

**Layer 3 — Semantic review & conflict protocol (the §4.5 validation pass + the change-set state machine).** Independent of how bytes are stored, this layer implements: the suggesting workflow (Tier 1 — per-item proposed change-sets awaiting accept/reject, human or agent authored, identically), alternative-draft integration (Tier 2 — merge a draft's operations into main, then validate), and the four-card conflict resolution fed by the validation pass's typed violation list and by multi-value retention where we deliberately keep concurrent options for a human. This layer is where "conflict as a first-class state" (D's philosophy) lives, implemented cheaply over Layers 1–2 rather than via a categorical engine.

**How an action flows, end to end.** A researcher edits question 4's third choice in a shared draft: Layer 1 applies it instantly and converges it to a co-editor's screen; the change is distilled into a `SetChoiceLabel` operation appended to Layer 2's log with author and clock; nothing is surfaced for review because they are co-editing the same draft. Now the agent, asked to translate the item, produces a proposed `SetLabel(bn, …)` *change-set* in Layer 3's "proposed" state — applied to no one's screen yet; the PI sees a suggestion card and accepts, at which point Layer 3 commits it through Layers 2 and 1. Meanwhile, an "alternative draft" exploring a shorter pilot diverges as its own commit lineage (Layer 2); when its author asks to bring it into main, Layer 3 merges the operation sets, runs the validation pass, finds that the short draft deleted question 3 while main added a dependency on it (§4.5), and surfaces a single four-card conflict; the PI resolves it; a merge commit lands with full attribution.

**Verdict.** E is more moving parts than any single approach, and that is its only real cost. In exchange it is the *only* design that is simultaneously excellent at real-time co-editing, excellent offline, audit-sound, append-native, review-clear, and bandwidth-frugal — because it stops trying to make one mechanism do all jobs and instead routes each job to the mechanism §2 proved is right for it. The layering is not gratuitous; it falls directly out of the fact that "co-editing" and "review" have mathematically different requirements (automatic convergence vs deliberate adjudication).

### 5.6 Side-by-side

| Axis | A Snapshot | B Op-log | C CRDT | D Patch-theory | E Hybrid |
|---|---|---|---|---|---|
| Real-time co-edit | ✗ merge-on-save | △ with effort | ✓✓ native | △ | ✓✓ (L1) |
| Offline / field | △ coarse | ✓ good | ✓✓ best | ✓ good | ✓✓ (L1) |
| Audit / provenance | ✓ Merkle | ✓✓ log | △ varies | ✓ patches | ✓✓ (L2) |
| "Append" first-class | △ | ✓✓ | ✓ | ✓ | ✓✓ (L2) |
| Review / suggesting | ✓ PR-shaped | ✓ | ✗ against grain | ✓ | ✓✓ (L3) |
| Conflict-as-state | △ | ✓ | ✗ synthesize | ✓✓ native | ✓ (L3) |
| Bandwidth (LMIC) | △ snapshots | ✓✓ ops | ✓ deltas | ✓ patches | ✓✓ |
| Semantic-conflict catch | needs val. pass | needs val. pass | needs val. pass | partial | ✓✓ val. pass |
| Build risk / cost | low | medium | medium (libs help) | very high | medium-high |
| Conceptual elegance | medium | high | high | very high | medium |

## 6. Mapping the engine to the two-tier collaboration UX

The UX theory document committed to a two-tier collaboration model with deliberate vocabulary translation. The hybrid engine (§5E) must serve it without leaking. This section is the contract between the engine and the interface.

### 6.1 Tier 1 (suggesting) → Layer 3 change-sets over Layer 1/2

Tier 1 — item-level suggesting, the 80–90% case — is a Layer 3 *change-set* in the "proposed" state: one or more proposed field edits attached to specific item ids, authored, not yet applied to the canonical draft. Accepting commits the change-set through Layers 2 and 1; rejecting discards it; replying threads a comment. The agent's suggestions are the same object with an agent author. No branching, no merge, no conflict machinery is involved in the common case — which is why the common case feels light. This maps to exactly what the interactive prototype already demonstrates.

### 6.2 Tier 2 (alternative drafts) → Layer 2 branches + Layer 3 integration

Tier 2 — alternative drafts for parallel exploration — is a Layer 2 named commit lineage. "Apply to main" is a Layer 3 integration: merge the draft's operation set into main (§5.2 merge), run the validation pass (§4.5), surface any conflicts. The user never hears "branch," "merge," or "rebase"; they hear "alternative version," "bring into the main version," and "compare."

### 6.3 Four-card conflict resolution → the validation pass output

When integration surfaces a conflict, Layer 3 hands the four-card UX exactly what it needs: the common-ancestor value (card 1), each side's value (cards 2–3), and an empty compose slot (card 4), with the optional AI synthesis as the fifth card produced on demand by the agent reading all three prior versions. The engine's job is to detect the conflict at item grain and supply the three values plus the typed reason (label clash, expression clash, delete-vs-depend, module breakage); the UX's job is to present them. Because the validation pass classifies *why* each conflict exists, the cards can be annotated with the reason, not just the values.

### 6.4 The vocabulary contract

| Engine term (internal) | User-facing term (never the left column) |
|---|---|
| commit | version / saved point |
| branch | alternative draft / alternative version |
| merge | bring into the main version |
| merge base / LCA | (never shown) common starting point |
| conflict | needs your decision |
| operation / patch | change |
| change-set proposal | suggestion |
| validation violation | "this would break: …" |
| HEAD / ref | current version |
| diff | what changed |

The engineering significance of this table is that the internal vocabulary can be as precise and Git-like as the implementers want, *provided* the boundary at Layer 3's API translates every term. Keeping the precise vocabulary internally is actually beneficial — it lets engineers reason rigorously — so long as it never escapes to the surface.

---

# PART IV — ENGINEERING, ECONOMICS, AND DECISION

## 7. Engineering considerations

### 7.1 Storage schema and the object store

The hybrid needs three persisted structures. The **operation log** is an append-only table, each row `(op_id, parent_hash, author, hlc_timestamp, op_type, target_ids, payload, entry_hash)`, where `entry_hash = H(parent_hash ‖ canonical(payload) ‖ author ‖ timestamp)` hash-chains the log for tamper-evidence. The **snapshot/commit store** is a content-addressed key-value store mapping content hashes to serialized trees and items, with commit objects pointing at root trees and parents — Git's model, storable in the same relational database (Fossil demonstrates this is fine, §3.1) or an object store. The **CRDT documents** are the live drafts, persisted in their library's native binary format (Yjs update blobs or Automerge documents), treated as a fast cache whose durable truth is the distilled operation log.

At scale — large item banks, instruments reused across studies, thousands of versions — the snapshot store benefits from **prolly trees** (Dolt, §3.3) for structural sharing plus indexed access. This is an optimization to defer until data volume justifies it; a plain content-addressed store suffices for V1.

### 7.2 Sync protocol, offline, and the field tier

The client follows the sync-engine pattern (Linear, §3.5): a local store, optimistic local application, a background channel that ships operations up and receives others down. Two transport profiles:

**Design tier (online, collaborative authoring).** A live websocket carries CRDT updates for real-time co-editing and operation-log entries for durability. Latency is low; the central server can impose order (a Jupiter-style authority, §2.8, is available here if we ever prefer OT for some component). This is the icddr,b researcher at a desk with five co-authors.

**Field tier (offline-first, intermittent).** The tablet holds a full local CRDT replica, edits offline for hours or days, and reconciles opportunistically when a connection appears. Sync ships only the new operations/updates since the last common point — bandwidth proportional to *change*, not document size — and the CRDT's convergence guarantee plus the validation pass handle reconciliation. This is the enumerator in peri-urban Chittagong. Note that *authoring* in the field is rarer than *deploying*; the more common field path is "publish a frozen version to ODK/Kobo and collect responses," for which the versioning system's job is simply to record exactly which immutable version was published and when (an as-of pin), so that the dataset is forever traceable to its instrument version.

### 7.3 Performance and scaling

The dominant scaling concerns are log growth and merge cost. **Log growth:** the operation log grows without bound; mitigations are periodic materialized snapshots (so reads need not replay from genesis) and *compaction* — collapsing long runs of superseded operations on a node into the net change, while preserving enough history for audit (a tension resolved by policy: keep full granular history for the compliance window, compact older history to per-item-per-day summaries). **Columnar encoding** (Automerge, §3.6) compresses the log for storage and transmission by an order of magnitude by exploiting the regularity of operation fields. **Merge cost:** id-matching makes diff near-linear in change size (§4.2); the validation pass is linear in the dependency-graph size, which for a survey instrument is small (hundreds of items, not millions). None of this is at risk of the Darcs exponential-merge trap (§3.2) precisely because we declined the patch-commutation engine. **Vector-clock cost** (§2.6) is the one genuinely field-sensitive item: with many tablets, prefer dotted version vectors or a server-assigned dot to avoid per-replica vector bloat.

### 7.4 Security, signing, and provenance integrity

The hash-chained log (§7.1) makes history tamper-evident: altering any past entry breaks every subsequent `entry_hash`. For stronger guarantees (a journal or IRB requiring cryptographic non-repudiation), entries or commits can be **signed** with an author key, so attribution is provable, not merely asserted — the analogue of signed Git commits. Access control sits at Layer 3: who may propose, who may accept, who may publish, who may detach a locked module. The published-version pin (§7.2) should be immutable and signed, because it is the artifact a dataset's provenance ultimately rests on.

### 7.5 Testing a merge engine

Merge engines fail in the corners, so they demand **property-based testing**: generate random instruments and random concurrent edit sequences, apply them along different paths, and assert the invariants — convergence (all paths reach the same state, the CRDT/SEC property of §2.7), validity (the validation pass holds post-merge), and intent preservation (no edit silently vanishes without becoming either applied or an explicit conflict). Fuzzing concurrent operation interleavings is the only reliable way to gain confidence in the §4.4 typed-merge rules and the §4.5 integrity checks. This testing discipline is itself an argument for adopting a mature CRDT library (whose convergence is already battle-tested) rather than hand-rolling one (whose corners you would have to discover yourself).

## 8. Economics

Architecture choices are also cost choices. Five economic dimensions matter, and they do not all point the same way.

### 8.1 Storage cost and the immutability tax

Append-only history (B, and Layer 2 of E) trades storage for trust: nothing is ever deleted, so storage grows monotonically. The relevant quantities are the operation rate and the per-operation encoded size. A survey instrument under active authoring might see thousands of operations over its design life; at columnar-encoded sizes (tens of bytes per operation after compression, §3.6) this is *kilobytes to low megabytes per instrument* of full history — economically negligible at the per-instrument level. The cost only becomes material at the *fleet* level (an institution with tens of thousands of instruments and item banks), where compaction policy (§7.3) and tiered storage (hot recent history, cold archived history) turn an unbounded liability into a bounded, predictable line item. The immutability tax is real but small, and it buys the compliance property that is otherwise expensive to retrofit. Snapshot-only storage (A) without structural sharing would be far *more* expensive (document size × version count); with Merkle sharing it collapses to change-proportional, the same order as the op-log.

### 8.2 Compute cost of merge, and where it runs

Merge compute is cheap in absolute terms for instrument-sized data (§7.3), so the economic question is *where* it runs. Server-side merge centralizes cost and simplifies clients but requires the server to hold keys/state and scales with concurrent merges; client-side merge (viable for CRDT and op-log approaches) pushes cost to the edge for free and supports the offline tier, at the price of shipping merge logic to every client. The hybrid runs convergence on the client (Layer 1, free, offline) and the heavier validation-and-integration pass wherever is convenient (client for solo work, server for shared review), which is the cost-optimal split.

### 8.3 Bandwidth economics for the field

In LMIC field deployments, bandwidth is metered and scarce, making the *wire size of synchronization* a direct operating cost and sometimes a feasibility gate. This strongly favours operation/delta-based sync (B, C, E) over snapshot shipping (naïve A): you transmit changes, not documents, and columnar encoding compresses those changes aggressively. The published-version-pin pattern (§7.2) further minimizes field bandwidth — enumerators download one frozen instrument version and upload only responses, never the authoring history. This dimension alone disqualifies any design that would require shipping whole instrument states over field links.

### 8.4 Build-versus-buy and the talent market

The sharpest economic fork. **Buying** the hard parts — adopting Yjs or Automerge for the CRDT layer (§3.6) — means the convergence machinery, the sequence-CRDT correctness (§2.7), and the columnar encoding are already built, tested, and maintained by an active community, at zero licence cost (both are permissively licensed). **Building** a custom patch-commutation engine (D) means hiring and retaining scarce specialists, funding a multi-quarter research-grade effort, and owning the maintenance and the corner-case bugs forever — the Darcs warning (§3.2) is an economic warning as much as a technical one. The hybrid is deliberately structured so that the *expensive, error-prone* part (real-time convergence) is *bought*, and the *bespoke* parts (the semantic model §4, the validation pass §4.5, the review state machine §4.6, the vocabulary translation §6.4) are exactly the parts that are *our domain value* and that no library can provide anyway. This aligns spend with differentiation: pay nothing for commodity convergence, invest everything in the survey-methodology semantics that are the actual product.

### 8.5 Total cost of ownership and the business model

The versioning architecture must serve the tiered business model from the feasibility assessment: a permanent free tier, and a self-hosted institutional tier for data sovereignty. The hybrid serves both because its durable truth (the operation log and snapshot store, Layer 2) is a self-contained, auditable, relational-or-object store that an institution can run on its own infrastructure (the Fossil deployment philosophy, §3.1) — no dependency on a proprietary cloud service for the core history. The CRDT layer (Layer 1) runs peer-to-peer or via a self-hostable relay. This means the *same* engine powers the hosted free tier and the self-hosted sovereign tier, with no architectural fork — a significant TCO advantage over a design whose collaboration depends on a proprietary cloud backend that cannot be handed to an institution. Data sovereignty is not a feature bolted on; it is a consequence of choosing an engine whose source of truth is portable.

## 9. Decision framework and recommendation

### 9.1 Requirements-to-approach matrix

Scoring the §1 requirements against §5's approaches (✓✓ strong, ✓ adequate, △ weak, ✗ poor):

| Requirement | A | B | C | D | E |
|---|---|---|---|---|---|
| Referential integrity (validation pass) | ✓ | ✓ | ✓ | ✓ | ✓✓ |
| Validated-module atomicity | ✓ | ✓ | ✓ | ✓ | ✓✓ |
| Offline-first field use | △ | ✓ | ✓✓ | ✓ | ✓✓ |
| Multilingual independent merge | ✓ | ✓ | ✓ | ✓ | ✓✓ |
| AI reviewer = human reviewer | ✓ | ✓✓ | △ | ✓ | ✓✓ |
| Tamper-evident audit / IRB | ✓ | ✓✓ | △ | ✓ | ✓✓ |
| Real-time co-editing | ✗ | △ | ✓✓ | △ | ✓✓ |
| Low bandwidth | △ | ✓✓ | ✓ | ✓ | ✓✓ |
| Review / suggesting clarity | ✓ | ✓ | ✗ | ✓ | ✓✓ |
| Build cost / risk | ✓✓ | ✓ | ✓ | ✗ | △ |

### 9.2 Recommendation

**Adopt the hybrid (§5E), but phase it so that early versions are mostly Approach A/B and the CRDT layer arrives only when real-time co-editing is actually needed.** The phasing matters because the matrix's one mark against E is build cost, and that cost is front-loaded in the CRDT integration — which is *not* needed for the first useful product.

**Phase 1 (MVP) — A+B core, no live co-editing.** Build the semantic model (§4) with strict stable identity, the content-addressed snapshot store (A), the hash-chained operation log (B), the validation pass (§4.5), and the Layer 3 review state machine (§4.6) powering Tier 1 suggesting and Tier 2 alternative drafts. Co-editing is "edit, save, sync, merge-on-conflict" — adequate for the review-centric workflow the prototype demonstrates. This delivers history, attribution, audit, branching, suggesting, structural diff, semantic-conflict detection, and the four-card resolution — the entire reviewing protocol — without the hardest engineering. It is irreversibly correct on the one thing that is irreversible: identity (§4.2). Everything else can evolve; identity assigned wrong in Phase 1 cannot be repaired later, so the discipline must be perfect from commit one.

**Phase 2 — add the CRDT co-editing layer (C → full E).** When simultaneous multi-cursor authoring and heavy offline field authoring become real requirements, introduce Yjs or Automerge as Layer 1, distilling its updates into the existing operation log. Because Layer 2 and Layer 3 already exist and are the durable truth, this is an additive change, not a rewrite — the CRDT becomes the live cache in front of a history layer that already works.

**Phase 3 — scale optimizations.** Prolly-tree storage (§3.3), columnar log encoding (§3.6), compaction and tiered storage (§7.3, §8.1), and signing (§7.4) as fleet size and compliance demands warrant.

**What is irreversible and must be right immediately:** stable identity (§4.2), the operation vocabulary (§5.2 — operations are a forever-schema once in history), and the hash-chaining of the log (§7.1 — retrofitting tamper-evidence onto existing untrusted history is impossible). **What is safely deferrable:** the CRDT layer, prolly trees, columnar encoding, signing, and every scaling optimization. Spend the early scrutiny on the irreversible decisions.

### 9.3 Why not simply adopt a CRDT library and be done

Because the product's distinctive value is the *review* experience — attributable, deliberate, methodology-aware suggestion and conflict resolution with an AI reviewer in the loop — and that experience runs *against* the CRDT grain (§5.3, §2.7). A CRDT-only product would have superb co-editing and a thin, awkward review story, which inverts our differentiation. The hybrid keeps the CRDT where it shines and builds our actual value-add as a semantic layer the substrate cannot provide.

## 10. Risks, open problems, and research questions

**Operation-vocabulary lock-in.** Once operations are written to durable history they must be supported forever; a poorly chosen vocabulary becomes permanent debt. Mitigation: design the operation set conservatively, version it, and treat additions as easy but removals as breaking.

**Distillation fidelity (Phase 2).** Converting CRDT keystroke-level updates into clean semantic operations for the log is non-trivial; a sloppy distiller produces either noisy history or lost intent. This is the main technical risk of the hybrid and deserves prototyping before Phase 2 commits.

**Semantic-conflict completeness.** The validation pass (§4.5) catches the integrity violations we enumerate; the open question is whether the enumeration is complete for survey methodology. New item types or logic features add new invariants. Mitigation: make the invariant set extensible and treat it as a living methodological artifact, co-owned with the research-methods side.

**Module lifecycle.** Validated-module versioning (what happens when PHQ-9 itself has a revised translation, or when a study legitimately needs a modified module flagged as non-standard) is under-specified here and needs its own design pass.

**Translation review at scale.** Per-language independent merge (§4.4) is specified, but the *workflow* of large multilingual instruments with separate translator and methodologist review lanes may need richer modelling than the current Tier-1/Tier-2 split.

**Identity across import/export.** Round-tripping through XLSForm (which has no place to store our stable ids) risks losing identity on export-edit-reimport. Mitigation: carry ids in an XLSForm column or a sidecar mapping; this is the riskiest interaction with the existing ecosystem and the most important to prototype early (it is also flagged in the UX theory document's next-steps).

---

# APPENDICES

## Appendix A — Formal definitions

**A.1 Join-semilattice and CRDT convergence.** A *join-semilattice* is a partially ordered set `(S, ≤)` in which every pair of elements `a, b` has a least upper bound `a ⊔ b` (the *join*). The join is commutative, associative, and idempotent. A state-based CRDT is a triple `(S, ⊔, s₀)` where `S` is a join-semilattice of states, `⊔` is the merge, and `s₀` is the initial state; each local update must be *inflationary* (`s ≤ update(s)`). **Theorem (Strong Eventual Consistency, Shapiro et al. 2011):** if all replicas start from `s₀`, apply updates locally, and periodically merge states via `⊔`, then any two replicas that have observed the same set of updates have equal state, and all replicas converge to the join of all updates — regardless of message order, duplication, or delay (assuming eventual delivery). The proof is immediate from the lattice axioms: the join of a set is independent of grouping (associativity), order (commutativity), and repetition (idempotence).

**A.2 Edit distance recurrence (Levenshtein).** For strings `a` (length `m`) and `b` (length `n`), the edit distance `D(i,j)` between prefixes satisfies `D(i,0)=i`, `D(0,j)=j`, and for `i,j>0`: `D(i,j) = D(i-1,j-1)` if `a[i]=b[j]`, else `1 + min(D(i-1,j) [delete], D(i,j-1) [insert], D(i-1,j-1) [substitute])`. The LCS recurrence (§2.3) is the special case forbidding substitution. Myers' algorithm computes the insert/delete edit script in `O((m+n)·D)` where `D` is the script length.

**A.3 Tree edit distance.** Given ordered labelled trees `T₁, T₂`, the tree edit distance is the minimum cost of a sequence of node insert, delete, and relabel operations transforming `T₁` into `T₂`, subject to ancestor-order preservation. Zhang–Shasha (1989) solves it in `O(|T₁|·|T₂|·min(depth₁,leaves₁)·min(depth₂,leaves₂))`, bounded by `O(n³)` for balanced inputs. Unordered tree edit distance is NP-complete (Zhang, Statman, Shasha 1992). With stable node ids (§4.2), the matching is given, and the cost collapses to `O(n)` field comparison plus position reconciliation.

**A.4 OT transformation properties.** Let `T(op_a, op_b)` transform `op_a` to account for the concurrent `op_b`. **TP1:** for concurrent `a, b` on the same state, `apply(apply(s, a), T(b, a)) = apply(apply(s, b), T(a, b))` (the two replicas converge after exchanging one op each). **TP2:** for concurrent `a, b, c`, `T(T(a, b), T(c, b)) = T(T(a, c), T(b, c))` (transformation is independent of the order in which prior concurrent ops were incorporated). Central-server (Jupiter) designs satisfy convergence without TP2 because each client transforms only against a linear server history.

**A.5 Patch commutation.** Patches `p: X → Y` and `q: X → Z` from a common state *commute* if there exist `p': Z → W` and `q': Y → W` with `p' ∘ q = q' ∘ p` (the square commutes). In the categorical model (Mimram–Di Giusto 2013), the merged state `W` is the *pushout* of `p` and `q`. When the pushout does not exist in the base category, it is constructed in a completion that represents the conflict explicitly, which is the formal content of "a conflict is a first-class state."

## Appendix B — Glossary (engine ↔ user)

*AST*: abstract syntax tree; here, the typed instrument tree. *CRDT*: conflict-free replicated data type. *SEC*: strong eventual consistency. *OT*: operational transformation. *LCA / merge base*: lowest common ancestor of two versions in the history DAG. *Merkle DAG*: directed acyclic graph whose nodes are named by content hashes. *HLC*: hybrid logical clock. *OR-set*: observed-remove set. *Prolly tree*: probabilistic / content-addressed B-tree. *Distillation*: converting low-level CRDT updates into semantic operations. *Validation pass*: the invariant check that gates a merge. *Change-set*: a reviewable bundle of proposed edits (a "suggestion"). For user-facing translations of commit/branch/merge/conflict, see §6.4.

## Appendix C — A worked merge (the heat-stress instrument)

Main draft, item `q5` (id `it_5a9`), label EN: "In the last 7 days, have you experienced any of the following while exposed to heat?" Two alternative drafts diverge from a common ancestor commit `C0`.

*Draft "clinical" (Dr. Rahman):* relabels `q5` EN to "During heat exposure in the last 7 days, did you experience any of these symptoms?" → operation `SetLabel(it_5a9, en, …)`.

*Draft "short pilot" (Aluk):* deletes `q3` (id `it_3f1`, hours outdoors) to shorten the form → `DeleteItem(it_3f1)`; and unrelatedly relabels `q5` EN to "In the past week, did any of these happen to you on a hot day?" → `SetLabel(it_5a9, en, …)`.

Integrating both into main: the two `DeleteItem`/`SetLabel` operations on *different* nodes compose cleanly. But the two `SetLabel(it_5a9, en, …)` operations are concurrent edits to the *same field* of the *same item* with different intent → a label-clash conflict at item grain, surfaced to the four-card UX (ancestor + Rahman + Aluk + compose, with optional AI synthesis). *Separately*, the validation pass runs on the candidate merge and finds that `q4`'s relevance expression `${hours_outdoors} >= 4` now references a deleted variable (Aluk's `DeleteItem(it_3f1)`) → a delete-vs-depend semantic conflict, surfaced with reasons "restore q3 / remove q4's skip logic / redirect the reference." Two conflicts, both at item grain, both legible, neither expressible in a text merge. The PI resolves the label clash by composing a hybrid wording and resolves the integrity conflict by restoring `q3`; a merge commit lands with both decisions and their rationales in the activity log.

## Appendix D — Annotated influences

*Lamport (1978), logical clocks* — causality without synchronized time. *Fidge/Mattern (1988), vector clocks* — detecting concurrency. *Myers (1986), O(ND) diff* — the diff every tool runs. *Zhang–Shasha (1989), tree edit distance* — the tree-diff baseline; the NP-completeness of the unordered case (Zhang–Statman–Shasha 1992) motivates stable ids. *Falleri et al. (2014), GumTree* — match-then-derive AST diffing; the work stable ids let us skip. *Ellis–Gibbs (1989), Ressel et al. (1996), Nichols et al. (1995, Jupiter)* — the OT lineage and the central-server escape from TP2. *Shapiro–Preguiça–Baquero–Zawirski (2011), CRDTs* — the convergence theorem. *Oster (WOOT 2006), Preguiça (Treedoc 2009), Weiss (Logoot 2009), Roh (RGA 2011), Nédelec (LSEQ 2013), Nicolaescu (YATA 2016)* — the sequence-CRDT genealogy behind Yjs. *Weidner–Kleppmann (Fugue 2023)* — non-interleaving. *Litt et al. (Peritext 2021–22)* — rich-text CRDT. *Kleppmann (Automerge; columnar encoding 2020)* — JSON CRDT and log compression. *Kleppmann et al. (local-first software 2019)* — the field-tier philosophy. *Roundy (Darcs), Mimram–Di Giusto (2013), Pijul* — patch theory and conflicts-as-states. *Datomic (Hickey)* — the immutable fact log. *Dolt* — prolly trees and structured-data version control. *Figma, Notion, Linear, Google Docs* — production proofs for structured-document multiplayer, per-block identity, sync engines, and suggesting mode. *REDCap, Qualtrics, SurveyCTO, ODK/Kobo* — the domain incumbents whose thin versioning defines the whitespace.

*Note on citations: years and attributions reflect the author's best knowledge of the literature and should be verified against primary sources before external publication. Several systems (Yjs, Automerge, Dolt, Pijul, Figma) continue to evolve; specific capabilities cited here should be re-confirmed against current releases.*

---

*End of design compendium. This document is intended to precede a formal Architecture Decision Record. The recommended next artifacts are: (1) an ADR committing to the §9.2 phasing; (2) a prototype of the XLSForm identity round-trip (§10), the highest-risk ecosystem interaction; and (3) a property-based test harness for the §4.4 merge rules (§7.5), to de-risk the merge core before it is built.*
