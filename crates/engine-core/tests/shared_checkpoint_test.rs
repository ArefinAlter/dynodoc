//! Shared checkpoint compatibility, atomic publication and erasure boundaries.

use engine_core::{
    log::append,
    materializer::Materializer,
    shared_checkpoint::{
        postgres::{self, PgStore},
        read_state, write_state, Address, Error, Limits, ObjectStore, MAX_OBJECT_BYTES,
        READ_BATCH_OBJECTS, WRITE_BATCH_BYTES, WRITE_BATCH_OBJECTS,
    },
    snapshot::{
        read_state_at, CheckpointFormat, PeriodicStorage, SnapshotEngine, SnapshotError,
        SnapshotReason,
    },
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
async fn concurrent_invalid_insert_is_checked_after_bulk_conflict(
    pool: PgPool,
) -> anyhow::Result<()> {
    let (_, source, _) = seed(&pool).await?;
    let (_, target, _) = seed(&pool).await?;
    postgres::take(&pool, source, 1, Limits::default()).await?;
    let (key, bytes): (String, Vec<u8>) = sqlx::query_as(
        "select address,bytes from checkpoint_object where document_id=$1 order by address limit 1",
    )
    .bind(source.0)
    .fetch_one(&pool)
    .await?;
    let mut corrupt = pool.begin().await?;
    sqlx::query("insert into checkpoint_object(document_id,address,bytes) values($1,$2,$3)")
        .bind(target.0)
        .bind(&key)
        .bind(b"corrupt".as_slice())
        .execute(&mut *corrupt)
        .await?;
    let worker_pool = pool.clone();
    let (pid_sender, pid_receiver) = tokio::sync::oneshot::channel();
    let worker = tokio::spawn(async move {
        let mut tx = worker_pool.begin().await?;
        let pid: i32 = sqlx::query_scalar("select pg_backend_pid()")
            .fetch_one(&mut *tx)
            .await?;
        pid_sender.send(pid).unwrap();
        let result = PgStore::new(&mut tx)
            .put_batch(target, &[(Address::try_from(key)?, bytes)])
            .await;
        tx.rollback().await?;
        result
    });
    let pid = pid_receiver.await?;
    // Wait for the real INSERT conflict, not an assumed scheduling delay. The
    // initial SELECT cannot see the other transaction's uncommitted corrupt row.
    tokio::time::timeout(std::time::Duration::from_secs(5), async {
        loop {
            let blocked: bool = sqlx::query_scalar("select cardinality(pg_blocking_pids($1))>0")
                .bind(pid)
                .fetch_one(&pool)
                .await
                .unwrap();
            if blocked {
                break;
            }
            tokio::time::sleep(std::time::Duration::from_millis(5)).await;
        }
    })
    .await?;
    corrupt.commit().await?;
    assert!(matches!(worker.await?, Err(Error::HashMismatch)));
    assert_eq!(object_count(&pool, target).await, 1);
    Ok(())
}

#[sqlx::test(migrations = "../../migrations")]
async fn bulk_store_validates_reuse_duplicates_scope_and_capacity(
    pool: PgPool,
) -> anyhow::Result<()> {
    let (_, source, _) = seed(&pool).await?;
    let (_, target, _) = seed(&pool).await?;
    let published = postgres::take(&pool, source, 1, Limits::default()).await?;
    let stored: Vec<(String, Vec<u8>)> = sqlx::query_as(
        "select address,bytes from checkpoint_object where document_id=$1 order by address",
    )
    .bind(source.0)
    .fetch_all(&pool)
    .await?;
    let mut objects: Vec<_> = stored
        .into_iter()
        .map(|(key, bytes)| Ok((Address::try_from(key)?, bytes)))
        .collect::<Result<_, Error>>()?;
    objects.push(objects[0].clone());
    let mut tx = pool.begin().await?;
    let mut store = PgStore::new(&mut tx);
    let flags = store.put_batch(target, &objects).await?;
    assert!(flags[..flags.len() - 1].iter().all(|new| *new));
    assert!(!flags.last().unwrap());
    assert_eq!(store.write_queries(), 1);
    assert!(store
        .put_batch(target, &objects)
        .await?
        .iter()
        .all(|new| !new));
    assert_eq!(
        store.write_queries(),
        1,
        "unchanged batches should issue no INSERT"
    );
    let state = read_state(&mut store, target, &published.root, Limits::default()).await?;
    assert_eq!(state.nodes.len(), 1);
    assert!(matches!(
        store
            .put_batch(target, &vec![objects[0].clone(); WRITE_BATCH_OBJECTS + 1])
            .await,
        Err(Error::Limit(_))
    ));
    assert!(matches!(
        store
            .put_batch(
                target,
                &[(objects[0].0.clone(), vec![0; WRITE_BATCH_BYTES + 1])]
            )
            .await,
        Err(Error::Limit(_))
    ));
    let bad = vec![(objects[0].0.clone(), b"corrupt".to_vec())];
    assert!(matches!(
        store.put_batch(target, &bad).await,
        Err(Error::HashMismatch)
    ));
    tx.rollback().await?;
    assert_eq!(object_count(&pool, target).await, 0);
    // A forged stored value must not be accepted merely because its address exists.
    sqlx::query("insert into checkpoint_object(document_id,address,bytes) values($1,$2,$3)")
        .bind(target.0)
        .bind(objects[0].0.as_str())
        .bind(b"corrupt".as_slice())
        .execute(&pool)
        .await?;
    let mut tx = pool.begin().await?;
    assert!(matches!(
        PgStore::new(&mut tx).put_batch(target, &objects).await,
        Err(Error::HashMismatch)
    ));
    tx.rollback().await?;
    assert_eq!(object_count(&pool, target).await, 1);
    Ok(())
}

#[sqlx::test(migrations = "../../migrations")]
async fn broad_index_batches_match_memory_graph_and_bound_query_growth(
    pool: PgPool,
) -> anyhow::Result<()> {
    let (_, doc, _) = seed(&pool).await?;
    let mut connection = pool.acquire().await?;
    let mut state = read_state_at(&mut connection, doc, 1).await?.state;
    let original = state.nodes.values().next().unwrap().clone();
    state.nodes.clear();
    for index in 0..2000 {
        let mut node = original.clone();
        node.id = NodeId(format!("{index:026}"));
        if index < 3 {
            node.current_fields = json!({"text": "ab".repeat(300_000)});
        }
        state.nodes.insert(node.id.clone(), node);
    }
    let mut reference = engine_core::shared_checkpoint::MemoryStore::default();
    let (expected_root, expected_stats) =
        write_state(&mut reference, doc, &state, Limits::default()).await?;
    let mut tx = pool.begin().await?;
    let mut store = PgStore::new(&mut tx);
    let (root, stats) = write_state(&mut store, doc, &state, Limits::default()).await?;
    assert_eq!(root, expected_root);
    assert_eq!(stats.new_objects, expected_stats.new_objects);
    assert_eq!(stats.new_bytes, expected_stats.new_bytes);
    assert!(
        store.write_queries() < 50,
        "bounded bulk writes should replace per-object INSERTs"
    );
    let before = store.read_queries();
    assert_eq!(
        read_state(&mut store, doc, &root, Limits::default()).await?,
        state
    );
    assert!(
        store.read_queries() - before < 170,
        "both indexes and values must be batched"
    );
    let before = store.write_queries();
    let (repeated, stats) = write_state(&mut store, doc, &state, Limits::default()).await?;
    assert_eq!(repeated, root);
    assert_eq!(stats.new_objects, 0);
    assert_eq!(store.write_queries(), before);
    tx.commit().await?;
    Ok(())
}

#[sqlx::test(migrations = "../../migrations")]
async fn overlapping_historical_publishers_remain_atomic_and_replay_equivalent(
    pool: PgPool,
) -> anyhow::Result<()> {
    let (actor, doc, _) = seed(&pool).await?;
    let mut tx = pool.begin().await?;
    for index in 0..140 {
        engine_core::log::append_in_tx(
            &mut tx,
            doc,
            &EventPayload::CommentAdded {
                node_id: None,
                body: format!("Repeated across batches {}", index % 71),
            },
            actor,
        )
        .await?;
    }
    tx.commit().await?;
    let (first, second) = tokio::join!(
        postgres::take(&pool, doc, 71, Limits::default()),
        postgres::take(&pool, doc, 141, Limits::default()),
    );
    first?;
    second?;
    let events =
        sqlx::query_as::<_, Event>("select * from event where document_id=$1 order by seq")
            .bind(doc.0)
            .fetch_all(&pool)
            .await?;
    for seq in [71, 141] {
        assert_eq!(
            postgres::load(&pool, doc, seq, Limits::default()).await?,
            Materializer::fold(&events[..seq as usize])?
        );
    }
    Ok(())
}

#[sqlx::test(migrations = "../../migrations")]
async fn mixed_history_selects_nearest_checkpoint_and_preserves_resume_seq(
    pool: PgPool,
) -> anyhow::Result<()> {
    let (actor, doc, node) = seed(&pool).await?;
    let engine = SnapshotEngine::new(pool.clone());
    let (_, other, _) = seed(&pool).await?;
    for seq in 2..=10 {
        for (document, target) in [(doc, Some(node.clone())), (other, None)] {
            append(
                &pool,
                document,
                &EventPayload::CommentAdded {
                    node_id: target,
                    body: format!("Revision {seq}"),
                },
                actor,
            )
            .await?;
        }
        if seq == 3 || seq == 7 {
            engine.take(doc, SnapshotReason::Periodic).await?;
        }
        if seq == 5 || seq == 7 {
            postgres::take(&pool, doc, seq, Limits::default()).await?;
        }
    }
    postgres::take(&pool, other, 10, Limits::default()).await?;
    let events =
        sqlx::query_as::<_, Event>("select * from event where document_id=$1 order by seq")
            .bind(doc.0)
            .fetch_all(&pool)
            .await?;
    let mut connection = pool.acquire().await?;
    for seq in 0..=10 {
        let expected = Materializer::fold(&events[..seq as usize])?;
        let actual = read_state_at(&mut connection, doc, seq).await?;
        let (base, format) = match seq {
            0..=2 => (0, CheckpointFormat::Genesis),
            3..=4 => (3, CheckpointFormat::Legacy),
            5..=6 => (5, CheckpointFormat::SharedV1),
            _ => (7, CheckpointFormat::SharedV1),
        };
        assert_eq!(actual.state, expected, "revision {seq}");
        assert_eq!(
            (actual.snapshot_seq, actual.checkpoint_format),
            (base, format)
        );
        assert_eq!(actual.replayed_events, (seq - base) as usize);
    }
    for seq in [-1, 11, i64::MAX] {
        assert!(matches!(
            read_state_at(&mut connection, doc, seq).await,
            Err(SnapshotError::InvalidRevision(_))
        ));
    }
    let (current, seq) = engine.read_current_state_with_seq(doc).await?;
    assert_eq!(seq, 10);
    assert_eq!(current, Materializer::fold(&events)?);
    let legacy = engine.take(doc, SnapshotReason::Deployed).await?;
    assert_eq!(legacy.through_seq, seq);
    assert_eq!(serde_json::to_value(&current)?, legacy.state);
    assert_eq!(
        read_state_at(&mut connection, doc, 10)
            .await?
            .checkpoint_format,
        CheckpointFormat::Legacy
    );
    Ok(())
}

#[sqlx::test(migrations = "../../migrations")]
async fn periodic_rollout_cadence_and_duplicate_builders_are_reversible(
    pool: PgPool,
) -> anyhow::Result<()> {
    let (actor, doc, node) = seed(&pool).await?;
    let legacy = SnapshotEngine::new(pool.clone());
    let shared = legacy
        .clone()
        .with_periodic_storage(PeriodicStorage::SharedV1);
    let mut lock = pool.begin().await?;
    sqlx::query("select pg_advisory_xact_lock(hashtextextended($1,0))")
        .bind(format!("dynodoc.periodic-checkpoint:{}", doc.0))
        .execute(&mut *lock)
        .await?;
    shared.ensure_recent(doc, 0).await?;
    assert_eq!(object_count(&pool, doc).await, 0);
    lock.rollback().await?;
    let (a, b) = tokio::join!(shared.ensure_recent(doc, 0), shared.ensure_recent(doc, 0));
    a?;
    b?;
    let mut connection = pool.acquire().await?;
    assert_eq!(
        read_state_at(&mut connection, doc, 1)
            .await?
            .checkpoint_format,
        CheckpointFormat::SharedV1
    );
    for seq in 2..=3 {
        append(
            &pool,
            doc,
            &EventPayload::FieldEdited {
                node_id: node.clone(),
                field: "label".into(),
                value: json!(seq),
            },
            actor,
        )
        .await?;
        shared.ensure_recent(doc, 1).await?;
    }
    let checkpoints: Vec<i64> = sqlx::query_scalar(
        "select through_seq from shared_checkpoint where document_id=$1 order by through_seq",
    )
    .bind(doc.0)
    .fetch_all(&pool)
    .await?;
    assert_eq!(checkpoints, [1, 3]);
    // Turning off shared writing does not duplicate a checkpoint or disable reads.
    legacy.ensure_recent(doc, 0).await?;
    let legacy_count: i64 = sqlx::query_scalar("select count(*) from snapshot")
        .fetch_one(&pool)
        .await?;
    assert_eq!(legacy_count, 0);
    append(
        &pool,
        doc,
        &EventPayload::CommentAdded {
            node_id: None,
            body: "After rollback".into(),
        },
        actor,
    )
    .await?;
    legacy.ensure_recent(doc, 0).await?;
    assert_eq!(
        read_state_at(&mut connection, doc, 4)
            .await?
            .checkpoint_format,
        CheckpointFormat::Legacy
    );
    assert_eq!(
        read_state_at(&mut connection, doc, 3)
            .await?
            .checkpoint_format,
        CheckpointFormat::SharedV1
    );
    Ok(())
}

#[sqlx::test(migrations = "../../migrations")]
async fn oversized_value_rolls_back_partial_objects_before_legacy_fallback(
    pool: PgPool,
) -> anyhow::Result<()> {
    let (actor, doc, _) = seed(&pool).await?;
    // More than one buffer of valid comments forces an actual database batch before
    // the oversized final comment fails. The savepoint must remove that batch.
    let mut tx = pool.begin().await?;
    for index in 0..70 {
        engine_core::log::append_in_tx(
            &mut tx,
            doc,
            &EventPayload::CommentAdded {
                node_id: None,
                body: format!("Valid comment {index}"),
            },
            actor,
        )
        .await?;
    }
    tx.commit().await?;
    append(
        &pool,
        doc,
        &EventPayload::CommentAdded {
            node_id: None,
            body: "x".repeat(MAX_OBJECT_BYTES),
        },
        actor,
    )
    .await?;
    let engine = SnapshotEngine::new(pool.clone()).with_periodic_storage(PeriodicStorage::SharedV1);
    engine.ensure_recent(doc, 0).await?;
    assert_eq!(object_count(&pool, doc).await, 0);
    let shared: i64 = sqlx::query_scalar("select count(*) from shared_checkpoint")
        .fetch_one(&pool)
        .await?;
    assert_eq!(shared, 0);
    let mut connection = pool.acquire().await?;
    let read = read_state_at(&mut connection, doc, 72).await?;
    assert_eq!(
        (
            read.snapshot_seq,
            read.replayed_events,
            read.checkpoint_format
        ),
        (72, 0, CheckpointFormat::Legacy)
    );
    assert_eq!(
        read.state.comments.last().unwrap().body.len(),
        MAX_OBJECT_BYTES
    );
    assert_eq!(
        engine.read_current_state_with_seq(doc).await?,
        (read.state, 72)
    );
    Ok(())
}

#[sqlx::test(migrations = "../../migrations")]
async fn batched_values_keep_duplicate_references_scope_and_limits(
    pool: PgPool,
) -> anyhow::Result<()> {
    let (_, doc, _) = seed(&pool).await?;
    let (_, other, _) = seed(&pool).await?;
    let mut connection = pool.acquire().await?;
    let mut state = read_state_at(&mut connection, doc, 1).await?.state;
    for index in 0..40 {
        state
            .comments
            .push(engine_core::materializer::CommentState {
                node_id: None,
                body: if index < 20 {
                    "Repeated".into()
                } else {
                    format!("Comment {index}")
                },
            });
    }
    let mut store = PgStore::new(&mut connection);
    let (root, _) = write_state(&mut store, doc, &state, Limits::default()).await?;
    let before = store.read_queries();
    assert_eq!(
        read_state(&mut store, doc, &root, Limits::default()).await?,
        state
    );
    assert!(
        store.read_queries() - before < 30,
        "value batching should avoid one query per comment"
    );
    assert!(matches!(
        read_state(
            &mut store,
            doc,
            &root,
            Limits {
                max_objects: 3,
                max_bytes: usize::MAX
            }
        )
        .await,
        Err(Error::Limit(_))
    ));
    assert!(matches!(
        read_state(
            &mut store,
            doc,
            &root,
            Limits {
                max_objects: usize::MAX,
                max_bytes: 1
            }
        )
        .await,
        Err(Error::Limit(_))
    ));
    assert!(matches!(
        store
            .prefetch(doc, &vec![root.clone(); READ_BATCH_OBJECTS + 1])
            .await,
        Err(Error::Limit(_))
    ));
    store.prefetch(doc, std::slice::from_ref(&root)).await?;
    assert!(matches!(
        store.get(other, &root).await,
        Err(Error::Missing(_))
    ));
    let absent = Address::try_from("a".repeat(64))?;
    store.prefetch(doc, std::slice::from_ref(&absent)).await?;
    assert!(matches!(
        store.get(doc, &absent).await,
        Err(Error::Missing(_))
    ));
    Ok(())
}

#[sqlx::test(migrations = "../../migrations")]
async fn every_revision_matches_replay_before_and_after_shared_adoption(
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
        assert_eq!(after.snapshot_seq, seq);
        assert_eq!(after.checkpoint_format, CheckpointFormat::SharedV1);
        assert_eq!(after.replayed_events, 0);
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
    // The budget fails before the buffered graph can be published.
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
    let engine = SnapshotEngine::new(pool.clone()).with_periodic_storage(PeriodicStorage::SharedV1);
    assert!(engine.read_current_state(doc).await.is_err());
    assert!(engine.ensure_recent(doc, 0).await.is_err());
    let snapshots: i64 = sqlx::query_scalar("select count(*) from snapshot")
        .fetch_one(&pool)
        .await?;
    assert_eq!(
        snapshots, 0,
        "corruption must not become a legacy capacity fallback"
    );
    sqlx::query("insert into shared_checkpoint(document_id,through_seq,codec_version,state_root,event_chain_hash) values($1,1,1,$2,$3)")
        .bind(doc.0).bind(&root).bind(vec![0u8;32]).execute(&pool).await?;
    assert!(matches!(
        postgres::load(&pool, doc, 1, Limits::default()).await,
        Err(Error::Invalid("checkpoint version or chain position"))
    ));
    Ok(())
}
