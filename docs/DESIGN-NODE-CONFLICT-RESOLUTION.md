# Design: automatic resolution of node and field conflicts

> **Status: design, not implemented.** 14 October 2026. Nothing here exists in
> source. It proposes a strategy framework, a policy-as-data format and new merge
> strategies; it changes no stored hash, event or applied migration. Part of the
> [remaining-engine-designs index](ENGINE-REMAINING-DESIGNS.md). Item: `E-NODERES`.
> This is the foundation the [cross-document merge design](DESIGN-CROSS-DOCUMENT-MERGE.md)
> calls for per-document decisions. Recommended to build first among the merge items.

## 0. Summary

Today a "real" node or field conflict (two sides changed the same value
differently) is never resolved automatically. `workspace_merge::select` returns
the team value and records a conflict; `convergence::converge` then reports the
whole file as not clean, and `engine-cli repository merge --same-file-merge clean`
leaves that path to an explicit resolution
([PORTABLE-PROJECT-HISTORY.md](PORTABLE-PROJECT-HISTORY.md), "Applying clean
same-file merges"). The status ledger lists "resolving real node conflicts
automatically" as not done ([PROJECT-STATUS.md](PROJECT-STATUS.md)).

Several of those conflicts are not real in the sense a user cares about, and
tests pin them as conflicts today: two people edit different items of the same
bullet list; one side deletes a word range the other bolded; two people insert
different words at the same gap
(`crates/engine-core/tests/richtext_test.rs`,
`conflicts_for_competing_insertions_delete_format_and_structural_edits`).
Others are genuine (two different colours for the same text) and must stay with a
human unless a policy says otherwise.

This design proposes:

1. a **strategy registry**: each strategy is a total, deterministic, symmetric
   function from `(base, left, right)` to *merged value*, *preferred side with the
   other preserved*, or *conflict*;
2. **strategies** for text, formatting marks, lists and same-shape tables,
   spreadsheet cells and formulas, and slide shapes;
3. **semantic invariants** checked on the merged result, with violations returned
   as conflicts (never silently repaired);
4. **policy as data** per project: which strategy applies where, which outcomes
   need review, which are blocked; policies can only tighten an organization floor;
5. **loser preservation**: an automatic "prefer" never discards the other side; it
   becomes a suggestion or comment that a reviewer can accept;
6. a **property-based test plan** with explicit laws every strategy must satisfy.

Honest limit stated up front: automatic merge of concurrent edits is *convergent
and lossless*, not intention-preserving in general. Where two people typed
different text into the same gap the engine can produce a deterministic, reviewable
result; it cannot know what they meant.

## 1. Purpose, goals, non-goals, personas, metrics

### 1.1 Problem statement

- Teams editing the same documents offline or on branches produce overlapping
  edits; the volume of false conflicts (non-overlapping in meaning, overlapping in
  representation) makes review slow and teaches users to take one side wholesale.
- Rich text is merged only inside inline containers (paragraph, heading, code
  block). A table or list is one JSON value, so edits to different cells conflict
  (`richtext::merge` returns `None` for anything not an inline container).
- Spreadsheet column insertion rewrites `cell_<index>` keys in every row, so a
  column insert on one side conflicts with any cell edit on the other.
- Text is aligned by Unicode scalar (`char`), not grapheme cluster
  (`richtext.rs` `inline`): an alignment can fall inside a Bengali conjunct or
  emoji sequence.

### 1.2 Goals

| ID | Goal |
|---|---|
| G1 | Reduce review burden by resolving conflicts that have a defensible deterministic outcome. |
| G2 | Never lose information silently; every automatic choice is attributable and reversible. |
| G3 | Make "what may be automatic" an explicit, auditable policy rather than code. |
| G4 | Keep the merge symmetric, idempotent and orientation independent (as `converge` is). |
| G5 | Preserve semantic validity: results satisfy document invariants or are returned as conflicts. |
| G6 | Support the three primary families: documents, workbooks, presentations. |

### 1.3 Non-goals

- Realtime character-level convergence (CRDT) or presence. Stage 20 stays deferred
  ([README](../README.md); OPEN-RESEARCH-REGISTER `T1`).
- Guessing intent with statistical or LLM methods in the merge path.
- Automatic resolution of semantic disagreements in meaning (two different
  numbers in the same cell).
- Formula evaluation. The formula engine is being replaced (owner decision,
  14 October 2026); this design needs only a reference tokenizer, not an evaluator.
- Cross-document identity (see the cross-document design).

### 1.4 Personas

| Persona | Need |
|---|---|
| Author | Wants her paragraph edits to survive when a colleague fixed a typo three lines up. |
| Reviewer | Wants a short list of real decisions, each with context. |
| Project owner | Wants to set "sheets: formulas always manual, text: auto with review". |
| Analyst | Inserts columns while others edit cells. |
| Compliance lead | Needs to show that automatic merges followed a written policy. |

### 1.5 Success metrics

| Metric | Target (hypothesis; measure on corpus) |
|---|---|
| Share of corpus conflicts that need a human, text family | at most 35 % of today's count |
| Lossy automatic outcomes (information not recoverable from record) | 0 |
| Strategy law violations (symmetry, idempotence, identity) over 10^6 generated cases | 0 |
| Invariant violations in automatic results | 0 |
| p95 merge time per 5,000-node document (excluding object reads) | under 2 s |
| Reviewer survey: "the automatic result was what I expected" | at least 80 % |

## 2. Current state against target

| Capability | Tag | Evidence / path |
|---|---|---|
| Node/field three-way: equal or one-sided changes merge | EXISTS | `crates/engine-core/src/workspace_merge.rs` `select` |
| Conflict with `"team"`/`"draft"` explicit resolutions | EXISTS | `workspace_merge.rs` `merge`, `merge_strict` |
| Delete versus edit is a conflict (`$node`) | EXISTS | `workspace_merge.rs` |
| Location (`parent_id`, `pos`) three-way as one value | EXISTS | `workspace_merge.rs` `location` |
| Rich text: non-overlapping hunk merge in paragraph/heading/codeBlock | EXISTS | `richtext.rs` `merge_inline`; 20 ms Myers deadline `text_diff`; `MAX_SCALARS = 100_000` |
| Mark property merge by type then property | EXISTS | `richtext.rs` `merge_property`, `merge_marks` |
| Competing same-gap insertions, delete versus format: conflict | EXISTS (as conflict) | `richtext_test.rs` |
| Lists and tables: edits to different items conflict | EXISTS (as conflict) | `richtext_test.rs` (`bulletList` case); comment in `richtext::merge` |
| Comments/suggestions merged as whole collections | EXISTS | `convergence.rs` `three_way` |
| Symmetric orientation handling and clean-in-both rule | EXISTS | `convergence.rs` `converge` |
| Four-card conflicts and integrity warnings for event branches | EXISTS | `crates/engine-core/src/merge.rs` |
| Structural validity gate (cycle, containment, unique var_name) | EXISTS | `crates/engine-core/src/ops.rs` `validate_op` (applied on write, not inside state merge) |
| Referential integrity pass (questionnaire references) | EXISTS | `crates/engine-core/src/integrity.rs` |
| Sheet protection guard | EXISTS | `crates/engine-core/src/governance.rs` `check_sheet_protection` |
| Property tests for text patches | PARTIAL | `crates/engine-core/tests/richtext_test.rs` (`proptest!` for round trip only) |
| Grapheme-cluster aware text alignment | MISSING | scalar tokens in `richtext.rs` |
| Strategy registry, policy data, loser preservation | MISSING | none |
| Same-shape table and list merge | MISSING | none |
| Spreadsheet formula-aware merge, structural shift normalization | MISSING | none; column identity is positional (`cell_<n>`) |
| Slide shape strategies beyond per-field three-way | MISSING | none |
| Post-merge invariant suite for state-level merge | MISSING | `ops::validate_op` runs only on event apply |
| Policy floors for merge automation | MISSING | review floors exist (`PROJECT-POLICY-AND-METADATA.md`), none for merge strategy |

## 3. Architecture

### 3.1 Placement

```
 converge(base,left,right)                      (convergence.rs, today)
    |
    v
 merge_states --> per node: delete/edit, $location, var_name, fields ...
                    |
                    +--> for each (node, field): lookup strategy in Registry(policy)
                              |
                              v
                       Strategy::resolve(base, left, right, ctx) -> Outcome
                              |
        +---------------------+------------------------+
        v                     v                        v
   Merged(value,evidence)  Prefer(side, loser)     Conflict(reason)
        |                     |                        |
        +----------+----------+                        |
                   v                                   v
           candidate merged state              conflict list (as today)
                   |
                   v
           invariant suite (INV-*)  --violation--> Conflict(INV-x)
                   |
                   v
           recorded in commit: strategy ids + policy hash + evidence
```

### 3.2 Components

| Component | Responsibility |
|---|---|
| `Strategy` trait | Pure function; no clock, randomness, IO or global state. |
| `Registry` | Maps `(node kind, node type, field pattern)` to a strategy id and parameters from the policy. |
| `Policy` | Data; canonical JSON; hashed; versioned. |
| `Invariants` | Pure validators over the merged `DocumentState`. |
| `LoserSink` | Turns a discarded side into a suggestion or comment entry in the merged state. |
| `Evidence` | Records which strategy decided what, for `verify-merge` and review UI. |

Proposed module layout: `crates/engine-core/src/resolve/{mod,policy,registry,text,marks,
list,table,sheet,formula,slide,invariants,evidence}.rs`. `workspace_merge::select`
becomes a thin call into the registry with the existing behaviour as the
`dynodoc.default.v1` strategy so the first release is a behavioural no-op.

### 3.3 Data flow: one field

1. Collect `base`, `left`, `right` values for `(node, field)`.
2. Shortcuts (unchanged): equal sides, one-sided change.
3. Registry lookup with node kind, type, field name and policy.
4. Strategy resolves or declines; declines become conflicts.
5. If `Prefer`, the sink writes the loser as a suggestion (or comment) *in the
   merged state*; the evidence names it.
6. After all fields, the invariant suite runs once; violations are conflicts that
   name the strategies that produced the offending values.

### 3.4 Trust boundaries

- The strategies and policy are code and data of the engine; they run on whichever
  machine performs the merge (client or server). A server verifying a merge
  re-runs the strategy from the recorded inputs (`verify-merge`); it does not trust
  the client's claim.
- Policy is authoritative only at publication on the shared service, under
  project-rule authority. A local policy file is advisory.

## 4. Strategy specifications

### 4.1 Laws every strategy must satisfy

| Law | Statement |
|---|---|
| L1 Symmetry | `resolve(b, l, r)` and `resolve(b, r, l)` produce the same merged value; for `Prefer`, the preferred *value* is the same (side labels swap) |
| L2 Idempotence | `resolve(b, x, x) = Merged(x)` |
| L3 Identity | `resolve(b, b, x) = Merged(x)` and `resolve(b, x, b) = Merged(x)` |
| L4 Totality | terminates within a documented bound for every input within limits; never panics |
| L5 Determinism | no dependence on time, address order of side, thread scheduling |
| L6 Invariant preservation | if inputs satisfy the field's invariants, so does a `Merged`/`Prefer` result |
| L7 No silent loss | any information present in an input and absent from the output is recorded in `evidence` and retrievable |
| L8 Bounded work | cost is a stated function of input size; large inputs return `Conflict(TooLarge)` |

L1 holds by construction when tie-breaks use content (string order, node ID) and
never "left"/"right" labels. `converge` already enforces orientation independence
at the whole-state level by running both orientations and canonicalizing; strategies
must satisfy L1 on their own so that check never fires.

### 4.2 Text: `dynodoc.text.diff3-grapheme.v1`

Scope: inline containers (paragraph, heading, code block), the same scope as
`richtext::merge_inline` today.

**Tokenization.** Extended grapheme clusters per UAX #29 (Revision 49,
Unicode 18.0.0, including the Indic conjunct rule GB9c). Each cluster is one atom;
atom attributes are the run's full metadata (marks and attrs) exactly as
`richtext.rs` interns them (`Styles`). Offsets in stored patches remain Unicode
scalar counts so existing events and the browser codec are unaffected; atoms are an
alignment-time concept. A Bengali conjunct such as U+0995 U+09CD U+09B7 is one atom
and cannot be split by a hunk boundary.

**Diff.** Myers with a deadline as today, with a patience-style unique-anchor
fallback for large paragraphs. The result is unchanged where the existing
algorithm already merges: the new strategy is a superset (test T8).

**Rules.** Let hunks be the diff of base to left and base to right.

| Case | Outcome | Auto | Notes |
|---|---|---|---|
| Hunks disjoint (no shared atom, no shared gap) | apply both | yes | existing behaviour |
| Same gap, identical inserted text | insert once | yes | `identical_insert: dedupe` |
| Same gap, different inserted text | both inserted, ordered by bytewise ascending text; ties impossible after dedupe | needs-review (default) | convergent, not intention-preserving |
| Both delete overlapping ranges | delete the union | yes | |
| One deletes a range, the other inserts inside it | keep the insert, delete the rest | needs-review | policy `delete_vs_insert: keep-insert | manual` |
| One deletes a range, the other changes marks only on atoms in it | deletion wins | yes | formatting on deleted text is moot |
| Both replace overlapping ranges differently | `Prefer` per policy (`prefer-role`, `manual`) | per policy | loser kept as suggestion |
| Insertion at a boundary where marks differ on the two sides | Peritext-style mark growth: inserted text inherits marks by each mark's `inclusive` flag | needs-review | see 4.3 |

Bounds: `MAX_SCALARS = 100,000` per paragraph (existing); at most 200 hunks per side
before returning `Conflict(TooManyHunks)`.

### 4.3 Formatting marks: `dynodoc.marks.set.v1`

| Case | Outcome |
|---|---|
| Independent mark types added on both sides | union |
| Same mark type, attributes differ in disjoint properties | property merge (exists) |
| Same property, different values (`color` red versus blue) | `Prefer` per policy; default `manual` |
| Added versus removed on the same atoms | `manual` (default) or `prefer-role` |
| Mark order | schema order from the branches, as `merge_marks` does today (never alphabetical) |
| Mark kinds not known to the engine | preserved verbatim; compared by canonical JSON |

Mark growth table (proposed, aligned with ProseMirror's mark `inclusive` notion and
the boundary rules studied in Peritext):

| Mark | Grows at end boundary | Grows at start boundary |
|---|---|---|
| bold, italic, underline, strike, code | yes | no |
| link, comment-anchor | no | no |
| textStyle (color, font) | yes | no |

These are defaults; the project policy can override per mark type.

### 4.4 Lists and tables: `dynodoc.block.same-shape.v1`

Today any edit in different items of a list or different cells of a table
conflicts, because nested content has no identity (comment in `richtext::merge`:
"Never align moved table/list children by array position").

**Safe subset.** When *both* sides preserve the container's shape relative to base
(same item/row/column count, same merged-cell layout, same attribute set), position
is identity, because nothing moved. Then:

1. Three-way per item/cell using the text strategy (4.2) for inline content and
   `same-shape` recursively for nested containers.
2. Container attributes (list type, table width, header row) three-way as
   properties.
3. If either side changed shape (insert/delete/move of items, rows or columns)
   require nested identifiers: items carry `attrs.itemId`, rows `attrs.rowId`,
   cells `attrs.cellId` (ULIDs written by the editors; MISSING today and gated on
   the format graph, [ENGINE-EVOLUTION-PLAN.md](ENGINE-EVOLUTION-PLAN.md) part 4).
   Without identifiers: `Conflict(ShapeChanged)`.
4. With identifiers (phase N4b): item insertions at the same position are ordered
   by identifier; delete versus edit of an item is a conflict; moves use the
   existing `$location` semantics on the nested identity.

| Case | Auto |
|---|---|
| Edits to different cells, shape unchanged | yes |
| Same cell, both edited | text strategy; may conflict |
| Row inserted on one side, cell edited on the other (identifiers present) | yes |
| Row inserted on both sides at the same position | yes, ordered by identifier, needs-review |
| Row deleted on one side, cell in it edited on the other | no |
| Column insert versus cell edit (no identifiers) | no (`ShapeChanged`) |

### 4.5 Spreadsheet cells: `dynodoc.sheet.cell.v1`

Model facts (from `frontend/researcher/src/lib/workspace/sheet-model.ts` and
`sheet-structure.ts`): a sheet is a `section` node with `kind: "sheet"`; a row is
an `item` node with fields `cell_<col>` (scalar or `=formula` string) and
`format_<col>`; columns are *positional* (`columns` is a count, limit 100 columns
and 3,000 rows); row order is `pos`.

| Case | Outcome | Auto |
|---|---|---|
| Different cells changed | merge | yes (exists) |
| Same cell, equal after canonicalization (formula AST equal) | take one | yes |
| Same cell, both changed to different scalars | conflict; policy may `prefer-role` | manual default |
| One side changed a formula, the other changed a cell it references | independent | yes (no derived values are stored) |
| One side changed value to formula or the reverse, the other edited the same cell | conflict | no |
| Date-only cells (`type: date`) both changed to different dates | conflict | no |
| Both sides added the same row identifier with different cells | per-cell three-way against an empty base | needs-review |
| Protected sheet (`sheet_protected`) edited on one side, protection turned on on the other | conflict | no |

**Formula canonical form.** A small tokenizer owned by Dynodoc (independent of the
formula engine being replaced). Tokens: strings, numbers, booleans, error
literals, operators, parentheses, function names (case-insensitive, stored upper
case), separators, and references. Whitespace outside strings is insignificant. Two
formulas are *equal* when their token streams are equal; a parse failure falls
back to exact string equality and marks the cell `unparsed` (never auto-merged
beyond equality).

### 4.6 Spreadsheet structure: `dynodoc.sheet.shift.v1` (phase N4b)

A column insert/delete rewrites `cell_<n>` and `format_<n>` keys in every row, and
rewrites formula references in every cell. State-based merge sees thousands of
changed fields. Normalization by *certificate*:

1. For left (and symmetrically right), search shifts `s = (op, k)` with
   `op in {insert-col k, delete-col k, insert-row, delete-row}` such that
   `left` equals `shift_s(base)` plus a residual of edits, where the residual
   touches at most `ceil(5 %)` of cells.
2. Accept the certificate only if it is **unique** (the smallest consistent `k`
   among equal-explanation candidates, and no alternative `op` explains the same
   data with a residual of equal or smaller size). Ambiguity returns
   `Conflict(AmbiguousShift)`.
3. Rewrite `right` forward through the same shift: keys shift, formula references
   are rewritten with Excel's rules (insert inside a range expands it; delete of a
   referenced cell yields a `#REF!` marker; absolute and relative markers are
   preserved as text).
4. Run cell-level three-way in the shifted coordinate system; then apply the
   shift to the result.

The certificate is cheap to *verify* and expensive to *find*; verification is what
`verify-merge` re-runs. Long term, stable column identifiers
(`sheet.col_ids: [..]`, cells keyed by identifier) remove the need for inference;
that is a format change owned by the format-graph work and is listed as an open
question (NC-Q5).

### 4.7 Slides: `dynodoc.slide.shape.v1`

Model facts (`frontend/researcher/src/lib/workspace/presentation.ts`): root `form`,
slide `section`, shape `item` with `kind`, `role`, `text`, `x`, `y`, `w`, `h`,
`style`, `src`, `alt`; slide fields `layout`, `notes`, `background`, `hidden`.

| Case | Outcome | Auto |
|---|---|---|
| Different shapes or different fields | merge (exists) | yes |
| `x` changed on one side, `y` on the other | per-field | yes |
| `x`/`y`/`w`/`h` same field, different values | `Prefer` per policy (`prefer-role`) or manual | manual default |
| Shape text both edited | text strategy | per 4.2 |
| Style properties disjoint | property merge | yes |
| `layout` changed on one side, geometry of placeholder-role shapes (`title`, `body`, `subtitle`) edited on the other | conflict (semantic coupling) | no |
| Slide `pos` both moved | `$location` conflict | manual |
| Shapes inserted at the same position | ordered by node ID; needs-review | yes |
| Shape deleted versus edited | conflict | no |
| `src` (image) changed on both sides | conflict | no; size limit `MAX_IMAGE_BYTES = 1 MiB` applies |
| Theme changed on one side, shape colours on the other | independent fields; visual coupling flagged | needs-review |

### 4.8 Structural and document-level fields

| Item | Strategy |
|---|---|
| Node deletion vs edit | `Conflict` (never auto); policy may allow `delete-wins-with-suggestion` for comments only |
| Location moves | existing `$location`; add cycle check at merge level (INV-G1) |
| Comments (collection) | element-wise union by comment identity, not whole-collection three-way (today `convergence.rs` conflicts if both sides add a comment) |
| Suggestions | element-wise by suggestion identifier; conflict only when the same identifier differs |
| `removed_choices` | union (exists) |
| `var_name` | three-way; duplicate result is `INV-G4` |

Element-wise comment merge is a strict improvement: two reviewers adding comments
to the same document currently make the whole file non-clean.

## 5. Semantic invariants (suite `INV`)

Evaluated on the merged state. Violation = conflict naming the participants. The
suite is pure and bounded; every check has a bound of `O(n)` or `O(n log n)`.

### 5.1 General

| ID | Invariant | Source |
|---|---|---|
| INV-G1 | the parent graph is acyclic | `ops::validate_op` (`MoveCreatesCycle`) |
| INV-G2 | containment is legal (`IllegalContainment`) and roots are valid (`InvalidRoot`) | `ops.rs` |
| INV-G3 | a live node's parent exists and is live | structure |
| INV-G4 | live `var_name` values are unique | `ops.rs`, `integrity.rs` |
| INV-G5 | references resolve; no relevance cycles; types compatible (questionnaire adapter) | `integrity.rs` |
| INV-G6 | when `content` is an object, `label` equals its plain text | `workspace_merge.rs` derivation |
| INV-G7 | comments and suggestions target existing nodes | `materializer` |
| INV-G8 | sheet protection flag type and edit guards hold | `governance.rs` |

### 5.2 Text and rich text

| ID | Invariant |
|---|---|
| INV-T1 | no extended grapheme cluster that is intact in all inputs is split in the output |
| INV-T2 | output is valid Unicode scalar values; no unpaired surrogates (JSON strings) |
| INV-T3 | document JSON conforms to the editor schema: inline containers hold only text; no empty text nodes; adjacent equal-metadata runs coalesced (canonical `children()` form) |
| INV-T4 | scalar count within `MAX_SCALARS`; depth within `MAX_DEPTH = 64` |
| INV-T5 | tables are rectangular and merged ranges do not overlap; lists contain only list items |

### 5.3 Spreadsheet

| ID | Invariant |
|---|---|
| INV-S1 | every `=...` cell either parses or is flagged `unparsed` and preserved verbatim |
| INV-S2 | references address cells inside the sheet bounds (columns <= 100, rows <= 3,000) or a named range that exists |
| INV-S3 | merged ranges (`merges`) are disjoint and within bounds |
| INV-S4 | `columns` is at least the highest used column index + 1 |
| INV-S5 | each date cell holds a valid date-only value consistent with its `type` (date-only acceptance rules in `NATIVE-CONNECTOR-ACCEPTANCE.md`) |
| INV-S6 | data-validation and conditional-format ranges lie within bounds (`conditionalRules` limits: at most 8 rules) |

### 5.4 Presentation

| ID | Invariant |
|---|---|
| INV-P1 | shape identifiers unique across the deck |
| INV-P2 | geometry values finite numbers |
| INV-P3 | image data within `MAX_IMAGE_BYTES` |
| INV-P4 | slide and shape `pos` values form a total order (ties broken by node ID) |

## 6. Policy as data

### 6.1 Format (proposed): `dynodoc.merge-policy.v1`

```yaml
version: 1
id: research-default
description: Text auto with review; spreadsheets manual for formulas.
defaults:
  unknown: manual                 # what to do when no rule matches
  loser: suggestion               # none | suggestion | comment
  roles_rank: [owner, manager, editor, contributor, reviewer]
rules:
  - id: text-paragraphs
    match: { family: document, type: [paragraph, heading, codeBlock], field: content }
    strategy: dynodoc.text.diff3-grapheme.v1
    params: { identical_insert: dedupe, delete_vs_insert: keep-insert, max_hunks: 200 }
    gate: auto-with-review      # auto | auto-with-review | manual | blocked
  - id: marks
    match: { family: document, field: content, aspect: marks }
    strategy: dynodoc.marks.set.v1
    params: { attribute_conflict: manual }
    gate: auto
  - id: lists-tables
    match: { family: document, type: [bulletList, orderedList, table] }
    strategy: dynodoc.block.same-shape.v1
    gate: auto-with-review
  - id: sheet-values
    match: { family: spreadsheet, field: "cell_*" }
    strategy: dynodoc.sheet.cell.v1
    params: { formula_equality: tokens, value_conflict: manual }
    gate: auto
  - id: sheet-shifts
    match: { family: spreadsheet, aspect: structure }
    strategy: dynodoc.sheet.shift.v1
    gate: manual                # opt in later
  - id: slides
    match: { family: presentation }
    strategy: dynodoc.slide.shape.v1
    params: { geometry_conflict: manual }
    gate: auto-with-review
floors:
  # keys here are *minimums*; rules cannot be more permissive than a floor
  gate_at_least: { "dynodoc.sheet.shift.v1": manual }
```

### 6.2 Evaluation

1. Rules are evaluated top to bottom; first match wins; a document must end with
   an implicit `unknown` rule.
2. `gate` strictness order: `blocked` > `manual` > `auto-with-review` > `auto`.
   The effective gate is the *strictest* of the rule, any project floor and any
   organization floor (same OR/max pattern as project review floors in
   [PROJECT-POLICY-AND-METADATA.md](PROJECT-POLICY-AND-METADATA.md)).
3. `auto-with-review`: the merge is produced and the commit marked
   `needs_review`; protected `main` cannot take it without the review decision
   binding the **resolution set hash** (below).
4. `blocked`: the strategy may not run; the conflict is surfaced.
5. Validation rejects unknown strategies, unknown parameters, rules shadowed by an
   earlier identical match, and any policy that weakens a floor.

### 6.3 Where policy lives and how it binds

| Aspect | Design |
|---|---|
| Storage | project metadata (shared) / file next to the repository (local); canonical JSON |
| Hash | `SHA-256("dynodoc.merge-policy\0v1\0" || canonical JSON)` |
| Commit binding | a merge commit lists `{policy_hash, strategies: [{id, version}]}` and embeds the canonical policy (at most 16 KiB) so `verify-merge` is self-contained; deduplicated when several records share one policy |
| Changes | audited metadata change (existing project-metadata audit, digests only); effective for merges created after it |
| Defaults | `dynodoc.default.v1` equals today's behaviour: strategy-less, conflict on any overlap except richtext hunks |

### 6.4 Review gating and the resolution set hash

The review of a candidate that includes automatic resolutions must show them. Define

```
resolution_set_hash = SHA-256("dynodoc.resolution-set\0v1\0" ||
        canonical JSON of sorted [{strategy, node, field, base, left, right, merged, loser}])
```

The review decision (approve or request changes) binds the candidate address
(existing), and for `auto-with-review` merges also the hash, so an approver cannot
unknowingly approve a different set of automatic choices than the one displayed.
This is an extension of the review row's fields
(`project_history_review`, `migrations/0036_shared_project_history.sql`) and of
the per-file change-request review for centralized documents.

## 7. Loser preservation

An automatic outcome that discards one side's value (`Prefer`, delete-wins,
`keep-insert`) never drops it:

| Kind | Mechanism |
|---|---|
| Text replace/replace | the losing run becomes a `suggestion` targeting the same node with `detail` = a `RichTextPatched`-style patch against the merged value |
| Property value (color) | suggestion with a `FieldEdited` detail |
| Cell value | suggestion with a `FieldEdited` detail; or a comment when the policy asks `loser: comment` |
| Deleted-vs-moot formatting | not preserved (declared moot by rule) |

Suggestions are an existing state element (`DocumentState.suggestions`, typed
`suggestion` objects, `SuggestionProposed` / `SuggestionAccepted` events). The
suggestion's author is a system actor `merge-resolver` plus the originating side's
authors in `detail.origin` so attribution is retained. This keeps the engine's rule
that content never vanishes without a recoverable record and reuses the review UI.

## 8. API and CLI surface (proposed)

```text
engine-cli repository DB PROJECT merge LEFT RIGHT --same-file-merge clean
        [--merge-policy POLICY.yaml] [--explain] ...
engine-cli repository DB PROJECT compare BASE LEFT RIGHT --merge-policy POLICY.yaml --explain
engine-cli merge-policy validate POLICY.yaml [--floor FLOOR.yaml]
engine-cli merge-policy hash POLICY.yaml
engine-cli repository DB PROJECT verify-merge COMMIT           # re-runs strategy ids and checks policy hash
```

`--explain` prints, per node and field: the rule that matched, the strategy, the
outcome, the loser and the gate. The existing report keys are unchanged; new keys
are additive (`strategies`, `gate`, `loser`).

HTTP (shared service, proposed): `GET /projects/{id}/merge-policy` and
`PUT /projects/{id}/merge-policy` (owner/manager; strengthening only; audited
metadata event with digest); `POST /projects/{id}/history/merge-preview` returns a
read-only plan (expensive-read admission applies).

## 9. Security and privacy

### 9.1 Threat model

| # | Threat | Control | Residual |
|---|---|---|---|
| T1 | Policy loosened to push through harmful merges | floors (OR/max), audit, review binding via resolution set hash | owner with floor authority |
| T2 | Crafted input exhausts CPU (pathological diff, thousands of hunks, huge tables) | bounds L8: 20 ms diff deadline retained, 200 hunks, `MAX_SCALARS`, shape budgets, workload admission | deadline is not a latency guarantee (existing note) |
| T3 | Strategy bug silently loses text | laws L1-L7 as property tests; no-loss assertion on every automatic result in CI; evidence | tests are not proof |
| T4 | A malicious side crafts content so the tie-break order favours it | tie-breaks are content-ordered and symmetric; both versions are kept; review gate | an attacker can still get their text *included* (not exclusive) |
| T5 | Invisible Unicode (bidi controls, zero-width joiners) used to disguise merged text | INV-T1 preserves clusters; add an optional "bidi/invisible characters introduced" warning to the review | rendering-level attacks are out of scope |
| T6 | Formula injection through merged cells (`=HYPERLINK`, external workbook links) | the engine stores formulas verbatim as today; merge never creates a formula the user did not author; export policy is a separate concern | adapters decide export behaviour |
| T7 | Resolver acts as a confused deputy for permissions | merging requires write rights on the target file; the merged result goes through `authorize` on apply (hosted) | |
| T8 | Policy discloses sensitive structure | policy contains no content; only type/field names | |

### 9.2 Not claimed

That automatic text merges preserve meaning; that two concurrent insertions are
ordered as the authors would choose; that formulas remain semantically valid after
shift normalization beyond the reference-rewriting rules; that the strategy set is
complete for any native format.

## 10. Test plan

### 10.1 Property-based tests (proptest is already a workspace dependency)

| ID | Property | Generator |
|---|---|---|
| P1 | Symmetry (L1) | random base; two random edit scripts per strategy; swap sides |
| P2 | Idempotence (L2) and identity (L3) | same |
| P3 | Totality and bound (L4, L8) | adversarial: 100,000-scalar paragraphs, 10,000 hunks, deep nesting |
| P4 | Invariant preservation (L6) | all INV checks on every `Merged`/`Prefer` result |
| P5 | No loss (L7) | multiset of grapheme atoms of both sides is covered by result plus loser records |
| P6 | Superset of existing behaviour | wherever `richtext::merge` returns `Some`, the new strategy returns the same value |
| P7 | Patch round trip | `diff(team, merged)` applied to `team` equals `merged` (the existing test pattern, `richtext_test.rs`) |
| P8 | Grapheme integrity (INV-T1) | strings from a grapheme-aware generator (Bengali conjuncts, emoji ZWJ, combining marks, RTL) |
| P9 | Shift certificates | generate shift + residual; the certificate is found and unique, or `AmbiguousShift` |
| P10 | Metamorphic: renaming node IDs consistently changes nothing but IDs | |
| P11 | Policy monotonicity | a stricter policy never produces a *more automatic* outcome |
| P12 | Convergence of pair merges across replicas | two replicas merging each other's work reach the same state address (existing property for `converge`) |

### 10.2 Example-based corpus

| Family | Examples |
|---|---|
| Text | same-gap insertions; delete/insert; formatting-only; Bengali conjunct boundaries; mixed LTR/RTL; 50,000-word paragraph |
| Marks | bold vs italic; color red vs blue; link href vs text edit |
| Lists/tables | disjoint item edits; row insert vs cell edit (with and without identifiers); header row; merged cells |
| Sheets | cell/cell; formula/input; insert column vs edit; delete row vs edit; date-only; protected sheet |
| Slides | move vs resize; layout change vs placeholder geometry; slide reorder |
| Review | automatic resolution set hash display and binding |

### 10.3 Golden vectors

- Independent Python implementation of the policy canonical form and hash, and of
  UAX #29 cluster segmentation for a fixed corpus (using the Unicode test data
  files for the pinned version): Rust tokenization must match.
- A merge record fixture (v4 shape from the cross-document design) for
  `verify-merge`.

### 10.4 Acceptance by phase

See section 11. Each phase ships its laws and invariants with it; none ships without
P1-P5 on its strategies.

## 11. Delivery plan

### 11.1 Phases

| Phase | Deliverable | Depends on | Effort (person-weeks) | Confidence |
|---|---|---|---|---|
| N0 | Policy schema, canonical form, hash vectors, strategy trait, laws harness, `dynodoc.default.v1` as a behavioural no-op | none | 2 - 3 | high |
| N1 | Registry, evidence recording, `--explain`, invariants INV-G*, element-wise comments/suggestions | N0 | 3 - 4 | medium |
| N2 | Grapheme tokenization (adopt a permissively licensed segmentation crate or implement UAX #29 against the pinned data tests), `text.diff3-grapheme.v1`, `marks.set.v1`, loser sink | N1 | 5 - 7 | medium-low |
| N3 | Same-shape lists and tables; formula tokenizer and `sheet.cell.v1`; `slide.shape.v1`; INV-T/S/P suites | N1 | 6 - 8 | medium |
| N4 | Shift certificates (`sheet.shift.v1`), reference rewriter; nested identifiers variant of lists/tables when the format graph lands | N3, part 4 | 5 - 7 | low |
| N5 | Review gating: resolution set hash, API/policy endpoints, floors, audit | N1 | 3 - 4 | medium |
| N6 | Property and corpus suites at scale, benchmarks, documentation, rollout | all | 3 - 4 | medium |
| Total | | | 27 - 37 | |

**Assumptions**: one engineer experienced with `richtext.rs`/`convergence.rs`; the
rich-text editor schema is stable; no change to the stored event or checkpoint
formats; a permissively licensed UAX #29 implementation is acceptable (verify
licence at selection; MIT/Apache-2.0 expected). **Excludes**: nested-identifier
capture in the six host adapters (part 4 of the evolution plan), a formula
*evaluation* engine, UI for policy editing beyond a JSON editor, live collaboration,
security review (budget 1 - 2 person-weeks), localization of review text.

### 11.2 Rollout and migration

1. N0-N1 ship as no-ops (policy absent means today's behaviour).
2. Strategies are opt-in by policy; the CLI flag `--merge-policy` first.
3. Commit-format gate: automatic records with new strategy ids require readers that
   know them. Today an importer rejects unknown strategies (`convergence::STRATEGY`
   equality in `Commit::validate`); ship with the v4 commit version proposed in the
   cross-document design, or with an allow-list of strategy ids and a version bump.
4. Production: policy defaults to `dynodoc.default.v1`; per-project enablement; a
   pilot project with `auto-with-review` for text only.
5. No data migration. Existing commits and states are unchanged.

### 11.3 Operations

| Topic | Design |
|---|---|
| Observability | counters per strategy (invocations, merged, prefer, conflict by reason), invariant violations by id, merge time histogram, loser suggestions created |
| Failure modes | strategy panic (impossible by law L4; caught and converted to conflict with an internal-error marker), timeout (diff deadline), policy invalid (merge refused with the validation error), floor violation (policy rejected at save) |
| Rollback | set policy to default; existing commits remain valid because they record strategy ids and embedded policy |

## 12. Risks and open questions

### 12.1 Risks

| Risk | Mitigation |
|---|---|
| Users over-trust automatic text merges | default `auto-with-review`; UI labels; "same-gap insertions" listed explicitly |
| Policy sprawl | closed rule vocabulary; versioned schema; examples shipped |
| Shift inference false positives | certificate uniqueness, residual cap, manual default |
| Segmentation library dependency/licence | pin version; vendor test data; fallback to scalar tokens with a flag |
| Behavioural change breaks golden fixtures | default policy is a no-op; fixtures versioned |
| Performance regressions in the hot merge path | benchmarks gate in CI (reuse `richtext_bench` pattern) |

### 12.2 Open questions

| # | Question | Recommended default | Decided by |
|---|---|---|---|
| NC-Q1 | Should `auto-with-review` be the default gate for text? | Yes | Product owner |
| NC-Q2 | Order of same-gap insertions: bytewise text, or author key order? | bytewise text (content only; symmetric) | Engineering lead |
| NC-Q3 | Allow `prefer-role` at all for values (cells, colours)? | Allowed only with `loser: suggestion`; default manual | Product owner |
| NC-Q4 | Where does policy live for local repositories? | file beside the SQLite database, hash recorded in commits | Engineering lead |
| NC-Q5 | Stable column identifiers versus shift inference | inference first (no format change), identifiers with part 4 | Engineering lead |
| NC-Q6 | Who is the loser suggestion's author? | system actor with origin authors in `detail` | Product owner |
| NC-Q7 | Should delete-versus-edit ever be automatic (policy)? | Only for comments; never for content nodes | Product owner |
| NC-Q8 | Do we adopt an op-aware strategy when per-document event history is available? | Defer; state-based only | Engineering lead |
| NC-Q9 | Allow organization-level floors in the first release? | Project floors first, organization later | Product owner |

## Appendix A. Glossary

| Term | Meaning |
|---|---|
| Strategy | named pure function that resolves a three-way field difference |
| Gate | policy level: auto, auto-with-review, manual, blocked |
| Loser | the side's value an automatic outcome does not use |
| Resolution set | the list of automatic decisions in one merge |
| Shift certificate | proof that one side equals the base after a structural column/row operation plus a small residual |
| Atom | one extended grapheme cluster with its run metadata |

## Appendix B. References

| Source | Used for | Checked |
|---|---|---|
| Unicode Standard Annex #29, Revision 49 (Unicode 18.0.0, 2026-09-01), https://www.unicode.org/reports/tr29/ | grapheme cluster boundaries; GB9c Indic conjunct rule | 14 October 2026 |
| Khanna, Kunal, Pierce, "A Formal Investigation of Diff3", FSTTCS 2007, LNCS 4855, 485-496, https://www.cis.upenn.edu/~sanjeev/papers/fsttcs07_diff3.pdf | assumptions behind three-way text merge, failure of intuitive properties | 14 October 2026 |
| Litt, Lim, Kleppmann, van Hardenberg, "Peritext: A CRDT for Collaborative Rich Text Editing", PACM HCI 6 (CSCW2), Article 531, 2022, https://www.inkandswitch.com/peritext/ | formatting boundary and intent rules | 14 October 2026 |
| Gentle and Kleppmann, "Collaborative Text Editing with Eg-walker: Better, Faster, Smaller", EuroSys 2025, https://arxiv.org/pdf/2409.14252 | event-graph replay as an alternative to snapshots; deferred (NC-Q8) | 14 October 2026 |
| Repository: [RICH-TEXT-VERSIONING-RESEARCH.md](RICH-TEXT-VERSIONING-RESEARCH.md), [RICH-TEXT-ALGORITHM-RESEARCH-REVIEW.md](RICH-TEXT-ALGORITHM-RESEARCH-REVIEW.md), [03-versioning-mathematics-vol2.md](03-versioning-mathematics-vol2.md) D.2 | model and prior research | in repository |

ProseMirror's mark `inclusive` behaviour was not re-verified against its current
documentation in this pass: unverified, check at build time. Excel's exact
reference-rewriting rules for ranges at insertion boundaries were not verified
against vendor documentation: unverified, check at build time.
