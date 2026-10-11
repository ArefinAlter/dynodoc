# Provable Provenance for Documents — Part 2: The Remaining Proofs

**Status:** internal research draft, 10 October 2026. Do not publish. The patent
items in `docs/OPEN-RESEARCH-REGISTER.md` (L2–L3) still apply.

Part 1 ([`PROVENANCE-PROOFS.md`](PROVENANCE-PROOFS.md)) proved G1–G4 and listed
what was still open in its Section 9. This part closes those items, and two
items from Part 0 that Part 1 did not cover: reversibility (L5) and external
editors (G5).

| Open item (Part 1 §9 or Part 0) | Result here |
|---|---|
| Sides with their own merges | **Proved** (Section 3: prefix-based symmetric merge, and Section 4: adopting merge) |
| Criss-cross merges (more than one lowest common ancestor) | **Proved** (Section 4: the adopting merge needs no merge base) |
| Equal field and format edits on both sides | **Proved** to merge without a conflict (Section 5) |
| Automatic ordering of same-spot inserts | **Proved** for a deterministic "segment" rule (Section 6). RGA is not needed. |
| Typing in a paragraph the other side split | **Proved** to merge, with a gap-local `Split` (Section 7) |
| Resolution events | **Proved** safe: they use the same clean-adoption check (Section 4.5) |
| Signed commit IDs | **Proved** as a corollary of T3 (Section 8) |
| L5 reversibility (Part 0) | **Proved** (Section 9) |
| G5 external editors (Part 0) | **Proved under stated assumptions** about the editor (Section 10) |
| The Rust and TypeScript code follow the model | **Not provable on paper.** The current code does not implement the model yet. This is a testing and verification task (Section 11). |

Everything in Part 1 (definitions, slots, assumptions A1–A6, lemmas L1–L9) is
used unchanged unless Section 1 says otherwise.

---

## 1. More model changes

| # | Change | Needed for |
|---|---|---|
| C15 | **Write records.** Every operation occurrence stores, for each slot it writes, the pair `(old, new)`: the value before and after, in the run where it was first applied. | The adopting merge (Section 4) and reversibility (Section 9) |
| C16 | **Gap-local `Split`.** `Split(p, a, y, q, π, f)` cuts the chain between `p` and `a = nxt(p)`. It does not read the rest of the paragraph (Section 7). | Typing and splitting the same paragraph (Section 7) |
| C17 | **`Unjoin(l, f, y)` and `Unerase(cs)`** are added as exact inverses of `Join` and `Erase`. `Unerase` reads only existence slots and sets `livec = true`. | Reversibility (Section 9) |
| C18 | **Each merge commit records its mode**, `S` (symmetric) or `A` (adopting), in its metadata, so the mode is hashed. | T1 determinism and T3 |
| C19 | **Sync de-duplicates IDs.** If an editor reports the same node ID twice, the first occurrence in document order keeps it, and the others get new IDs. | G5 (Section 10) |
| C20 | **Stronger hold rule.** In the merge procedure (Part 1 §8.2, step 2), the closure also holds a later operation that **writes** a slot an earlier held operation wrote. Any set closed under this rule is also closed under Part 1's rule, so Part 1's theorems still hold. More operations may be reported, which is safe. | L11 |
| C21 | **Transformed occurrences keep their actual writes.** When a merge executes an occurrence in a changed form (the renaming or substitution of Section 6), the merge commit stores the writes it actually made, and those writes are used as its write record from then on. | L10 and L11 |

C15 adds data only. It changes no precondition used in Part 1, so Part 1's
proofs are unaffected.

---

## 2. Runs of commits

### L10. Every commit has a run

> For every commit `k` there is a sequence `run(k)` of distinct operation
> occurrences, valid from `D∅`, with `apply(D∅, run(k)) = state(k)`.

Define `run(k)` by recursion:

| Commit | `run(k)` |
|---|---|
| no parents | the operations of `E`, in order |
| one parent `p` | `run(p)`, then the operations of `E` |
| merge, mode S (Section 3) | `run(b)`, then `κ¹`, then `κ²` (with the renaming of Section 6, if used), then `E` |
| merge, mode A (Section 4) | `run(k₁)`, then the adopted occurrences, then `E` |

*Proof.* By induction over commits in topological order.
- **Validity and result.** The no-parent and one-parent cases are the
  definition of `state(k)`. For mode S, Part 1 L8 and L9 (and Section 6 here)
  show that `κ¹` and then `κ²` are valid from `state(b) = apply(D∅, run(b))`.
  Mode A is valid by its definition (Section 4).
- **Distinctness.** Each case appends only occurrences that are not already in
  the run:
  - `E` belongs to the new commit;
  - `κⁱ` comes from side `i`'s remainder, and Section 3 requires the two
    remainders to be disjoint;
  - mode A adopts only occurrences outside `Applied(k₁)`. ∎

`Applied(k)` is the set of occurrences in `run(k)`.

**Remark.** Adopted occurrences keep their original identity (event and
index). Part 1 T2 therefore applies unchanged: each character still has
exactly one inserting occurrence.

### L11. Every occurrence in a run is clean

An occurrence `o` is **clean** at state `S` when:
- its precondition holds at `S`;
- every slot `s` it writes holds `old_o(s)` or `new_o(s)` at `S`;
- the values it would write at `S` are exactly its recorded `new_o`.

> Every occurrence is clean at its own turn in `run(k)`, for every `k`.

*Proof.* By induction over commits.
- **Inside an event,** the occurrence is applied in the context where its write
  records were taken, so it is clean by definition of the records.
- **Mode S kept runs.** By L8 and L9 (and L9′ of Section 5), every kept
  occurrence reads the same values and writes the same values as in its own
  run. Consider a slot it writes:
  - by C20, no earlier **held** operation on its side wrote that slot, so on
    its own side the slot holds its own-run old value;
  - any write to that slot by the **other** side has the same value as this
    occurrence's own write (otherwise they interfere), so the slot holds
    `new`.

  Either way the occurrence is clean.
- **Transformed occurrences** (Section 6) are clean with respect to the write
  records stored by C21.
- **Mode A** applies only clean occurrences, by definition. ∎

---

## 3. Symmetric merge with merges inside the sides

Part 1 required both sides to be linear. Only one property of linear sides
was actually used.

**Mode S applies to `k₁, k₂` with base `b` when:**
- `run(b)` is a prefix of both `run(k₁)` and `run(k₂)`;
- the two remainders `σ¹, σ²` (the parts after `run(b)`) share no occurrence.

> **T4-S.** If mode S applies, Part 1 T4(a)–(e) hold for the merge of `k₁` and
> `k₂` over `b`, with `σⁱ` as above.

*Proof.* Part 1's proof of T4 used linear sides only to get: `σⁱ` is valid
from `state(b)`, and `apply(state b, σⁱ) = state(kᵢ)`. By L10,
`apply(D∅, run(kᵢ)) = state(kᵢ)`, and `run(kᵢ) = run(b) · σⁱ` with
`apply(D∅, run(b)) = state(b)`. So both facts hold. Disjointness makes `κ¹`
and `κ²` distinct occurrences, as the proof assumes. The rest of Part 1
Section 8.4 is unchanged. ∎

Linear sides satisfy the condition. So do sides that contain fast-forwards,
mode A merges, and mode S merges that held nothing on their first-parent
side. When the condition fails, Section 4 applies.

---

## 4. The adopting merge (any graph, no merge base needed)

### 4.1 Procedure

To merge `k₂` into `k₁` (mode A):

1. Let `τ` be `run(k₂)` restricted to the occurrences **not** in `Applied(k₁)`,
   in `run(k₂)` order.
2. Start with `S = state(k₁)`. For each `o` in `τ`:
   - if `o` is clean at `S`, apply it and add it to the result;
   - otherwise, skip it and add it to the report `K`.
3. Add a visibility flag to `K` for every adopted occurrence that wrote to a
   node or character which is visible in `state(k₂)` but invisible in the
   result `R`.
4. The merge commit has `state(k) = apply(R, E)`. Every resolution in `E` must
   itself be clean.

No merge base is used, so criss-cross histories and sides with any internal
structure are covered.

### 4.2 Theorem

> **T4-A.** For the adopting merge:
> - **(a) Nothing is silently lost.** Every occurrence of `τ` is either applied,
>   writing exactly its recorded values, or listed in `K`.
> - **(b) Nothing is corrupted.** `R` is well-formed.
> - **(c) No blind overwrite.** If a slot `s` has a different value in `R` than
>   in `state(k₁)`, then every adopted occurrence that wrote `s` found `s`
>   holding its recorded old value or its new value. So it replaced only a value
>   its author had seen, never a value the author did not know about.
> - **(d) Attribution.** Part 1 T2 holds for the merge commit.
> - **(e) Already merged.** If `Applied(k₂) ⊆ Applied(k₁)`, then `τ` is empty and
>   `R = state(k₁)`.
> - **(f) Fast-forward.** If `run(k₁)` is a prefix of `run(k₂)`, every occurrence
>   of `τ` is clean, nothing is reported, and `R = state(k₂)`.
> - **(g) Determinism.** `R` and `K` are functions of `state(k₁)`, `run(k₂)` and
>   `Applied(k₁)`.

### 4.3 Proof

- **(a)** follows from step 2: each occurrence either passes the clean test and
  is applied (and cleanness says its writes equal `new_o`), or fails and is
  listed.
- **(b)** Each applied occurrence meets its precondition. `state(k₁)` is
  well-formed by Part 1 T1(a). Then use Part 1 L1 for each step.
- **(c)** By L7 (last writer), if `s` changed, some adopted occurrence wrote
  it. At the moment each adopted writer of `s` was applied, it was clean, so `s`
  held `old_o(s)` or `new_o(s)`.
- **(d)** Adoption applies original occurrences by reference, so the T2 proof
  of Part 1 (existence, uniqueness, stability) applies to `run(k)` unchanged.
- **(e)** `τ` is empty by definition, so `R = state(k₁)`.
- **(f)** Here `τ` is exactly the rest of `run(k₂)` after `run(k₁)`, and
  `S` starts at `state(k₁) = apply(D∅, run(k₁))`. At each step, `S` equals the
  state at that point of `run(k₂)`. By L11, each occurrence is clean there. So
  everything is applied and `R = apply(D∅, run(k₂)) = state(k₂)`.
- **(g)** Every step is a function of its inputs (L2, and the clean test
  compares fixed recorded values). ∎

### 4.4 Why recorded old values detect conflicts

**Example.** Base value `v₀`. Side 1 sets `v₁`, and side 2 sets `v₂`. Side 2's
occurrence recorded `old = v₀`. In `state(k₁)` the slot holds `v₁`, which is
neither `v₀` nor `v₂`, so it is not clean, and it is reported.

**The same slot, changed to the same value on both sides,** is clean (it holds
`new`), so it is not a conflict.

**A side that changed a slot and changed it back** leaves `v₀`. A later
adoption is then clean, and that is correct: the net effect of that side on
the slot was nothing.

### 4.5 Resolution events

A resolution is either an `Adopt` of a held or skipped occurrence, or a new
event written by the merger. Both pass the clean test before they are applied,
so T4-A(b) and (c) extend to `state(k)`. A new event written by the merger is
attributed to the merger, which is correct: the merger wrote it.

**Mode S or mode A?** Mode S is symmetric and holds back both sides of a
conflict. Mode A favours `k₁`: on a contested slot, `k₁`'s value stays and
`k₂`'s change is reported. Both are safe. The mode is recorded in the commit
(C18).

---

## 5. Equal edits on both sides

### 5.1 Absorbing operations

An occurrence `o` **absorbs** a set of slots `T ⊆ Wr(o)` when the following
holds. Take any state `S` where `o` is valid, and change each slot of `T` to
the value `o` writes there; call the result `S′`. Then `o` is still valid at
`S′`, `Dep(o, S′) = Dep(o, S)`, and `o` writes the same values at `S′`.

**Which operations absorb their written slots** (using C3):

| Operation | Absorbs | Why |
|---|---|---|
| `SetField`, `Format`, `Decide` | yes | The precondition accepts old **or** new. The read set is fixed. The written value is a constant. |
| `Delete`, `Restore`, `Erase`, `Resolve` | yes | They read only existence slots and write constants. |
| `Move(x, …, p′, π′)` | yes | The precondition accepts the new place. The ancestor path of `p′` does not contain `x`, since `p′` is not below `x`, so the path is unchanged. |
| `Create`, `Insert`, `Split` | no | They need new IDs |
| `Join`, `MoveText` | no | Their written values depend on whole chains |

### 5.2 Refined interference

`o¹` and `o²`, on different sides, **interfere′** unless every slot that one
writes and the other reads or writes:
- is written by **both**,
- with the **same value**,
- and both absorb it.

### L9′. Independent runs commute (refined)

> L9 (Part 1) still holds when "interfere" is replaced by "interfere′".

*Proof.* Follow Part 1's proof of L9 for the run `σ¹` then `σ²`. Take `o²ₗ` and
a slot `s ∈ Dep(o²ₗ)`.
- **If no `σ¹` operation writes `s`,** the argument of L9 applies.
- **Otherwise,** every `σ¹` writer of `s` writes the value `v` that `o²ₗ` writes,
  and `o²ₗ` absorbs `s`. If an earlier `σ²` operation wrote `s`, it is the last
  writer in both runs, so the values agree. If not, the merged run holds `v`,
  while `o²ₗ`'s own run holds the base value.

So at `o²ₗ`'s turn, the merged state agrees with `o²ₗ`'s own-run state `S²ₗ` on
`Dep`, except on a set `T` of slots, where it holds `o²ₗ`'s own written values.
By absorption, `S²ₗ[T]` gives the same validity, read set and writes. By Part 1
L5, only `Dep` matters, so `o²ₗ` is valid and writes the same values.

The other order is symmetric. Final states: every slot written by both sides
receives equal values from every cross pair (otherwise they interfere′). So, by
L7, both orders end with the same value. ∎

> **Corollary (Part 1 T4(c), extended).** In mode S using interfere′, equal
> `SetField`, `Format`, `Move` and `Decide` changes on both sides merge without a
> conflict.

Part 1's T4 proof uses L9 only through its conclusion, so all of T4(a)–(e) hold
with interfere′.

---

## 6. Automatic order for inserts at the same spot

### 6.1 The rule

Let `a` be a character or start marker, and let `b₀` be `nxt(a)` in `state(b)`.
Anchor `a` is **combinable** when:

1. **(i) Only inserts touch the gap.** Every cross-side pair that interferes′
   through the slot `nxt(a)` consists of two `Insert`s anchored at `a`, and
   these pairs interfere′ through no other slot. No other operation on either
   side reads or writes `nxt(a)`.
2. **(ii) Each side's new text is one segment.** In `state(kᵢ)`, following `nxt`
   from `a` reaches `b₀`, and every character strictly between them is new on
   side `i`. Call this segment `Sᵢ`. It is non-empty, because side `i` inserted
   at `a`.

**Order.** The segment whose first character has the **larger** ID goes first.
Call it `S_f`, and the other `S_s`. The rule depends only on IDs, not on which
side is "side 1".

**Combined merge.**
- Remove the pairs of (i) from `C₀`, then compute `κ¹` and `κ²` as in Part 1.
- **If side 1's segment is first,** rename every side-2 `Insert` anchored at `a`
  to anchor `ℓ`, where `ℓ` is the last character of `S₁` after `κ¹`.
- **If side 2's segment is first,** leave side 2's inserts at `a` unchanged.
- In both cases, apply `κ¹` and then `κ²`.

### 6.2 Theorem

> **T4-G.** If every anchor named above is combinable, then:
> - the combined merge is valid and well-formed;
> - for each such anchor, the merged chain is `a → S_f → S_s → b₀`, with each
>   segment contiguous and in the order its own side had;
> - every other slot is as stated in T4(a);
> - the result is the same if the sides are swapped.

*Proof.*

**Side 1's segment first.** After `κ¹`, `nxt(ℓ) = b₀` by (ii). On side 2's own
run, the slot `nxt(a)` holds `b₀` at the start and is touched only by side 2's
inserts at `a`, by (i). In the merged run, the renamed inserts read and write
`nxt(ℓ)` instead. That slot holds `b₀` at the start of `κ²` and is touched by no
other `κ²` operation, because side 2 cannot name `ℓ`, a new character of side 1
(A2).

So the map "own-run `nxt(a)` ↦ merged-run `nxt(ℓ)`" turns side 2's own run into
its merged run, slot for slot, with the same values. L9′'s argument then applies
with this renaming. Side 2's operations are valid and write the same values (on
renamed slots). At the end:
- `nxt(a) = head(S₁)`, side 1's last write;
- `nxt(ℓ) = ` side 2's last value of `nxt(a)`, which is `head(S₂)`;
- the last character of `S₂` points to `b₀`, as on side 2.

**Side 2's segment first.** Side 2's inserts at `a` now read `nxt(a) = head(S₁)`
instead of `b₀`. The only value this changes is the successor written at the
end of `S₂`'s tail, which becomes `head(S₁)` instead of `b₀`. Every later side-2
operation reading that slot is an insert at the tail of `S₂`. It extends `S₂`
and passes the same pointer on. So again side 2's run maps to its merged run,
now with this one value substituted, and the chain is
`a → S₂ → S₁ → b₀`.

**Contiguity and order.** Inside each segment, only that side's operations
place characters (A2). So each segment keeps its own order.

**Symmetry.** The final state is described without mentioning sides: every
slot as in T4(a), plus the three pointers above, determined by `S_f` and `S_s`.
`S_f` and `S_s` are chosen by ID. So swapping the sides gives the same state.

**Well-formedness.** The result is reached by valid operations, so use Part 1
L1. Several combinable anchors are handled independently: their anchors are
different, and each renaming target is a new character of one side, so the
renamings never touch the same slot. ∎

**Non-interleaving** still holds: each side's segment is contiguous.

**When the rule does not apply** (for example, one side also split inside its
own new text), the pair stays a conflict, as in Part 1.

---

## 7. Typing in a paragraph the other side split

### 7.1 Gap-local `Split` (C16)

| Field | Content |
|---|---|
| Operation | `Split(p, a, y, q, π, f)`, where `p` is a character or start marker |
| Precondition | `nxt(p) = a ≠ ⊣`. `y` is new. `q` is a container. `(kind q, f(kind)) ∈ G`. |
| `Dep` | the existence of `p`; `nxt(p)`; `ex(y)`; `ex(q)`; `fld(q, kind)` |
| `Wr` | `nxt(p) = ⊣`; `nxt(◁_y) = a`; `ex(y)`; `place(y) = (q, π)`; `live(y) = true`; `fld(y, ·) = f` |

The editor passes the parent `q` of the split paragraph. Well-formedness does
not require `q` to be that parent, so correctness does not depend on it.

### L1, L5 and L6 for the new `Split`

- **W7:** the chain through `p` becomes `… p ⊣`, and a new chain starts
  `◁_y a … ⊣`, using the old end. Every character stays on exactly one chain.
- **W2 and W5:** `y` gets a container parent, and the grammar holds.
- **Locality and self-determination:** the read set is fixed by the IDs in the
  operation.

Part 1's L1, L2, L3, L5 and L6 therefore still hold.

### 7.2 Theorem

> **T4-T.** In mode S, a `Split(p, a, …)` on one side and an `Insert(d, …)` on
> the other side, with `d ≠ p`, do not interfere. The merged paragraph texts are
> the same as if the insert had happened first and the split second.

*Proof.* The `Split` writes `nxt(p)`, `nxt(◁_y)` and the slots of `y`. The
`Insert` reads `nxt(d)` and writes `nxt(d)` and its new characters' slots. The
two sets are disjoint because `d ≠ p` and all the other slots are new. So by L9
they commute. Check the texts in the order insert-then-split:
- if `d` comes before `p` in the chain, the inserted run comes before the cut
  and stays in the first paragraph;
- if `d` is `a` or after it, the run moves with `a` into `y`.

The order split-then-insert puts the run after `d` in whichever chain now holds
`d`. That is the same paragraph in each case. ∎

**Typing exactly at the split point** (`d = p`) still conflicts. That is
correct: the text could belong to the end of the first paragraph or the start
of the second. Two splits at different points of the same paragraph also merge,
giving three paragraphs. Two splits at the same point conflict, because both
write `nxt(p)` with different `y`.

`Join` and `MoveText` still read whole chains and stay conservative.

---

## 8. Signed commit IDs

> **T3-Sig.** Suppose a signer signs commit IDs with a signature scheme that is
> existentially unforgeable under chosen-message attack (EUF-CMA). Then a
> verifier who accepts a history, because its top commit carries a valid
> signature by that signer, accepts exactly a history the signer signed. The
> only alternatives are a forged signature or a collision of `H`.

*Proof.* Let the accepted top commit be `k′` with a valid signature on
`id(k′)`.
- If the signer never signed `id(k′)`, the signature is a forgery.
- Otherwise the signer signed `id(k′) = id(k)` for some history it held. By T3,
  the two histories are identical, or a collision is found. ∎

This turns tamper **evidence** into tamper **resistance** against anyone
without the signing key. Publishing the ID in an append-only public log (for
example, an RFC 9162 transparency log) gives the same effect.

---

## 9. Reversibility (Part 0, L5)

> **L5.** For every content operation `o` valid at a well-formed `D`, except
> creating the root, there is a sequence `o⁻¹` of operations, built only from
> `o`'s data and write record, such that
> `view(apply(apply(D, o), o⁻¹)) = view(D)`. For every operation that creates
> no new ID, the whole state is restored, not only the view.

*Proof.* The table below gives `o⁻¹` and what it restores. In each row, the
inverse is valid after `o`: its precondition reads only slots that `o` just set
to the recorded new values.

| `o` | `o⁻¹` | Restores |
|---|---|---|
| `SetField(x, k, v, v′)` | `SetField(x, k, v′, v)` | the whole state |
| `Format(cs, k, v, v′)` | `Format` back to each recorded old value | the whole state |
| `Move(x, p, π, p′, π′)` | `Move(x, p′, π′, p, π)`. Valid because `p` was `x`'s parent, so `p` is not below `x`. | the whole state |
| `Delete(x)` or `Restore(x)` | the other one if the recorded old `live` differs from the new; otherwise nothing | the whole state |
| `Erase(cs)` | `Unerase` of the characters whose recorded old `livec` was `true` (C17) | the whole state |
| `MoveText(cs, …)` | a sequence of `MoveText` putting each maximal block of `cs` back after its recorded predecessor | the whole state |
| `Join(x, y)` | `Unjoin(l, f, y)` (C17): `nxt(l) = ⊣`, `nxt(◁_y) = f`, and `live(y)` back to its recorded old value; precondition `nxt(l) = f` and `nxt(◁_y) = ⊣` | the whole state |
| `Split(p, a, y, …)` | `Join(host(p), y)`. It rejoins `a…` after `p` and kills `y`. | the view: `y` is left as an empty tombstone |
| `Insert(a, cs)` | `Erase(cs)` | the view: the characters stay as tombstones |
| `Create(x, …)`, not the root | `Delete(x)` | the view: `x` stays as a tombstone |
| `PutAsset(h, b)` | nothing | the view: unreferenced assets are not visible |

Why the "whole state" rows hold:
- **`SetField`, `Format`, `Move`:** these write back exactly the recorded old
  values. All other slots were untouched (Part 1 L5).
- **`Join` and `Unjoin`:** they write exactly the slots that `Join` wrote, with
  the old values.
- **`Split` then `Join`:** this restores `nxt(p) = a` and `nxt(◁_y) = ⊣`.

Why the "view" rows hold: tombstones and empty nodes are not part of the view. ∎

**Why only the view, sometimes.** Restoring the full state after creating an ID
would mean removing the ID. That breaks permanence (Part 1 L3), and with it G2.
Tombstones are the price of permanent identity.

---

## 10. External editors (G5)

### 10.1 Setting

The setting is Part 0, Section 10, with these definitions:

- **Observed view** `V′ = obs_X(s′)`: a tree of nodes. Each node has an ID (from
  a carrier, or new), a class, fields and, for text nodes, a string. IDs are
  de-duplicated by C19.
- **F3 (grammar).** `V′` has one root, it carries the ID of `D`'s root, and
  every edge in `V′` satisfies `G`.
- **The sync procedure `δ(D, V′)`** emits one sync event (author: the syncing
  person, or a `Claim` for an unauthenticated connector) with these operations,
  in this order:
  1. **Retype.** A node of `V′` whose ID exists in `D` with a **different class**,
     or with a kind outside its compatibility class, gets a new ID (it is
     treated as a new node).
  2. **Delete:** `Delete(x)` for every visible node of `D` that is missing from
     `V′`.
  3. **Place nodes, top-down.** Walk `V′` breadth-first from the root. For each
     node `x` with parent `p′`:
     - if `x` is new, `Create(x, …, p′, π, f)`;
     - else, if its parent or order differs, `Move(x, …, p′, π)`;
     - if `x` is tombstoned in `D`, `Restore(x)`.

     The keys `π` are chosen in increasing order of `V′`'s sibling order (`Pos`
     is dense and unbounded). Existing nodes whose order is already right keep
     their keys.
  4. **Fields:** `SetField` with recorded old values for every changed field,
     and `SetField(kind)` within a compatibility class.
  5. **Text.** For each text node, take the live string `t` of `D` and the
     observed string `t′`. Choose a longest common subsequence `L` of the
     character sequences.
     - `Erase` every live character of `t` not in `L`.
     - For each maximal block of `t′` characters not in `L`, `Insert` it with
       new IDs, anchored at the `L`-character just before it, or at `◁ₓ` if
       there is none.

### 10.2 Theorem

> **T5.** Let `D` be well-formed, with `view(D) ∈ Sup_X`, and let `V′` satisfy
> F3. Then:
> - **(a)** `δ(D, V′)` is valid at `D`.
> - **(b)** `view(apply(D, δ))` equals `V′`: same node IDs, parents, sibling
>   order, classes, fields and live strings.
> - **(c)** If `V′ = view(D)`, then `δ` is empty. With F1, a round trip with no
>   edit changes nothing.
> - **(d)** Under F2, every node the user's edit did not touch keeps its ID. If
>   its text is unchanged, every one of its characters keeps its ID.
> - **(e)** For a node whose text changed, the number of characters that keep
>   their ID is the largest possible: the length of the longest common
>   subsequence. All other characters are new, and their writer is the sync
>   event's author.

*Proof.*

**(a), deletes and restores.** These read only existence slots, and the root
is never missing from `V′` (F3). So they are always valid.

**(a), placement.** When `x` is placed under `p′`, `p′` was placed earlier
(breadth-first order), and so were all of `p′`'s ancestors in `V′`. So `p′`'s
current ancestor path is its path in `V′`, and that path does not contain `x`,
because `x` is `p′`'s child in the tree `V′`. So the `Move` cycle check passes.
The grammar check passes by F3: kinds are changed only within compatibility
classes, which have identical rows and columns in `G`. `Create` uses a new ID
(new nodes and retyped nodes get new IDs; C19 removes duplicates). Its parent is
a container by F3.

**(a), fields.** The recorded old values are the current ones.

**(a), text.** Anchors are existing characters (in `L`) or start markers. Each
anchor receives at most one run, so no two inserts share an anchor. The
inserted IDs are new. Erased characters exist.

**(b), structure.** After step 3, every node of `V′` has its `V′` parent and an
order key in `V′` order. Missing nodes are dead (step 2). So the visible tree
equals `V′`'s tree.

**(b), text.** The live characters of each text node are the `L`-characters, in
order, plus each new block inserted right after its preceding `L`-character.
Dead characters in between are invisible. That is exactly `t′`.

**(c).** If `V′ = view(D)`:
- no node is new, missing or retyped;
- parents and orders already match, so no `Move` is emitted;
- fields are equal, so no `SetField` is emitted;
- `t = t′`, and the only longest common subsequence of a string with itself is
  the identity alignment (a strictly increasing map from `{1…n}` onto
  `{1…n}` must be the identity), so no `Erase` or `Insert` is emitted.

**(d).** By F2, an untouched node keeps its carrier, so `V′` reports its old ID.
With an unchanged class and kind, step 1 keeps it. If its text is unchanged,
the argument of (c) applies to that node.

**(e).** The kept characters are exactly `L`, which has maximum length by
choice. All others are inserted with new IDs, by the sync event. ∎

### 10.3 What T5 does not say

- **When equal characters repeat,** for example `"aa"` becoming `"a"`, several
  longest common subsequences exist. Which character keeps its ID is then a
  choice, not the user's intent. Attribution at that level is "a correct
  maximum", not "the true one".
- **F1, F2 and F3 are claims about each editor.** They must be measured for each
  editor and version, as in the LibreOffice experiment. T5 says what follows
  *if* they hold.
- **Text moved inside an external editor** arrives as delete plus insert,
  because characters have no carriers. Its writer becomes the syncing person.

---

## 11. Why "the code follows the model" is not proved here

A paper proof can only say that the **model** has the guarantees. Showing that
the **code** has them needs one of these:

1. **Refinement proof.** Write the model in a proof assistant (Lean, Coq or
   Isabelle). Then prove that each Rust function implements the matching model
   operation, for example with Verus or Creusot for Rust.
2. **Model-based testing.** Implement the model as a small reference program.
   Then check with property tests that the real engine and editor produce the
   same states, roots and conflicts on random histories.

Neither can be done today, because the **current code does not implement this
model**: it has no character IDs, merges re-record events, and so on (Part 0,
Section 12). This item is therefore an engineering task:
1. build the model's reference implementation;
2. move the engine towards it, one change at a time (decision 012);
3. add a property test for each lemma (Part 1, Section 9);
4. later, optionally, carry out a machine-checked proof.

---

## 12. Status after Part 2

| Guarantee | Proved for |
|---|---|
| G1 Reconstruction | all histories, with both merge modes (T1, L10, C18) |
| G2 Attribution | all histories, with both merge modes and resolutions (T2, T4-A(d)) |
| G3 Integrity | all histories (T3); tamper resistance with signatures (T3-Sig) |
| G4 Safe merge | mode S when runs share a prefix (T4-S, with L9′, T4-G and T4-T); mode A for every pair of commits (T4-A) |
| G5 External editors | under F1–F3 (T5) |
| Reversibility | the view for every operation; the whole state for operations that create no ID (L5) |
| Code conformance | not provable on paper; reference implementation and tests needed (Section 11) |

**Assumptions still in force:** A1 (hash), A2 (unique IDs), A3 (authentication),
A4 (encoding), A6 (honest client, for T1(b) only), and F1–F3 (for G5 only). A5
(unique merge base) is **no longer needed**: mode A uses no merge base.
