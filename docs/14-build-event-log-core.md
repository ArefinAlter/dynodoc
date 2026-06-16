# 03 · Event Log Core (the irreducible foundation)

### Claude Code prompt to start this stage

> This is the irreducible heart of the system. The event log is the system of record; all other state is derived from it. Reference: `dynodoc_versioning_complete.md` Volume II §D.2 (commutativity theorem) and §E.2 (hash chain + tamper-evidence theorem). For this stage in `crates/engine-core`:
>
> 1. Define the `Event` struct in `engine-shared` mirroring the database row, plus an `EventPayload` enum with variants for every semantic op_type: `NodeCreated`, `FieldEdited`, `NodeMoved`, `NodeDeleted`, `CommentAdded`, `VoteCast`, `SuggestionProposed`, `SuggestionAccepted`, `SuggestionRejected`, `ModerationDecided`, `Deployed`, `ConsultationOpened`, `ConsultationClosed`. Use serde with internally-tagged JSON.
> 2. Implement `EventLog::append(payload, author, parent_event) -> Event` in `engine-core/src/log.rs`. This function:
>    - Opens a serializable transaction.
>    - Reads the previous chain_hash with `SELECT chain_hash FROM event WHERE document_id = $1 ORDER BY seq DESC LIMIT 1 FOR UPDATE` (use `bytea` default of 32 zero bytes for the genesis case).
>    - Computes `content_hash = SHA256(op_type || target_node_id || target_field || canonical_json(payload) || author_id)`. Use canonical JSON (sorted keys, no whitespace) so the hash is reproducible.
>    - Computes `chain_hash = SHA256(content_hash || prev_chain_hash)`.
>    - Inserts the row with all hashes populated.
>    - Commits. Returns the inserted event.
> 3. Implement `EventLog::verify_chain(document_id) -> Result<()>` that walks every event in seq order, recomputes `content_hash` and `chain_hash`, and verifies they match what's stored. This is the tamper-detection function; it should be cheap (linear in N events) and is called by the audit tool.
> 4. Implement `EventLog::read_range(document_id, from_seq, to_seq) -> Vec<Event>` and `EventLog::read_since_snapshot(document_id, snapshot_seq) -> Vec<Event>` for replay use.
> 5. Add property-based tests using `proptest`:
>    - Appending N events and then verifying produces no errors.
>    - Tampering with any byte of any event's `payload` causes `verify_chain` to fail.
>    - Reordering events (which the trigger prevents, but test the verify logic) is detected.
>    - Two events appended concurrently get distinct, ordered seq numbers (use tokio::spawn to test).
> 6. Write benchmarks using `criterion`: measure single-event append latency (target: P99 < 5ms on a modest dev machine), batch append of 1000 events, full chain verify on 100k events (target: < 1 second).
>
> Use `ring::digest::SHA256` for hashing. Use canonical JSON via the `canonical-json` crate or implement it (sort object keys recursively, no whitespace).

### Why this is the most important stage

Everything else in the system rests on three properties this stage establishes:

**Property 1: events are immutable once written.** Enforced by the trigger from stage 02 *and* by the application never having an UPDATE/DELETE path. Any code that reads an event treats it as data, never as a thing to revise.

**Property 2: the chain detects tampering.** This is the tamper-evidence theorem from §E.2.1 made concrete. After this stage, anyone can `engine-cli verify <document_id>` and get cryptographic confirmation that the log has not been altered. This is the audit guarantee.

**Property 3: appends are linearizable per document.** The `FOR UPDATE` lock on the prior event row serializes appends within a document, which is what makes the chain consistent. Across documents, appends are independent and parallel.

### The append function in detail

```rust
// engine-core/src/log.rs (pseudocode shape; actual code uses sqlx::query!)
pub async fn append(
    pool: &PgPool,
    document_id: DocumentId,
    payload: EventPayload,
    target: Option<EventTarget>,
    author: AuthorContext,
    parent_event: Option<EventId>,
) -> Result<Event, EventError> {
    let mut tx = pool.begin().await?;

    // 1. Take the per-document write lock by reading the latest event
    let prev = sqlx::query!(
        "SELECT seq, chain_hash FROM event
         WHERE document_id = $1 ORDER BY seq DESC LIMIT 1 FOR UPDATE",
        document_id.0
    ).fetch_optional(&mut *tx).await?;

    let prev_chain_hash = prev.as_ref()
        .map(|r| r.chain_hash.clone())
        .unwrap_or_else(|| vec![0u8; 32]);

    // 2. Compute content hash
    let event_id = Ulid::new().to_string();
    let canonical = canonical_json(&payload)?;
    let content_hash = sha256_concat(&[
        payload.op_type().as_bytes(),
        target.as_ref().map(|t| t.node_id.as_bytes()).unwrap_or(&[]),
        target.as_ref().and_then(|t| t.field.as_deref()).unwrap_or("").as_bytes(),
        &canonical,
        author.id.0.as_bytes(),
    ]);

    // 3. Compute chain hash
    let chain_hash = sha256_concat(&[&content_hash, &prev_chain_hash]);

    // 4. Insert
    let row = sqlx::query_as!(EventRow,
        "INSERT INTO event (id, document_id, op_type, target_node_id, target_field,
                            payload, author_id, author_tier, parent_event_id,
                            content_hash, chain_hash, prev_chain_hash)
         VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9, $10, $11, $12)
         RETURNING *",
        event_id, document_id.0, payload.op_type(), target.as_ref().map(|t| &t.node_id),
        target.and_then(|t| t.field), canonical, author.id.0, author.tier as i16,
        parent_event.map(|p| p.0), content_hash, chain_hash, prev_chain_hash
    ).fetch_one(&mut *tx).await?;

    tx.commit().await?;
    Ok(row.into())
}
```

The hash inputs include the author and target precisely so that an attacker cannot rewrite an event's author or target after the fact; the chain commits to both.

### Watch out for

**Canonical JSON.** Two JSON serializers can produce different bytes for the same logical value (key order, whitespace, number formatting). The hash must be over a canonical form. Pick a library and lock it in.

**Bytea vs hex strings.** Store `chain_hash` as `bytea` (raw bytes), not hex strings. Saves space and is faster to compare. Hex only when displaying.

**Generating event IDs at exactly write time.** A ULID generated at `append` entry, before the lock, is correct (it embeds creation time). Don't generate it inside the lock — keeps the critical section short.

### Verification checklist

- [ ] Property tests pass (1000 iterations of tampering scenarios).
- [ ] `engine-cli verify <document_id>` runs and reports OK for a fresh document.
- [ ] Manually corrupting a `payload` byte in psql, then running verify, reports the corruption (and which event seq fails).
- [ ] Benchmark shows P99 append latency under 5ms on a dev machine with local Postgres.
- [ ] Concurrent append from two tokio tasks produces two distinct events with consecutive seq values (no gaps, no duplicates).
