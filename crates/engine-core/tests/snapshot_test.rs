//! Stage-15 snapshot/materialization integration tests.

use engine_core::{
    log::append,
    materializer::DocumentState,
    snapshot::{SnapshotEngine, SnapshotReason},
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
