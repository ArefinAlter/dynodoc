//! Shared checkpoint compatibility, atomic publication and erasure boundaries.

use engine_core::{
    log::append,
    materializer::Materializer,
    shared_checkpoint::{
        postgres::{self, PgStore},
        read_state, Error, Limits,
    },
    snapshot::{read_state_at, SnapshotEngine, SnapshotReason},
};
use engine_shared::{DocumentId, Event, EventPayload, IdentityId, NodeId, NodeType};
use serde_json::json;
use sqlx::PgPool;

async fn seed(pool: &PgPool) -> anyhow::Result<(IdentityId, DocumentId, NodeId)> {
    let actor: uuid::Uuid = sqlx::query_scalar(
        "insert into identity(email,display_name) values($1,'Checkpoint author') returning id",
    )
    .bind(format!("{}@checkpoint.test", uuid::Uuid::new_v4()))
    .fetch_one(pool)
    .await?;
    let doc: uuid::Uuid = sqlx::query_scalar(
        "insert into document(title,created_by) values('Shared checkpoint',$1) returning id",
    )
    .bind(actor)
    .fetch_one(pool)
    .await?;
    let node = NodeId(ulid::Ulid::new().to_string());
    sqlx::query(
        "insert into node(id,document_id,type,pos,current_fields) values($1,$2,'form','a0','{}')",
    )
    .bind(&node.0)
    .bind(doc)
    .execute(pool)
    .await?;
    append(
        pool,
        DocumentId(doc),
        &EventPayload::NodeCreated {
            node_id: node.clone(),
            node_type: NodeType::Form,
            parent_id: None,
            pos: "a0".into(),
            fields: json!({"label":"Original"}),
            var_name: None,
        },
        IdentityId(actor),
    )
    .await?;
    Ok((IdentityId(actor), DocumentId(doc), node))
}

async fn object_count(pool: &PgPool, doc: DocumentId) -> i64 {
    sqlx::query_scalar("select count(*) from checkpoint_object where document_id=$1")
        .bind(doc.0)
        .fetch_one(pool)
        .await
        .unwrap()
}

#[sqlx::test(migrations = "../../migrations")]
async fn every_checkpoint_matches_replay_and_legacy_reads_are_unchanged(
    pool: PgPool,
) -> anyhow::Result<()> {
    let (actor, doc, node) = seed(&pool).await?;
    let legacy = SnapshotEngine::new(pool.clone());
    for index in 0..8 {
        let payload = if index % 2 == 0 {
            EventPayload::FieldEdited {
                node_id: node.clone(),
                field: "label".into(),
                value: json!(format!("Edit {index}")),
            }
        } else {
            EventPayload::CommentAdded {
                node_id: Some(node.clone()),
                body: format!("Comment {index}"),
            }
        };
        append(&pool, doc, &payload, actor).await?;
        if index == 2 || index == 5 {
            legacy.take(doc, SnapshotReason::Periodic).await?;
        }
    }
    let events =
        sqlx::query_as::<_, Event>("select * from event where document_id=$1 order by seq")
            .bind(doc.0)
            .fetch_all(&pool)
            .await?;
    let mut connection = pool.acquire().await?;
    for seq in 0..=9 {
        let expected = Materializer::fold(&events[..seq as usize])?;
        let before = read_state_at(&mut connection, doc, seq).await?;
        let published = postgres::take(&pool, doc, seq, Limits::default()).await?;
        assert_eq!(published.through_seq, seq);
        assert_eq!(
            postgres::load(&pool, doc, seq, Limits::default()).await?,
            expected
        );
        let after = read_state_at(&mut connection, doc, seq).await?;
        assert_eq!(before.state, after.state);
        assert_eq!(before.snapshot_seq, after.snapshot_seq);
        assert_eq!(after.state, expected);
        let repeated = postgres::take(&pool, doc, seq, Limits::default()).await?;
        assert_eq!(repeated.root, published.root);
        assert_eq!(repeated.stats.new_objects, 0);
    }
    Ok(())
}

#[sqlx::test(migrations = "../../migrations")]
async fn failed_and_interrupted_publication_roll_back_and_can_retry(
    pool: PgPool,
) -> anyhow::Result<()> {
    let (_, doc, _) = seed(&pool).await?;
    // The first node is inserted, then the small total budget fails on its map.
    assert!(matches!(
        postgres::take(
            &pool,
            doc,
            1,
            Limits {
                max_objects: 1,
                max_bytes: usize::MAX
            }
        )
        .await,
        Err(Error::Limit(_))
    ));
    assert_eq!(object_count(&pool, doc).await, 0);
    let manifests: i64 = sqlx::query_scalar("select count(*) from shared_checkpoint")
        .fetch_one(&pool)
        .await?;
    assert_eq!(manifests, 0);
    for seq in [-1, 2, i64::MAX] {
        assert!(postgres::take(&pool, doc, seq, Limits::default())
            .await
            .is_err());
    }
    assert_eq!(object_count(&pool, doc).await, 0);
    // Explicit transaction interruption after writing a complete reachable graph.
    let mut tx = pool.begin().await?;
    let state = read_state_at(&mut tx, doc, 1).await?.state;
    engine_core::shared_checkpoint::write_state(
        &mut PgStore::new(&mut tx),
        doc,
        &state,
        Limits::default(),
    )
    .await?;
    tx.rollback().await?;
    assert_eq!(object_count(&pool, doc).await, 0);
    postgres::take(&pool, doc, 1, Limits::default()).await?;
    assert_eq!(
        postgres::load(&pool, doc, 1, Limits::default()).await?,
        state
    );
    Ok(())
}

#[sqlx::test(migrations = "../../migrations")]
async fn concurrent_publication_is_idempotent(pool: PgPool) -> anyhow::Result<()> {
    let (actor, doc, node) = seed(&pool).await?;
    for index in 0..20 {
        append(
            &pool,
            doc,
            &EventPayload::ChoiceRemoved {
                node_id: node.clone(),
                choice_id: NodeId(format!("removed-{index}")),
            },
            actor,
        )
        .await?;
    }
    let (a, b) = tokio::join!(
        postgres::take(&pool, doc, 21, Limits::default()),
        postgres::take(&pool, doc, 21, Limits::default())
    );
    let (a, b) = (a?, b?);
    assert_eq!(a.root, b.root);
    assert!(a.stats.new_objects == 0 || b.stats.new_objects == 0);
    assert_eq!(
        object_count(&pool, doc).await as usize,
        a.stats.new_objects + b.stats.new_objects
    );
    let manifests: i64 = sqlx::query_scalar("select count(*) from shared_checkpoint")
        .fetch_one(&pool)
        .await?;
    assert_eq!(manifests, 1);
    Ok(())
}

#[sqlx::test(migrations = "../../migrations")]
async fn scope_immutability_and_audited_erasure_are_preserved(pool: PgPool) -> anyhow::Result<()> {
    let (actor, doc, _) = seed(&pool).await?;
    let (_, other, _) = seed(&pool).await?;
    let published = postgres::take(&pool, doc, 1, Limits::default()).await?;
    let mut connection = pool.acquire().await?;
    assert!(matches!(
        read_state(
            &mut PgStore::new(&mut connection),
            other,
            &published.root,
            Limits::default()
        )
        .await,
        Err(Error::Missing(_))
    ));
    postgres::take(&pool, other, 1, Limits::default()).await?;
    let other_count = object_count(&pool, other).await;
    for query in [
        "update checkpoint_object set bytes=bytes where document_id=$1",
        "delete from checkpoint_object where document_id=$1",
        "update shared_checkpoint set state_root=state_root where document_id=$1",
        "delete from shared_checkpoint where document_id=$1",
        "delete from document where id=$1",
    ] {
        assert!(
            sqlx::query(query).bind(doc.0).execute(&pool).await.is_err(),
            "{query}"
        );
    }
    sqlx::query("update document set deleted_at=now() where id=$1")
        .bind(doc.0)
        .execute(&pool)
        .await?;
    assert!(postgres::take(&pool, doc, 1, Limits::default())
        .await
        .is_err());
    assert!(postgres::load(&pool, doc, 1, Limits::default())
        .await
        .is_err());
    let mut tx = pool.begin().await?;
    sqlx::query("insert into erasure_receipt(resource_id,kind,actor_id,reason,counts) values($1,'documents',$2,'owner_request','{}')")
        .bind(doc.0).bind(actor.0).execute(&mut *tx).await?;
    sqlx::query("delete from document where id=$1")
        .bind(doc.0)
        .execute(&mut *tx)
        .await?;
    tx.commit().await?;
    assert_eq!(object_count(&pool, doc).await, 0);
    let remaining: i64 =
        sqlx::query_scalar("select count(*) from shared_checkpoint where document_id=$1")
            .bind(doc.0)
            .fetch_one(&pool)
            .await?;
    assert_eq!(remaining, 0);
    assert_eq!(object_count(&pool, other).await, other_count);
    assert!(postgres::load(&pool, other, 1, Limits::default())
        .await
        .is_ok());
    Ok(())
}

#[sqlx::test(migrations = "../../migrations")]
async fn forged_metadata_and_corrupt_stored_objects_fail_closed(
    pool: PgPool,
) -> anyhow::Result<()> {
    let (_, doc, _) = seed(&pool).await?;
    let root = "a".repeat(64);
    // Simulate pre-existing corrupt storage without weakening immutable triggers.
    sqlx::query("insert into checkpoint_object(document_id,address,bytes) values($1,$2,$3)")
        .bind(doc.0)
        .bind(&root)
        .bind(b"corrupt".as_slice())
        .execute(&pool)
        .await?;
    sqlx::query("insert into shared_checkpoint(document_id,through_seq,codec_version,state_root,event_chain_hash) values($1,0,1,$2,$3)")
        .bind(doc.0).bind(&root).bind(vec![0u8;32]).execute(&pool).await?;
    assert!(matches!(
        postgres::load(&pool, doc, 0, Limits::default()).await,
        Err(Error::HashMismatch)
    ));
    assert!(matches!(
        postgres::take(&pool, doc, 0, Limits::default()).await,
        Err(Error::Invalid(
            "different checkpoint already published at revision"
        ))
    ));
    assert_eq!(object_count(&pool, doc).await, 1);
    sqlx::query("insert into shared_checkpoint(document_id,through_seq,codec_version,state_root,event_chain_hash) values($1,1,1,$2,$3)")
        .bind(doc.0).bind(&root).bind(vec![0u8;32]).execute(&pool).await?;
    assert!(matches!(
        postgres::load(&pool, doc, 1, Limits::default()).await,
        Err(Error::Invalid("checkpoint version or chain position"))
    ));
    Ok(())
}
