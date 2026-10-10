//! Disposable PostgreSQL checkpoint measurement; never use a production database.
//! Requires an empty, migrated database and DYNODOC_CHECKPOINT_BENCH=disposable.
//! cargo run --release -p engine-core --example shared_checkpoint_pg_bench

use engine_core::{
    log::{append_in_tx, verify_chain},
    materializer::{apply_payload, DocumentState},
    shared_checkpoint::{
        postgres::{self, PgStore},
        read_state, write_state, Address, Limits, ObjectStore,
    },
    snapshot::{read_state_at, SnapshotEngine},
};
use engine_shared::{DocumentId, EventPayload, IdentityId, NodeId, NodeType};
use serde_json::json;
use sqlx::PgPool;
use std::sync::{
    atomic::{AtomicBool, Ordering},
    Arc,
};
use std::time::Instant;

// Same verified reader, with the optional batch hint disabled for comparison.
struct Unbatched<'a>(PgStore<'a>);
impl ObjectStore for Unbatched<'_> {
    async fn put(
        &mut self,
        doc: DocumentId,
        address: &Address,
        bytes: &[u8],
    ) -> Result<bool, engine_core::shared_checkpoint::Error> {
        self.0.put(doc, address, bytes).await
    }
    async fn get(
        &mut self,
        doc: DocumentId,
        address: &Address,
    ) -> Result<Vec<u8>, engine_core::shared_checkpoint::Error> {
        self.0.get(doc, address).await
    }
}

async fn wal(pool: &PgPool) -> anyhow::Result<String> {
    Ok(
        sqlx::query_scalar("select pg_current_wal_insert_lsn()::text")
            .fetch_one(pool)
            .await?,
    )
}

fn fixture_text(index: usize, workload: &str) -> String {
    if workload == "repeated" {
        return "Synthetic document text. ".repeat(24);
    }
    // Fixed deterministic ASCII payload with little repeated content; same length
    // as the repetitive fixture. This is synthetic entropy, not a native corpus.
    let mut value = index as u64 + 1;
    (0..("Synthetic document text. ".len() * 24))
        .map(|_| {
            value ^= value << 13;
            value ^= value >> 7;
            value ^= value << 17;
            b"abcdefghijklmnopqrstuvwxyzABCDEFGHIJKLMNOPQRSTUVWXYZ0123456789"[(value % 62) as usize]
                as char
        })
        .collect()
}

// Benchmark-only reference: the previous individual INSERT/reuse SELECT path,
// with the same codec and exact legacy input. No API or production flag exposes it.
async fn publish_individually(
    pool: &PgPool,
    doc: DocumentId,
    seq: i64,
) -> anyhow::Result<(usize, usize, Address)> {
    let mut tx = pool.begin().await?;
    sqlx::query("select pg_advisory_xact_lock(hashtextextended($1,0))")
        .bind(format!("dynodoc.periodic-checkpoint:{}", doc.0))
        .execute(&mut *tx)
        .await?;
    sqlx::query("select id from document where id=$1 for key share")
        .bind(doc.0)
        .fetch_one(&mut *tx)
        .await?;
    let chain: Vec<u8> =
        sqlx::query_scalar("select chain_hash from event where document_id=$1 and seq=$2")
            .bind(doc.0)
            .bind(seq)
            .fetch_one(&mut *tx)
            .await?;
    let state = read_state_at(&mut tx, doc, seq).await?.state;
    let mut store = Unbatched(PgStore::new(&mut tx));
    let (root, _) = write_state(&mut store, doc, &state, Limits::default()).await?;
    assert_eq!(
        read_state(&mut store, doc, &root, Limits::default()).await?,
        state
    );
    let counts = (store.0.write_queries(), store.0.read_queries());
    sqlx::query("insert into shared_checkpoint(document_id,through_seq,codec_version,state_root,event_chain_hash) values($1,$2,1,$3,$4)")
        .bind(doc.0).bind(seq).bind(root.as_str()).bind(chain).execute(&mut *tx).await?;
    tx.commit().await?;
    Ok((counts.0, counts.1, root))
}

async fn storage(pool: &PgPool) -> anyhow::Result<serde_json::Value> {
    let mut result = serde_json::Map::new();
    for table in [
        "snapshot",
        "checkpoint_object",
        "shared_checkpoint",
        "event",
    ] {
        let (table_bytes, index_bytes, total_bytes): (i64, i64, i64) = sqlx::query_as(
            "select pg_table_size($1::regclass),pg_indexes_size($1::regclass),pg_total_relation_size($1::regclass)",
        ).bind(table).fetch_one(pool).await?;
        result.insert(table.into(), json!({"table_including_toast_bytes":table_bytes,"index_bytes":index_bytes,"total_bytes":total_bytes}));
    }
    Ok(result.into())
}

// Measure a strong management/erasure FOR UPDATE wait without changing content.
// Content writers use NO KEY UPDATE since part 2d; this is no longer their lock.
// NOWAIT polling first confirms that publication actually holds a conflicting
// lock; a missed very short publication is reported as null, not zero latency.
async fn strong_lock_probe(
    pool: PgPool,
    doc: DocumentId,
    finished: Arc<AtomicBool>,
) -> anyhow::Result<Option<f64>> {
    while !finished.load(Ordering::Acquire) {
        let mut tx = pool.begin().await?;
        let attempt = sqlx::query("select id from document where id=$1 for update nowait")
            .bind(doc.0)
            .execute(&mut *tx)
            .await;
        tx.rollback().await?;
        match attempt {
            Ok(_) => tokio::time::sleep(std::time::Duration::from_millis(2)).await,
            Err(sqlx::Error::Database(error)) if error.code().as_deref() == Some("55P03") => {
                let mut tx = pool.begin().await?;
                let start = Instant::now();
                sqlx::query("select id from document where id=$1 for update")
                    .bind(doc.0)
                    .execute(&mut *tx)
                    .await?;
                let elapsed = start.elapsed().as_secs_f64() * 1000.0;
                tx.rollback().await?;
                return Ok(Some(elapsed));
            }
            Err(error) => return Err(error.into()),
        }
    }
    Ok(None)
}

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
    let mode = std::env::var("DYNODOC_CHECKPOINT_BENCH_MODE").unwrap_or_else(|_| "batched".into());
    anyhow::ensure!(
        ["batched", "individual"].contains(&mode.as_str()),
        "unknown benchmark mode"
    );
    let workload =
        std::env::var("DYNODOC_CHECKPOINT_BENCH_WORKLOAD").unwrap_or_else(|_| "repeated".into());
    anyhow::ensure!(
        ["repeated", "varied"].contains(&workload.as_str()),
        "unknown benchmark workload"
    );
    let blocks = 8000usize;
    let edits: usize = std::env::var("DYNODOC_CHECKPOINT_BENCH_EDITS")
        .unwrap_or_else(|_| "5".into())
        .parse()?;
    anyhow::ensure!((1..=20).contains(&edits), "benchmark edits must be 1..=20");
    let actor = IdentityId(sqlx::query_scalar("insert into identity(email,display_name) values('checkpoint-bench@example.test','Synthetic benchmark') returning id").fetch_one(&pool).await?);
    let doc = DocumentId(sqlx::query_scalar("insert into document(title,created_by) values('Disposable checkpoint benchmark',$1) returning id").bind(actor.0).fetch_one(&pool).await?);
    let mut expected = DocumentState::default();
    let mut tx = pool.begin().await?;
    let mut ids = Vec::new();
    for index in 0..blocks {
        let id = NodeId(format!("{index:026}"));
        sqlx::query(
            "insert into node(id,document_id,type,pos,current_fields) values($1,$2,'form',$3,'{}')",
        )
        .bind(&id.0)
        .bind(doc.0)
        .bind(format!("p{index:08}"))
        .execute(&mut *tx)
        .await?;
        let event = append_in_tx(
            &mut tx,
            doc,
            &EventPayload::NodeCreated {
                node_id: id.clone(),
                node_type: NodeType::Form,
                parent_id: None,
                pos: format!("p{index:08}"),
                fields: json!({"content":{"text":fixture_text(index, &workload),"bold":false}}),
                var_name: None,
            },
            actor,
        )
        .await?;
        apply_payload(&mut expected, &serde_json::from_value(event.payload)?)?;
        ids.push(id);
    }
    tx.commit().await?;
    let baseline = serde_json::to_vec(&expected)?.len();
    let initial_sizes = storage(&pool).await?;
    let legacy = SnapshotEngine::new(pool.clone());
    let mut results = Vec::new();
    for edit in 0..=edits {
        if edit > 0 {
            let mut tx = pool.begin().await?;
            let event = append_in_tx(
                &mut tx,
                doc,
                &EventPayload::FieldEdited {
                    node_id: ids[(edit * 397) % blocks].clone(),
                    field: "revision".into(),
                    value: json!(edit),
                },
                actor,
            )
            .await?;
            apply_payload(&mut expected, &serde_json::from_value(event.payload)?)?;
            tx.commit().await?;
        }
        let before = wal(&pool).await?;
        let start = Instant::now();
        // Explicit legacy creation at this revision preserves a matching baseline.
        legacy
            .take(doc, engine_core::snapshot::SnapshotReason::Periodic)
            .await?;
        let legacy_ms = start.elapsed().as_secs_f64() * 1000.0;
        let legacy_wal: f64 = sqlx::query_scalar(
            "select pg_wal_lsn_diff(pg_current_wal_insert_lsn(),$1::pg_lsn)::float8",
        )
        .bind(&before)
        .fetch_one(&pool)
        .await?;
        let before = wal(&pool).await?;
        let finished = Arc::new(AtomicBool::new(false));
        let probe = tokio::spawn(strong_lock_probe(pool.clone(), doc, finished.clone()));
        let start = Instant::now();
        let seq = (blocks + edit) as i64;
        let (write_queries, read_queries, published_root) = if mode == "individual" {
            publish_individually(&pool, doc, seq).await?
        } else {
            let published = postgres::take(&pool, doc, seq, Limits::default()).await?;
            (
                published.object_write_queries,
                published.object_read_queries,
                published.root,
            )
        };
        let shared_ms = start.elapsed().as_secs_f64() * 1000.0;
        finished.store(true, Ordering::Release);
        let strong_lock_wait_ms = probe.await??;
        let shared_wal: f64 = sqlx::query_scalar(
            "select pg_wal_lsn_diff(pg_current_wal_insert_lsn(),$1::pg_lsn)::float8",
        )
        .bind(&before)
        .fetch_one(&pool)
        .await?;
        let mut connection = pool.acquire().await?;
        let start = Instant::now();
        let actual = read_state_at(&mut connection, doc, seq).await?;
        let normal_read_ms = start.elapsed().as_secs_f64() * 1000.0;
        assert_eq!(actual.state, expected);
        assert_eq!(
            actual.checkpoint_format,
            engine_core::snapshot::CheckpointFormat::SharedV1
        );
        let root: String = sqlx::query_scalar(
            "select state_root from shared_checkpoint where document_id=$1 and through_seq=$2",
        )
        .bind(doc.0)
        .bind(seq)
        .fetch_one(&mut *connection)
        .await?;
        let root = Address::try_from(root)?;
        assert_eq!(root, published_root);
        let mut store = PgStore::new(&mut connection);
        let start = Instant::now();
        assert_eq!(
            read_state(&mut store, doc, &root, Limits::default()).await?,
            expected
        );
        let batched_ms = start.elapsed().as_secs_f64() * 1000.0;
        let batched_queries = store.read_queries();
        let mut unbatched = Unbatched(PgStore::new(&mut connection));
        let start = Instant::now();
        assert_eq!(
            read_state(&mut unbatched, doc, &root, Limits::default()).await?,
            expected
        );
        let unbatched_ms = start.elapsed().as_secs_f64() * 1000.0;
        let unbatched_queries = unbatched.0.read_queries();
        let start = Instant::now();
        let value = sqlx::query_scalar(
            "select state from snapshot where document_id=$1 and through_seq=$2",
        )
        .bind(doc.0)
        .bind(seq)
        .fetch_one(&mut *connection)
        .await?;
        let decoded: DocumentState = serde_json::from_value(value)?;
        assert_eq!(decoded, expected);
        let legacy_read_ms = start.elapsed().as_secs_f64() * 1000.0;
        results.push(json!({"revision":seq,"root":root.as_str(),"publication_object_inserts":write_queries,"publication_object_selects":read_queries,"strong_lock_wait_ms":strong_lock_wait_ms,"legacy_write_ms":legacy_ms,"shared_write_verify_ms":shared_ms,"legacy_wal_bytes":legacy_wal,"shared_wal_bytes":shared_wal,"normal_read_ms":normal_read_ms,"batched_read_ms":batched_ms,"batched_object_selects":batched_queries,"unbatched_read_ms":unbatched_ms,"unbatched_object_selects":unbatched_queries,"legacy_read_ms":legacy_read_ms}));
        eprintln!("Measured checkpoint {edit}/{edits}");
    }
    verify_chain(&pool, doc).await?;
    let (objects, bytes): (i64,i64) = sqlx::query_as("select count(*),sum(octet_length(bytes))::bigint from checkpoint_object where document_id=$1").bind(doc.0).fetch_one(&pool).await?;
    println!(
        "{}",
        serde_json::to_string_pretty(
            &json!({"mode":mode,"workload":workload,"fixed_node_ids":true,"blocks":blocks,"checkpoints":edits+1,"baseline_json_bytes":baseline,"unique_objects":objects,"unique_canonical_bytes":bytes,"initial_relations":initial_sizes,"final_relations":storage(&pool).await?,"measurements":results,"scope":"Single local disposable PostgreSQL run. WAL is cluster-wide and may include background work. Sequential warm reads, no native assets or concurrent load. Both publication modes derive from a matching legacy checkpoint. Individual mode preserves per-object SQL as a benchmark reference, with current codec/validation. Separate fresh databases and identical fixed-ID fixtures are required. Legacy-write timing includes previous shared reads. No production rollout or service-load claim."})
        )?
    );
    Ok(())
}
