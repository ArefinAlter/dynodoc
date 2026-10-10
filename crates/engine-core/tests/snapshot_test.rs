//! Stage-15 snapshot/materialization integration tests.

use engine_core::{
    log::append,
    materializer::{DocumentState, Materializer},
    snapshot::{read_state_at, SnapshotEngine, SnapshotError, SnapshotReason},
};
use engine_shared::{DocumentId, EventPayload, IdentityId, NodeId};
use serde_json::json;
use sqlx::PgPool;

async fn seed(pool: &PgPool) -> anyhow::Result<(IdentityId, DocumentId, NodeId)> {
    let identity_id: uuid::Uuid = sqlx::query_scalar(
        "insert into identity (email, display_name) values ($1, $2) returning id",
    )
    .bind("author@dynodoc.local")
    .bind("Snapshot Author")
    .fetch_one(pool)
    .await?;

    let document_id: uuid::Uuid =
        sqlx::query_scalar("insert into document (title, created_by) values ($1, $2) returning id")
            .bind("Snapshot Instrument")
            .bind(identity_id)
            .fetch_one(pool)
            .await?;

    // The `event.target_node_id` FK requires the node row to exist. In the live
    // system the stage-16 op handlers insert the node row in the same transaction as
    // the NodeCreated event; here (stage-15 in isolation) we insert it directly so
    // the append FK is satisfied. The fold itself works purely off the event stream.
    let root = NodeId(ulid::Ulid::new().to_string());
    sqlx::query(
        "insert into node (id, document_id, parent_id, type, pos, current_fields, var_name)
         values ($1, $2, null, 'form', 'a0', '{}'::jsonb, null)",
    )
    .bind(&root.0)
    .bind(document_id)
    .execute(pool)
    .await?;

    append(
        pool,
        DocumentId(document_id),
        &EventPayload::NodeCreated {
            node_id: root.clone(),
            node_type: engine_shared::NodeType::Form,
            parent_id: None,
            pos: "a0".into(),
            fields: json!({ "label": "Form" }),
            var_name: None,
        },
        IdentityId(identity_id),
    )
    .await
    .map_err(|err| anyhow::anyhow!(err))?;

    Ok((IdentityId(identity_id), DocumentId(document_id), root))
}

fn field_edit(node: &NodeId, field: &str, value: &str) -> EventPayload {
    EventPayload::FieldEdited {
        node_id: node.clone(),
        field: field.to_string(),
        value: json!(value),
    }
}

#[sqlx::test(migrations = "../../migrations")]
async fn historical_reads_use_only_the_nearest_earlier_checkpoint(
    pool: PgPool,
) -> anyhow::Result<()> {
    let (actor, doc, node) = seed(&pool).await?;
    let engine = SnapshotEngine::new(pool.clone());
    for seq in 2..=9 {
        append(
            &pool,
            doc,
            &field_edit(&node, "title", &seq.to_string()),
            actor,
        )
        .await?;
        if seq == 3 || seq == 6 {
            engine.take(doc, SnapshotReason::Periodic).await?;
        }
    }

    // A newer checkpoint belonging to another document must never be selected.
    let other: uuid::Uuid = sqlx::query_scalar(
        "insert into document (title, created_by) values ('Other', $1) returning id",
    )
    .bind(actor.0)
    .fetch_one(&pool)
    .await?;
    for seq in 1..=9 {
        append(
            &pool,
            DocumentId(other),
            &EventPayload::CommentAdded {
                node_id: None,
                body: seq.to_string(),
            },
            actor,
        )
        .await?;
    }
    engine
        .take(DocumentId(other), SnapshotReason::Periodic)
        .await?;

    let events = sqlx::query_as::<_, engine_shared::Event>(
        "select * from event where document_id=$1 order by seq",
    )
    .bind(doc.0)
    .fetch_all(&pool)
    .await?;
    let mut connection = pool.acquire().await?;
    for seq in 0..=9 {
        let at = read_state_at(&mut connection, doc, seq).await?;
        let expected_snapshot = if seq >= 6 {
            6
        } else if seq >= 3 {
            3
        } else {
            0
        };
        assert_eq!(at.snapshot_seq, expected_snapshot);
        assert_eq!(at.replayed_events as i64, seq - expected_snapshot);
        assert_eq!(at.state, Materializer::fold(&events[..seq as usize])?);
    }
    Ok(())
}

#[sqlx::test(migrations = "../../migrations")]
async fn historical_reads_reject_revisions_outside_the_history(pool: PgPool) -> anyhow::Result<()> {
    let (_, doc, _) = seed(&pool).await?;
    let mut connection = pool.acquire().await?;
    for seq in [-1, 2, i64::MAX] {
        assert!(matches!(
            read_state_at(&mut connection, doc, seq).await,
            Err(SnapshotError::InvalidRevision(s)) if s == seq
        ));
    }
    assert_eq!(
        read_state_at(&mut connection, doc, 0).await?.state,
        DocumentState::default()
    );
    Ok(())
}

#[sqlx::test(migrations = "../../migrations")]
async fn take_and_read_current_state_round_trip(pool: PgPool) -> anyhow::Result<()> {
    let (actor, doc, node) = seed(&pool).await?;
    append(&pool, doc, &field_edit(&node, "title", "First"), actor).await?;
    append(&pool, doc, &field_edit(&node, "hint", "Helpful"), actor).await?;

    let engine = SnapshotEngine::new(pool.clone());
    let snapshot = engine.take(doc, SnapshotReason::Periodic).await?;
    let state = engine.read_current_state(doc).await?;

    assert_eq!(snapshot.through_seq, 3);
    assert_eq!(
        state.nodes.get(&node).unwrap().current_fields["title"],
        json!("First")
    );
    assert_eq!(
        state.nodes.get(&node).unwrap().current_fields["hint"],
        json!("Helpful")
    );
    Ok(())
}

#[sqlx::test(migrations = "../../migrations")]
async fn deployed_event_triggers_immediate_snapshot(pool: PgPool) -> anyhow::Result<()> {
    let (actor, doc, node) = seed(&pool).await?;
    append(&pool, doc, &field_edit(&node, "title", "Ready"), actor).await?;
    append(
        &pool,
        doc,
        &EventPayload::Deployed { snapshot_id: None },
        actor,
    )
    .await?;

    let latest_seq: Option<i64> =
        sqlx::query_scalar("select max(through_seq) from snapshot where document_id = $1")
            .bind(doc.0)
            .fetch_one(&pool)
            .await?;

    assert_eq!(latest_seq, Some(3));
    Ok(())
}

#[sqlx::test(migrations = "../../migrations")]
async fn ensure_recent_takes_snapshot_after_threshold(pool: PgPool) -> anyhow::Result<()> {
    let (actor, doc, node) = seed(&pool).await?;
    let engine = SnapshotEngine::new(pool.clone());

    append(&pool, doc, &field_edit(&node, "a", "1"), actor).await?;
    append(&pool, doc, &field_edit(&node, "b", "2"), actor).await?;
    engine.ensure_recent(doc, 1).await?;

    let latest_seq: Option<i64> =
        sqlx::query_scalar("select max(through_seq) from snapshot where document_id = $1")
            .bind(doc.0)
            .fetch_one(&pool)
            .await?;

    assert_eq!(latest_seq, Some(3));
    Ok(())
}

// Materialization correctness at scale (1000-node snapshot + 500-event tail). The
// wall-clock perf target (<10 ms, release) is the deferred `criterion` bench, not a
// debug `cargo test` assertion that flakes on slow CI runners.
#[test]
fn materializes_large_document_from_snapshot_plus_tail() -> anyhow::Result<()> {
    let mut nodes = std::collections::BTreeMap::new();
    for idx in 0..1000 {
        let node_id = NodeId(format!("01SNAP{:08}", idx));
        nodes.insert(
            node_id.clone(),
            engine_core::materializer::MaterializedNode {
                id: node_id,
                parent_id: None,
                node_type: "item".into(),
                pos: format!("a{idx:04}"),
                current_fields: json!({ "label": format!("Question {idx}") }),
                var_name: Some(format!("q_{idx}")),
                deleted: false,
            },
        );
    }

    let snapshot_value = serde_json::to_value(DocumentState {
        nodes,
        comments: Vec::new(),
        suggestions: std::collections::BTreeMap::new(),
        removed_choices: std::collections::HashSet::new(),
    })?;
    let snapshot: engine_shared::Snapshot = engine_shared::Snapshot {
        id: engine_shared::SnapshotId(uuid::Uuid::nil()),
        document_id: DocumentId(uuid::Uuid::nil()),
        through_seq: 1000,
        state: snapshot_value,
        merkle_root: vec![0; 32],
        event_chain_hash: vec![0; 32],
        created_at: chrono::Utc::now(),
    };

    let mut tail = Vec::new();
    for idx in 0..500 {
        tail.push(engine_shared::Event {
            id: engine_shared::EventId(format!("01EVT{idx:08}")),
            document_id: DocumentId(uuid::Uuid::nil()),
            seq: 1001 + idx,
            event_type: "FieldEdited".into(),
            actor_id: IdentityId(uuid::Uuid::nil()),
            target_node_id: Some(NodeId(format!("01SNAP{:08}", idx % 1000))),
            payload: serde_json::to_value(EventPayload::FieldEdited {
                node_id: NodeId(format!("01SNAP{:08}", idx % 1000)),
                field: "label".into(),
                value: json!(format!("Updated {idx}")),
            })?,
            content_hash: vec![0; 32],
            prev_chain_hash: vec![0; 32],
            chain_hash: vec![0; 32],
            created_at: chrono::Utc::now(),
        });
    }

    let state = engine_core::materializer::Materializer::from_snapshot(&snapshot, &tail)?;

    assert_eq!(state.nodes.len(), 1000);
    // The tail re-edited node 0's "label"; confirm the fold applied it.
    assert_eq!(
        state.nodes[&NodeId("01SNAP00000000".to_string())].current_fields["label"],
        json!("Updated 0")
    );
    Ok(())
}
use engine_core::snapshot_verification::{self, verify_document};

async fn legacy_fixture(
    pool: &PgPool,
    doc: DocumentId,
    seq: i64,
) -> anyhow::Result<engine_shared::Snapshot> {
    let events = engine_core::log::read_range(pool, doc, 1, seq).await?;
    let state = Materializer::fold(&events)?;
    let chain = events.last().map_or(vec![0; 32], |e| e.chain_hash.clone());
    Ok(sqlx::query_as("insert into snapshot(document_id,through_seq,state,merkle_root,event_chain_hash) values($1,$2,$3,$4,$5) returning *")
        .bind(doc.0).bind(seq).bind(json!(state)).bind(engine_core::snapshot::merkle_root(&state)?.as_slice())
        .bind(chain).fetch_one(pool).await?)
}

#[sqlx::test(migrations = "../../migrations")]
async fn legacy_backfill_is_explicit_and_preserves_bounded_ordinary_replay(
    pool: PgPool,
) -> anyhow::Result<()> {
    let (actor, doc, node) = seed(&pool).await?;
    let first = legacy_fixture(&pool, doc, 1).await?;
    for seq in 2..=5 {
        append(
            &pool,
            doc,
            &field_edit(&node, "title", &seq.to_string()),
            actor,
        )
        .await?;
    }
    let later = legacy_fixture(&pool, doc, 4).await?;
    let report = verify_document(&pool, doc, false).await?;
    assert_eq!((report.snapshots, report.replayed_events), (2, 5));
    let mut connection = pool.acquire().await?;
    assert!(matches!(
        read_state_at(&mut connection, doc, 5).await,
        Err(SnapshotError::Unverified { .. })
    ));
    let report = verify_document(&pool, doc, true).await?;
    assert_eq!((report.snapshots, report.replayed_events), (2, 5));
    for snapshot in [&first, &later] {
        assert_eq!(
            snapshot_verification::load(&mut connection, doc, snapshot.id.0)
                .await?
                .as_ref(),
            Some(snapshot)
        );
    }
    let at = read_state_at(&mut connection, doc, 5).await?;
    assert_eq!((at.snapshot_seq, at.replayed_events), (4, 1));
    assert_eq!(at.state.nodes[&node].current_fields["title"], "5");
    // Idempotent reruns never rewrite the receipts or snapshots.
    let before: serde_json::Value = sqlx::query_scalar(
        "select jsonb_agg(to_jsonb(v) order by snapshot_id) from snapshot_verification v",
    )
    .fetch_one(&pool)
    .await?;
    verify_document(&pool, doc, true).await?;
    let after: serde_json::Value = sqlx::query_scalar(
        "select jsonb_agg(to_jsonb(v) order by snapshot_id) from snapshot_verification v",
    )
    .fetch_one(&pool)
    .await?;
    assert_eq!(before, after);
    Ok(())
}

#[sqlx::test(migrations = "../../migrations")]
async fn legacy_backfill_rejects_non_node_corruption_and_rolls_back_all_receipts(
    pool: PgPool,
) -> anyhow::Result<()> {
    let (actor, doc, _) = seed(&pool).await?;
    legacy_fixture(&pool, doc, 1).await?;
    append(
        &pool,
        doc,
        &EventPayload::CommentAdded {
            node_id: None,
            body: "Actual review".into(),
        },
        actor,
    )
    .await?;
    let bad = legacy_fixture(&pool, doc, 2).await?;
    // Simulate an already-corrupt old row, not an ordinary supported operation.
    sqlx::query("alter table snapshot disable trigger snapshot_no_update")
        .execute(&pool)
        .await?;
    sqlx::query("update snapshot set state=jsonb_set(state,'{comments}','[]') where id=$1")
        .bind(bad.id.0)
        .execute(&pool)
        .await?;
    sqlx::query("alter table snapshot enable trigger snapshot_no_update")
        .execute(&pool)
        .await?;
    engine_core::log::verify_chain(&pool, doc).await?;
    assert!(matches!(
        verify_document(&pool, doc, true).await,
        Err(SnapshotError::Unverified { seq: 2, .. })
    ));
    let count: i64 = sqlx::query_scalar("select count(*) from snapshot_verification")
        .fetch_one(&pool)
        .await?;
    assert_eq!(count, 0, "the valid earlier receipt must also roll back");
    assert!(matches!(
        verify_document(&pool, doc, false).await,
        Err(SnapshotError::Unverified { seq: 2, .. })
    ));
    Ok(())
}

#[sqlx::test(migrations = "../../migrations")]
async fn snapshot_guards_allow_only_audited_whole_document_erasure(
    pool: PgPool,
) -> anyhow::Result<()> {
    let (actor, doc, _) = seed(&pool).await?;
    let other = DocumentId(
        sqlx::query_scalar(
            "insert into document(title,created_by) values('Other',$1) returning id",
        )
        .bind(actor.0)
        .fetch_one(&pool)
        .await?,
    );
    let engine = SnapshotEngine::new(pool.clone());
    let snapshot = engine.take(doc, SnapshotReason::Periodic).await?;
    let survivor = engine.take(other, SnapshotReason::Periodic).await?;
    for query in [
        "update snapshot set state=state where document_id=$1",
        "delete from snapshot where document_id=$1",
        "update snapshot_verification set fingerprint=fingerprint where document_id=$1",
        "delete from snapshot_verification where document_id=$1",
        "delete from document where id=$1",
    ] {
        assert!(
            sqlx::query(query).bind(doc.0).execute(&pool).await.is_err(),
            "{query}"
        );
    }
    for query in [
        "truncate snapshot cascade",
        "truncate snapshot_verification",
    ] {
        assert!(sqlx::query(query).execute(&pool).await.is_err(), "{query}");
    }
    let mut connection = pool.acquire().await?;
    assert!(
        snapshot_verification::load(&mut connection, other, snapshot.id.0)
            .await?
            .is_none()
    );
    sqlx::query("update document set deleted_at=now() where id=$1")
        .bind(doc.0)
        .execute(&pool)
        .await?;
    let mut tx = pool.begin().await?;
    sqlx::query("insert into erasure_receipt(resource_id,kind,actor_id,reason,counts) values($1,'documents',$2,'owner_request','{}')")
        .bind(doc.0).bind(actor.0).execute(&mut *tx).await?;
    sqlx::query("delete from document where id=$1")
        .bind(doc.0)
        .execute(&mut *tx)
        .await?;
    tx.commit().await?;
    let remaining: i64 =
        sqlx::query_scalar("select count(*) from snapshot_verification where document_id=$1")
            .bind(doc.0)
            .fetch_one(&pool)
            .await?;
    assert_eq!(remaining, 0);
    assert_eq!(
        snapshot_verification::load(&mut connection, other, survivor.id.0).await?,
        Some(survivor)
    );
    Ok(())
}

#[sqlx::test(migrations = "../../migrations")]
async fn receipts_bind_state_and_envelope_even_if_snapshot_guard_is_bypassed(
    pool: PgPool,
) -> anyhow::Result<()> {
    let (_, doc, _) = seed(&pool).await?;
    let snapshot = SnapshotEngine::new(pool.clone())
        .take(doc, SnapshotReason::Periodic)
        .await?;
    for query in [
        "update snapshot set state=jsonb_set(state,'{comments}','[{\"node_id\":null,\"body\":\"forged\"}]') where id=$1",
        "update snapshot set merkle_root=decode(repeat('00',32),'hex') where id=$1",
        "update snapshot set event_chain_hash=decode(repeat('00',32),'hex') where id=$1",
        "update snapshot set through_seq=0 where id=$1",
        "update snapshot set created_at=created_at+interval '1 day' where id=$1",
    ] {
        let mut tx = pool.begin().await?;
        sqlx::query("alter table snapshot disable trigger snapshot_no_update").execute(&mut *tx).await?;
        sqlx::query(query).bind(snapshot.id.0).execute(&mut *tx).await?;
        assert!(matches!(snapshot_verification::load(&mut tx, doc, snapshot.id.0).await, Err(SnapshotError::Unverified {..})), "{query}");
        tx.rollback().await?;
    }
    Ok(())
}

#[sqlx::test(migrations = "../../migrations")]
async fn concurrent_snapshot_creation_produces_one_verified_row(
    pool: PgPool,
) -> anyhow::Result<()> {
    let (_, doc, _) = seed(&pool).await?;
    let engine = SnapshotEngine::new(pool.clone());
    let (a, b) = tokio::join!(
        engine.take(doc, SnapshotReason::Periodic),
        engine.take(doc, SnapshotReason::Periodic)
    );
    assert_eq!(a?, b?);
    let receipts: i64 =
        sqlx::query_scalar("select count(*) from snapshot_verification where document_id=$1")
            .bind(doc.0)
            .fetch_one(&pool)
            .await?;
    assert_eq!(receipts, 1);
    Ok(())
}

#[sqlx::test(migrations = "../../migrations")]
async fn verified_snapshot_tail_rejects_event_tampering(pool: PgPool) -> anyhow::Result<()> {
    let (actor, doc, node) = seed(&pool).await?;
    SnapshotEngine::new(pool.clone())
        .take(doc, SnapshotReason::Periodic)
        .await?;
    append(&pool, doc, &field_edit(&node, "title", "real"), actor).await?;
    sqlx::query("alter table event disable trigger event_no_update")
        .execute(&pool)
        .await?;
    sqlx::query("update event set payload=jsonb_set(payload,'{value}','\"forged\"') where document_id=$1 and seq=2").bind(doc.0).execute(&pool).await?;
    sqlx::query("alter table event enable trigger event_no_update")
        .execute(&pool)
        .await?;
    let mut connection = pool.acquire().await?;
    assert!(matches!(
        read_state_at(&mut connection, doc, 2).await,
        Err(SnapshotError::Event(
            engine_core::log::EventError::ChainBroken { seq: 2, .. }
        ))
    ));
    Ok(())
}

#[sqlx::test(migrations = false)]
async fn schema_27_upgrade_requires_replay_without_rewriting_old_snapshots(
    pool: PgPool,
) -> anyhow::Result<()> {
    let all = sqlx::migrate!("../../migrations");
    let old = sqlx::migrate::Migrator {
        migrations: std::borrow::Cow::Owned(
            all.iter().filter(|m| m.version <= 27).cloned().collect(),
        ),
        ..sqlx::migrate::Migrator::DEFAULT
    };
    old.run(&pool).await?;
    let (actor, doc, node) = seed(&pool).await?;
    append(
        &pool,
        doc,
        &field_edit(&node, "label", "Before upgrade"),
        actor,
    )
    .await?;
    let snapshot = legacy_fixture(&pool, doc, 2).await?;
    // This operation was allowed before the new migration.
    sqlx::query("update snapshot set state=state where id=$1")
        .bind(snapshot.id.0)
        .execute(&pool)
        .await?;
    all.run(&pool).await?;
    let mut connection = pool.acquire().await?;
    assert!(matches!(
        snapshot_verification::load(&mut connection, doc, snapshot.id.0).await,
        Err(SnapshotError::Unverified { .. })
    ));
    assert!(sqlx::query("update snapshot set state=state where id=$1")
        .bind(snapshot.id.0)
        .execute(&pool)
        .await
        .is_err());
    verify_document(&pool, doc, true).await?;
    assert_eq!(
        snapshot_verification::load(&mut connection, doc, snapshot.id.0).await?,
        Some(snapshot)
    );
    append(
        &pool,
        doc,
        &field_edit(&node, "label", "After upgrade"),
        actor,
    )
    .await?;
    let current = SnapshotEngine::new(pool.clone())
        .take(doc, SnapshotReason::Periodic)
        .await?;
    assert_eq!(current.through_seq, 3);
    assert_eq!(verify_document(&pool, doc, false).await?.snapshots, 2);
    Ok(())
}

#[sqlx::test(migrations = "../../migrations")]
async fn audit_does_not_skip_negative_or_future_legacy_revisions(
    pool: PgPool,
) -> anyhow::Result<()> {
    let (actor, _, _) = seed(&pool).await?;
    for seq in [-1, 1] {
        let doc = DocumentId(
            sqlx::query_scalar(
                "insert into document(title,created_by) values('Bad revision',$1) returning id",
            )
            .bind(actor.0)
            .fetch_one(&pool)
            .await?,
        );
        sqlx::query("insert into snapshot(document_id,through_seq,state,merkle_root,event_chain_hash) values($1,$2,$3,$4,$4)")
            .bind(doc.0).bind(seq).bind(json!(DocumentState::default())).bind(vec![0u8;32]).execute(&pool).await?;
        assert!(
            matches!(verify_document(&pool, doc, true).await, Err(SnapshotError::Unverified {seq: bad,..}) if bad==seq)
        );
    }
    Ok(())
}

#[sqlx::test(migrations = "../../migrations")]
async fn backfill_rejects_unknown_snapshot_fields_instead_of_silently_ignoring_them(
    pool: PgPool,
) -> anyhow::Result<()> {
    let (_, doc, node) = seed(&pool).await?;
    let snapshot = legacy_fixture(&pool, doc, 1).await?;
    sqlx::query("alter table snapshot disable trigger snapshot_no_update")
        .execute(&pool)
        .await?;
    for path in [
        vec!["unexpected".to_string()],
        vec![
            "nodes".to_string(),
            node.0.clone(),
            "unexpected".to_string(),
        ],
    ] {
        sqlx::query(
            "update snapshot set state=jsonb_set($2,$3,'\"unrecorded content\"') where id=$1",
        )
        .bind(snapshot.id.0)
        .bind(&snapshot.state)
        .bind(path)
        .execute(&pool)
        .await?;
        // Deserializing/folding only known fields would incorrectly accept this.
        assert!(matches!(
            verify_document(&pool, doc, true).await,
            Err(SnapshotError::Unverified { .. })
        ));
    }
    sqlx::query("alter table snapshot enable trigger snapshot_no_update")
        .execute(&pool)
        .await?;
    let receipts: i64 = sqlx::query_scalar("select count(*) from snapshot_verification")
        .fetch_one(&pool)
        .await?;
    assert_eq!(receipts, 0);
    Ok(())
}
