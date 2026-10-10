# Provable Provenance for Documents: A Formal Model of Identity, Attribution and History

**Part 0: the model.** The proofs come later, in separate parts.

**Status:** internal research draft, 10 October 2026. This file describes the
*vision* of Dynodoc, not the current code. Section 12 lists how the current code
differs. Do not publish it: the patent items in `docs/OPEN-RESEARCH-REGISTER.md`
(L1–L5) still apply.

**Corrections:** writing the proofs required 14 changes to this model (deletes
are no longer recursive, text order is a successor function, same-anchor
inserts are conflicts, and more). They are listed in
[`PROVENANCE-PROOFS.md` §1](PROVENANCE-PROOFS.md#1-changes-the-proofs-required).
Where the two files disagree, the proofs file wins.

Related: [decision 012](../decisions/012-event-model-v2-and-replay-proof.md) and
[the mathematics companion](../03-versioning-mathematics-vol2.md). Section E.2.1
of the companion already proves tamper evidence for a hash chain.

---

## 0. What the model must support

The vision in one sentence: *every part of a document has a stable identity,
and every change is recorded, attributed, verifiable and reviewable, across any
editor.*

The model must be precise enough to state and prove five guarantees:

| # | Guarantee | Plain meaning |
|---|---|---|
| G1 | Reconstruction | Any recorded version can be rebuilt exactly |
| G2 | Attribution | Every character traces to the one event that wrote it, even after split, join, move and merge |
| G3 | Integrity | Any change to recorded history is detected |
| G4 | Safe merge | Merging never silently loses or corrupts work; anything unclear becomes a conflict |
| G5 | Identity across editors | Syncing with an external editor keeps the identity of everything the user did not touch (under stated assumptions) |

**Main design choices.**

1. **Every character has an identity.** This is the only way to make "who
   wrote this sentence" exact after split, join and merge. An implementation
   may compress characters into runs (Section 11). The model does not.
2. **Operations address things by ID, never by index.** An index changes
   meaning when someone edits earlier text. An ID does not.
3. **Nothing is removed.** A deleted node or character becomes a tombstone.
   So IDs stay meaningful forever.
4. **History is a graph of commits**, like Git. A merge *references* the work
   it merges. It never *copies* that work under the merger's name.

---

## 1. Basic sets

| Symbol | Meaning |
|---|---|
| `Σ` | Unicode scalar values (the characters) |
| `Id` | A countably infinite set of identifiers, with a fixed strict total order `<ᵢ` (for example, byte order) |
| `Actor` | Verified identities (people or services that signed in) |
| `Claim` | Unverified names: pairs `(name, source)`, for example a Word tracked-change author |
| `Author = Actor ∪ Claim` | Who made a change |
| `Pos` | Position keys with a strict total order `<ₚ` that is **dense** (`a <ₚ b ⇒ ∃c. a <ₚ c <ₚ b`) and **unbounded** (there is always a smaller and a larger key) |
| `Key` | Field names |
| `Val` | Field values (JSON-like). `⊥` means "absent" |
| `MarkKind` | Formatting dimensions: bold, italic, colour, font, link, … |
| `Style = MarkKind ⇀ Val` | A finite partial map. A missing kind means "the default". The default is never stored, so every style has exactly one representation. |
| `Hash = {0,1}²⁵⁶` | Digests. `H : Bytes → Hash` is SHA-256 |
| `Bytes` | Finite byte strings |

**Node classes and kinds.** Every node has a fixed **class**:

```
Class = { container, text, embed }
```

- `container` holds other nodes: the document root, sections, lists, list items,
  tables, rows, cells, footnotes.
- `text` holds characters and has no children: paragraphs, headings, code
  blocks, cell text.
- `embed` holds no characters or children: images, rules, equations, page breaks.

A node also has a **kind** (paragraph, heading, table, row, …). The kind is
stored as the field `kind` and may change; for example, a paragraph may become a
heading and keep its ID. The class never changes.

**Grammar.** `G ⊆ Kind × Kind` says which kind may contain which. Exactly one
kind, `root`, may have no parent. The grammar is a parameter of the model.
Different products (reports, questionnaires) use different grammars.

---

## 2. Document state

A state is a tuple:

```
D = ⟨ N, Q, C, B, M, S ⟩
```

| Part | Type | Meaning |
|---|---|---|
| `N` | `Id ⇀ Node` | The nodes |
| `Q` | `Id ⇀ Id*` | For each text node, its character sequence: a list of character IDs, **including tombstones** |
| `C` | `Id ⇀ Char` | The characters |
| `B` | `Hash ⇀ Bytes` | Assets (images, files), addressed by their own hash |
| `M` | `Id ⇀ Comment` | Comments (annotation layer) |
| `S` | `Id ⇀ Suggestion` | Suggestions (annotation layer) |

```
Node       = ⟨ class, parent ∈ Id ∪ {⊥}, pos ∈ Pos, fields : Key ⇀ Val, live ∈ 𝔹 ⟩
Char       = ⟨ ch ∈ Σ, style ∈ Style, live ∈ 𝔹 ⟩
Comment    = ⟨ anchor, reply_to ∈ Id ∪ {⊥}, body, resolved ∈ 𝔹 ⟩
Suggestion = ⟨ target ∈ Id, proposal, status ∈ {open, accepted, rejected}, reason ⟩
```

- A comment `anchor` is `⊥` (the whole document), a node ID, or a character
  range `(c₁, c₂)` with both characters in the same text node.
- The **content** of `D` is `⟨N, Q, C, B⟩`. The **annotations** are `⟨M, S⟩`.
- `ids(D) = dom N ∪ dom C ∪ dom M ∪ dom S`. The four domains are disjoint.
- **Children order.** The children of `x` are the nodes with `parent = x`,
  ordered by the pair `(pos, id)` lexicographically. Two siblings may share a
  `pos`; the ID breaks the tie, so the order is always total.
- **The visible document** `view(D)` is the tree of live nodes, with only the
  live characters in each text node, in order. This is what a reader sees.
  Tombstones are not visible.
- **The empty state** `D∅` has all six parts empty.

---

## 3. Well-formedness

`WF(D)` holds when all of the following hold:

| # | Invariant |
|---|---|
| W1 | **One root.** Exactly one node has `parent = ⊥`. Its kind is `root`, and it is live. |
| W2 | **Parents exist.** Every other node's parent is in `dom N`. |
| W3 | **No cycles.** Following `parent` from any node reaches the root. |
| W4 | **Live parents.** If a node is live, its parent is live. |
| W5 | **Grammar.** For every live non-root node `x`: `(kind(parent x), kind x) ∈ G`. |
| W6 | **Classes.** Only `container` nodes have children. `dom Q` is exactly the set of `text` nodes. |
| W7 | **Each character in exactly one place.** Every `c ∈ dom C` occurs exactly once in exactly one list `Q(x)`. Every ID in any `Q(x)` is in `dom C`. |
| W8 | **Assets are honest.** `B(h) = b ⇒ H(b) = h`. Every asset hash referenced by a field is in `dom B`. |
| W9 | **Anchors exist.** Every comment anchor and `target` refers to an ID in `ids(D)`. Tombstones are allowed, so anchors never dangle. |

---

## 4. Operations

An operation is a pure function on states with a **precondition**.
`apply(D, o)` is defined only when `pre(o, D)` holds.

`host(c)` is the text node whose list contains character `c`; by W7 it is unique.
Text operations address characters **only by ID**. They do not name the node, so
a text edit still applies after another person splits, joins or moves the text.

`fresh(x, D)` means `x ∉ ids(D)`. Because nothing is ever removed from `ids`
(Lemma L3), a fresh ID was never used before in this history.

### 4.1 Content operations

| Operation | Precondition | Effect |
|---|---|---|
| `Create(x, κ, p, π, f)` | `fresh(x)`. Either `p = ⊥`, `f(kind) = root` and no root exists, or `p` is a live container and `(kind p, f(kind)) ∈ G`. | Adds node `x` (class `κ`, parent `p`, pos `π`, fields `f`, live). If `κ = text`, sets `Q(x) = ε`. |
| `Move(x, p, π, p′, π′)` | `x` is live and not the root. `(p, π)` is `x`'s current place. `p′` is a live container with `(kind p′, kind x) ∈ G`. `p′` is not `x` or a descendant of `x`. | Sets parent `p′` and pos `π′`. |
| `Delete(x, R)` | `x` is live and not the root. `R` is exactly the set of live descendants of `x`. | Sets `live = false` for `x` and every node in `R`. |
| `Restore(x, R)` | `x` is dead and its parent is live. Every node in `R` is dead and was killed by the `Delete` that killed `x`. | Sets `live = true` for `x` and `R`. |
| `SetField(x, k, v, v′)` | `x` is live. `fields(x)(k) = v`. If `k = kind`, the grammar still holds for `x`'s parent and children. | Sets `fields(x)(k) = v′`. `v′ = ⊥` removes the field. |
| `Insert(a, [(c₁,ch₁,s₁) … (cₙ,chₙ,sₙ)])` | The anchor `a` is either `◁ₓ` (the start of text node `x`) or a character; it may be a tombstone. The anchor's node is live. All `cᵢ` are fresh and distinct. | Puts `c₁…cₙ` right after `a`, in the list that contains `a` (or at the start of `Q(x)`). Adds each `cᵢ` with character `chᵢ`, style `sᵢ`, live. |
| `Erase([c₁ … cₙ])` | Each `cᵢ` is a live character, and `host(cᵢ)` is live. | Sets `live = false` for each `cᵢ`. |
| `Format([(c₁, v₁) … (cₙ, vₙ)], k, v′)` | Each `host(cᵢ)` is live, and `style(cᵢ)(k) = vᵢ` (the old value). | Sets `style(cᵢ)(k) = v′` for every `i`. `v′ = default` removes the kind. |
| `Split(x, a, y, π, f)` | `x` is a live text node. `a ∈ Q(x)`. `fresh(y)`. `(kind(parent x), f(kind)) ∈ G`. No live sibling sits strictly between `x` and `(π, y)` in sibling order. | Adds text node `y` (parent of `x`, pos `π`, fields `f`, live). The part of `Q(x)` from `a` to the end moves to `Q(y)`. **The characters keep their IDs.** |
| `Join(x, y)` | `x` and `y` are live text nodes with the same parent. `y` is the next live sibling after `x`. | `Q(x) := Q(x) · Q(y)`, then `Q(y) := ε` and `y` becomes a tombstone. **The characters keep their IDs.** |
| `PutAsset(h, b)` | `H(b) = h`. | `B(h) := b`. Applying it again changes nothing. |

### 4.2 Annotation operations

| Operation | Precondition | Effect |
|---|---|---|
| `Comment(m, anchor, r, body)` | `fresh(m)`. The anchor exists. `r = ⊥` or `r ∈ dom M`. | Adds comment `m`, not resolved. A reply is a comment with `r ≠ ⊥`. |
| `Resolve(m, b)` | `m ∈ dom M` | `resolved(m) := b`, an explicit value, never a toggle |
| `Propose(s, t, P)` | `fresh(s)`. `t ∈ ids(D)`. `P` is a finite list of content operations. | Adds open suggestion `s`. **The content does not change.** |
| `Decide(s, d, reason)` | `s` is open. `d ∈ {accepted, rejected}`. | Sets the status. Section 5 says how an acceptance is recorded. |

**Explicit old values.** `Move`, `SetField` and `Format` carry the value they
replace. This has two uses. First, every operation can be reversed from its own
data (Lemma L5). Second, a merge can see exactly what each side assumed
(Section 9).

---

## 5. Events

An event wraps one or more operations:

```
e = ⟨ ops ∈ Op⁺, author α ∈ Author, recorder ρ ∈ Actor, time t, stream σ, via ∈ Id ∪ {⊥} ⟩
```

- **Atomic.** `apply(D, e)` applies `ops` in order. The event is valid only if
  every operation is valid at its turn. A rich-text edit is a single event
  holding several `Insert`, `Erase` and `Format` operations.
- **Author and recorder are separate.** `α` is who made the change. `ρ` is the
  signed-in actor whose action recorded it: the merger, the importer, or a sync
  connector. When `α ∈ Claim`, the model makes no claim that the name is true.
- **Streams.** `σ = content` events contain only content operations, and
  `σ = annotation` events contain only annotation operations. A comment
  therefore never changes the content root (Section 7).
- **Outcomes, not intents.** Accepting suggestion `s` is a `Decide(s, accepted)`
  event, followed in the same commit by content events with `via = s`. These
  content events are the actual change. Replay never re-runs the proposal.

---

## 6. History

### 6.1 Commits

History is a finite directed acyclic graph of commits:

```
k = ⟨ parents P, events E = e₁ … eₘ, content root r, annotation root q, author α, recorder ρ, time t, message ⟩
```

- A commit has 0 parents (the first commit), 1 parent (an ordinary commit) or
  2 parents (a merge).
- `Hist(k)` is the set of all events in `k` and in all its ancestors.

### 6.2 The state of a commit

`state(k)` is defined by recursion over the graph:

- **No parents:** `state(k) = apply(D∅, E)`.
- **One parent `k₁`:** `state(k) = apply(state(k₁), E)`.
- **Two parents `k₁, k₂`:** let `b` be their merge base (their lowest common
  ancestor, assumed unique; see open question Q4). Then
  `state(k) = apply(merge(state b, k₁, k₂), E)`. Here `merge` is the function
  of Section 9, and `E` contains **only the conflict resolutions**.

So a merge never re-records the merged work. The events of both branches stay
in `Hist(k)` with their original authors. This is what makes G2 hold after a
merge.

### 6.3 Hashes

`enc` is a canonical, **injective**, length-framed encoding of events, commits
and states. Lists are framed by length, and maps are sorted by key.

**Event chain inside a commit.** Let `h₀ = id(first parent)`, or `0²⁵⁶` if
there is none. Then:

```
hᵢ = H( H(enc eᵢ) ‖ hᵢ₋₁ )        for i = 1 … m
```

**Commit ID:**

```
id(k) = H( enc(P) ‖ hₘ ‖ r ‖ q ‖ enc(α, ρ, t, message) )
```

A commit ID therefore binds its parents, every event in order, both roots and
its metadata, exactly as a Git commit binds its tree and parents. A linear log
is the special case where every commit has one parent.

### 6.4 References and tags

- A **branch** is a mutable pointer `name ↦ commit id`. Moving a pointer is an
  operational action, not provenance, and it is not part of history.
- A **tag** is an immutable record `⟨name, commit id, kind⟩`, stored in
  history. A tag of kind `release` is valid only if a deployment gate
  `Gate(state k)` holds. `Gate` is a parameter, for example the questionnaire
  integrity checks.

---

## 7. Roots

- `R(D)` is the **content root**: a Merkle root over the sorted leaves
  `enc(x, N(x))`, `enc(x, Q(x))`, `enc(c, C(c))` and the asset hashes.
- `A(D)` is the **annotation root**, built the same way over `M` and `S`.

**Rule.** When a client makes commit `k`, it computes `r` and `q` **from its own
local state**, the state the author saw. A server or any other client
recomputes them by replay. Because the two computations are independent,
"the roots match" is a real check, not "replay equals replay".

---

## 8. Attribution

For a commit `k` and IDs in `state(k)`:

| Function | Definition |
|---|---|
| `writer(c)` | the author of the unique event in `Hist(k)` that contains `Insert(…, c, …)` |
| `eraser(c)` | the author of the event that erased `c`, if `c` is dead |
| `formatter(c, j)` | the author of the latest `Format` of kind `j` on `c`, along any path to `k`. Two concurrent formats of the same kind with different values are a merge conflict, so "latest" is unique after resolution. |
| `creator(x)` | the author of the event that created node `x` (by `Create` or `Split`) |
| `editor(x, j)` | the author of the latest `SetField(x, j, …)` |
| `recorder(…)` | the same functions, but using `ρ` instead of `α` |

"Who wrote this sentence?" is `{ writer(c) : c in the sentence }`. This is a
set, because a sentence can have several writers.

---

## 9. Merge

### 9.1 Slots

Every operation writes some **slots**:

| Operation | Slots written |
|---|---|
| `Create`, `Split` (new node) | `(y, all)`, the new node, plus `(p, gap after its predecessor)` |
| `Move` | `(x, place)` |
| `Delete`, `Restore` | `(z, live)` for `x` and every `z ∈ R` |
| `SetField(x, k, …)` | `(x, field k)` |
| `Insert(a, …)` | `(after a)` |
| `Erase` | `(c, live)` for each `c` |
| `Format(…, k, …)` | `(c, style k)` for each `c` |
| `Join(x, y)` | `(x, tail)`, `(y, all)` |

### 9.2 Conflicts

Let `O₁` and `O₂` be the operations on the two branches since the base `b`. Two
operations `o₁ ∈ O₁` and `o₂ ∈ O₂` **conflict** when either:

- **(a) Competing writes:** they write the same slot with different values. Two
  inserts at the same `(x, after a)` are **not** a conflict; rule (c) orders
  them.
- **(b) Joint invalidity:** each is valid alone, but no order of the two keeps
  `WF`, or one breaks the other's precondition. Examples:
  - one side deletes `p` while the other creates a child under `p`;
  - two moves that together would make a cycle;
  - an edit to a node the other side deleted;
  - an `Insert` at `◁_y` (the start of `y`) on one side, while the other side
    joins `y` away. An insert after a *character* of `y` is **not** a conflict:
    the character moved with the join, so the insert follows it.

**(c) Same-anchor inserts.** If both sides insert after the same anchor `a`, each side's run of characters stays together. The runs are ordered by
the ID of their first character (larger first). This is the non-interleaving
rule of RGA-style sequences. The same rule applies to new blocks in the same
gap of a parent's children: a run created by one event stays together.

### 9.3 Result

`merge(state b, k₁, k₂)` is either:

- **Clean:** the state reached by applying `O₁` and then `O₂` to `state b`. The
  old-value fields in `O₂` are updated to the values after `O₁`. Rule (c) orders
  same-anchor runs.
- **Conflicted:** the clean part, plus a set of conflicts `K`. Each conflict
  lists the two operations and their slot. A commit with two parents can only
  be made after every conflict in `K` is resolved by an event in `E`.

---

## 10. External editors

An external editor `X` (Word, Google Docs, LibreOffice, …) is modelled as:

| Element | Meaning |
|---|---|
| `St_X` | the editor's own document states |
| `Sup_X` | the subset of `view` documents that `X` can represent (its supported features) |
| `exp_X : Sup_X → St_X` | export from Dynodoc to the editor, with **carriers** (bookmarks, content controls, named ranges) holding node IDs |
| `obs_X : St_X → View` | reading the editor's state back. Elements with a surviving carrier get their ID. Other elements get fresh IDs. |
| `U_X` | the class of user edits for which carriers are expected to survive (for example, typing inside one paragraph) |

Characters have no carriers in any external editor. `sync` therefore maps
characters by a **minimal diff** inside each identified node: unchanged
characters keep their IDs, and inserted characters are new.

**Assumptions about the editor.** These are about the editor, not about
Dynodoc. They must be tested for each editor and version.

- **F1 Round trip.** For `D ∈ Sup_X`: `obs_X(exp_X D) = view D`, with every ID
  kept.
- **F2 Carrier persistence.** For an edit `u ∈ U_X`, every node that `u` does
  not touch keeps its carrier.

---

## 11. Assumptions and allowed refinements

**Assumptions.**

| # | Assumption |
|---|---|
| A1 | `H` is collision resistant |
| A2 | Freshly generated IDs are globally unique. For random 80-bit ULID suffixes, this holds with probability at least `1 − n²/2⁸¹` for `n` IDs generated in the same millisecond. |
| A3 | `ρ` is authenticated: the recorder really is the signed-in actor. A verified author `α ∈ Actor` is either equal to `ρ` or was itself authenticated when the original event was made. |
| A4 | `enc` is injective, and all implementations compute the same bytes |
| A5 | F1 and F2 hold, used only for G5 |

**Allowed refinements.** An implementation may differ from the model if it is
provably equivalent:

- **Runs of characters.** A run of characters inserted together can be stored
  as one object with a base ID and an offset, as Yjs and Automerge do.
  Character `i` of the run has ID `(base, i)`.
- **Index-based patches.** Patches like today's `RichTextPatched` can be used
  on a linear history: the "before" hash pins the exact base, so indexes map to
  character IDs one-to-one. Merges must use the IDs.
- **Snapshots.** Snapshots are caches of `state(k)`. They are not part of the
  model.

---

## 12. The current code compared with this model

| Model | Current Dynodoc | Gap |
|---|---|---|
| Character IDs | none (Unicode-scalar indexes in patches) | needed for exact G2 after merge |
| Split / Join | create, delete and field edits | needed for G2 |
| Author ≠ recorder | one `actor` | merger becomes author |
| Merge references branches | merge re-records events under the merger | breaks G2 |
| Commit graph | one linear chain; drafts in `draft_edit` without a chain | needed for G3 on drafts |
| Explicit old values | absent in `FieldEdited`, and patch deletes keep only counts | needed for L5 |
| Content root ≠ annotation root | one Merkle root over nodes | comments affect nothing today, but there is no annotation root |
| Root computed by the client | the root is computed only by the server | needed to make G1 non-circular |
| Joint-invalidity conflicts | cycles and orphans can pass the merge | needed for G4 |
| Same-gap runs stay together | blocks interleave | needed for G4 |
| Metadata as fields | the title and settings are plain updates | needed for G1 |
| Nested IDs (rows, cells, list items) | positions inside one block's JSON | needed for G2 and G4 |

---

## 13. What to prove

### Lemmas

| # | Lemma |
|---|---|
| L1 | **Preservation.** `WF(D) ∧ pre(o, D) ⇒ WF(apply(D, o))`, for every operation. |
| L2 | **Determinism.** `apply` is a function: no clock, randomness, network or locale is used. |
| L3 | **Permanence.** `ids(D) ⊆ ids(apply(D, o))`. An ID's class never changes. A character's `ch` never changes after `Insert`. |
| L4 | **Binding.** Under A1 and A4: `R(D) = R(D′) ⇒` the contents of `D` and `D′` are equal, and the same holds for `A`. |
| L5 | **Visible reversibility.** For every valid `o` there is `o⁻¹`, built only from `o`'s own data, with `view(apply(apply(D, o), o⁻¹)) = view(D)`. Tombstones may remain, which is why the equality is on `view`. |
| L6 | **Commutation.** If `o₁` and `o₂` do not conflict in the sense of Section 9.2, then (with old values updated) applying them in either order gives the same state. This extends §D.2 of the companion to structural operations. |
| L7 | **Non-interleaving.** Under rule (c), each side's run of inserted characters or blocks stays contiguous in the merged sequence. |

### Theorems

| # | Theorem |
|---|---|
| T1 (G1) | For every commit `k` built from valid events, `state(k)` is defined, `WF(state k)` holds, and `R(state k) = r`, the root the author's client computed. With L4, this means the server's rebuilt document equals the author's document. |
| T2 (G2) | For every `k` and every character `c` in `state(k)`: exactly one event in `Hist(k)` inserts `c`. `ch(c)` never changes. `writer(c)` is unchanged by every `Move`, `Split`, `Join` and merge. Under A3, a verified writer is the actor who made the change. |
| T3 (G3) | Changing, removing, adding or reordering any event, any commit field or any parent link in `Hist(k)` changes `id(k)`, unless a collision of `H` is found. This follows from §E.2.1 of the companion. |
| T4 (G4) | For every merge: (a) every operation of `O₁ ∪ O₂` is either reflected in the result or named in a conflict; (b) the result is `WF`; (c) `merge(b, k, b) = state k` and `merge(b, k, k) = state k`; (d) swapping the two sides gives the same state and the same conflicts; (e) L7 holds. |
| T5 (G5) | Under F1 and F2: after an edit `u ∈ U_X` and a sync, replay gives `obs_X` of the edited state, and every node and character that `u` did not touch keeps its ID. Characters changed in `X` are attributed to the syncing user at capture granularity, not per keystroke. |

**Suggested order:** L2, L3, L1, T1 (with L4), then T3, then T2, then L5, L6,
L7 and T4, and T5 last.

---

## 14. A worked example

1. **Rahim** creates paragraph `p` and types `Hello world`. That is one event
   with `Insert(◁ₚ, [c₁…c₁₁])`, and `writer(cᵢ) = Rahim`.
2. **Karim**, on branch K, splits `p` before `w`:
   `Split(p, c₇, q, π, {kind: paragraph})`. Now `Q(p) = c₁…c₆` and
   `Q(q) = c₇…c₁₁`.
3. **Abir**, on branch A at the same time, makes `world` bold:
   `Format([(c₇,⊥)…(c₁₁,⊥)], bold, true)`.
4. **Merge.** Karim's split writes `(q, all)` and `(p, tail)`. Abir's format
   writes `(c₇…c₁₁, style bold)`. No slot is shared. Abir's format names
   only the characters, so it still applies after they moved to `q`. So the merge is **clean**: `q` contains a bold `world`.
5. **Attribution after the merge.**
   - `writer(c₇) = Rahim`. The split and the merge did not change it.
   - `formatter(c₇, bold) = Abir`.
   - `creator(q) = Karim`.
   - The merge commit has `E = ε`. It copies nothing, so Rahim's authorship is
     not overwritten.

In today's code, step 4 would re-record both branches' work under the merger,
and step 2 would lose the link between `world` and Rahim.

---

## 15. Open questions

| # | Question |
|---|---|
| Q1 | Is a character-level ID acceptable in storage? Measure run-compressed storage on the footprint corpus. |
| Q2 | Should `time` be bound in event hashes, or only in commit hashes? |
| Q3 | Which `MarkKind`s are independent? For example, is "link" one kind or two (URL and title)? This sets what counts as a conflict. |
| Q4 | Criss-cross merges with more than one lowest common ancestor need a rule, such as merging the bases recursively as Git does. |
| Q5 | Should `Join` across different parents be allowed? Today's rule requires siblings. |
| Q6 | Which grammar `G` is used for Word and Google Docs documents: tables, rows, cells, lists, footnotes? |
| Q7 | For each editor, what is `U_X`? This must be measured, as in the LibreOffice experiment. |
