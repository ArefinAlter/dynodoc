//! The append-only, hash-chained event log — the system of record (docs/14).
//!
//! All other state (the materialized `node`/`document` tree, snapshots) is a fold
//! over this log. Three properties make it trustworthy:
//!
//! 1. **Immutable.** The `event` table's triggers (0002_events.sql) forbid
//!    UPDATE/DELETE, and this module exposes no path that revises an event.
//! 2. **Tamper-evident.** Each event carries `content_hash` (over its semantic
//!    content) and `chain_hash = SHA-256(content_hash ‖ prev_chain_hash)`, genesis
//!    folding 32 zero bytes. [`verify_chain`] recomputes both for every event and
//!    detects any payload edit, reorder, or gap (docs/04 §E.2).
//! 3. **Linearizable per document.** [`append`] serializes a document's appends by
//!    taking a row lock on its `document` row, so `seq` is a gap-free 1-based
//!    sequence and the chain is consistent. Appends to *different* documents run in
//!    parallel.
//!
//! ## Hash construction (must stay stable — it is the audit contract)
//!
//! `content_hash` is SHA-256 over four length-framed fields, in order: the event
//! type string, the target node id (empty if none), the canonical-JSON payload, and
//! the actor's 16 UUID bytes. Length-framing (`u64` big-endian length before each
//! field) makes the concatenation unambiguous. The payload is canonicalized
//! (recursively sorted keys, no whitespace) so the digest is reproducible across
//! the JSONB round-trip.

use engine_shared::{DocumentId, Event, EventPayload, IdentityId};
use ring::digest::{Context, SHA256};
use serde_json::Value;
use sqlx::PgPool;
use uuid::Uuid;

use crate::snapshot::{self, SnapshotEngine, SnapshotReason, DEFAULT_MAX_EVENTS_IN_TAIL};

/// The genesis `prev_chain_hash`: 32 zero bytes, so the column stays NOT NULL.
pub const ZERO_HASH: [u8; 32] = [0u8; 32];

/// Errors from event-log operations.
#[derive(Debug, thiserror::Error)]
pub enum EventError {
    /// A database or transaction error.
    #[error("database error: {0}")]
    Db(#[from] sqlx::Error),
    /// The payload could not be serialized to JSON.
    #[error("payload serialization failed: {0}")]
    Serialize(#[from] serde_json::Error),
    /// No `document` row exists for the given id (nothing to append to).
    #[error("document {0} does not exist")]
    DocumentNotFound(Uuid),
    /// [`verify_chain`] found the stored chain inconsistent at this `seq`.
    #[error("chain broken at seq {seq}: {detail}")]
    ChainBroken { seq: i64, detail: String },
    /// The event committed but the required post-commit snapshot step failed.
    #[error("post-commit snapshot failed: {0}")]
    PostCommitSnapshot(String),
}

/// Append one semantic event to a document's log and return the inserted row.
///
/// Opens a transaction, locks the document row to serialize concurrent appends,
/// computes the hashes, and inserts with `seq = prev_seq + 1` (genesis `seq = 1`).
/// After the commit it runs the post-commit snapshot step (immediate for `Deployed`,
/// otherwise a best-effort background `ensure_recent`).
pub async fn append(
    pool: &PgPool,
    document_id: DocumentId,
    payload: &EventPayload,
    actor_id: IdentityId,
) -> Result<Event, EventError> {
    let mut tx = pool.begin().await?;
    let inserted = append_in_tx(&mut tx, document_id, payload, actor_id).await?;
    tx.commit().await?;

    post_commit_snapshot(pool, document_id, std::slice::from_ref(payload)).await?;
    Ok(inserted)
}

/// Append one event inside a caller-owned transaction, returning the inserted row
/// *without* committing or taking any snapshot.
///
/// This is the linearization primitive: it locks the document row, reads the prior
/// chain head, and inserts at `seq = prev_seq + 1`. Calling it twice on the same
/// transaction appends two events atomically (the governance `accept` path appends
/// the now-canonical wrapped event and the `SuggestionAccepted` event together —
/// docs/16 "two-event semantics"); the second call sees the first's row, so the
/// chain stays gap-free. The caller is responsible for `commit()` and for running
/// [`post_commit_snapshot`] afterward.
pub async fn append_in_tx(
    tx: &mut sqlx::Transaction<'_, sqlx::Postgres>,
    document_id: DocumentId,
    payload: &EventPayload,
    actor_id: IdentityId,
) -> Result<Event, EventError> {
    // Generate the event id outside the lock-sensitive work (a ULID embeds its own
    // creation time, so it is correct here and keeps the lock window short).
    let event_id = ulid::Ulid::new().to_string();

    let event_type = payload.event_type();
    let target_node_id = payload.target_node_id().map(|n| n.0.clone());
    let payload_value = serde_json::to_value(payload)?;
    let canonical = canonical_json(&payload_value);

    let content_hash = content_hash(
        event_type.as_str(),
        target_node_id.as_deref(),
        &canonical,
        &actor_id.0,
    );

    // Serialize all appends for this document. Locking the document row (rather than
    // the latest event row) is what actually makes appends linearizable: a lock on
    // the latest *existing* event does not stop a concurrent txn from selecting that
    // same row and computing a duplicate next `seq`.
    let exists: Option<(Uuid,)> =
        sqlx::query_as("select id from document where id = $1 for update")
            .bind(document_id.0)
            .fetch_optional(&mut **tx)
            .await?;
    if exists.is_none() {
        return Err(EventError::DocumentNotFound(document_id.0));
    }

    let prev: Option<(i64, Vec<u8>)> = sqlx::query_as(
        "select seq, chain_hash from event where document_id = $1 order by seq desc limit 1",
    )
    .bind(document_id.0)
    .fetch_optional(&mut **tx)
    .await?;

    let (prev_seq, prev_chain_hash) = match prev {
        Some((seq, hash)) => (seq, hash),
        None => (0, ZERO_HASH.to_vec()),
    };
    let next_seq = prev_seq + 1;
    let chain_hash = chain_hash(&content_hash, &prev_chain_hash);

    let inserted: Event = sqlx::query_as(
        "insert into event
           (id, document_id, seq, type, actor_id, target_node_id, payload,
            content_hash, prev_chain_hash, chain_hash)
         values ($1, $2, $3, $4, $5, $6, $7, $8, $9, $10)
         returning *",
    )
    .bind(&event_id)
    .bind(document_id.0)
    .bind(next_seq)
    .bind(event_type.as_str())
    .bind(actor_id.0)
    .bind(target_node_id)
    .bind(&payload_value)
    .bind(&content_hash[..])
    .bind(&prev_chain_hash[..])
    .bind(&chain_hash[..])
    .fetch_one(&mut **tx)
    .await?;

    Ok(inserted)
}

/// Run the post-commit snapshot step for a batch of just-committed events: take an
/// immediate snapshot if any event requires one (`Deployed`), otherwise spawn a
/// best-effort background `ensure_recent`. Call this once, after `commit()`.
pub async fn post_commit_snapshot(
    pool: &PgPool,
    document_id: DocumentId,
    payloads: &[EventPayload],
) -> Result<(), EventError> {
    if payloads.iter().any(snapshot::requires_immediate_snapshot) {
        SnapshotEngine::new(pool.clone())
            .take(document_id, SnapshotReason::Deployed)
            .await
            .map_err(|err| EventError::PostCommitSnapshot(err.to_string()))?;
    } else {
        snapshot::spawn_ensure_recent(pool.clone(), document_id, DEFAULT_MAX_EVENTS_IN_TAIL);
    }
    Ok(())
}

/// Replay a document's log in `seq` order, recompute every hash, and confirm it
/// matches what is stored. Linear in the number of events. Returns
/// [`EventError::ChainBroken`] naming the exact `seq` on the first inconsistency —
/// this is the function the auditor CLI calls.
pub async fn verify_chain(pool: &PgPool, document_id: DocumentId) -> Result<(), EventError> {
    let events: Vec<Event> =
        sqlx::query_as("select * from event where document_id = $1 order by seq asc")
            .bind(document_id.0)
            .fetch_all(pool)
            .await?;

    let mut prev_chain_hash = ZERO_HASH.to_vec();

    for (i, ev) in events.iter().enumerate() {
        let expected_seq = i as i64 + 1;
        if ev.seq != expected_seq {
            return Err(EventError::ChainBroken {
                seq: ev.seq,
                detail: format!(
                    "expected seq {expected_seq}, found {} (gap or reorder)",
                    ev.seq
                ),
            });
        }

        let canonical = canonical_json(&ev.payload);
        let recomputed_content = content_hash(
            &ev.event_type,
            ev.target_node_id.as_ref().map(|n| n.0.as_str()),
            &canonical,
            &ev.actor_id.0,
        );
        if recomputed_content[..] != ev.content_hash[..] {
            return Err(EventError::ChainBroken {
                seq: ev.seq,
                detail: "content_hash mismatch (type, target, payload, or actor altered)".into(),
            });
        }

        if ev.prev_chain_hash[..] != prev_chain_hash[..] {
            return Err(EventError::ChainBroken {
                seq: ev.seq,
                detail: "prev_chain_hash does not match the prior event's chain_hash".into(),
            });
        }

        let recomputed_chain = chain_hash(&recomputed_content, &prev_chain_hash);
        if recomputed_chain[..] != ev.chain_hash[..] {
            return Err(EventError::ChainBroken {
                seq: ev.seq,
                detail: "chain_hash mismatch".into(),
            });
        }

        prev_chain_hash = ev.chain_hash.clone();
    }

    Ok(())
}

/// Read events with `from_seq <= seq <= to_seq`, in `seq` order.
pub async fn read_range(
    pool: &PgPool,
    document_id: DocumentId,
    from_seq: i64,
    to_seq: i64,
) -> Result<Vec<Event>, EventError> {
    let events = sqlx::query_as(
        "select * from event
         where document_id = $1 and seq between $2 and $3
         order by seq asc",
    )
    .bind(document_id.0)
    .bind(from_seq)
    .bind(to_seq)
    .fetch_all(pool)
    .await?;
    Ok(events)
}

/// Read every event after a snapshot's `through_seq`, in `seq` order — the tail the
/// materializer folds onto a snapshot. Pass `0` to read the whole log.
pub async fn read_since_snapshot(
    pool: &PgPool,
    document_id: DocumentId,
    snapshot_seq: i64,
) -> Result<Vec<Event>, EventError> {
    let events =
        sqlx::query_as("select * from event where document_id = $1 and seq > $2 order by seq asc")
            .bind(document_id.0)
            .bind(snapshot_seq)
            .fetch_all(pool)
            .await?;
    Ok(events)
}

// --- hashing -------------------------------------------------------------------

/// SHA-256 over the event's semantic content, length-framed (see module docs).
fn content_hash(
    event_type: &str,
    target_node_id: Option<&str>,
    canonical_payload: &[u8],
    actor_id: &Uuid,
) -> [u8; 32] {
    let mut ctx = Context::new(&SHA256);
    feed(&mut ctx, event_type.as_bytes());
    feed(&mut ctx, target_node_id.unwrap_or("").as_bytes());
    feed(&mut ctx, canonical_payload);
    feed(&mut ctx, actor_id.as_bytes());
    finish(ctx)
}

/// `chain_hash = SHA-256(content_hash ‖ prev_chain_hash)`. Both inputs are
/// fixed-width 32-byte digests, so plain concatenation is unambiguous.
fn chain_hash(content_hash: &[u8], prev_chain_hash: &[u8]) -> [u8; 32] {
    let mut ctx = Context::new(&SHA256);
    ctx.update(content_hash);
    ctx.update(prev_chain_hash);
    finish(ctx)
}

/// Feed one length-framed field into the digest: `u64` big-endian length, then bytes.
fn feed(ctx: &mut Context, bytes: &[u8]) {
    ctx.update(&(bytes.len() as u64).to_be_bytes());
    ctx.update(bytes);
}

fn finish(ctx: Context) -> [u8; 32] {
    let digest = ctx.finish();
    let mut out = [0u8; 32];
    out.copy_from_slice(digest.as_ref());
    out
}

/// Canonical JSON: object keys sorted recursively, no insignificant whitespace, so
/// the same logical value always serializes to the same bytes (and the hash is
/// reproducible after the value round-trips through Postgres JSONB).
///
/// Note: JSONB normalizes numbers, so a payload with non-canonical numeric literals
/// (e.g. `1.0`) could in principle differ across the round-trip. PoC payloads are
/// string/object-shaped, so this is not hit; a production-grade decimal
/// canonicalization is a later hardening step.
pub(crate) fn canonical_json(value: &Value) -> Vec<u8> {
    let mut out = Vec::new();
    write_canonical(value, &mut out);
    out
}

fn write_canonical(value: &Value, out: &mut Vec<u8>) {
    match value {
        Value::Object(map) => {
            out.push(b'{');
            let mut keys: Vec<&String> = map.keys().collect();
            keys.sort();
            for (i, key) in keys.iter().enumerate() {
                if i > 0 {
                    out.push(b',');
                }
                write_json_string(key, out);
                out.push(b':');
                write_canonical(&map[*key], out);
            }
            out.push(b'}');
        }
        Value::Array(items) => {
            out.push(b'[');
            for (i, item) in items.iter().enumerate() {
                if i > 0 {
                    out.push(b',');
                }
                write_canonical(item, out);
            }
            out.push(b']');
        }
        // Scalars (null, bool, number, string) have a single compact serde form.
        scalar => serde_json::to_writer(out, scalar).expect("scalar JSON is infallible"),
    }
}

fn write_json_string(s: &str, out: &mut Vec<u8>) {
    serde_json::to_writer(out, &Value::String(s.to_string())).expect("string JSON is infallible");
}

#[cfg(test)]
mod tests {
    //! In-memory property tests for the verification *math*, no database required.
    //! These exercise the same `content_hash`/`chain_hash` functions that the
    //! DB-backed [`verify_chain`] uses; the DB integration lives in
    //! `tests/event_log_test.rs`.

    use super::*;
    use proptest::prelude::*;

    /// A standalone copy of the verify loop over synthetic rows, so the
    /// tamper/reorder properties can be checked without Postgres.
    #[derive(Clone)]
    struct Row {
        seq: i64,
        event_type: String,
        target: Option<String>,
        payload: Value,
        content_hash: Vec<u8>,
        prev_chain_hash: Vec<u8>,
        chain_hash: Vec<u8>,
    }

    /// Build a well-formed chain of `n` FieldEdited-shaped events for one actor.
    fn build_chain(actor: Uuid, payloads: &[Value]) -> Vec<Row> {
        let mut rows = Vec::new();
        let mut prev = ZERO_HASH.to_vec();
        for (i, payload) in payloads.iter().enumerate() {
            let canonical = canonical_json(payload);
            let ch = content_hash("FieldEdited", Some("node-1"), &canonical, &actor);
            let chain = chain_hash(&ch, &prev);
            rows.push(Row {
                seq: i as i64 + 1,
                event_type: "FieldEdited".into(),
                target: Some("node-1".into()),
                payload: payload.clone(),
                content_hash: ch.to_vec(),
                prev_chain_hash: prev.clone(),
                chain_hash: chain.to_vec(),
            });
            prev = chain.to_vec();
        }
        rows
    }

    /// The same checks `verify_chain` runs, over in-memory rows. Returns the first
    /// bad `seq`, or `None` if the chain is intact.
    fn verify_rows(rows: &[Row], actor: Uuid) -> Option<i64> {
        let mut prev = ZERO_HASH.to_vec();
        for (i, r) in rows.iter().enumerate() {
            let expected_seq = i as i64 + 1;
            if r.seq != expected_seq {
                return Some(r.seq);
            }
            let canonical = canonical_json(&r.payload);
            let ch = content_hash(&r.event_type, r.target.as_deref(), &canonical, &actor);
            if ch[..] != r.content_hash[..] {
                return Some(r.seq);
            }
            if r.prev_chain_hash[..] != prev[..] {
                return Some(r.seq);
            }
            let chain = chain_hash(&ch, &prev);
            if chain[..] != r.chain_hash[..] {
                return Some(r.seq);
            }
            prev = r.chain_hash.clone();
        }
        None
    }

    fn payloads_strategy() -> impl Strategy<Value = Vec<Value>> {
        // Realistic PoC shape: small objects of string fields. Avoids JSONB numeric
        // normalization concerns and keeps the canonical form stable.
        prop::collection::vec(
            prop::collection::hash_map("[a-z_]{1,8}", "[ -~]{0,24}", 1..4).prop_map(|m| {
                Value::Object(m.into_iter().map(|(k, v)| (k, Value::String(v))).collect())
            }),
            1..12,
        )
    }

    proptest! {
        #![proptest_config(ProptestConfig::with_cases(1000))]

        /// A freshly built chain always verifies.
        #[test]
        fn intact_chain_verifies(payloads in payloads_strategy()) {
            let actor = Uuid::from_u128(7);
            let rows = build_chain(actor, &payloads);
            prop_assert_eq!(verify_rows(&rows, actor), None);
        }

        /// Tampering with any byte of any event's payload is detected at that seq.
        #[test]
        fn payload_tamper_is_detected(
            payloads in payloads_strategy(),
            idx in 0usize..12,
        ) {
            let actor = Uuid::from_u128(7);
            let mut rows = build_chain(actor, &payloads);
            let target = idx % rows.len();
            // Mutate the stored payload without recomputing its hash.
            rows[target].payload = serde_json::json!({ "tampered": "yes" });
            prop_assert_eq!(verify_rows(&rows, actor), Some(rows[target].seq));
        }

        /// Swapping two adjacent events breaks the chain linkage.
        #[test]
        fn reorder_is_detected(payloads in payloads_strategy().prop_filter(
            "need at least two events", |p| p.len() >= 2)) {
            let actor = Uuid::from_u128(7);
            let mut rows = build_chain(actor, &payloads);
            rows.swap(0, 1);
            // verify_rows checks seq contiguity first; the swap makes seq[0] == 2.
            prop_assert!(verify_rows(&rows, actor).is_some());
        }
    }

    #[test]
    fn canonical_json_sorts_keys_and_is_stable() {
        let a = serde_json::json!({ "b": 1, "a": { "d": 2, "c": 3 } });
        let b = serde_json::json!({ "a": { "c": 3, "d": 2 }, "b": 1 });
        assert_eq!(canonical_json(&a), canonical_json(&b));
        assert_eq!(canonical_json(&a), br#"{"a":{"c":3,"d":2},"b":1}"#.to_vec());
    }

    #[test]
    fn genesis_chain_hash_folds_zero_prev() {
        let actor = Uuid::from_u128(1);
        let canonical = canonical_json(&serde_json::json!({ "x": "y" }));
        let ch = content_hash("FieldEdited", Some("n1"), &canonical, &actor);
        let chain = chain_hash(&ch, &ZERO_HASH);
        // Deterministic: recomputing gives the same digest.
        assert_eq!(chain, chain_hash(&ch, &ZERO_HASH));
        assert_ne!(chain, ch);
    }
}
