//! Disposable PostgreSQL comparison of writer locks during shared publication.
//! See docs/CHECKPOINT-CONCURRENCY.md for scope, gates and reproduction.

use engine_core::{
    log::{append_in_tx, verify_chain},
    materializer::{apply_payload, DocumentState},
    shared_checkpoint::{postgres, Limits},
    snapshot::{read_current_state, SnapshotEngine, SnapshotReason},
};
use engine_shared::{DocumentId, EventPayload, IdentityId, NodeId, NodeType};
use serde_json::json;
use sqlx::postgres::PgPoolOptions;
use std::{
    sync::{
        atomic::{AtomicBool, Ordering},
        Arc,
    },
    time::{Duration, Instant},
};

#[tokio::main(flavor = "current_thread")]
async fn main() -> anyhow::Result<()> {
    anyhow::ensure!(
        std::env::var("DYNODOC_CHECKPOINT_BENCH").as_deref() == Ok("disposable"),
        "set DYNODOC_CHECKPOINT_BENCH=disposable and an explicit empty DATABASE_URL"
    );
    let mode =
        std::env::var("DYNODOC_CHECKPOINT_WRITER_LOCK").unwrap_or_else(|_| "no-key-update".into());
    anyhow::ensure!(
        ["update", "no-key-update"].contains(&mode.as_str()),
        "writer lock must be update or no-key-update"
    );
    let pool = PgPoolOptions::new()
        .max_connections(16)
        .connect(&std::env::var("DATABASE_URL")?)
        .await?;
    let count: i64 = sqlx::query_scalar("select count(*) from document")
        .fetch_one(&pool)
        .await?;
    anyhow::ensure!(count == 0, "requires an empty migrated disposable database");
    let mut actors = Vec::new();
    for index in 0..10 {
        actors.push(IdentityId(sqlx::query_scalar("insert into identity(email,display_name) values($1,'Synthetic collaborator') returning id")
            .bind(format!("writer-{index}@checkpoint.test")).fetch_one(&pool).await?));
    }
    let doc = DocumentId(sqlx::query_scalar("insert into document(title,created_by) values('Disposable contention benchmark',$1) returning id")
        .bind(actors[0].0).fetch_one(&pool).await?);
    let mut base = DocumentState::default();
    let mut ids = Vec::new();
    let mut tx = pool.begin().await?;
    for index in 0..8000 {
        let id = NodeId(format!("{index:026}"));
        let fields =
            json!({"content":{"text":"Synthetic document text. ".repeat(24),"bold":false}});
        sqlx::query(
            "insert into node(id,document_id,type,pos,current_fields) values($1,$2,'form',$3,$4)",
        )
        .bind(&id.0)
        .bind(doc.0)
        .bind(format!("p{index:08}"))
        .bind(&fields)
        .execute(&mut *tx)
        .await?;
        let op = EventPayload::NodeCreated {
            node_id: id.clone(),
            node_type: NodeType::Form,
            parent_id: None,
            pos: format!("p{index:08}"),
            fields,
            var_name: None,
        };
        append_in_tx(&mut tx, doc, &op, actors[0]).await?;
        apply_payload(&mut base, &op)?;
        ids.push(id);
    }
    tx.commit().await?;
    SnapshotEngine::new(pool.clone())
        .take(doc, SnapshotReason::Periodic)
        .await?;

    // Pause the real publisher at its first object INSERT. The builder has pinned
    // history and holds KEY SHARE. A statement trigger avoids per-object delays;
    // both benchmark modes use the same synthetic barrier, never production code.
    sqlx::raw_sql("create function pause_contention_bench() returns trigger language plpgsql as $$ begin perform pg_advisory_xact_lock(7422,1); return null; end $$; create trigger pause_contention before insert on checkpoint_object for each statement execute function pause_contention_bench();")
        .execute(&pool).await?;
    let mut gate = pool.begin().await?;
    sqlx::query("select pg_advisory_xact_lock(7422,1)")
        .execute(&mut *gate)
        .await?;
    let gate_pid: i32 = sqlx::query_scalar("select pg_backend_pid()")
        .fetch_one(&mut *gate)
        .await?;
    let committed = Arc::new(AtomicBool::new(false));
    let worker_pool = pool.clone();
    let publication_done = committed.clone();
    let publisher = tokio::spawn(async move {
        let result = postgres::take(&worker_pool, doc, 8000, Limits::default()).await;
        publication_done.store(true, Ordering::Release);
        result
    });
    tokio::time::timeout(Duration::from_secs(10), async {
        loop {
            let blocked: bool = sqlx::query_scalar("select exists(select 1 from pg_stat_activity where datname=current_database() and $1=any(pg_blocking_pids(pid)))")
                .bind(gate_pid).fetch_one(&pool).await?;
            if blocked {
                break;
            }
            tokio::time::sleep(Duration::from_millis(2)).await;
        }
        anyhow::Ok(())
    })
    .await??;

    let barrier = Arc::new(tokio::sync::Barrier::new(11));
    let mut writers = tokio::task::JoinSet::new();
    let mut expected = base.clone();
    for index in 0..10 {
        let worker_pool = pool.clone();
        let barrier = barrier.clone();
        let publication_done = committed.clone();
        let strong = mode == "update";
        let node = ids[index].clone();
        let actor = actors[index];
        let op = EventPayload::FieldEdited {
            node_id: node.clone(),
            field: "concurrent_edit".into(),
            value: json!(index),
        };
        apply_payload(&mut expected, &op)?;
        writers.spawn(async move {
            barrier.wait().await;
            let start = Instant::now();
            let mut tx = worker_pool.begin().await?;
            // Reproduce the old writer entry lock without altering production.
            let query = if strong {
                "select id from document where id=$1 for update"
            } else {
                "select id from document where id=$1 for no key update"
            };
            sqlx::query(query).bind(doc.0).fetch_one(&mut *tx).await?;
            let lock_ms = start.elapsed().as_secs_f64() * 1000.0;
            sqlx::query("update node set current_fields=current_fields || $2 where id=$1")
                .bind(&node.0)
                .bind(json!({"concurrent_edit":index}))
                .execute(&mut *tx)
                .await?;
            let event = append_in_tx(&mut tx, doc, &op, actor).await?;
            tx.commit().await?;
            let commit_ms = start.elapsed().as_secs_f64() * 1000.0;
            anyhow::Ok((
                index,
                event.seq,
                lock_ms,
                commit_ms,
                !publication_done.load(Ordering::Acquire),
            ))
        });
    }
    let release = Instant::now();
    barrier.wait().await;
    gate.commit().await?;
    let published = tokio::time::timeout(Duration::from_secs(60), publisher).await???;
    let publication_after_release_ms = release.elapsed().as_secs_f64() * 1000.0;
    let mut measurements = tokio::time::timeout(Duration::from_secs(60), async {
        let mut measurements = Vec::new();
        while let Some(row) = writers.join_next().await {
            measurements.push(row??);
        }
        anyhow::Ok(measurements)
    })
    .await??;
    measurements.sort_by_key(|row| row.0);
    let mut seqs: Vec<_> = measurements.iter().map(|row| row.1).collect();
    seqs.sort_unstable();
    assert_eq!(seqs, (8001..=8010).collect::<Vec<_>>());
    let overlapping = measurements.iter().filter(|row| row.4).count();
    if mode == "no-key-update" {
        anyhow::ensure!(
            overlapping > 0,
            "no concurrent commit overlapped publication"
        );
    }
    assert_eq!(
        postgres::load(&pool, doc, 8000, Limits::default()).await?,
        base
    );
    let (actual, seq) = read_current_state(&mut *pool.acquire().await?, doc).await?;
    assert_eq!(seq, 8010);
    assert_eq!(actual, expected);
    verify_chain(&pool, doc).await?;
    println!(
        "{}",
        serde_json::to_string_pretty(&json!({
            "writer_lock":mode,"blocks":8000,"concurrent_writers":10,
            "baseline_json_bytes":serde_json::to_vec(&base)?.len(),
            "pinned_seq":8000,"final_seq":seq,"checkpoint_root":published.root.as_str(),
            "publication_after_barrier_release_ms":publication_after_release_ms,
            "commits_completed_before_publication_return":overlapping,
            "writers":measurements.iter().map(|row|json!({"writer":row.0,"seq":row.1,"lock_acquired_ms":row.2,"committed_ms":row.3,"before_publication_return":row.4})).collect::<Vec<_>>(),
            "scope":"One synthetic 8000-block document; ten real core transactions with derived node updates and hash-chained appends, excluding API authorization, validation and full-state materialization. Pool size 16. Both modes use a test-only first-object INSERT barrier and the current publisher/codec. Old mode takes FOR UPDATE before the current core append; current mode uses NO KEY UPDATE. Timings include pool wait and writer serialization, not service percentiles, native-file or 100000-user acceptance. Publication timing starts at barrier release, excluding initial legacy read/encoding. Overlap is relative to publisher return, not a server commit timestamp."
        }))?
    );
    Ok(())
}
