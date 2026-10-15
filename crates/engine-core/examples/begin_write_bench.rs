//! Disposable PostgreSQL measurement of what a write reads first:
//! `snapshot::read_current_state`, which `begin_write` calls (latest sequence, checked
//! snapshot read and parse, then a fold of the event tail). Never use a production
//! database: it requires an empty, migrated database and
//! DYNODOC_CHECKPOINT_BENCH=disposable, like the checkpoint benchmark.
//! cargo run --release -p engine-core --example begin_write_bench
#[path = "support/corpus.rs"]
mod corpus;

use engine_core::{
    log::append_in_tx,
    snapshot::{read_current_state, SnapshotEngine, SnapshotReason},
};
use engine_shared::{DocumentId, EventPayload, IdentityId};
use serde_json::json;
use sqlx::PgPool;
use std::time::Instant;

const TAIL: usize = 500;

#[tokio::main(flavor = "current_thread")]
async fn main() -> anyhow::Result<()> {
    anyhow::ensure!(
        std::env::var("DYNODOC_CHECKPOINT_BENCH").as_deref() == Ok("disposable"),
        "explicit disposable database acknowledgement required"
    );
    let pool = PgPool::connect(&std::env::var("DATABASE_URL")?).await?;
    let count: i64 = sqlx::query_scalar("select count(*) from document")
        .fetch_one(&pool)
        .await?;
    anyhow::ensure!(
        count == 0,
        "benchmark requires an empty migrated disposable database"
    );
    let actor = IdentityId(sqlx::query_scalar("insert into identity(email,display_name) values('begin-write-bench@example.test','Synthetic benchmark') returning id").fetch_one(&pool).await?);
    let mut results = Vec::new();
    for blocks in [1000, 8000] {
        let doc = DocumentId(sqlx::query_scalar("insert into document(title,created_by) values('Disposable write benchmark',$1) returning id").bind(actor.0).fetch_one(&pool).await?);
        let (corpus, ids) = corpus::report(blocks as u64, blocks);
        let mut tx = pool.begin().await?;
        for op in &corpus.events {
            // Events reference node rows, so write each row first, as the API does.
            if let EventPayload::NodeCreated {
                node_id,
                node_type,
                parent_id,
                pos,
                fields,
                ..
            } = op
            {
                sqlx::query("insert into node(id,document_id,parent_id,type,pos,current_fields) values($1,$2,$3,$4,$5,$6)")
                    .bind(&node_id.0)
                    .bind(doc.0)
                    .bind(parent_id.as_ref().map(|p| p.0.as_str()))
                    .bind(node_type.as_str())
                    .bind(pos)
                    .bind(fields)
                    .execute(&mut *tx)
                    .await?;
            }
            append_in_tx(&mut tx, doc, op, actor).await?;
        }
        tx.commit().await?;
        SnapshotEngine::new(pool.clone())
            .take(doc, SnapshotReason::Periodic)
            .await?;
        let mut tx = pool.begin().await?;
        for i in 0..TAIL {
            let edit = EventPayload::FieldEdited {
                node_id: ids[(i * 397) % blocks].clone(),
                field: "label".into(),
                value: json!(format!("Edited {i}")),
            };
            append_in_tx(&mut tx, doc, &edit, actor).await?;
        }
        tx.commit().await?;
        let snapshot_bytes: i32 = sqlx::query_scalar(
            "select octet_length(state::text) from snapshot where document_id=$1",
        )
        .bind(doc.0)
        .fetch_one(&pool)
        .await?;
        let mut connection = pool.acquire().await?;
        read_current_state(&mut connection, doc).await?;
        let mut runs = Vec::new();
        for _ in 0..9 {
            let start = Instant::now();
            std::hint::black_box(read_current_state(&mut connection, doc).await?);
            runs.push(start.elapsed().as_secs_f64() * 1000.0);
        }
        runs.sort_by(f64::total_cmp);
        results.push(json!({"blocks": blocks, "tail_events": TAIL, "snapshot_json_bytes": snapshot_bytes, "read_current_state_ms": runs[4]}));
    }
    println!(
        "{}",
        serde_json::to_string_pretty(&json!({
            "date": chrono::Utc::now().date_naive().to_string(),
            "platform": format!("{} {}; release build; single-thread Tokio runtime; local PostgreSQL", std::env::consts::OS, std::env::consts::ARCH),
            "measurement": "Median of 9 warm snapshot::read_current_state calls on one connection: latest sequence, checked legacy snapshot read and JSON parse, and a fold of the event tail",
            "limitations": "One disposable local database, sequential reads, no concurrent writers, no lock wait; the document lock and role check that begin_write also takes are excluded.",
            "results": results,
        }))?
    );
    Ok(())
}
