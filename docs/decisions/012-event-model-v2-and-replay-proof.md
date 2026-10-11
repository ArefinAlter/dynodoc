# 012 — Proposed: event model v2 and a replay proof

**Status: proposed draft for owner review. Not accepted.** Nothing here changes
code, migrations or stored history until the owner accepts it. Keep this internal:
the patent items in `OPEN-RESEARCH-REGISTER.md` (L2–L3) still apply.

10 October 2026.

## Why

The event set becomes a public contract when the engine is open-sourced. It is
much cheaper to fix it before launch than after. Today's model has these gaps:

1. **The merger becomes the author.** Sharing a draft appends team events with
   the merger as actor (`engine-api/src/reviews.rs`). The real author survives
   only in `draft_edit`, which has no hash chain.
2. **Split and join lose authorship.** Enter and Backspace become create, delete
   and field-edit events. "Who wrote this sentence?" is wrong after them.
3. **The node table and replay disagree for choices.** `ChoiceRemoved` sets
   `deleted = true` in the node table, but replay keeps the choice live and puts
   it in `removed_choices`. (Re-adding a removed choice is rejected earlier, by
   validation, with `NodeExists`. This was found by reading code; a regression
   test is still needed.)
4. **Document metadata is outside the chain.** Title and settings are plain
   updates, so replay cannot rebuild them.
5. **Edits are not fully reversible from the log.** `FieldEdited` does not keep
   the old value. A `RichTextPatched` delete keeps a count, not the deleted text.
6. **No proof covers replay.** `03-versioning-mathematics-vol2.md` proves tamper
   evidence (§E.2.1) and disjoint-field commutativity (§D.2).
   `VERSIONING-CONFORMANCE.md` says §D.2 does not cover structural operations.
   Nothing proves that replay rebuilds what the author saw, or that the node
   table equals replay.

## Proposal

### 1. Compatibility: add, never rewrite

- Stored events are never converted. Their hashes include the payload, so any
  conversion would break the chain.
- New events use **event format v2**. Old (v1) events stay readable forever.
- New code stops writing the v1-only types (`ChoiceAdded`, `ChoiceRemoved`,
  `CommentReplied`, `Deployed`). Readers keep supporting them.
- Schema changes use new migrations only.

### 2. Event set written from v2 on (16 types)

| Group | Type | Payload rule |
|---|---|---|
| Content | `NodeCreated` | Unchanged. Choices become ordinary nodes of type `Choice`. |
| | `NodeMoved` | Also stores the old parent and position. |
| | `NodeDeleted` | Stores the deleted subtree (fields, children, positions). |
| | `NodeRestored` | Restores a tombstone with its original ID. Rejected if the node is live. |
| | `FieldEdited` | Stores `previous` and `value`. |
| | `RichTextPatched` | v2 delete ops carry the deleted text, not only a count. |
| | `NodeSplit` | Source node, new node ID, split offset in Unicode scalars, new position. The text after the offset moves with its authorship. |
| | `NodesJoined` | Kept node, removed node, the removed node's content and its join offset. |
| Assets | `AssetAdded` | Content address (SHA-256), media type and byte size. Blocks reference the asset ID. |
| Comments | `CommentAdded` | Comment ID, optional block anchor, optional `parent` for replies. |
| | `CommentResolved` | Explicit `resolved: true/false`. |
| Suggestions | `SuggestionProposed` | Unchanged. |
| | `SuggestionAccepted` | Records the decision. The resulting change is ordinary content events in the same transaction, each referencing the suggestion. |
| | `SuggestionRejected` | Unchanged. |
| History | `Committed` | Commit ID, parent commit IDs, last included `seq`, content root, annotation root, message. |
| | `Tagged` | Name, commit ID and kind. Kind `release` replaces `Deployed` and is allowed only after the integrity gate passes. |

**Why choices are nodes, not a field on the question.** If they were one field,
two people adding different choices to one question would conflict on the
whole list. As nodes, every choice keeps its own stable ID.

Table rows and cells (`Table → Row → Cell`) are a separate decision. They need
only new node types and nesting rules, not new event types.

### 3. Rules every v2 event follows

1. **Deterministic.** The same state plus the same event always gives the same
   result. No clock, randomness, network or locale is read during apply.
2. **Explicit values.** No toggles: `resolved: true`, never "flip".
3. **Reversible.** Every event stores enough to undo it without reading
   earlier history.
4. **IDs are never reused.** Deleted nodes become tombstones.
5. **Outcomes, not intents.** Accepted suggestions and imports log the change
   that happened, not the instruction that caused it.
6. **Validated before append.** Node and position IDs are checked for format and
   length. Parents must exist and be live. Nesting and acyclicity are checked.

### 4. Fields on every event

| Field | Today | v2 |
|---|---|---|
| `seq` | yes | unchanged |
| previous-event hash | yes: `chain_hash` already includes the previous event's `chain_hash` | **no new field needed** |
| `stream` | no | `content` or `annotation` |
| `author` | `actor_id` (who appended) | who made the change: a verified identity or an imported claim |
| `recorder` | no | the identity whose action appended the event (merger, importer) |
| timestamp | stored, not hashed | server time, hashed in v2 (see open questions) |
| target block | yes | unchanged |
| `hash_version` | no | `1` for existing events, `2` for new ones |

### 5. Hash v2

- `content_hash` v2 is the length-framed SHA-256 of: hash version, stream, type,
  target, canonical payload, author, recorder and timestamp.
- `chain_hash` keeps its current formula: `SHA-256(content_hash ‖ prev_chain_hash)`.
- The verifier chooses the formula by `hash_version`, so v1 events still verify.
- Minor fix: the comment in `migrations/0002_events.sql` gives the order as
  `prev ‖ content`, but the code uses `content ‖ prev`. Correct the documentation,
  not the code.

### 6. Two state roots

- The **chain head** changes with every event, because it covers the whole log.
- The **content root** is a Merkle root over content state only.
- The **annotation root** is a Merkle root over comments and suggestions.
- A comment changes the chain head and the annotation root, never the content
  root. "The document's hash" means the content root.
- A content-addressed Merkle tree for *storage* (sharing unchanged blocks
  between snapshots) is useful but is a separate decision. It is not needed for
  the proof.

### 7. Metadata, layout and import

- **Metadata.** Title, page setup and document fonts become `FieldEdited` on the
  root node. Access control and membership stay in the operational audit log.
- **Layout.** Pagination is not logged. Layout inputs (page setup, fonts,
  spacing) are stored. Identical page breaks across Word, LibreOffice and Google
  Docs are **not** guaranteed.
- **Import.** A `.docx` import expands into ordinary events inside one commit.
  Every log therefore starts from an empty document.
  - Tracked-change authors from the file are **claims**, not verified
    identities. The importer is the recorder.
  - File timestamps are stored in the payload as claims, not as event time.
  - Outside the guarantee: anything the importer cannot represent (see the
    LibreOffice findings L1–L6 in the ID engine report, which is kept outside
    the repo), plus threads, resolved
    comments and smart chips lost by Google's export.

### 8. Normalization

One canonical document form, applied identically in Rust and TypeScript:

- sorted mark order;
- adjacent text runs with equal marks are merged;
- no empty text runs;
- fixed attribute defaults;
- Unicode scalar counting;
- canonical JSON (sorted keys, fixed number format).

Shared fixtures run in both languages. This covers Dynodoc's own document model,
not raw Word XML.

## Proof plan (paper 1: linear history)

**Definitions.** The state is a map from ID to node, plus the two annotation maps.
`apply(state, event)` is one step. `replay(log)` folds from the empty state.
`project(state)` gives the node table rows.

**Invariants** that hold after every event:

| Invariant | Statement |
|---|---|
| I1 | Every live node's parent exists and is live |
| I2 | The parent graph has no cycle |
| I3 | Nesting rules hold |
| I4 | IDs are unique and never reused |
| I5 | Sibling positions are strictly ordered |
| I6 | Every state is in normal form |

**Lemmas**, each checked by a property test:

| Lemma | Statement |
|---|---|
| L1 | Every validated event preserves I1–I6 (case analysis over the 16 types) |
| L2 | `apply` is deterministic |
| L3 | Normal form is unique, and Rust and TypeScript produce identical canonical bytes |
| L4 | Patch round trip: applying `make_patch(x, y)` to `x` gives `y`, including formatting |
| L5 | `between(a, b)` lies strictly between `a` and `b` whenever `a < b`, under the collation the database uses |
| L6 | **Projection**: after every event, the node table equals `project(replay(log))` |
| L7 | Every v2 event is reversible from its own payload |

**Main theorem.** For every commit `c`, the content root of `replay(log up to c)`
equals the content root stored in `c`.

The theorem is meaningful only because the stored root is computed
**independently**. The editor computes it in the browser from what the author
saw, and the server recomputes it by replay. The theorem then says: *what the
author saw is what the server rebuilds.*

**Corollary.** Tamper detection follows from §E.2.1, assuming SHA-256 is
collision resistant.

**Scope.** Linear history only. Merge correctness, including when a merge keeps
the tree valid, is paper 2. §D.2 explicitly leaves this open.

## Fixes that do not wait for this decision

These are bugs under the current model:

- Make `ChoiceRemoved` agree between replay and the node table.
- Scope every node `UPDATE` by `document_id` (`FieldEdited`, `NodeMoved`,
  `NodeDeleted`, `ChoiceRemoved`).
- Make `between()` reject `a >= b`, and fix the trailing-space case: today
  `between("m", "m ")` returns `"m O"`, which sorts after `"m "`.
- Add the projection property test (L6) for the current 15 types.

## Rollout, one PR each

1. The fixes above.
2. Migration: add `hash_version`, `stream`, `recorder` and author-claim columns
   to `event`. Write hash v2.
3. Record recorder and author separately on merge.
4. `NodeSplit` and `NodesJoined` in the engine and editor.
5. Reversible payloads (`previous`, deleted subtree, deleted text).
6. Metadata as root-node fields. Choices as nodes. Write `Tagged` instead of
   `Deployed`.
7. `Committed`, with roots computed by both client and server.
8. Paper 1, with each lemma linked to its test.

## Open questions for the owner

1. Should the v2 event format ship **before** the engine's open-source launch?
   This proposal recommends yes.
2. Should the timestamp be inside the hash? Doing so makes timestamps
   tamper-evident, but the exact stored value (microsecond `timestamptz`) must
   then be hashed.
3. Should `RichTextPatched` store deleted text, or define reversibility relative
   to the checked base state? This proposal recommends storing the text: it costs
   space but keeps L7 local.
4. Branching: are `Committed` parent links enough, or is a separate commit DAG
   needed before paper 2?
5. Table rows and cells: write a separate decision (013) after this one.

## Consequences

- Storage grows because deletions and previous values are stored. Measure this
  on a synthetic footprint corpus before step 5 (the generator from the ID
  engine report is kept outside the repo).
- The OpenAPI schema and the change-bundle protocol (decision 010) need v2 event
  shapes.
- Readers must support v1 and v2 forever. Tests must cover both.
