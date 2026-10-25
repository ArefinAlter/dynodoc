//! Stage-14 event-log integration tests (DB-backed).
//!
//! Each `#[sqlx::test(migrations = "../../migrations")]` provisions a fresh migrated
//! database. These exercise the real append path, the hash chain through the JSONB
//! round-trip, per-document linearization under concurrency, and tamper detection.
//! The pure verification *math* is property-tested in `src/log.rs` (no DB).
//!
//! Requires a reachable Postgres (DATABASE_URL): the dev container or CI service.

use engine_core::log::{
    append, append_in_tx, read_range, read_since_snapshot, verify_chain, verify_from, EventError,
    ZERO_HASH,
};
use engine_shared::{DocumentId, EventPayload, IdentityId, NodeId};
use serde_json::json;
use sqlx::PgPool;

/// Seed an identity, a document, and one node (so events may target it via the FK).
async fn seed(pool: &PgPool) -> sqlx::Result<(IdentityId, DocumentId, NodeId)> {
    let identity_id: uuid::Uuid = sqlx::query_scalar(
        "insert into identity (email, display_name) values ($1, $2) returning id",
    )
    .bind("author@dynodoc.local")
    .bind("Test Author")
    .fetch_one(pool)
    .await?;

    let document_id: uuid::Uuid =
        sqlx::query_scalar("insert into document (title, created_by) values ($1, $2) returning id")
            .bind("Sample Instrument")
            .bind(identity_id)
            .fetch_one(pool)
            .await?;

    let node_id = ulid::Ulid::generate().to_string();
    sqlx::query(
        "insert into node (id, document_id, parent_id, type, pos, current_fields, var_name)
         values ($1, $2, null, 'item', 'a0', '{}'::jsonb, $3)",
    )
    .bind(&node_id)
    .bind(document_id)
    .bind("q1")
    .execute(pool)
    .await?;

    Ok((
        IdentityId(identity_id),
        DocumentId(document_id),
        NodeId(node_id),
    ))
}

fn field_edit(node: &NodeId, field: &str, value: &str) -> EventPayload {
    EventPayload::FieldEdited {
        node_id: node.clone(),
        field: field.to_string(),
        value: json!(value),
    }
}

#[sqlx::test(migrations = "../../migrations")]
async fn append_builds_a_verifiable_chain(pool: PgPool) -> anyhow::Result<()> {
    let (actor, doc, node) = seed(&pool).await?;

    let e1 = append(
        &pool,
        doc,
        &EventPayload::NodeCreated {
            node_id: node.clone(),
            node_type: engine_shared::NodeType::Item,
            parent_id: None,
            pos: "a0".into(),
            fields: json!({ "label": "Seeded" }),
            var_name: Some("q1".into()),
        },
        actor,
    )
    .await?;
    let e2 = append(&pool, doc, &field_edit(&node, "label", "How many?"), actor).await?;
    let e3 = append(&pool, doc, &field_edit(&node, "hint", "in hours"), actor).await?;
    let e4 = append(
        &pool,
        doc,
        &EventPayload::Deployed { snapshot_id: None },
        actor,
    )
    .await?;

    // Genesis is seq 1 with a zero prev-hash; seq is gap-free and 1-based.
    assert_eq!((e1.seq, e2.seq, e3.seq, e4.seq), (1, 2, 3, 4));
    assert_eq!(e1.prev_chain_hash, ZERO_HASH.to_vec());
    assert_eq!(e2.prev_chain_hash, e1.chain_hash);
    assert_eq!(e3.prev_chain_hash, e2.chain_hash);
    assert_eq!(e4.prev_chain_hash, e3.chain_hash);

    // The document-level Deployed event carries no target.
    assert_eq!(e4.target_node_id, None);
    assert_eq!(
        e1.target_node_id.as_ref().map(|n| n.0.as_str()),
        Some(node.0.as_str())
    );

    // The whole chain verifies.
    verify_chain(&pool, doc).await?;
    Ok(())
}

#[sqlx::test(migrations = "../../migrations")]
async fn reads_ranges_and_snapshot_tail(pool: PgPool) -> anyhow::Result<()> {
    let (actor, doc, node) = seed(&pool).await?;
    for i in 0..5 {
        append(
            &pool,
            doc,
            &field_edit(&node, "label", &format!("v{i}")),
            actor,
        )
        .await?;
    }

    let mid = read_range(&pool, doc, 2, 4).await?;
    assert_eq!(mid.iter().map(|e| e.seq).collect::<Vec<_>>(), vec![2, 3, 4]);

    let tail = read_since_snapshot(&pool, doc, 3).await?;
    assert_eq!(tail.iter().map(|e| e.seq).collect::<Vec<_>>(), vec![4, 5]);

    // snapshot_seq 0 means "no snapshot" → the whole log.
    let all = read_since_snapshot(&pool, doc, 0).await?;
    assert_eq!(all.len(), 5);
    Ok(())
}

#[sqlx::test(migrations = "../../migrations")]
async fn appending_to_missing_document_errors(pool: PgPool) -> anyhow::Result<()> {
    let (actor, _doc, node) = seed(&pool).await?;
    let ghost = DocumentId(uuid::Uuid::from_u128(0xDEAD));
    let err = append(&pool, ghost, &field_edit(&node, "label", "x"), actor)
        .await
        .unwrap_err();
    assert!(matches!(err, EventError::DocumentNotFound(_)), "got: {err}");
    Ok(())
}

#[sqlx::test(migrations = "../../migrations")]
async fn concurrent_appends_get_distinct_consecutive_seqs(pool: PgPool) -> anyhow::Result<()> {
    let (actor, doc, node) = seed(&pool).await?;

    // Two appends racing on the same document. The document-row lock serializes
    // them, so they must land on distinct, consecutive seqs with no gap or dup.
    let (p1, p2) = (pool.clone(), pool.clone());
    let (n1, n2) = (node.clone(), node.clone());
    let h1 = tokio::spawn(async move { append(&p1, doc, &field_edit(&n1, "a", "1"), actor).await });
    let h2 = tokio::spawn(async move { append(&p2, doc, &field_edit(&n2, "b", "2"), actor).await });

    let s1 = h1.await??.seq;
    let s2 = h2.await??.seq;

    let mut seqs = [s1, s2];
    seqs.sort_unstable();
    assert_eq!(seqs, [1, 2], "expected consecutive seqs, got {seqs:?}");

    verify_chain(&pool, doc).await?;
    Ok(())
}

#[sqlx::test(migrations = "../../migrations")]
async fn verify_chain_detects_payload_tampering(pool: PgPool) -> anyhow::Result<()> {
    let (actor, doc, node) = seed(&pool).await?;
    append(&pool, doc, &field_edit(&node, "label", "original"), actor).await?;
    append(&pool, doc, &field_edit(&node, "label", "second"), actor).await?;
    verify_chain(&pool, doc).await?; // intact to start

    // Simulate an attacker with raw DB write access: the append-only trigger must be
    // bypassed to mutate a stored event, which is exactly what tamper-evidence guards.
    sqlx::query("alter table event disable trigger event_no_update")
        .execute(&pool)
        .await?;
    sqlx::query("update event set payload = $1 where document_id = $2 and seq = 2")
        .bind(json!({ "type": "FieldEdited", "node_id": node.0, "field": "label", "value": "FORGED" }))
        .bind(doc.0)
        .execute(&pool)
        .await?;
    sqlx::query("alter table event enable trigger event_no_update")
        .execute(&pool)
        .await?;

    match verify_chain(&pool, doc).await {
        Err(EventError::ChainBroken { seq, .. }) => assert_eq!(seq, 2),
        other => panic!("expected ChainBroken at seq 2, got {other:?}"),
    }
    Ok(())
}

#[sqlx::test(migrations = "../../migrations")]
async fn verify_from_checks_events_after_a_trusted_position(pool: PgPool) -> anyhow::Result<()> {
    let (actor, doc, node) = seed(&pool).await?;
    for i in 0..6 {
        append(
            &pool,
            doc,
            &field_edit(&node, "label", &format!("v{i}")),
            actor,
        )
        .await?;
    }
    let trusted: Vec<u8> =
        sqlx::query_scalar("select chain_hash from event where document_id = $1 and seq = 3")
            .bind(doc.0)
            .fetch_one(&pool)
            .await?;
    verify_from(&pool, doc, 0, &ZERO_HASH).await?;
    verify_from(&pool, doc, 3, &trusted).await?;

    let broken_at = |result: Result<(), EventError>| match result {
        Err(EventError::ChainBroken { seq, .. }) => seq,
        other => panic!("expected ChainBroken, got {other:?}"),
    };
    // A position that does not match, or does not exist, must not pass silently.
    assert_eq!(broken_at(verify_from(&pool, doc, 3, &[7u8; 32]).await), 3);
    assert_eq!(broken_at(verify_from(&pool, doc, 99, &trusted).await), 99);

    sqlx::query("alter table event disable trigger event_no_update")
        .execute(&pool)
        .await?;
    sqlx::query("update event set payload = jsonb_set(payload, '{value}', '\"FORGED\"') where document_id = $1 and seq = 5")
        .bind(doc.0)
        .execute(&pool)
        .await?;
    sqlx::query("alter table event enable trigger event_no_update")
        .execute(&pool)
        .await?;
    assert_eq!(broken_at(verify_from(&pool, doc, 3, &trusted).await), 5);
    Ok(())
}

#[sqlx::test(migrations = "../../migrations")]
async fn full_chain_verification_rejects_negative_sequence_rows(
    pool: PgPool,
) -> anyhow::Result<()> {
    let (actor, doc, node) = seed(&pool).await?;
    append(&pool, doc, &field_edit(&node, "label", "valid"), actor).await?;

    // The event table allows INSERTs with a negative sequence. Auditing from
    // genesis must inspect this malformed row, even alongside a valid chain.
    sqlx::query(
        "insert into event
         (id, document_id, seq, type, actor_id, payload, content_hash, prev_chain_hash, chain_hash)
         values ($1, $2, -1, 'FieldEdited', $3, '{}'::jsonb, $4, $5, $6)",
    )
    .bind(ulid::Ulid::generate().to_string())
    .bind(doc.0)
    .bind(actor.0)
    .bind(ZERO_HASH.as_slice())
    .bind(ZERO_HASH.as_slice())
    .bind([7u8; 32].as_slice())
    .execute(&pool)
    .await?;

    assert!(matches!(
        verify_chain(&pool, doc).await,
        Err(EventError::ChainBroken { seq: -1, .. })
    ));
    assert!(matches!(
        verify_from(&pool, doc, 0, &ZERO_HASH).await,
        Err(EventError::ChainBroken { seq: -1, .. })
    ));
    Ok(())
}

#[sqlx::test(migrations = "../../migrations")]
async fn verify_from_handles_sequence_boundaries_without_overflow(
    pool: PgPool,
) -> anyhow::Result<()> {
    let (actor, doc, node) = seed(&pool).await?;
    let event = append(&pool, doc, &field_edit(&node, "label", "anchor"), actor).await?;
    assert!(matches!(
        verify_from(&pool, doc, -1, &event.chain_hash).await,
        Err(EventError::ChainBroken { seq: -1, .. })
    ));
    assert!(matches!(
        verify_from(&pool, doc, 0, &event.chain_hash).await,
        Err(EventError::ChainBroken { seq: 0, .. })
    ));

    // A trusted final row can have an empty suffix. Verifying that suffix must
    // not compute MAX + 1; earlier rows are intentionally outside this audit.
    sqlx::query("alter table event disable trigger event_no_update")
        .execute(&pool)
        .await?;
    sqlx::query("update event set seq=$1 where document_id=$2")
        .bind(i64::MAX)
        .bind(doc.0)
        .execute(&pool)
        .await?;
    sqlx::query("alter table event enable trigger event_no_update")
        .execute(&pool)
        .await?;
    verify_from(&pool, doc, i64::MAX, &event.chain_hash).await?;
    Ok(())
}

/// Two events appended on one transaction are atomic and stay gap-free: the second
/// `append_in_tx` sees the first's row, so seq is 1,2 and the chain links. This is the
/// primitive the stage-16 governance accept relies on (wrapped event + SuggestionAccepted
/// in one transaction — FR-19 "atomically").
#[sqlx::test(migrations = "../../migrations")]
async fn append_in_tx_appends_two_events_atomically(pool: PgPool) -> anyhow::Result<()> {
    let (actor, doc, node) = seed(&pool).await?;

    let mut tx = pool.begin().await?;
    let e1 = append_in_tx(
        &mut tx,
        doc,
        &EventPayload::NodeCreated {
            node_id: node.clone(),
            node_type: engine_shared::NodeType::Item,
            parent_id: None,
            pos: "a0".into(),
            fields: json!({ "label": "Seeded" }),
            var_name: Some("q1".into()),
        },
        actor,
    )
    .await?;
    let e2 = append_in_tx(
        &mut tx,
        doc,
        &field_edit(&node, "label", "How many?"),
        actor,
    )
    .await?;
    tx.commit().await?;

    assert_eq!(
        (e1.seq, e2.seq),
        (1, 2),
        "seq is gap-free within the transaction"
    );
    assert_eq!(e1.prev_chain_hash, ZERO_HASH.to_vec());
    assert_eq!(
        e2.prev_chain_hash, e1.chain_hash,
        "the chain links across the two appends"
    );
    verify_chain(&pool, doc).await?;
    Ok(())
}

/// A transaction dropped without commit rolls back *both* appends — nothing reaches the
/// log. Atomicity holds in the failure direction too: a half-applied accept is impossible.
#[sqlx::test(migrations = "../../migrations")]
async fn append_in_tx_rolls_back_without_commit(pool: PgPool) -> anyhow::Result<()> {
    let (actor, doc, node) = seed(&pool).await?;

    {
        let mut tx = pool.begin().await?;
        append_in_tx(
            &mut tx,
            doc,
            &field_edit(&node, "label", "uncommitted"),
            actor,
        )
        .await?;
        append_in_tx(
            &mut tx,
            doc,
            &field_edit(&node, "hint", "uncommitted"),
            actor,
        )
        .await?;
        // tx dropped here without commit -> rollback.
    }

    let events = read_range(&pool, doc, 1, i64::MAX).await?;
    assert!(events.is_empty(), "rolled-back appends leave no events");
    Ok(())
}

#[sqlx::test(migrations = "../../migrations")]
async fn known_limit_created_at_not_hashed(pool: PgPool) -> anyhow::Result<()> {
    let (actor, doc, node) = seed(&pool).await?;
    let original = append(&pool, doc, &field_edit(&node, "label", "Synthetic"), actor).await?;
    // Only this disposable database owner bypasses the production append-only guard.
    let mut tx = pool.begin().await?;
    sqlx::query("alter table event disable trigger event_no_update")
        .execute(&mut *tx)
        .await?;
    let changed: chrono::DateTime<chrono::Utc> = sqlx::query_scalar(
        "update event set created_at=created_at+interval '1 day' where id=$1 returning created_at",
    )
    .bind(&original.id.0)
    .fetch_one(&mut *tx)
    .await?;
    sqlx::query("alter table event enable trigger event_no_update")
        .execute(&mut *tx)
        .await?;
    tx.commit().await?;
    assert_ne!(changed, original.created_at);
    let stored = read_range(&pool, doc, 1, 1).await?.remove(0);
    assert_eq!(stored.content_hash, original.content_hash);
    assert_eq!(stored.chain_hash, original.chain_hash);
    verify_chain(&pool, doc).await?;
    Ok(())
}

#[sqlx::test(migrations = "../../migrations")]
async fn known_limit_event_id_not_hashed(pool: PgPool) -> anyhow::Result<()> {
    let (actor, doc, _) = seed(&pool).await?;
    let original = append(
        &pool,
        doc,
        &EventPayload::CommentAdded {
            node_id: None,
            body: "Synthetic comment".into(),
        },
        actor,
    )
    .await?;
    let replacement = ulid::Ulid::generate().to_string();
    assert_ne!(replacement, original.id.0);
    let mut tx = pool.begin().await?;
    sqlx::query("alter table event disable trigger event_no_update")
        .execute(&mut *tx)
        .await?;
    sqlx::query("update event set id=$1 where id=$2")
        .bind(&replacement)
        .bind(&original.id.0)
        .execute(&mut *tx)
        .await?;
    sqlx::query("alter table event enable trigger event_no_update")
        .execute(&mut *tx)
        .await?;
    tx.commit().await?;
    let stored = read_range(&pool, doc, 1, 1).await?.remove(0);
    assert_eq!(stored.id.0, replacement);
    assert_eq!(stored.content_hash, original.content_hash);
    assert_eq!(stored.chain_hash, original.chain_hash);
    verify_chain(&pool, doc).await?;
    // The discussions reader uses event IDs as thread IDs. The base materializer
    // retains comment content only; this assertion is about the event commitment.
    Ok(())
}

#[sqlx::test(migrations = "../../migrations")]
async fn known_limit_chain_copy_to_other_document_verifies(pool: PgPool) -> anyhow::Result<()> {
    let (actor, source, _) = seed(&pool).await?;
    let original = append(
        &pool,
        source,
        &EventPayload::CommentAdded {
            node_id: None,
            body: "Synthetic unanchored comment".into(),
        },
        actor,
    )
    .await?;
    let other: uuid::Uuid = sqlx::query_scalar(
        "insert into document(title,created_by) values('Other synthetic document',$1) returning id",
    )
    .bind(actor.0)
    .fetch_one(&pool)
    .await?;
    sqlx::query("insert into event(id,document_id,seq,type,actor_id,target_node_id,payload,content_hash,prev_chain_hash,chain_hash,created_at) select $1,$2,seq,type,actor_id,target_node_id,payload,content_hash,prev_chain_hash,chain_hash,created_at from event where document_id=$3")
        .bind(ulid::Ulid::generate().to_string()).bind(other).bind(source.0).execute(&pool).await?;
    let copied = read_range(&pool, DocumentId(other), 1, 1).await?.remove(0);
    assert_ne!(copied.document_id, original.document_id);
    assert_ne!(copied.id, original.id);
    assert_eq!(copied.payload, original.payload);
    assert_eq!(copied.content_hash, original.content_hash);
    assert_eq!(copied.chain_hash, original.chain_hash);
    verify_chain(&pool, source).await?;
    verify_chain(&pool, DocumentId(other)).await?;
    Ok(())
}
