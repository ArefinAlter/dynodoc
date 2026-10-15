# 10 · Referential Integrity and Semantic Merge Engine

### Claude Code prompt to start this stage

> Complete the engine's L1 (merge engine) and the integrity pass. After this stage, the Phase-0 product is feature-complete. Reference: Volume II §C.3.6 (the silent-breakage class), §D.1 (referential invariant), §C.1.3 (O(n) match via stable identity). In `crates/engine-core/`:
>
> 1. **The expression parser**: in `engine-core/src/expr.rs`, write a recursive-descent parser for ODK XLSForm-style expressions: `${var_name}`, comparison operators, arithmetic, `and`/`or`/`not`, function calls (`selected(${q1}, 'yes')`, `count(...)`, `regex(...)`, etc.). The output is an AST. Don't try to evaluate; we only need to extract referenced varNames and shape-check.
> 2. **The integrity checker** in `engine-core/src/integrity.rs`:
>    - Walk every node; for every field of type `expression` (`relevance`, `constraint`, `calculation`), parse the expression.
>    - Collect (var_name, source_node_id) pairs into a reference graph.
>    - Check every referenced `var_name` resolves to some node's `varName`. Unresolved → `IntegrityViolation::DanglingRef`.
>    - Check the graph has no cycles (depth-first search). Cycle → `IntegrityViolation::CircularRef`.
>    - Check type compatibility: if `${q1} > 5` and `q1` is type `text`, that's a `TypeMismatch`.
>    - Check choice references: if a constraint references a choice value, the choice must exist on the referenced item.
>    - Return all violations in one pass (don't stop at first error — users want to fix in batch).
> 3. **The semantic merge engine** in `engine-core/src/merge.rs` (historical: the module was unused and was removed on 15 October 2026; workspace merging lives in `workspace_merge.rs`):
>    - Three-way merge: given a common ancestor `O` and two branches `A` and `B` (each a sequence of events since `O`), produce a merged event sequence.
>    - Phase 1: identify per-node, per-field changes in each branch (using stable IDs from §D.1).
>    - Phase 2: for each node × field, classify the change pair: `BothAbsent`, `OnlyInA`, `OnlyInB`, `BothSame`, `BothDifferent`.
>    - Phase 3: auto-merge the easy cases (`OnlyInA`, `OnlyInB`, `BothSame`). Surface `BothDifferent` as a `FourCardConflict` for human decision.
>    - Phase 4: run `integrity_check` on the proposed merged state. If violations, the merge is *blocked* until the conflicts are resolved by the author.
>    - Output: `MergeResult { auto_merged_events, pending_conflicts, integrity_warnings }`.
> 4. **The four-card conflict UI hook** (the UI itself is in the frontend; this stage just exposes the data):
>    - `GET /documents/:id/conflicts` returns pending conflicts.
>    - `POST /documents/:id/conflicts/:conflict_id/resolve` accepts `{ choice: "left" | "right" | "custom", custom_value: ... }` and emits the resolution event.
> 5. Tests:
>    - Integrity test: rename a varName that's referenced elsewhere → dangling ref detected.
>    - Integrity test: a → b → c → a cycle in relevance → cycle detected.
>    - Merge test: A adds an item with `relevance="${x} > 5"`; B renames varName x → y. The naive merge produces a dangling ref; the integrity pass blocks the merge until B's rename is resolved.
>    - Merge test: A edits prompt of item 7; B edits constraint of item 7. Both auto-merge (different fields).
>    - Merge test: A edits prompt of item 7 to "Foo"; B edits prompt of item 7 to "Bar". Four-card conflict.

### Why the integrity pass is the load-bearing defense

The §C.3.6 critique of git-line-merge-on-JSON had two parts: spurious conflicts (annoying) and silent semantic breakage (dangerous). Spurious conflicts are mitigated by per-field semantic merge. Silent breakage is mitigated by *this* — the referential integrity pass that runs on every merge and every `Deployed` event, refusing to commit until the form is internally coherent.

Concretely:
- An author renaming a variable can't break references elsewhere; the rename event must include all references being updated atomically, or the integrity check fails.
- A merge that would produce a dangling reference is blocked, not silently broken.
- A `Deployed` event on a state with circular relevance is refused (such a form would loop at runtime).

This is the property that lets users trust the system with high-stakes work. They can branch, experiment, and merge knowing that *the system itself enforces the invariants the form must satisfy to be runnable*.

### The expression parser is small and important

Don't try to handle every ODK function on day one. Start with the subset that real-world questionnaires use: `${var}`, comparison ops, arithmetic, boolean, `selected()`, `count()`, `if()`. Add more on demand. The AST is straightforward:

```rust
#[derive(Debug, Clone, PartialEq)]
pub enum Expr {
    VarRef(String),
    Literal(Value),
    BinOp(BinOp, Box<Expr>, Box<Expr>),
    UnaryOp(UnaryOp, Box<Expr>),
    Call(String, Vec<Expr>),
}

pub fn parse(input: &str) -> Result<Expr, ParseError> { /* ... */ }
pub fn referenced_vars(expr: &Expr) -> HashSet<String> { /* walk and collect VarRef */ }
```

Use `nom` or `winnow` for the parser. About 200–400 lines.

### Watch out for

**Choice list references.** A constraint like `selected(${q1}, 'yes')` references both the variable `q1` and the choice value `'yes'`. The integrity check must validate both: `q1` exists, *and* `q1`'s choice list contains `'yes'`. Easy to forget the second part.

**Merge integrity is transitive.** If A and B both auto-merge cleanly per-field but the *combined* state fails integrity (e.g. A renamed a var, B added a reference to the old name), the check must run on the merged state, not each branch separately.

**Surface unresolved conflicts clearly.** The frontend (stage 08's `/conflicts` route) shows a queue. Each card displays: the field, the ancestor value, A's value, B's value, the choice buttons. Don't make the user dig.

### Verification checklist

- [ ] Expression parser handles the ODK subset.
- [ ] Integrity check catches dangling refs, cycles, type mismatches, choice-reference errors.
- [ ] Merge engine auto-merges per-field changes correctly.
- [ ] Same-field conflicts surface as four-card decisions.
- [ ] Merge is blocked by integrity violations.
- [ ] A `Deployed` event on a broken state is refused with the violation list.
