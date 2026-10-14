# Design: cross-document and project-level merge

> **Status: design, not implemented.** 14 October 2026. Nothing here exists in
> source. It proposes a commit format (v4), new CLI commands and project-level
> validators; it changes no applied migration or existing hash. Part of the
> [remaining-engine-designs index](ENGINE-REMAINING-DESIGNS.md). Item: `E-XDOC`.
> Depends on [node conflict resolution](DESIGN-NODE-CONFLICT-RESOLUTION.md) for the
> in-document strategies it calls. Interacts with
> [commit pruning](DESIGN-COMMIT-PRUNING.md) (pinning rules for new record types).

## 0. Summary

The portable project history merges at two levels. The project planner
(`crates/engine-core/src/repository/compare.rs`) compares a map of
`path -> {document UUID, state root}` three ways. For a path edited on both sides
with the same document UUID, the opt-in convergence step
(`crates/engine-core/src/repository/convergence.rs`) merges the two typed states
node by node and field by field, and records the result in a signed v3 commit
([PORTABLE-PROJECT-HISTORY.md](PORTABLE-PROJECT-HISTORY.md)).

Everything that crosses a document boundary is outside both levels:

- a paragraph, row or slide shape moved from one file to another;
- one file split in two, or two files joined;
- a rename combined with an edit on the other side;
- constraints that span files (identity, namespace, cross-file references,
  permission and classification boundaries).

This design adds, in order of risk:

1. an **identity-keyed project planner** (`plan_v2`) that aligns files by document
   UUID, not by path, so a rename and an edit on the other side merge;
2. **relocation records** (commit format v4): signed lineage for nodes moved or
   copied between documents, with deterministic destination identifiers;
3. **edit-follows-move** and split/join merge rules that are automatic only under
   explicit conditions;
4. **project-level constraint validators** (namespace, node-identity, reference,
   policy, permission) evaluated on the merged candidate;
5. a **conflict taxonomy** with a fixed answer to "can this ever be auto-resolved";
6. CLI flows, a resolution file format and a model-based test corpus.

### 0.1 What was found while grounding this design

- `compare::plan` aligns by path. A rename on one side with an edit on the other is
  reported as a delete/edit conflict at the old path plus an unconflicted add at the
  new path carrying the *old* state, and the CLI then refuses any resolution that
  names the unconflicted new path. The only resolution that passes the identity
  check discards the edit. This is derived from reading `plan` and `prepare_merge`;
  there is no regression test for the case, and the first task of phase P1 is to add
  one (section 11).
- `node.id` is the primary key of the PostgreSQL `node` table
  (`migrations/0001_core.sql`) with an immutability trigger. A node identifier
  therefore cannot exist in two documents of one installation. Portable typed
  states are document-scoped objects and *could* hold the same identifier twice, but
  such a state could never be applied to a centralized document. Relocation must
  mint new identifiers.
- Merge bases: criss-cross and disconnected histories are refused (`prepare_merge`
  requires a unique best common ancestor). This design keeps that refusal in v4.

## 1. Purpose, goals, non-goals, personas, metrics

### 1.1 Problem statement

Teams working on multiple documents reorganize: they move a methods section from
`Proposal.docx` to `Appendix.docx`, split a workbook, rename files, restructure
folders. Two people doing this concurrently produce histories that the current
engine can only merge by whole-file choice. Choosing a side silently drops the
other person's work, or the conflict list is unusable because identity was lost.

### 1.2 Goals

| ID | Goal |
|---|---|
| G1 | Rename, move-between-directories and edit combine without manual file surgery. |
| G2 | Nodes moved or copied between documents keep verifiable lineage, and concurrent edits follow the move when that is unambiguous. |
| G3 | Split and join of files are first-class, signed operations. |
| G4 | Project-level constraints are checked on every merge result, and violations are explicit conflicts naming every participant. |
| G5 | The set of conflicts that can never be auto-resolved is fixed, documented and enforced by code. |
| G6 | Merging is deterministic and orientation-independent (swapping the sides yields the same merged states). |
| G7 | Every automatic decision is recorded in the signed commit with a strategy identifier and re-verifiable by `verify-merge`. |

### 1.3 Non-goals

- Realtime collaborative editing or CRDT convergence across documents.
- Automatically deciding semantic equivalence across formats (a Word table versus
  an Excel range). Cross-format moves are not in scope.
- Merging criss-cross histories with virtual bases (tracked separately in the
  roadmap ledger; refused here).
- Browser authoring of project commits (decision 014).
- Rewriting native files. Native checkout of merged results is connector work.
- Heuristic identity binding as an automatic act. Similarity proposes, a person or
  an explicit relocation record binds.

### 1.4 Personas and use cases

| Persona | Use case |
|---|---|
| Research team lead | Moves the "Ethics" section into a shared appendix while a colleague edits it. |
| Analyst | Splits a 40-sheet workbook into regional workbooks while others fix formulas. |
| Programme coordinator | Renames a folder tree; teams keep editing inside. |
| Reviewer | Reviews a merge that touched five files and needs to see what moved where. |
| Compliance officer | Needs assurance that no content moved into a document with a wider audience without approval. |

### 1.5 Success metrics

| Metric | Target (hypothesis) |
|---|---|
| Rename + edit scenarios merged without manual step | 100 % in the corpus (families F1) |
| False automatic merges (merged state loses or duplicates content versus the oracle) | 0 across 10^6 generated scenarios |
| Conflicts that required a human in corpus families where an automatic rule is defined | 0 |
| Merge time, 100 files of 5,000 nodes each, 5 % changed | under 10 s on the reference laptop (excludes object fetch) |
| Reviewer comprehension | usability session: 80 % of testers correctly explain what moved where from the report |

## 2. Current state against target

| Capability | Tag | Evidence / path |
|---|---|---|
| Path-level three-way project plan, deterministic, order independent | EXISTS | `crates/engine-core/src/repository/compare.rs` (`plan`) |
| File/directory collision and shared-document-identity conflicts | EXISTS | `compare.rs` tests `different_paths_are_not_independent_when_they_share_identity_or_namespace`; `Commit::validate` in `repository.rs` |
| Rename = same document UUID at a new path in a commit | EXISTS | `repository.rs` ("Stable document UUIDs persist across path changes") |
| Rename on one side combined with an edit on the other | PARTIAL | reported as delete/edit at the old path (by code reading; untested); no valid non-lossy resolution via `prepare_merge` |
| Same-document node/field merge, both orientations must be clean | EXISTS | `convergence.rs` (`converge`, `merge_states`) |
| Automatic apply of clean same-file merges, signed v3 record, `verify-merge` | EXISTS | `repository/project_merge.rs`, `repository.rs` (v3), `local.rs` (`check_same_file_provenance`) |
| Rich-text run merge inside paragraphs | EXISTS | `crates/engine-core/src/richtext.rs` (`merge`, `merge_inline`) |
| Same-file merge limited to 64 files, one document at a time | EXISTS | `convergence.rs` `MAX_REPORTED_FILES` |
| Move/move or move/edit within one document | PARTIAL | `$location` three-way in `workspace_merge.rs`; no cycle check at merge level (checked later by `ops::validate_op` on apply) |
| Copy detection proposing bindings | EXISTS | `crates/engine-core/src/similarity.rs`, `crates/engine-api/src/copies.rs` |
| Node identifier uniqueness across documents | EXISTS (centralized only) | `migrations/0001_core.sql` `node.id primary key`, `node_id_no_reassign` |
| Cross-document node move, copy, split, join | MISSING | none; no lineage record |
| Relocation records in commits (v4) | MISSING | none |
| Project-level validators beyond namespace/identity | MISSING | integrity is per document (`integrity.rs`) |
| Path portability checks (case-fold, Unicode normalization, reserved names) | MISSING | `valid_path` checks separators, controls, `.`/`..` only |
| Virtual merge base for criss-cross | MISSING | refused in `prepare_merge` and `compare` |
| Project-wide merge in the web workspace | MISSING | per-file drafts only; decision 014 |

## 3. Architecture

### 3.1 Layers

```
                    +-----------------------------------------------------------+
  CLI / engine ---> | project merge orchestrator (new: plan_v2 + validators)    |
                    +--------+---------------------+----------------------------+
                             |                     |
              identity-keyed |                     | constraint validators
              file alignment |                     | (namespace, node-id, refs,
                             v                     v  policy, permission)
                    +----------------+     +--------------------------+
                    | relocation     |     | conflict taxonomy +      |
                    | resolver       |     | resolution file          |
                    | (edit-follows- |     +--------------------------+
                    |  move, split,  |
                    |  join)         |
                    +-------+--------+
                            |  per-document three-way
                            v
                    +----------------------------+
                    | node resolver framework    |  <- DESIGN-NODE-CONFLICT-RESOLUTION.md
                    | (converge today)           |
                    +----------------------------+
                            |
                            v   typed states (document-scoped objects)
```

### 3.2 Components

| Component | Responsibility | Today |
|---|---|---|
| Identity index | Map `document UUID -> (path, state root)` per commit; reject duplicate UUIDs. | implicit in `Commit::validate` |
| `plan_v2` | Three-way merge keyed by UUID; separate decisions for path, presence, state. | `plan` is path keyed |
| Lineage collector | Gather relocation records on the base..left and base..right ranges. | none |
| Relocation resolver | Map nodes through relocations; apply edit-follows-move; handle split/join. | none |
| Constraint engine | Evaluate C1..C7 (section 5.3) on the candidate project state. | namespace and identity only |
| Resolution file | Machine-readable choices for conflicts (section 7). | `--resolutions` path map |
| Recorder | Emit v4 commit with strategy ids and evidence. | v3 records |

### 3.3 Data flow of one merge

```
 base, left, right commits
        |
        v  1 index by UUID
 identity maps
        |
        v  2 per-UUID decisions: path / presence / state
 candidate file map + [NeedsDocMerge, RenameRename, DeleteEdit ...]
        |
        v  3 gather relocation records (left range, right range)
 relocation index
        |
        v  4 per-document merge through the resolver framework, with relocation mapping
 merged states + document conflicts
        |
        v  5 constraints C1..C7 on candidate
 violations -> conflicts
        |
        v  6 run in both orientations; clean only if equal
 plan { files, resolutions[], conflicts[] } (+ stored merged states when committing)
```

### 3.4 Trust boundaries

- The merging user must be able to read every document in the scope of the three
  commits. Servers already require current access to the whole transitive scope to
  read an envelope (`PRIVATE-PROJECT-TRANSFERS.md`).
- The merge result is signed by the merging user's device key. The server checks
  signature, scope and current permission on publication; it does not trust a
  client's claim that a rule was automatic: `verify-merge` re-runs the named
  strategies on the recorded inputs.
- Relocation records are claims by the author. A record is only *accepted* if both
  states are consistent with it (section 5.4).

## 4. Concepts and identity model

### 4.1 Identities

| Identity | Scope | Lifetime | Where enforced |
|---|---|---|---|
| Project UUID | installation | permanent | commit `project` field |
| Document UUID | project (unique per commit) | permanent across renames | `Commit::validate` |
| Path | project namespace | changes by rename | `valid_path`, collision checks |
| Node ID (ULID) | document (portable); installation (centralized `node` table) | permanent | `NodeId` in typed state; `node.id` primary key in PostgreSQL |
| Relocation ID | project | permanent | v4 record (proposed) |

### 4.2 Operations and how each is represented

| Operation | Representation today | Proposed representation |
|---|---|---|
| Rename file | same UUID, new path in file map | unchanged |
| Move file to another directory | same as rename | unchanged |
| Delete file | path absent | unchanged |
| Add file | new UUID | unchanged |
| Move subtree to another document | none (delete + create, no link) | `relocations[kind=move]` |
| Copy subtree to another document | none | `relocations[kind=copy]` (source stays) |
| Split file | none | `relocations[kind=split]`, plus new file entry |
| Join files | none | `relocations[kind=join]`, plus deleted file entry |

### 4.3 Why identifiers are re-minted

`node.id` is globally unique per installation in PostgreSQL. A portable state that
is later applied to a centralized document (the Projects adoption work) would
fail on insert if the same ULID existed in another document
(`crates/engine-api/src/ops/apply.rs`, `insert_node`). Therefore a relocated node
receives a fresh identifier derived deterministically from the relocation:

```
dest_id(n) = ulid_from( SHA-256( "dynodoc.node-derive\0v1\0"
                                  || relocation_id || source_node_id )[0..16] )
```

with the high bits cleared so the string form is a valid ULID (first character
`0`-`7`). Determinism lets two replicas that perform the same relocation
independently produce identical identifiers, so their concurrent results merge
without duplicates. The relocation ID is

```
relocation_id = SHA-256( "dynodoc.relocation\0v1\0" || from_document || from_root_node
                         || to_document || to_parent_node || to_position )
```

Two replicas moving the same node to the same slot converge. Moving it to
different slots produces two relocation IDs: both copies exist and the result is a
duplicate-relocation conflict (`XD-R7`). That is correct behaviour: the engine has
no basis to prefer one slot.

### 4.4 Rename semantics

A rename is a path change under a stable document UUID. Rules:

1. Identity is the UUID; the path is a label. The merge plans path and state
   independently (section 6.1).
2. Concurrent renames of the same UUID to different paths are a conflict
   (`XD-F3`), as today.
3. Rename on one side, edit on the other: merged file has the renamed path and the
   edited state. Automatic.
4. Rename of a directory is a set of per-file renames; a file added under the old
   directory on the other side is a *namespace* question: it keeps its path
   (the engine does not guess "directory rename detection" as Git does) and
   a warning `XD-N4` lists files whose siblings moved. Directory moves are a UI
   concept, not an engine identity.
5. Case-only renames and Unicode-normalization renames are ordinary renames; the
   portability validators (section 5.3, C7) flag collisions.

## 5. Specifications (proposed)

### 5.1 Commit format v4

Version 4 is a superset of v3 with a unified resolution array and relocation
records. Versions 1-3 never serialize the new fields; addresses and signatures of
existing commits are unchanged. The signature domain is
`dynodoc.project-commit-signature\0v4\0`; the address domain is
`dynodoc.project-commit\0v4\0`. Old readers reject v4 (unknown version), as they
reject v2 and v3 today; upgrade readers and servers before exchanging v4.

```json
{
  "version": 4,
  "project": "5b0f5b77-...",
  "parents": ["<left>", "<right>"],
  "files": { "Appendix.docx": {"document": "...", "state": "..."} },
  "author_key": "<hex>", "authored_at_ms": 1791000000000, "message": "Merge methods move",
  "project_resolutions": [
    { "id": "r1", "strategy": "dynodoc.project.rename-follow.v1",
      "document": "...", "base": "...", "left": "...", "right": "...", "merged": "...",
      "path": {"base": "Old.docx", "left": "New.docx", "right": "Old.docx", "merged": "New.docx"} },
    { "id": "r2", "strategy": "dynodoc.same-file.node-field.v1", "document": "...",
      "base": "...", "left": "...", "right": "...", "merged": "..." },
    { "id": "r3", "strategy": "dynodoc.project.edit-follows-move.v1",
      "relocation": "<relocation_id>", "document": "...", "base": "...", "left": "...",
      "right": "...", "merged": "..." }
  ],
  "relocations": [
    { "id": "<relocation_id>", "kind": "move",
      "from": {"document": "...", "root": "01H...A"},
      "to":   {"document": "...", "parent": "01H...B", "position": "a3"},
      "count": 14, "source_state_after": "<root>", "dest_state_after": "<root>" }
  ]
}
```

Limits: at most 256 relocations and 256 project resolutions per commit; canonical
bytes still at most 1 MiB. `count` is the number of nodes in the moved subtree and
is checked on import.

### 5.2 Relocation record invariants

| ID | Invariant |
|---|---|
| R1 | `from.document != to.document` for move/copy; for split `to.document` is a document that does not exist in the first parent; for join `from.document` is absent from the resulting file map. |
| R2 | In the commit's resulting states: for `move`, the subtree rooted at `from.root` is tombstoned (deleted) in the source state; for `copy` it is live in both. |
| R3 | The destination subtree has exactly `count` live nodes and every identifier equals `dest_id(source)`. |
| R4 | The destination parent exists, is live, and the node types satisfy `ops::validate_op` containment (`IllegalContainment`). |
| R5 | No pending suggestion targets a node in the subtree (otherwise `XD-R8`); comments anchored to subtree nodes are re-anchored by the same mapping. |
| R6 | Relocation IDs are unique within a commit and equal the formula in 4.3. |
| R7 | Subtree size limit: 10,000 nodes per record (larger moves use several records). |
| R8 | A relocation never targets a document the signer could not write under project rules at that commit (checked by the service, not by the portable verifier). |

### 5.3 Project constraints

| ID | Constraint | Status |
|---|---|---|
| C1 | Paths are unique; no path is both file and directory | EXISTS |
| C2 | Document UUIDs are unique per commit | EXISTS |
| C3 | A live node identifier appears in at most one document of the project | proposed |
| C4 | Every relocation record is consistent with the resulting states (5.2) | proposed |
| C5 | Typed cross-document references resolve to a live node, and the reference graph has no forbidden cycles | proposed; only where such reference kinds exist |
| C6 | Project policy: protected files modified only through a review that satisfies the floor; destination rules of the target document apply (`PROJECT-POLICY-AND-METADATA.md`) | proposed at publication |
| C7 | Checkout portability: no two paths collide under case folding or NFC normalization; no reserved names; length limits | proposed (warning by default; conflict when two *distinct* files fold equal) |
| C8 | Audience: content does not move from a document into a document readable by a wider audience without an explicit approval record | proposed (hosted); local projects use labels |

C3 is checked incrementally. Maintaining a per-document node-identifier summary
(a sorted index object referenced from the file entry) would avoid reading full
states; until then C3 is evaluated for documents touched by the merge only, plus a
cheap check against relocation destinations (all derived identifiers are new by
construction unless a relocation ID collides).

### 5.4 Strategy identifiers

| Strategy ID | Meaning |
|---|---|
| `dynodoc.same-file.node-field.v1` | EXISTS: current convergence |
| `dynodoc.project.rename-follow.v1` | path from one side, state from the other |
| `dynodoc.project.edit-follows-move.v1` | edits on the source side applied to relocated nodes |
| `dynodoc.project.split.v1`, `dynodoc.project.join.v1` | structural ops, each with sub-resolutions |
| `dynodoc.project.identical-relocation.v1` | both sides made the same relocation |

An importer accepts only strategy IDs it knows; unknown IDs are rejected (same rule
as `STRATEGY` in `convergence.rs`). The record does not re-run on import (so import
does not depend on the algorithm staying unchanged); `verify-merge` does.

## 6. Merge algorithm

### 6.1 `plan_v2` (identity keyed)

```
fn plan_v2(base, left, right) -> Plan
  I_b, I_l, I_r : uuid -> (path, state)
  for uuid in sorted(keys(I_b) ∪ keys(I_l) ∪ keys(I_r)):
      b, l, r = I_b[uuid], I_l[uuid], I_r[uuid]          # each may be None
      # presence
      if b.is_some():
          if l.is_none() && r.is_none():                    -> deleted on both: drop
          if l.is_none():  if r.state == b.state && r.path == b.path -> delete (left wins)
                           else                                  -> conflict XD-F2 (delete/edit)
          if r.is_none():  symmetric
      else:                 # added
          if l.is_some() && r.is_some():                         -> both added same UUID:
                 if l == r -> take; else conflict XD-F7 (add/add same identity, differing)
      # path
      path = three_way(b.path, l.path, r.path)   -> Take(p) | Conflict XD-F3
      # state
      state = three_way(b.state, l.state, r.state)
                -> Take(s) | NeedsDocMerge(b.state, l.state, r.state)
  candidate = { path -> (uuid, state) } for resolved uuids
  namespace check -> conflicts XD-F4 (two UUIDs, same path), XD-F5 (file/dir), XD-F6 (case/normalization)
```

`plan_v2` returns exactly the conflicts of `plan` for histories without renames
(property test T1 in section 10). It differs only where a UUID's path changed.

### 6.2 Per-document merge with relocations

1. Build the **relocation index** `R_L`, `R_R` from v4 records in commits between
   base and each side (bounded by `MAX_ANCESTORS`, read bodies only).
2. For each document with `NeedsDocMerge`, assemble three states.
3. If a relocation on one side moved nodes **out of** this document, compute
   `T = subtree(root)` at base; the other side's edits to nodes in `T` are
   *follow candidates*.
4. Apply the resolver framework to the *non-relocated* remainder as usual.
5. For follow candidates, build a virtual three-way over the destination: base =
   subtree at base mapped through `dest_id`; team = destination's relocated subtree;
   draft = the source side's edited subtree mapped through `dest_id`. Run node
   resolvers; accept only when clean.
6. Edits that created new children under `T` on the source side map to new
   deterministic identifiers (the same `dest_id` derivation, using the relocation
   ID and the new child's identifier).
7. Edits that deleted nodes of `T` delete the mapped destination nodes.

### 6.3 Edit-follows-move: automatic iff

| # | Condition |
|---|---|
| E1 | A signed relocation record exists for the subtree (not inferred by similarity). |
| E2 | The destination copy equals the base subtree mapped through `dest_id` for every node the source side edited (otherwise both sides edited: ordinary three-way on the mapped node, which may conflict). |
| E3 | No node outside `T` references a node inside `T` by an explicit reference changed in either side. |
| E4 | All strategies invoked for the mapped edits are automatic under the project policy. |
| E5 | Post-merge validators C1-C8 hold on the candidate, including C8 (audience): the *edit* lands in the destination document; its author's right to write there is checked by the service on publication. |

If any condition fails the result is `XD-R1` (move/edit) with participants listed.

### 6.4 Split and join

- **Split** `A -> A, A'`: the record lists the moved subtree roots and the new
  document UUID. Concurrent edit on the other side of a moved node follows
  (6.3). Concurrent edit of *document-level* properties of A (title,
  settings, root fields) stays with A. A concurrent second split of the same file
  into different documents is `XD-S2` (never automatic).
- **Join** `B -> A`: B's root children are relocated into A under a chosen parent;
  B is removed from the file map and its UUID tombstoned in the project. Edits to
  B's root-level fields (title, settings) have no destination and are reported as
  `XD-S3` with the lost values retained in the resolution record.
- Both produce a v4 record; `verify-merge` re-derives destination identifiers and
  states.

### 6.5 Orientation independence

The merge runs in both orientations (left/right swapped), exactly as `converge`
does for the same-file case. The merged project is clean only if both orientations
produce byte-identical merged states and the same path decisions. Tie-breaks
(5.1 `three_way` with equal-but-different values, same-position insertions) use
content-independent orders: document UUID, then node ID, never time or side.

### 6.6 Worked example

Base commit `B`: `Proposal.docx` (UUID `P`) with sections `S1`, `S2`, `S3`;
`Budget.xlsx` (UUID `X`). Left commit `L` and right commit `R` descend from `B`.

| Side | Action |
|---|---|
| Left | Moves section `S2` (nodes `n1..n14`) from `Proposal.docx` to a new file `Appendix.docx` (UUID `A`) as a `split`; renames `Budget.xlsx` to `Finance/Budget.xlsx` |
| Right | Edits paragraph `n5` in `S2` (text), edits cell `C4` in `Budget.xlsx`, adds paragraph `n20` at the end of `S2` |

Planning by path (today): `Budget.xlsx` is deleted on the left and edited on the
right, a delete/edit conflict, and `Finance/Budget.xlsx` appears with the old state.
`Proposal.docx` is edited on both sides: node merge sees `S2` deleted on the left
and `n5`, `n20` edited or added on the right: delete/edit conflicts on 14 nodes.

Planning by identity (`plan_v2`):

1. UUID `X`: path `Budget.xlsx -> Finance/Budget.xlsx` on the left only, state
   changed on the right only. Decision `rename-follow` (XD-F8): file
   `Finance/Budget.xlsx` with the right's state. Automatic.
2. UUID `P`: state changed on both sides, `NeedsDocMerge`. The lineage collector
   finds the left's split record `rho` (source `P`, roots `[S2]`, destination `A`).
   The nodes `n1..n14` are tombstoned on the left; the right's edits to `n5`
   (field `content`) and the new child `n20` are *follow candidates*.
3. Mapping: `dest_id(n5) = derive(rho, n5)`, and `dest_id(n20) = derive(rho, n20)`.
   Virtual three-way on `A`: base = `S2` at `B` mapped; team = `A` on the left;
   draft = right's `S2` mapped. `n5` content merges by the paragraph run merger;
   `n20` is a new child. Clean in both orientations.
4. Result: `Proposal.docx` = left's state (without `S2`); `Appendix.docx` = left's
   state plus the right's edits; `Finance/Budget.xlsx` with the right's cell edit.
   Constraints C1-C8 hold. Record: one `rename-follow`, one `edit-follows-move`
   (with `relocation = rho`), one `same-file` for `P` (remainder).
5. If the right had **deleted** `S2` instead of editing it, the pair is XD-R2
   (move versus delete): the left kept the content (as `A`), the right deleted
   it. Never automatic.

## 7. Conflict taxonomy

Legend for *Auto*: **A** always automatic when detected; **C** conditionally (the
conditions are listed); **N** never automatic.

### 7.1 File level

| ID | Name | Detected by | Auto | Rule / why |
|---|---|---|---|---|
| XD-F1 | Same document edited on both sides | per-UUID state three-way | C | Node merge through resolver framework; clean in both orientations |
| XD-F2 | Delete/edit file | presence | N | A person decides whether the edit or the deletion wins |
| XD-F3 | Concurrent rename to different paths | path three-way | N | No basis to prefer a name |
| XD-F4 | Two documents claim one path | namespace | N | Different identities; "add/add" |
| XD-F5 | File/directory collision | namespace | N | |
| XD-F6 | Collision under case folding or NFC | portability (C7) | N | Breaks checkout on common filesystems |
| XD-F7 | Same UUID added on both sides with different states | presence | C | Treat as two independent creations of one identity: node merge with empty base; auto only if clean |
| XD-F8 | Rename on one side, edit on the other | `plan_v2` | A | Rename-follow; no information lost |
| XD-F9 | Rename on one side, delete on the other | presence + path | N | Delete wins is lossy; person decides |

### 7.2 Relocation level

| ID | Name | Auto | Rule |
|---|---|---|---|
| XD-R1 | Move on one side, edit on the other | C | E1-E5 |
| XD-R2 | Move on one side, delete of source container on the other | N | |
| XD-R3 | Move to different destinations | N | |
| XD-R4 | Identical relocation on both sides | A | `identical-relocation` |
| XD-R5 | Destination container deleted on the other side | N | |
| XD-R6 | Copy on one side, edit of the source on the other | A | the copy has a snapshot; no interaction |
| XD-R7 | Duplicate relocation (same source, different slots) | N | Content duplicated; person chooses |
| XD-R8 | Pending suggestion targets the moved subtree | N | Accept or reject first; or re-anchor by tool |
| XD-R9 | Moved subtree referenced from outside | N | Reference semantics unknown unless typed (C5) |
| XD-R10 | Move whose destination crosses an audience boundary | N | C8 |

### 7.3 Split and join

| ID | Name | Auto | Rule |
|---|---|---|---|
| XD-S1 | Split on one side, edit of moved nodes on the other | C | E1-E5 |
| XD-S2 | Two different splits of one file | N | |
| XD-S3 | Join drops document-level properties | N | Values retained in record; person confirms |
| XD-S4 | Join where both documents contain the same node identifiers (copies) | N | Duplicates; person chooses |
| XD-S5 | Join/split vs rename | A | rename follows UUID |

### 7.4 Constraint level

| ID | Name | Auto | Rule |
|---|---|---|---|
| XD-C1 | Node identifier live in two documents | N | C3 |
| XD-C2 | Dangling cross-document reference | N | C5 |
| XD-C3 | Reference cycle across documents | N | |
| XD-C4 | Protected file modified without satisfying review floor | N | Not mergeable by one user; publication path enforces review |
| XD-C5 | Relocation record inconsistent with states | N | Rejects the commit |
| XD-C6 | Policy: destination rules stricter than source | N | Review required |

### 7.5 Which conflicts can never be auto-resolved (fixed list)

Delete versus edit (file or node); competing renames; competing destinations;
anything crossing an audience boundary; anything whose identity link rests only on
similarity; duplicate relocations; pending suggestion targets; competing splits;
any constraint violation in C3-C6, C8; any strategy not registered or not permitted
by project policy. This list is enforced by a closed enum in code: an unknown class
defaults to `N`.

### 7.6 Deterministic resolution rules

| ID | Rule |
|---|---|
| D1 | Swapping left and right yields the same merged state addresses. |
| D2 | Independent relocations commute: order of application does not change results. |
| D3 | Same-position insertion ties are ordered by node ID (ULID lexicographic), never by timestamp. |
| D4 | Idempotence: merging a result with either parent yields the result. |
| D5 | No silent loss: any value not present in the result is either unchanged-by-design or listed in the resolution record with a retrievable state root. |
| D6 | Associativity is not claimed (three-way merges are not associative in general). |
| D7 | Strategy versions are pinned in the commit; a policy change never reinterprets old commits. |
| D8 | Nothing crosses an audience boundary automatically. |

## 8. API, CLI and UI surface (proposed)

### 8.1 Engine CLI

```text
engine-cli repository DB PROJECT compare BASE LEFT RIGHT [--project-constraints] [--format json|text]
engine-cli repository DB PROJECT merge LEFT RIGHT [--same-file-merge clean]
        [--project-merge clean] [--resolutions FILE] --key KEY.der --message TEXT --authored-at-ms MS
engine-cli repository DB PROJECT relocate --from DOC:NODE --to DOC:PARENT[:POS] [--copy]
        --onto COMMIT --key KEY.der --message TEXT --authored-at-ms MS
engine-cli repository DB PROJECT split --document DOC --nodes NODE,... --new-path PATH
        --onto COMMIT --key KEY.der ...
engine-cli repository DB PROJECT join --from DOC --into DOC:PARENT --onto COMMIT --key KEY.der ...
engine-cli repository DB PROJECT verify-merge COMMIT        # now re-runs every strategy id in v4
engine-cli repository DB PROJECT lineage NODE               # relocation history of a node
```

`relocate`, `split` and `join` create ordinary one-parent commits containing a v4
`relocations` array; they do not move a branch (publish with `set-head`, as
`recover` and `merge` do today).

### 8.2 Resolution file v2

```json
{
  "version": 2,
  "choices": [
    { "conflict": "XD-F3:a1b2", "take": "left" },
    { "conflict": "XD-F2:c3d4", "take": "delete" },
    { "conflict": "XD-R3:e5f6", "take": "left" },
    { "conflict": "node:01H...:content", "take": "value",
      "value": { "type": "paragraph", "content": [ ... ] } }
  ]
}
```

Conflict IDs are `class:short-hash` of the participants, so they are stable across
runs. Every unresolved conflict is listed in the error; unknown IDs are rejected
(like unknown paths in `--resolutions` today). The v1 path map remains accepted for
file-level conflicts.

### 8.3 Report shape (excerpt)

```json
{
  "files": [ { "document": "...", "path": "Appendix.docx", "decision": "rename-follow",
               "path_from": "Old.docx" } ],
  "relocations": [ { "id": "...", "kind": "move", "count": 14,
                     "from": "Proposal.docx#01H...A", "to": "Appendix.docx#01H...B" } ],
  "conflicts": [ { "id": "XD-R3:e5f6", "class": "XD-R3", "auto": "never",
                   "participants": ["Proposal.docx", "Appendix.docx", "Annex.docx"],
                   "detail": "node moved to two different destinations" } ],
  "evidence": { "documents_read": 3, "nodes_compared": 5421, "orientations": 2 }
}
```

### 8.4 UI flows

| Surface | Flow |
|---|---|
| Terminal (authoritative) | `compare` -> read report -> edit resolutions -> `merge` -> inspect with `show` -> `set-head`. |
| Projects (web, hosted) | Read-only: a merge report renders as grouped conflict cards ("Moved", "Renamed", "Needs a decision"), each showing both documents side by side with the moved subtree highlighted. Authoring stays in the CLI per decision 014. |
| Office/Google add-ins | No cross-document operations. The add-in pulls the merged file result; relocated paragraphs appear as ordinary content in the destination file. |

Conflict card wireframe:

```
 Needs a decision (2)                          [Show only what moved]
 +---------------------------------------------------------------+
 | Moved to two places               XD-R3                       |
 |  "3.2 Sampling frame" (14 blocks)                             |
 |   Left   -> Appendix.docx, after "Annex A"                    |
 |   Right  -> Annex.docx,    before "Glossary"                  |
 |   [Keep left] [Keep right] [Keep both as copies] [Open diff]  |
 +---------------------------------------------------------------+
 | Edit in a moved section           XD-R1   (can be automatic)  |
 |  Blocked by: block 7 is referenced from Budget.xlsx           |
 +---------------------------------------------------------------+
```

## 9. Security and privacy

### 9.1 Threat model

| # | Threat | Category | Control | Residual |
|---|---|---|---|---|
| T1 | Content exfiltration by moving text into a document with a wider audience | Information disclosure | C8, XD-R10 never automatic; service checks destination rules; audit event on publish | Local-only projects have no ACL; labels are advisory |
| T2 | Forged relocation record claims lineage for unrelated nodes | Spoofing | R2/R3 checked against both states; derived identifiers must match | A signer can still relocate content they can read, which is their right |
| T3 | Identifier collision attack: craft a node whose derived ID collides with an existing node | Tampering | SHA-256 derived 128 bits; C3 check; import rejects duplicates | cryptographic assumption |
| T4 | Merge confuses documents with the same title | Spoofing | identity by UUID, never by title or similarity (decision 012) | |
| T5 | Resource exhaustion by many relocations | DoS | 256 records, 10,000 nodes per record; file budget; workload admission | service-level fairness not modelled |
| T6 | Lineage reveals structure of documents a reader should not see | Information disclosure | commit scope already requires access to all documents referenced | file-level sharing differs from commit-level scope: such users see the destination file only |
| T7 | Audit gap: automatic merge hides who changed what | Repudiation | v4 records name strategy and all three states; `verify-merge`; per-node authorship remains in each document's history | |

### 9.2 Privacy

Relocation and resolution records contain only UUIDs, ULIDs, paths (already in the
commit) and state roots. No text. Erasure of a document removes envelopes whose
scope includes it, including merge commits (existing behaviour,
[SHARED-PROJECT-HISTORY.md](SHARED-PROJECT-HISTORY.md)); lineage is therefore
erased together with the content it refers to.

### 9.3 Not claimed

- That a merged document means what either author intended in prose.
- That similarity-proposed links are correct.
- That formula references stay semantically valid across relocation except where a
  typed reference model exists.
- That merging is associative or that criss-cross histories merge.

## 10. Test corpus design

### 10.1 Scenario families

| Family | Content | Cases (initial) |
|---|---|---|
| F1 renames | rename only, rename + edit both orientations, rename vs delete, directory renames, case-only renames, concurrent renames | 120 |
| F2 relocation | move/edit, move/move, move/delete, container deleted, copy/edit, nested subtree, comments and suggestions attached | 200 |
| F3 split/join | split + edit, double split, join + title edit, join of copies | 80 |
| F4 constraints | node-ID collision, dangling reference, cycle, audience boundary, policy floor | 80 |
| F5 scale | 100 files x 5,000 nodes; 4,096 files; 10,000-node subtree | 10 |
| F6 formats | DOCX-derived, XLSX-derived (rows/cells), PPTX-derived (slides/shapes) states | 60 |
| F7 adversarial | forged relocation records, hostile Unicode (Bengali conjuncts, RTL, ZWJ), path portability traps | 100 |

### 10.2 Generators and oracle

1. **Reference model**: a small in-memory model of a project as
   `Map<DocUuid, Tree>` with explicit operations (rename, move, split, join, edit,
   delete). It is intentionally naive (no hashing, no objects).
2. **Scenario generator** (seeded, deterministic): produce base project; generate
   two operation sequences `E_L`, `E_R` from a grammar; apply each to produce the
   left and right commits through the real engine.
3. **Expected result**: model merge applies `E_L`, then `E_R` transformed
   (a hand-written transformation per pair class) and records whether the pair is
   *expected-automatic* (taxonomy `A` or `C` with conditions satisfied) or
   *expected-conflict* with class.
4. **Comparison**: engine plan versus model: same decision class, and when
   automatic, same logical result (compare reconstructed trees, not addresses).

### 10.3 Properties

| ID | Property |
|---|---|
| T1 | `plan_v2` equals `plan` on histories without renames |
| T2 | Orientation independence (D1) |
| T3 | Idempotence (D4) |
| T4 | No silent loss (D5): every node of either side is present or listed |
| T5 | Commutation of independent relocations (D2) |
| T6 | `verify-merge` reproduces every recorded strategy |
| T7 | Never-automatic list enforced: for every generated case in a class marked `N`, the engine reports a conflict |
| T8 | Conservation: node count of merged project equals the model's expectation |
| T9 | Import rejects every mutation of a record field (signature and consistency) |
| T10 | Differential: for single-document scenarios the result equals the existing `converge` output |

### 10.4 Fixtures

- Independent Python implementation of `dest_id`, `relocation_id` and the v4
  canonicalization, producing `project-relocation-v4.json` in the style of
  `project-merge-v3.json` (public synthetic seed; never regenerated to make a
  failing check pass).
- Native-derived corpora: the A3 edit matrix from
  [OPEN-RESEARCH-REGISTER.md](OPEN-RESEARCH-REGISTER.md) once real documents are
  collected (A4).

## 11. Delivery plan

### 11.1 Phases

| Phase | Deliverable | Depends on | Effort (person-weeks) | Confidence |
|---|---|---|---|---|
| P0 | Add regression tests documenting current rename+edit behaviour; v4 format spec; vector generator; strategy registry skeleton | none | 1.5 - 2 | high |
| P1 | `plan_v2`, rename-follow, report format, resolution file v2, `compare --project-constraints` | P0 | 3 - 4 | medium |
| P2 | Relocation records: creation commands, deterministic IDs, import validation (R1-R8), `lineage` | P0 | 4 - 5 | medium |
| P3 | Edit-follows-move, split, join, comment/suggestion handling | P1, P2, node resolver framework phase N2 | 5 - 6 | medium-low |
| P4 | Constraint engine C3-C8, path portability validators, audience labels | P1 | 3 - 4 | medium |
| P5 | Model-based corpus, property tests, benchmarks (F5) | P1-P4 incrementally | 4 - 5 | medium |
| P6 | Service: v4 acceptance in publication, scope/permission checks, audit events, API for report | P2, P4 | 4 - 5 | medium-low |
| P7 | Projects read-only report UI, docs, runbook | P6 | 2 - 3 | medium |
| Total | | | 26 - 34 | |

**Assumptions**: one engineer plus half a reviewer; node resolver framework phases
N1-N2 available; per-document state memory budget holds three states resident.
**Excludes**: browser authoring or WebAssembly build; cross-format moves;
criss-cross virtual bases; connector (add-in) support for relocation capture; large
(100+ MB) document support, which needs changed-node publication and an index
([DESIGN-LARGE-FILE-ACCEPTANCE.md](DESIGN-LARGE-FILE-ACCEPTANCE.md)); legal review
of audience labelling.

### 11.2 Acceptance criteria

| Phase | Acceptance |
|---|---|
| P0 | rename+edit test fails or passes as documented; vectors reproduce |
| P1 | T1, T2 on 10^5 generated histories; rename-follow scenarios clean |
| P2 | forged-record corpus rejected; derived IDs equal Python vectors |
| P3 | F2/F3 expected classes match; no `A`/`C` case lossy against the oracle |
| P4 | F4 cases flagged with exact participants; C7 warnings on Windows/macOS fold collisions |
| P6 | restricted-runtime-login tests: publication refuses a relocation into a document the signer cannot write; audit event emitted |

### 11.3 Rollout

1. Engine release with v4 readers only (no writers) so servers and clients upgrade
   in lockstep (v3 precedent: "upgrade every participating reader and server").
2. Writers behind `--project-merge clean` and the `relocate` commands.
3. Service accepts v4 after the production schema/migration rehearsal.
4. Projects shows reports; no hosted authoring.

### 11.4 Migration

No data migration. v1-v3 commits are untouched. Existing conflicting paths remain
resolvable with the v1 resolution map.

## 12. Operations

| Topic | Design |
|---|---|
| Observability | counters: merges by outcome, conflicts by class, relocations per commit, strategy ids used, orientation mismatches (should be zero), time per merge |
| Failure modes | missing object during merge (fail closed, no write), over budget (explicit `Limit` error listing which files were not examined), orientation mismatch (internal error, never auto), stale branch at publish (409 as today) |
| Rollback | v4 commits cannot be read by old readers; roll back by restoring a dump or by publishing a corrective merge commit; no history rewrite |
| Budgets | 64 files per merge today (`MAX_REPORTED_FILES`); proposal: configurable up to 512 with streaming state reads; 3 states resident per document |

## 13. Risks and open questions

### 13.1 Risks

| Risk | Mitigation |
|---|---|
| Deterministic ID derivation surprises users (same move twice yields same ID) | document; `lineage` command; restore semantics for tombstoned destination |
| Users mistake edit-follows-move for intention-preserving merge | UI labels "moved with edits"; report lists applied edits |
| Constraint checks are expensive | evaluate only touched documents; node-identifier index objects later |
| Format growth (v4) fragments readers | single strategy registry; vectors; version gates |
| Audience labels are unreliable locally | advisory only; hosted checks authoritative |

### 13.2 Open questions

| # | Question | Recommended default | Decided by |
|---|---|---|---|
| XD-Q1 | Relocation slot-based ID versus lineage-generation-based ID | slot-based (4.3) | Engineering lead |
| XD-Q2 | Should rename+delete (XD-F9) be automatic "delete wins"? | No; person decides | Product owner |
| XD-Q3 | Directory rename detection for added files | No; warn only (XD-N4) | Product owner |
| XD-Q4 | Where do audience labels live (project metadata vs file rules)? | file rules, strengthened-only like review floors | Product owner |
| XD-Q5 | Re-anchor pending suggestions automatically on move? | No in v1; require resolution first | Engineering lead |
| XD-Q6 | Per-file node-identifier index for C3 at scale | defer until F5 shows need | Engineering lead |
| XD-Q7 | Should hosted publication reject v4 commits if the author lacks write on any destination, or only warn? | reject | Security lead |
| XD-Q8 | Case-fold collisions as conflict versus warning | conflict only when two distinct documents collide | Product owner |

## Appendix A. Glossary

| Term | Meaning |
|---|---|
| Relocation | signed record that a subtree moved or was copied between documents |
| Edit-follows-move | applying one side's edits of moved nodes to their new location |
| Orientation | assignment of the two parents to "team" and "draft" roles during a merge |
| Participant | a file or node named in a conflict |
| Audience | set of principals who can read a document |

## Appendix B. References

| Source | Used for | Checked |
|---|---|---|
| Khanna, Kunal, Pierce, "A Formal Investigation of Diff3", FSTTCS 2007, LNCS 4855, pp. 485-496, https://www.cis.upenn.edu/~sanjeev/papers/fsttcs07_diff3.pdf | assumptions and failure modes of three-way merge | 14 October 2026 |
| Kleppmann, Mulligan, Gomes, Beresford, "A highly-available move operation for replicated trees", IEEE TPDS 33(7), 2021, https://martin.kleppmann.com/papers/move-op.pdf | cycle anomaly under concurrent moves; why merge validates acyclicity | 14 October 2026 |
| Repository: [03-versioning-mathematics-vol2.md](03-versioning-mathematics-vol2.md) D.2 | footprint theorem; why different IDs alone do not imply independence | in repository |
