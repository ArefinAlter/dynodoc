# 04 · Snapshot Engine and State Materialization

### Claude Code prompt to start this stage

> The event log is the system of record but reads must be O(snapshot tail), not O(N total events). This stage builds the snapshot engine and the materialization function that rebuilds current state from a snapshot plus a tail of events. Reference: Volume II §C.3.4 (snapshot-less event sourcing is rejected; snapshotted fold is the pattern) and §D.5 (storage equations). In `crates/engine-core`:
>
> 1. Implement `Materializer::fold(events: &[Event]) -> DocumentState` in `engine-core/src/materializer.rs`. `DocumentState` is a struct holding `HashMap<NodeId, Node>`. The fold applies each event's effect:
>    - `NodeCreated` → insert into the map.
>    - `FieldEdited` → update `current_fields` of the target node.
>    - `NodeMoved` → update `parent_id` and `position_key`.
>    - `NodeDeleted` → mark `deleted = true` (tombstone; don't remove from map).
>    - Comments, votes, suggestions, moderation decisions don't change the document tree directly; they're tracked in side maps (`SuggestionState`, `ModerationState`).
> 2. Implement `Materializer::from_snapshot(snapshot: &Snapshot, tail: &[Event]) -> DocumentState`: deserializes the snapshot's `nodes` JSONB to a `DocumentState`, then folds the tail events on top.
> 3. Implement `SnapshotEngine::take(document_id, reason) -> Snapshot` in `engine-core/src/snapshot.rs`:
>    - Reads all events from the last snapshot (or genesis) to the current latest seq.
>    - Folds them into a `DocumentState`.
>    - Computes a Merkle root over node hashes (Vol II §E.3.1) — for each node, `H(00 || canonical_json(node))`; internal nodes `H(01 || left || right)`; the tree is balanced binary, padding with zero hashes if node count is not a power of two.
>    - Serializes the state to JSONB.
>    - Inserts into `snapshot` table.
>    - For snapshots > 100 KB, write the JSONB to MinIO and store only a pointer (URI + content_hash) in Postgres.
> 4. Implement `SnapshotEngine::ensure_recent(document_id, max_events_in_tail)`: if more than `max_events_in_tail` events have been appended since the last snapshot, take a new one. Call this from the event-append happy path (after commit) so snapshots stay current automatically. Use a background tokio task to avoid blocking the appender.
> 5. Implement `SnapshotEngine::read_current_state(document_id) -> DocumentState`: finds the latest snapshot, reads the tail events since that snapshot, and returns the materialized state. This is the function the API server calls on every read.
> 6. Mandatory snapshot triggers: `Deployed`, `ConsultationOpened`, `ConsultationClosed` events must each trigger an immediate snapshot in the same transaction (or just after, with strong ordering). These are politically and legally important moments and need a clean checkpoint.
> 7. Tests:
>    - Property test: for any sequence of events, `fold(events)` equals `from_snapshot(snap_at_k, events[k..])` for any k. (Snapshots are correct.)
>    - Property test: changing one byte of one node's content changes the Merkle root. (Tamper evidence at the snapshot level.)
>    - Benchmark: materialization of a 1000-node document from a snapshot + 500-event tail completes in < 10 ms.

### Why snapshots are essential

The naive event-sourcing pattern — fold all events on every read — degrades linearly with consultation lifetime. After 600,000 events (a national consultation's worth), a read would touch all of them. That is unacceptable. Snapshots cap the read cost to `O(events since last snapshot)`, which we hold at a few hundred — a few milliseconds.

The cadence trade-off (Vol II §D.5):
- Snapshots every 500 events → reads fold at most 500 events → P99 read under 5 ms.
- Snapshot storage: at 100 KB per snapshot, 600,000 events / 500 = 1,200 snapshots = 120 MB per consultation. Trivial.
- Mandatory snapshots at high-stakes events make the audit story strong: "the deployed version is pinned, here is its Merkle root."

### The Merkle root deserves attention

The snapshot's `merkle_root` column is the cryptographic commitment to that snapshot's state. It is what the closeout report cites. It is what an external transparency log anchors. Compute it correctly (Vol II §E.3.1 — leaf prefix `0x00`, internal prefix `0x01`, balanced binary, deterministic ordering by node ULID).

```rust
fn merkle_root(nodes: &BTreeMap<NodeId, Node>) -> [u8; 32] {
    if nodes.is_empty() { return [0u8; 32]; }
    // Sort by ULID (stable, deterministic)
    let mut layer: Vec<[u8; 32]> = nodes.values()
        .map(|n| {
            let bytes = canonical_json(n);
            let mut hasher = Sha256::new();
            hasher.update(&[0x00]);
            hasher.update(&bytes);
            hasher.finalize().into()
        })
        .collect();
    // Pair up, hash, repeat until one root
    while layer.len() > 1 {
        if layer.len() % 2 == 1 { layer.push([0u8; 32]); }
        layer = layer.chunks(2).map(|pair| {
            let mut hasher = Sha256::new();
            hasher.update(&[0x01]);
            hasher.update(&pair[0]);
            hasher.update(&pair[1]);
            hasher.finalize().into()
        }).collect();
    }
    layer[0]
}
```

### Watch out for

**Non-deterministic JSON.** Same canonical-JSON warning from stage 03. Field order matters.

**Tombstones in materialized state.** Don't remove deleted nodes from the map; keep them with `deleted = true`. Concurrent events may reference them.

**Snapshot pointer indirection.** If you offload large snapshots to MinIO, the Postgres row holds a URI and a hash. The reader verifies the hash after fetching from object storage — if the bytes don't match, the snapshot is corrupted and you must rebuild from events.

### Verification checklist

- [ ] Property tests pass.
- [ ] A `Deployed` event triggers a snapshot atomically.
- [ ] Materialization benchmark hits target.
- [ ] Merkle root is reproducible across two independent materialization runs.
- [ ] Storing a snapshot > 100 KB to MinIO works; fetching and verifying integrity works.
