# Provable Provenance for Documents — Part 1: Proofs of G1–G4

**Status:** internal research draft, 10 October 2026. Do not publish. The patent
items in `docs/OPEN-RESEARCH-REGISTER.md` (L1–L5) still apply.

This part proves four guarantees of the model in
[`PROVENANCE-FORMAL-MODEL.md`](PROVENANCE-FORMAL-MODEL.md) (Part 0):

| # | Guarantee | Theorem |
|---|---|---|
| G1 | Reconstruction: any version can be rebuilt exactly | T1 |
| G2 | Attribution: every character traces to its author, even after split, join and move | T2 |
| G3 | Integrity: any change to history is detected | T3 |
| G4 | Safe merge: nothing is silently lost or corrupted; anything unclear is a conflict | T4 |

G5 (external editors) is not proved here. It depends on how each editor
behaves, and that must be measured.

**How to read this.** Section 1 lists the changes the proofs forced on Part 0.
Section 2 restates the parts of the model the proofs use, in their corrected
form. Sections 3–4 prove general lemmas. Sections 5–8 prove the theorems.
Section 9 says exactly what is **not** proved.

---

## 1. Changes the proofs required

Writing the proofs exposed gaps in Part 0. Each change below is needed by a
specific proof step. Where this part and Part 0 disagree, **this part wins**.

| # | Change | Why the proof needs it |
|---|---|---|
| C1 | **Delete is not recursive.** `Delete(x)` marks only `x`. A node is *visible* when it and all its ancestors are live. `Restore(x)` brings back the whole subtree. | A recursive delete must read every descendant. It would then conflict with every concurrent edit anywhere under that node, and its precondition could not be checked locally (T4). |
| C2 | **Text operations do not require a live host.** `Insert`, `Erase` and `Format` work on tombstoned nodes too. Edits that become invisible are reported by a *visibility check* after merging (Section 8.4). | Otherwise every text edit reads the node's `live` flag and conflicts with any delete. The check keeps "edit vs delete" reported without breaking commutation. |
| C3 | **Idempotent preconditions.** An operation that carries an old value accepts the state if it holds the old value **or** the new value. `Erase` accepts a dead character, and `Delete` a dead node. `Decide(s, d)` accepts status `open` or `d`. | Two branches that delete the same node or erase the same character then merge without a conflict (T4(c)). It also prepares a finer rule for equal field edits (Section 9). |
| C4 | **Kind changes only within a compatibility class.** `SetField(x, kind, …)` may only change between kinds whose rows and columns in `G` are identical. For example, paragraph, heading and code block. | Otherwise a kind change must read all of `x`'s children, which causes conflicts with every concurrent child creation (T4) and makes the grammar invariant hard to keep (L1). |
| C5 | **Text order is a successor function** `nxt`. Each text node `x` has a start marker `◁ₓ`; following `nxt` from `◁ₓ` gives `Q(x)` and ends at `⊣`. | Two inserts at different anchors then write different slots, so they commute by a general lemma (L6). |
| C6 | **Same-anchor inserts are a conflict.** Two branches inserting right after the same character conflict; the person merging chooses the order. | This is what makes non-interleaving and commutation provable here. Automatic RGA-style ordering is a later refinement (Section 9). |
| C7 | **Restructuring operations name their hosts.** `Split(x, …)`, `Join(x, y)` and `MoveText` list every text node whose sequence they change. | Their preconditions must read only the slots they name (Section 3.2). |
| C8 | **`MoveText` is added.** It moves characters to another place, keeping their IDs. | G2 must hold after moving text, not only after moving nodes. |
| C9 | **`Adopt(e, i)` is added for conflict resolution.** It applies operation `i` of an event `e` that the merge held back. It applies the original operation; it does not copy it. | Otherwise resolving a conflict re-records held work under the merger's name, which breaks G2. |
| C10 | **Domain-separated hashing.** Every hash input starts with a one-byte tag. The Merkle tree is the RFC 6962 tree with leaf prefix `0x00` and node prefix `0x01`. | T3 and Lemma L4 compare hashes of different kinds. Tags make those comparisons collision arguments. |
| C11 | **The commit ID includes the number of events `m`.** | T3 compares event chains of possibly different lengths. |
| C12 | **The empty state is well-formed**, and the first event must create the root. | T1 starts its induction from the empty state. |
| C13 | **Split and Join no longer require the nodes to be adjacent.** Adjacency is the editor's concern. | Checking adjacency reads all siblings, which makes conflicts global. |
| C14 | **Assets must exist before use.** `Create` and `SetField` may reference an asset hash only if it is already in `B`. | This is part of invariant W8. |

---

## 2. The model as used in the proofs

### 2.1 Slots

A state is a total assignment of values to **slots**. A slot that was never
written holds its default value (`absent`, `false` or `⊥`).

| Slot | Value | Written by |
|---|---|---|
| `ex(x)` | exists, plus class | `Create`, `Split` (new node) |
| `root` | the root ID or `⊥` | `Create` of the root |
| `place(x)` | `(parent, pos)` | `Create`, `Split`, `Move` |
| `live(x)` | `𝔹` | `Create`, `Split`, `Delete`, `Restore`, `Join` |
| `fld(x, k)` | `Val ∪ {⊥}` | `Create`, `Split`, `SetField` |
| `exc(c)` | exists, plus the character `ch` | `Insert` |
| `nxt(a)` | next character ID or `⊣`, for every character `a` and every start marker `◁ₓ` | `Insert`, `MoveText`, `Split`, `Join`, `Create` (for `◁ₓ`) |
| `livec(c)` | `𝔹` | `Insert`, `Erase` |
| `sty(c, k)` | `Val` or default | `Insert`, `Format` |
| `ast(h)` | `Bytes` or `⊥` | `PutAsset` |
| `com(m)`, `res(m)` | comment record; resolved flag | `Comment`; `Comment`, `Resolve` |
| `sug(s)`, `st(s)` | suggestion record; status | `Propose`; `Propose`, `Decide` |

The **existence slots** `ex`, `exc`, `root`, `com`, `sug` and `ast` are
**monotone**: once set, no operation changes them.

**Derived notions.**
- `Q(x)` is the chain from `◁ₓ`.
- `host(c)` is the node whose chain contains `c`.
- `x` is **visible** if `x` and all its ancestors are live.
- `view(D)` is the visible nodes with the live characters of each visible text
  node.

### 2.2 Well-formedness `WF(D)`

| # | Invariant |
|---|---|
| W1 | If any node exists, `root = r`, `r` is the only node with parent `⊥`, its kind is `root`, its class is `container`, and it is live. If no node exists, `root = ⊥`. |
| W2 | Every non-root node's parent exists and is a `container`. |
| W3 | Following `parent` from any node reaches the root (no cycles). |
| W5 | For every non-root node `x`: `(kind(parent x), kind x) ∈ G`. |
| W6 | A start marker `◁ₓ` exists exactly for text nodes `x`. |
| W7 | For each text node, the chain from `◁ₓ` reaches `⊣` without repeating. Every existing character lies on exactly one chain. |
| W8 | `ast(h) = b ≠ ⊥ ⇒ H(b) = h`. Every asset hash in any field has `ast(h) ≠ ⊥`. |
| W9 | Every comment anchor, reply target and suggestion target exists. |

(W4, "live parents", from Part 0 is replaced by the visibility notion of C1.)

### 2.3 Operations: preconditions, reads and writes

For each operation `o` and state `S` where it applies, `Dep(o, S)` is the set of
slots it **reads**, and `Wr(o)` is the set of slots it **writes**.

| Operation | Precondition (on the slots in `Dep`) | `Dep(o, S)` | `Wr(o)` and the values written |
|---|---|---|---|
| `Create(x, κ, p, π, f)` | `x` is new. Root case: `p = ⊥`, `root = ⊥`, `κ = container`, `kind = root`. Otherwise `p` is a container and `(kind p, f(kind)) ∈ G`. Assets in `f` exist. | `ex(x)`, `root`; `ex(p)`, `fld(p, kind)`; `ast(h)` for each `h` in `f` | `ex(x)`, `place(x) = (p, π)`, `live(x) = true`, `fld(x, ·) = f`; `nxt(◁ₓ) = ⊣` if `κ = text`; `root = x` in the root case |
| `Move(x, p, π, p′, π′)` | `x ≠ root`. `place(x) ∈ {(p, π), (p′, π′)}`. `p′` is a container. `(kind p′, kind x) ∈ G`. `p′` is not `x` and not below `x`. | `root`, `place(x)`, `ex(p′)`, `fld(p′, kind)`, `fld(x, kind)`, and `place(z)` for every `z` on the ancestor path of `p′` in `S` | `place(x) = (p′, π′)` |
| `Delete(x)` | `x` exists and `x ≠ root` | `ex(x)`, `root` | `live(x) = false` |
| `Restore(x)` | `x` exists | `ex(x)` | `live(x) = true` |
| `SetField(x, k, v, v′)` | `x` exists. `fld(x, k) ∈ {v, v′}`. If `k = kind`, `v` and `v′` are compatible (C4). Assets in `v′` exist. | `ex(x)`, `fld(x, k)`, `ast(h)` for each `h` in `v′` | `fld(x, k) = v′` |
| `Insert(a, c₁…cₙ)` | `a` exists (a character, or `◁ₓ` of an existing text node). All `cᵢ` are new. | `exc(a)` or `ex(x)`; `nxt(a)`; `exc(cᵢ)` | `exc(cᵢ)`, `livec(cᵢ) = true`, `sty(cᵢ, ·)`; `nxt(a) = c₁`; `nxt(cᵢ) = cᵢ₊₁`; `nxt(cₙ) = ` the old `nxt(a)` |
| `Erase(c₁…cₙ)` | each `cᵢ` exists | `exc(cᵢ)` | `livec(cᵢ) = false` |
| `Format(c₁…cₙ, k, v, v′)` | each `cᵢ` exists, and `sty(cᵢ, k) ∈ {vᵢ, v′}` | `exc(cᵢ)`, `sty(cᵢ, k)` | `sty(cᵢ, k) = v′` |
| `MoveText(cs; xs; a; xₐ)` | Each `c ∈ cs` lies on `Q(x_c)`, where the host `x_c ∈ xs` is named. `a` lies on `Q(xₐ)` or is `◁` of `xₐ`. `a ∉ cs`. | `exc` of `cs` and of `a`; every `nxt` slot on the chains of `xs ∪ {xₐ}` | `nxt` slots on those chains: remove `cs` and put them, in the given order, right after `a` |
| `Split(x, a, y, π, f)` | `x` is a text node. `a` lies on `Q(x)`. `y` is new. `(kind(parent x), f(kind)) ∈ G`. | `ex(x)`, `place(x)`, `ex(parent x)`, `fld(parent x, kind)`, `ex(y)`, every `nxt` slot on `Q(x)` | `y`'s slots (`ex`, `place = (parent x, π)`, `live = true`, `fld = f`); the `nxt` slots that cut `Q(x)` before `a` and start `Q(y)` at `a` |
| `Join(x, y)` | `x ≠ y`, and both are text nodes | `ex(x)`, `ex(y)`, every `nxt` slot on `Q(x)` and `Q(y)` | `nxt` slots that append `Q(y)` to `Q(x)` and empty `Q(y)`; `live(y) = false` |
| `PutAsset(h, b)` | `H(b) = h` | none | `ast(h) = b` |
| `Comment(m, anchor, r, body)` | `m` is new; anchor and `r` exist | existence slots of `m`, the anchor and `r` | `com(m)`, `res(m) = false` |
| `Resolve(m, b)` | `m` exists | `com(m)` | `res(m) = b` |
| `Propose(s, t, P)` | `s` is new; `t` exists | `sug(s)`, existence of `t` | `sug(s)`, `st(s) = open` |
| `Decide(s, d, reason)` | `st(s) ∈ {open, d}` | `st(s)` | `st(s) = d` (and the reason) |
| `Adopt(e, i)` | the precondition of operation `i` of event `e` | as that operation | as that operation |

Every written value is either a constant from the operation's own data or a
function of the values in `Dep(o, S)`. `Adopt` is used only in merge
resolutions (Section 8).

### 2.4 Events, commits and hashes

Events and commits are as in Part 0, Sections 5–6, with C9–C11. Hashing uses
one-byte tags:

```
event hash     H("E" ‖ enc(e))
chain step     hᵢ = H("L" ‖ H("E" ‖ enc(eᵢ)) ‖ hᵢ₋₁),   h₀ = id(first parent) or 0²⁵⁶
commit ID      id(k) = H("K" ‖ enc(P) ‖ enc(m) ‖ hₘ ‖ r ‖ q ‖ enc(α, ρ, t, message))
Merkle tree    RFC 6962: MTH(∅) = H(""), MTH([d]) = H(0x00 ‖ d),
               MTH(D) = H(0x01 ‖ MTH(D[0:j]) ‖ MTH(D[j:n])), with j the largest power of 2 below n
content root   R(D) = MTH(sorted leaves "N"‖enc(x, node slots), "C"‖enc(c, char slots), "S"‖enc(◁ₓ, nxt), "A"‖h)
annotation root A(D) = the same over comment and suggestion slots
```

### 2.5 Assumptions

| # | Assumption |
|---|---|
| A1 | Nobody can find `u ≠ v` with `H(u) = H(v)` (collision resistance), or any `u` with `H(u) = 0²⁵⁶` (preimage resistance). |
| A2 | Every ID that an operation creates (`Create`, `Split`, `Insert`, `Comment`, `Propose`) is generated **once**. No two operation occurrences anywhere create the same ID. |
| A3 | The recorder `ρ` of every event is authenticated. A verified author `α ∈ Actor` was authenticated when the event was first recorded. |
| A4 | `enc` is injective, and every implementation produces the same bytes. |
| A5 | Any two commits that are merged have a unique lowest common ancestor. |
| A6 | (Only for T1(b).) The author's client implements `apply` and `merge` as specified here. |

---

## 3. Lemmas about single operations

### L1. Preservation

> If `WF(D)` and the precondition of `o` holds at `D`, then `WF(apply(D, o))`.

*Proof.* Check each invariant against each operation. Only the operations
that write the slots an invariant mentions can break it.

- **W1 (root).** Only the root case of `Create` writes `root`, and it requires
  `root = ⊥`, so the root is unique. No operation sets a parent to `⊥`: `Move`
  writes a container `p′`, and `Split` writes `parent x`, which is not `⊥`
  because `x` is a text node and the root is a container. `Delete` refuses the
  root, so the root stays live. `Move` refuses the root, so it keeps parent `⊥`.
- **W2 (parents exist and are containers).** `Create` and `Move` require a
  container parent. `Split` uses `parent x`, which is a container by W2 at `D`.
  Classes never change.
- **W3 (no cycles).** Only `Move` changes a parent of an existing node. Let
  `T` be the set of nodes at or below `x`. For a node not in `T`, its ancestor
  path does not pass through `x`, so the path is unchanged and reaches the root.
  For a node in `T`, its path reaches `x`, then `p′`. Since `p′ ∉ T`, the path
  from `p′` is unchanged and reaches the root. New nodes from `Create` and
  `Split` are leaves under existing nodes, so they create no cycle.
- **W5 (grammar).** `Create`, `Move` and `Split` check `G` for the edge they
  create. `SetField(·, kind, …)` changes kinds only within a compatibility class
  (C4), and those kinds have identical rows and columns in `G`, so every edge
  stays allowed.
- **W6 (start markers).** `Create` of a text node writes `nxt(◁ₓ)`. `Split`
  does so for the new text node `y`. Nothing else creates start markers, and
  classes never change.
- **W7 (chains).**
  - `Insert` splices new characters, which by the precondition appear nowhere
    else, between `a` and its old successor. The chain stays finite and has no
    repeats.
  - `MoveText` removes the characters of `cs` from their chains and re-inserts
    them once each after `a`. Since `a ∉ cs`, the anchor survives the removal.
  - `Split` cuts one chain into two, so every character stays on exactly one
    chain.
  - `Join` concatenates two different chains (`x ≠ y`) and empties one.
- **W8 (assets).** `PutAsset` requires `H(b) = h`. `Create` and `SetField`
  reference only existing assets (C14).
- **W9 (anchors).** `Comment` and `Propose` require their targets to exist.
  Existence slots are monotone, so targets keep existing. ∎

### L2. Determinism

> `apply(D, o)` is a function of `D` and `o`.

*Proof.* In the table of Section 2.3, every written value is either a constant
from `o` or a function of slot values in `D`. `MoveText` places `cs` in the
order given in `o`. No clock, randomness, network or locale appears anywhere. ∎

### L3. Permanence

> No operation removes an ID. The class of a node and the character `ch` of a
> character never change.

*Proof.* Existence slots, including `exc(c)`, which holds `ch`, are written
only when the ID is created. The precondition "is new" ensures this happens
once per state. No operation writes `false` or `⊥` into an existence slot. ∎

### L4. Root binding

> If `R(D) = R(D′)` (or `A(D) = A(D′)`), then the content (or annotations) of
> `D` and `D′` are equal, unless a collision of `H` is found.

*Proof.* First, `MTH(L) = MTH(L′)` implies `L = L′` or a collision. The proof
is by induction on `|L| + |L′|`, comparing the hash inputs:

- **Both lists have at least 2 entries.** Both inputs have the form
  `0x01 ‖ left ‖ right`, with 32-byte halves. Either the inputs are equal, or
  this is a collision. If they are equal, the left halves are equal and the
  right halves are equal. Apply induction to each half, which gives `L = L′`.
- **One list has 1 entry and the other at least 2.** The inputs start with
  `0x00` and `0x01`, so they differ, and this is a collision.
- **Both lists have 1 entry.** The inputs are `0x00 ‖ d` and `0x00 ‖ d′`, so
  either `d = d′` or this is a collision.
- **One list is empty.** `H("")` is compared with a non-empty input, so this is
  a collision.

Second, the leaves are the tagged encodings of all slots, sorted. `enc` is
injective (A4) and tags separate the kinds of leaves. So equal leaf lists mean
equal slot values. ∎

### L5. Locality

> The precondition of `o` and the values it writes depend only on the values of
> the slots in `Dep(o, S)`. Applying `o` changes only slots in `Wr(o)`.

*Proof.* Read the table of Section 2.3 row by row. Each precondition and each
written value mentions only slots listed in `Dep`. ∎

### L6. Self-determination

> If `S` and `S′` give the same values to all slots in `Dep(o, S)`, then
> `Dep(o, S′) = Dep(o, S)`.

*Proof.* Most rows list fixed slots, named by IDs in `o`. Three rows list slots
that depend on the state:
- **`Move`:** the ancestor path of `p′`. It is found by reading `place(p′)`,
  then `place` of that parent, and so on. If those slots have the same values in
  `S′`, the walk visits the same nodes.
- **`Split`, `Join`, `MoveText`:** a named chain. It is found by reading `nxt`
  from `◁ₓ` onwards. If those slots have the same values in `S′`, the walk visits
  the same slots.
- **`Split`:** `ex(parent x)` and `fld(parent x, kind)`. The parent is read
  from `place(x)`, which is in `Dep`. ∎

---

## 4. Lemmas about sequences

A **run** is a sequence of operations `σ = o₁ … oₙ` applied to a state `S`.
`Sⱼ` is the state just before `oⱼ`. The run is **valid** if each `oⱼ` meets its
precondition at `Sⱼ`. Write `Dⱼ = Dep(oⱼ, Sⱼ)`.

### L7. Last writer

> In a valid run, the final value of a slot `s` is the value written by the last
> operation whose `Wr` contains `s`. If no operation writes `s`, the value is the
> one in `S`.

*Proof.* By L5, an operation changes only its `Wr` slots, so the value of `s`
changes only at writes to `s`. ∎

### L8. Kept subsequence

Let `σ` be valid from `S`. Let `Hd ⊆ σ` be **closed**: if `oᵢ ∈ Hd`, `j > i`
and `Wr(oᵢ) ∩ Dⱼ ≠ ∅`, then `oⱼ ∈ Hd`. Let `κ = σ \ Hd`, keeping the order.

> Then `κ` is valid from `S`. Each operation in `κ` reads the same values and
> writes the same values as in `σ`. Every slot that no operation of `Hd` writes
> has the same final value after `κ` as after `σ`.

*Proof.* Run `σ` and `κ` side by side and use induction over the positions of
`σ`. The invariant is: *at each point, every slot that no `Hd` operation has
written so far holds the same value in both runs.*

- **Step with `oⱼ ∈ Hd`.** The `κ` run does nothing. `oⱼ` writes only slots in
  `Wr(oⱼ)`, and from now on these count as written by `Hd`, so the invariant
  does not mention them.
- **Step with `oⱼ ∈ κ`.** No `Hd` operation before `oⱼ` wrote a slot in `Dⱼ`;
  otherwise `oⱼ` would be in `Hd` by closure. By the invariant, the `κ` state
  agrees with `Sⱼ` on `Dⱼ`. By L6 the read set is the same, and by L5 `oⱼ` is
  valid and writes the same values in both runs. Afterwards, every slot in
  `Wr(oⱼ)` holds the same value in both runs.

At the end, a slot that no `Hd` operation wrote has the same value in both
runs. ∎

### L9. Independent runs commute

Let `σ¹` and `σ²` be valid from `S`. Write `Dⱼ¹` and `Dⱼ²` for their read sets
in their own runs. Suppose that for every `o¹ⱼ ∈ σ¹` and `o²ₗ ∈ σ²`:

1. `Wr(o¹ⱼ) ∩ D²ₗ = ∅`;
2. `Wr(o²ₗ) ∩ D¹ⱼ = ∅`;
3. on every slot in `Wr(o¹ⱼ) ∩ Wr(o²ₗ)`, the two write the same value.

> Then `σ²` is valid from `apply(S, σ¹)`, `σ¹` is valid from `apply(S, σ²)`,
> and `apply(apply(S, σ¹), σ²) = apply(apply(S, σ²), σ¹)`. Each operation reads
> and writes the same values as in its own run.

*Proof.* Consider the run `σ¹` followed by `σ²`.

- **The `σ¹` part** is exactly `σ¹`'s own run.
- **The `σ²` part.** Use induction over its operations: each `o²ₗ` reads in
  `D²ₗ` the same values as in `σ²`'s own run. By (1), no `σ¹` operation wrote a
  slot in `D²ₗ`. So, by L7, the value of each such slot is either the value in
  `S` or the value written by an earlier `σ²` operation, and those values are
  equal by induction. By L6 and L5, `o²ₗ` is valid and writes the same values.
- **The other order** follows symmetrically, using (2).
- **Final states.** By L7, consider each slot `s`:
  - if neither run writes `s`, its value is the one in `S`;
  - if one run writes `s`, its value is that run's last write;
  - if both write `s`, the two orders end with different writers. By (3), these
    two writes have the same value.

So the final states are equal. ∎

---

## 5. Theorem T1 — Reconstruction (G1)

A history is **admissible** if, for every commit `k`:
- its events are valid when applied from its start state (`D∅`, the parent's
  state, or the merge result);
- its merge base is unique (A5);
- its roots `r` and `q` were computed by the author's client from that client's
  own state.

> **T1.** In an admissible history, for every commit `k`:
> - **(a)** `state(k)` is defined, and `WF(state k)` holds.
> - **(b)** Under A6, `state(k)` equals the author's state at commit time, so
>   `view(state k)` is exactly what the author saw.
> - **(c)** If anyone rebuilds a state `D′` with `R(D′) = r` and `A(D′) = q`,
>   then `D′ = state(k)`, unless a collision of `H` is found. If a replay is
>   wrong, the root check exposes it.

*Proof.* Induction over the commits in topological order (parents before
children).

- **(a), no parents.** `D∅` is well-formed (C12). The events are valid by
  admissibility. By L1 applied at each operation, `state(k)` is well-formed.
- **(a), one parent.** By induction, `state(parent)` is well-formed. Then use L1
  as above.
- **(a), two parents.** The merge result is defined and well-formed by T4(b)
  (Section 8). The resolution events are valid by admissibility, so use L1.
- **(b).** The author's client started from the same start state (induction)
  and applied the same operations. By L2 and A6 it reached the same state. For a
  merge, `merge` is a deterministic function of `state(b)` and the two sides
  (Section 8.2), so the same argument applies.
- **(c).** By (b), `R(state k) = r` and `A(state k) = q`. If `R(D′) = r`, then
  L4 gives `D′ = state(k)` in content, unless there is a collision. The same
  holds for annotations. ∎

**Why (c) is not circular.** `r` comes from the author's client, and `R(D′)`
comes from whoever rebuilds the state. These are two independent computations,
so if the server or a second implementation computes even one slot
differently, the roots differ.

---

## 6. Theorem T2 — Attribution (G2)

For a commit `k`, `Hist(k)` is the set of events in `k` and all its ancestors.
An **insertion of `c`** is an occurrence of `Insert(…, c, …)` inside an event of
`Hist(k)`. `Adopt` does **not** count as an insertion: it applies an existing
one.

> **T2.** Let `c` be a character of `state(k)`. Then:
> - **(a)** exactly one event `e_c ∈ Hist(k)` contains an insertion of `c`;
>   define `writer(c) = α(e_c)`;
> - **(b)** `ch(c)` is the character `e_c` inserted, and it has never changed;
> - **(c)** for every later commit `k′` whose state contains `c`, `writer(c)` is
>   the same. In particular, `Move`, `Split`, `Join`, `MoveText`, merges and
>   `Adopt` never change it;
> - **(d)** under A3, if `writer(c) ∈ Actor`, it is the authenticated person who
>   made the edit. If it is a `Claim`, it is the name the source gave, and
>   nothing more.

*Proof.*

**(a), existence.** Only `Insert` writes `exc(c)` (Section 2.3).
`state(k)` is built only by applying operations of events in `Hist(k)`, either
directly, through a merge (which applies operations of both sides, all in
`Hist(k)`), or through `Adopt` (which applies an operation of an event in
`Hist(k)`). So some event in `Hist(k)` contains an insertion of `c`.

**(a), uniqueness.** Suppose two different occurrences insert `c`.
- If they are in the same event, the precondition "all `cᵢ` new" fails for the
  second one, because by L3 `c` already exists.
- If one is applied while computing a state from which the other is later
  applied, the same argument holds.
- Otherwise they are on different branches, and the same ID was generated
  twice, which contradicts A2.

**(b).** `ch(c)` lives in `exc(c)`. This slot is written only by the insertion
(L3).

**(c).** `writer(c)` depends only on `c` and the unique event `e_c`. Also
`Hist(k) ⊆ Hist(k′)` for every later `k′`, and by (a), applied at `k′`, `e_c`
is still the unique inserting event. The operations listed do not write
`exc(c)` (Section 2.3). They change only `nxt`, `place` and `live` slots, so
the character that the reader sees after a split or move **is the same `c`**.
A merge does not re-record events: it applies the original operations, so their
events and authors are unchanged (Part 0, Section 6.2). `Adopt` applies an
original operation by reference (C9).

**(d).** This is A3 applied to the event `e_c`, whose author field was set
when the event was first recorded and is never rewritten. By (a) the event is
unique, and by T3 any change to it is detected. ∎

**What T2 does not say.**
- **Pasting** text from outside, or from another document, creates new
  characters. Their writer is the person who pasted. To credit an earlier
  writer, an optional `copied_from` link would be needed.
- **Retyped** text is new text. Its writer is the person who retyped it.
- **Cut and paste inside a document** keeps authorship only if the editor
  records it as `MoveText`. If it records erase plus insert, the writer is the
  person who pasted. This is an obligation on each editor integration.
- **External editors** do not carry character IDs. Text captured through sync
  is attributed to the syncing person at capture granularity (Part 0,
  Section 10).

---

## 7. Theorem T3 — Integrity (G3)

> **T3.** Let `k` be a commit in history `𝓗` and `k′` a commit in history
> `𝓗′`, with `id(k) = id(k′)`. Then either:
> - **(i)** `k` and `k′` have the same parents, the same events in the same
>   order, the same roots and the same metadata, and recursively all their
>   ancestors are identical; or
> - **(ii)** a collision of `H` can be computed from `𝓗` and `𝓗′`.

*Proof.* Induction on the depth of `k` in `𝓗`: the length of the longest path
from `k` to a commit without parents.

1. **Commit level.** The inputs of `id(k)` and `id(k′)` both start with `"K"`.
   If they differ, this is a collision. So assume they are equal. `enc` is
   self-delimiting (A4), so the input splits uniquely into
   `enc(P), enc(m), hₘ, r, q, enc(meta)`. All six parts are therefore equal:
   same parent IDs in the same order, same number of events `m`, same chain
   value, same roots and same metadata.
2. **Chain level.** Both chains have length `m`, and `h₀ = h′₀` because both are
   determined by `P`. For `i = m` down to `1`, `hᵢ = h′ᵢ` compares two inputs
   starting with `"L"`, so either they are equal or this is a collision. If they
   are equal, `H("E" ‖ enc eᵢ) = H("E" ‖ enc e′ᵢ)` and `hᵢ₋₁ = h′ᵢ₋₁`. Then
   again, either `enc eᵢ = enc e′ᵢ`, so `eᵢ = e′ᵢ` by A4, or this is a
   collision. So all events are equal, in order. This includes their authors,
   recorders, times and operations.
3. **Parents.** The parent IDs are equal. By induction (each parent has smaller
   depth in `𝓗`), each parent and its ancestors are identical, or a collision is
   found. ∎

**Corollary (detection).** A verifier who holds a trusted `id(k)`, for example
one published or signed at a release, can check any copy of the history.
Changing, adding, removing or reordering any event, root, metadata field or
parent link anywhere at or before `k` changes `id(k)`. The only alternative is
finding a collision (A1). Adding a comment changes `q` and so `id(k)`, but it
does not change `r`.

**What T3 does not say.** It gives tamper **evidence**, not prevention. Someone
who can rewrite the whole history can compute a new, different `id(k)`. That is
detected only against a trusted copy of the old ID. Signing or publishing
commit IDs is outside this proof.

---

## 8. Theorem T4 — Safe merge (G4)

### 8.1 Setting

Let `b` be the merge base of `k₁` and `k₂`. In this part, each **side** is
linear from `b`: a chain of single-parent commits. The general case is
discussed in Section 9. Let `σⁱ` be the operations of the events on side `i`,
in order. By T1(a), `σⁱ` is valid from `state(b)`, and
`apply(state b, σⁱ) = state(kᵢ)`.

### 8.2 The merge procedure

1. **Interference.** `o¹ ∈ σ¹` and `o² ∈ σ²` **interfere** if any of
   conditions (1)–(3) of L9 fails for them, with read sets taken from their own
   runs. Let `C₀` be the set of all operations that interfere with some
   operation on the other side.
2. **Hold.** For each side, let `Hdⁱ` be the smallest set that contains
   `C₀ ∩ σⁱ` and is closed in the sense of L8, within `σⁱ`. Let `κⁱ = σⁱ \ Hdⁱ`.
3. **Merge result.** `M = apply(apply(state b, κ¹), κ²)`.
4. **Report** `K`, which contains:
   - every interfering pair from step 1, with the slot involved;
   - every held operation from step 2, with the conflict it depends on;
   - every **visibility flag**: a kept operation that wrote to a node or
     character which is visible in its own side's state but invisible in `M`.
     This is how "you edited it, the other side deleted it" is reported.
5. **Resolve.** The merge commit `k` has `state(k) = apply(M, E)`, where `E`
   contains only resolution events, which may use `Adopt`.

The procedure is a deterministic function of `state(b)`, `σ¹` and `σ²`. This
is what T1(b) needs.

### 8.3 The theorem

> **T4.** With `M` and `K` as above:
> - **(a) Nothing is silently lost.** For every slot `s`, write `v_b`, `v₁`,
>   `v₂`, `v_M` for its value in `state(b)`, `state(k₁)`, `state(k₂)` and `M`.
>   If no held operation writes `s`, then `v_M = vᵢ` when side `i` writes `s`
>   (and `v₁ = v₂` when both do), and `v_M = v_b` when neither does. If a held
>   operation writes `s`, that operation is listed in `K`.
> - **(b) Nothing is corrupted.** `M` is well-formed.
> - **(c) Neutral cases.** If one side has no operations, `M` is the other
>   side's state and `K = ∅`. Merging a commit with itself gives that commit's
>   state. If both sides delete the same node or erase the same character, that
>   is not a conflict.
> - **(d) Symmetry.** Swapping the sides gives the same `M` and the same `K`.
> - **(e) No interleaving.** No character from one side ends up inside a run of
>   characters that one `Insert` placed on the other side.

### 8.4 Proof

**Step 1: the kept parts are valid.** `Hdⁱ` is closed, so by L8, `κⁱ` is valid
from `state(b)`. Each kept operation reads and writes exactly what it did in
`σⁱ`. So its read sets are the same as in `σⁱ` (L6).

**Step 2: the kept parts are independent.** Take `o¹ ∈ κ¹` and `o² ∈ κ²`.
Neither is in `C₀`, so they do not interfere. Their read sets in the kept runs
are those of their own runs (step 1), so conditions (1)–(3) of L9 hold for
`κ¹` and `κ²`. By L9, `κ²` is valid after `κ¹`, the two orders give the same
state, and every kept operation reads and writes what it did on its own side.

**(b).** `M` is reached from the well-formed `state(b)` (T1(a)) by valid
operations (steps 1–2). By L1 at each operation, `M` is well-formed.

**(a).** Take a slot `s` that no held operation writes.
- By L8, after `κⁱ` it has its value from `state(kᵢ)`.
- If only side `i` writes `s`, then by step 2 and L7, its value in `M` is side
  `i`'s last write, which is `vᵢ`.
- If neither side writes `s`, its value is `v_b`.
- If both sides write `s`: every cross pair of writes to `s` has equal values,
  since otherwise both operations would be in `C₀` and held. So the last writes
  agree, and `v₁ = v₂ = v_M`.

Any held operation is in `K` by step 4 of the procedure.

**(c).** If `σ²` is empty, then `C₀ = ∅`, nothing is held, and
`M = apply(state b, σ¹) = state(k₁)`. The visibility check compares `M` with
`state(k₁)` itself, so it finds nothing. If `k₁ = k₂`, the merge base is `k₁`
itself, both sides are empty, and `M = state(k₁)`. `Delete` and `Erase` read
only existence slots and write a constant. Two of them on the same target
therefore write the same value, read nothing the other writes, and do not
interfere.

**Note.** Two equal `SetField` or `Format` edits *do* interfere, because each
reads the slot the other writes. T4 reports them as a conflict. This is
conservative, not unsafe (Section 9).

**(d).** Interference is defined symmetrically, so `C₀` does not depend on
which side is first. `Hdⁱ` depends only on side `i`. `M` is the same in both
orders by L9. `K` is built from symmetric parts.

**(e).** Let an `Insert` on side 1 place run `r` after anchor `a`. Putting
another character between two characters of `r` requires writing `nxt(c)` for
some `c ∈ r`. Characters of `r` are new on side 1 (A2), so no operation on side
2 names them. The only side-2 operations that write chain slots near `r` are:
- inserts at the same anchor, which write `nxt(a)`;
- `MoveText`, `Split` and `Join` on `r`'s host, which read the whole chain.

Both kinds write `nxt(a)`, or read the chain containing `a`, so they interfere
with the insert of `r` and are held. So in `M`, `r` is exactly as side 1 left
it, except for later side-1 operations. ∎

**What "conflict" means here.** The procedure is **conservative**. It reports a
conflict whenever it cannot *prove* that both changes are independent. Some
reported conflicts would have been safe to merge. Examples:
- both sides insert at the same spot;
- one side types in a paragraph that the other side split.

T4 guarantees safety: nothing is lost or corrupted without a report. It does
not guarantee that every safe merge happens automatically. That is a separate
quality goal.

---

## 9. What is not proved

| Gap | Status |
|---|---|
| **Sides with their own merges** (non-linear sides) | Not proved. The plan: show that each commit's state is reached from any ancestor's state by one valid run, then reuse Section 8. This needs a "path linearization" lemma. |
| **Criss-cross merges** (more than one lowest common ancestor) | Excluded by A5 |
| **Automatic ordering of same-anchor inserts** (RGA) | Not used: they are conflicts (C6). Adding RGA ordering would need RGA's known proofs (Isabelle/HOL; Gomes, Kleppmann, Mulligan and Beresford, 2017) adapted to `Split`, `Join` and `MoveText`. |
| **Finer split-versus-typing merges** | Reported as conflicts today |
| **Equal field and format edits on both sides** | Reported as conflicts today. With C3, a refined interference rule (a shared slot is allowed when both sides write the same value and each precondition accepts that value) should let them merge; this is not yet proved. |
| **G5: external editors** | Depends on F1 and F2 per editor; must be measured |
| **The code** | These proofs are about the model. The Rust and TypeScript code must be shown to follow it: property tests per lemma, and shared fixtures for A4. |
| **Resolution events** | `Adopt` and other resolutions must be valid when applied. That is checked at merge time; it is not proved in advance. |
| **Signatures and anchoring of commit IDs** | Outside T3 |

### How to test each lemma

| Lemma | Property test |
|---|---|
| L1 | Random valid operation sequences → `WF` after every step |
| L2 | Apply the same run twice → identical states |
| L4 | Random states that differ in one slot → different roots |
| L5 and L6 | For each operation: change a slot outside `Dep` → same validity and same writes |
| L8 | Random run and random closed `Hd` → kept run valid, same writes |
| L9 | Random independent pairs of runs → both orders equal |
| T4 | Random two-branch histories → (a)–(e) checked directly |

---

## 10. The proofs in one paragraph

Every operation reads and writes a **known, small set of slots**. That fact
gives replay that is deterministic and keeps the rules (T1). Characters keep
their IDs through every structural operation, and merges never copy events, so
each character has exactly one inserting event and one author (T2). Hashes
chain every event, root and parent into the commit ID, so changing anything
needs a collision (T3). In a merge, operations that touch the same slots are
held back and reported, and the rest commute, so the result keeps every
unreported change and stays valid (T4).
