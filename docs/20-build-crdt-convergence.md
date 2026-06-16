# 09 · CRDT Convergence Layer (for live co-edit)

### Claude Code prompt to start this stage

> Add the CRDT layer for real-time co-editing of text fields and concurrent restructuring. Reference: Volume II §B (CRDT families and proofs), §C.2 (move CRDT), §D.2 (the disjoint-node commutativity theorem). In `crates/engine-core/src/crdt/`:
>
> 1. **Use Loro** (Rust CRDT library; implements text CRDT with no-interleaving guarantee plus move CRDT with cycle prevention plus fractional indexing). Don't hand-roll a CRDT. Crate: `loro` on crates.io.
> 2. **The CRDT lives alongside the event log, not instead of it**. The event log remains the system of record. The CRDT is a separate per-document state used only during *active co-editing sessions*. When all editors disconnect, the CRDT state is sealed into a snapshot and discarded.
> 3. **Routing operations to the CRDT layer**: only operations on text fields within a single node and concurrent edits to the same field of the same node use the CRDT path. Everything else (different nodes, different fields, structural ops) takes the disjoint-node path: append directly to the event log (Vol II §D.2 says these commute trivially).
> 4. **The session protocol**:
>    - When a user opens a document for editing, they join a "live session" if one exists or create one.
>    - The session loads the latest snapshot + tail events into a Loro document.
>    - Loro emits a binary "update" blob when local edits happen. The session broadcasts these to other connected editors via SSE.
>    - Each editor's Loro merges incoming updates into their local state — Loro guarantees convergence via the CRDT semantics.
>    - On session close (last editor leaves), the final Loro state is diffed against the snapshot and the diff is emitted as a sequence of semantic events (`FieldEdited`, `NodeMoved`, etc.) appended to the event log.
> 5. **Same-field same-node concurrent edits to scalar fields** (constraint, relevance, varName) bypass the CRDT layer and surface as a four-card conflict to the author (Vol II §D.2 register-clash case). Track these in `same_field_conflict` derived state.
> 6. **Move operations** use Loro's move CRDT with the Kleppmann cycle-check (Vol II §C.2.2). Cycle-creating moves become deterministic no-ops.
> 7. **Persistence**: every 5 seconds during an active session, persist the Loro state to a `live_session` table (one row per document with the binary blob). On crash recovery, sessions can resume from this state instead of replaying.
>
> Tests:
>    - Two editors concurrently typing in the same text field converge to a no-interleaving result.
>    - Concurrent moves that would create a cycle are deterministically resolved (the later-timestamped move is no-op'd on all replicas).
>    - On session close, the diff against snapshot produces exactly the right sequence of semantic events such that materialization reaches the same state.
>    - Property test: any sequence of concurrent ops, when sealed, produces an event sequence that materializes to the converged CRDT state.

### Why this is stage 09 and not earlier

The CRDT layer is needed only for *live co-editing*, which is a Phase-2 feature. Phases 0–1 ship without it. Many real-world consultations and even research-team workflows don't need it (one author at a time, edits committed via API, reasonable latency tolerance). Building this stage is a non-trivial investment; defer it until you have evidence the friction without it is hurting users.

### The seal-on-close pattern

This is the design's elegance. The CRDT is *ephemeral* — it exists only while editors are active. Once the session ends, the converged state is diffed against the prior snapshot and the diff is appended to the event log as ordinary semantic events. The audit trail captures the final outcome; the in-flight CRDT machinery doesn't pollute the log with operational chatter.

```
Snapshot S → live session opens
              ├─ Loro state initialized from S
              ├─ editors typing, moving, etc.
              ├─ Loro state evolves
              ├─ Loro state evolves
              └─ last editor leaves
                  ├─ compute diff(Loro_final, S) as semantic events
                  ├─ append events to log (chained, signed)
                  └─ take new snapshot S'
```

The events appended at seal time are attributed to the *author who triggered the seal* (typically the last editor out, or by configurable policy). This is honest — the CRDT layer merged contributions across editors during the session; the final commit reflects collective work.

Alternative: attribute each diff-event to its original author (Loro tracks the originator of each operation). More granular but more work.

### Watch out for

**Loro's API is binary-blob-centric.** You don't read out "operations"; you exchange opaque update blobs. This is fine but means the network protocol between editors is opaque to humans. Log enough metadata (session ID, sender, blob size, blob hash) for debugging.

**Memory cost of long-lived sessions.** A document edited heavily for hours can accumulate substantial CRDT metadata (operation log per text field). Loro compresses this on idle; force compaction on session close.

**The diff-to-events translation.** This is the trickiest part: turning a CRDT diff into a minimal sequence of semantic `FieldEdited`, `NodeMoved`, `NodeCreated`, `NodeDeleted` events. Use Loro's "checkout" mechanism to walk between states.

### Verification checklist

- [ ] Two browsers in the same session see each other's typing within 100ms.
- [ ] No-interleaving holds: a concurrent paste of "Hello" and "World" produces one or the other, not "HWeolrllod".
- [ ] Cycle-creating moves are no-op'd deterministically (both editors land in the same tree).
- [ ] On session close, the event log reflects the final state via a clean diff.
- [ ] Property tests pass.
