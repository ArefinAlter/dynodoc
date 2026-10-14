//! Compression at rest (schema 3) and `compact`: round trips, mixed rows, the
//! 2 -> 3 upgrade with data, hostile rows, accounting equality, idempotence,
//! resumability, crash safety and a 20k-object smoke run.
use super::compact::Stage;
use super::tests::{cleanup, path};
use super::*;
use crate::{
    materializer::MaterializedNode,
    repository::{tests::signed, FileEntry},
    shared_checkpoint::MemoryStore,
};
use engine_shared::NodeId;
use serde_json::json;
use std::time::Instant;

const WORDS: [&str; 64] = [
    "the",
    "budget",
    "for",
    "quarter",
    "was",
    "revised",
    "after",
    "review",
    "with",
    "finance",
    "and",
    "operations",
    "teams",
    "to",
    "reflect",
    "updated",
    "headcount",
    "plans",
    "vendor",
    "contracts",
    "renewal",
    "dates",
    "customer",
    "support",
    "volume",
    "forecast",
    "assumptions",
    "risk",
    "register",
    "timeline",
    "milestone",
    "delivery",
    "approved",
    "pending",
    "owner",
    "committee",
    "meeting",
    "notes",
    "action",
    "items",
    "scope",
    "change",
    "request",
    "impact",
    "cost",
    "estimate",
    "schedule",
    "dependency",
    "stakeholder",
    "summary",
    "appendix",
    "table",
    "figure",
    "section",
    "draft",
    "final",
    "version",
    "comment",
    "suggestion",
    "accepted",
    "rejected",
    "deferred",
    "priority",
    "launch",
];

/// Deterministic English-like prose: varied words, no long repeats.
fn prose(seed: u64, words: usize) -> String {
    let mut x = seed.wrapping_mul(0x9E37_79B9_7F4A_7C15) | 1;
    let mut out = String::new();
    for i in 0..words {
        x ^= x << 13;
        x ^= x >> 7;
        x ^= x << 17;
        if i > 0 {
            out.push(' ');
        }
        out.push_str(WORDS[(x % WORDS.len() as u64) as usize]);
        if i % 17 == 16 {
            out.push('.');
        }
    }
    out
}

fn document(count: usize, seed: u64) -> DocumentState {
    let mut state = DocumentState::default();
    for i in 0..count {
        let id = NodeId(format!("{i:026}"));
        state.nodes.insert(
            id.clone(),
            MaterializedNode {
                id,
                parent_id: None,
                node_type: "paragraph".into(),
                pos: format!("p{i:06}"),
                var_name: None,
                deleted: false,
                current_fields: json!({"label": prose(seed * 1000 + i as u64, 60)}),
            },
        );
    }
    state
}

fn doc() -> DocumentId {
    DocumentId(uuid::Uuid::new_v4())
}

/// A real schema-2 database, written exactly as the previous release wrote it
/// (identity rows, no codec column), with one commit and a `main` branch.
async fn legacy(file: &Path, project: uuid::Uuid, docs: &[(DocumentId, DocumentState)]) {
    let pool = SqlitePoolOptions::new()
        .max_connections(1)
        .connect_with(
            SqliteConnectOptions::new()
                .filename(file)
                .create_if_missing(true),
        )
        .await
        .unwrap();
    let mut tx = pool.begin().await.unwrap();
    sqlx::raw_sql(SCHEMA).execute(&mut *tx).await.unwrap();
    sqlx::query("insert into local_meta values(1,1,?)")
        .bind(project.to_string())
        .execute(&mut *tx)
        .await
        .unwrap();
    sqlx::raw_sql(include_str!("storage_v2.sql"))
        .execute(&mut *tx)
        .await
        .unwrap();
    let mut files = BTreeMap::new();
    for (index, (document, state)) in docs.iter().enumerate() {
        let (root, _) = typed::write_state(
            &mut Store {
                connection: &mut tx,
                read_budget: None,
                packed: false,
            },
            *document,
            state,
            Limits::default(),
            Default::default(),
        )
        .await
        .unwrap();
        files.insert(
            format!("File{index}.docx"),
            FileEntry {
                document: *document,
                state: root,
            },
        );
    }
    let commit = signed(project, vec![], files, "Offline history");
    let address = commit.verify().unwrap();
    sqlx::query("insert into commits values(?,?,0)")
        .bind(address.as_str())
        .bind(serde_json::to_vec(&commit).unwrap())
        .execute(&mut *tx)
        .await
        .unwrap();
    sqlx::query("insert into refs values('main',?,1)")
        .bind(address.as_str())
        .execute(&mut *tx)
        .await
        .unwrap();
    tx.commit().await.unwrap();
    pool.close().await;
}

/// Every object as `export-object` would give it: decoded canonical bytes.
async fn packets(repo: &LocalRepository) -> Vec<(ObjectKey, Vec<u8>)> {
    let mut all = Vec::new();
    let mut cursor: Option<ObjectKey> = None;
    loop {
        let page = repo.inventory(cursor.as_ref(), 256).await.unwrap();
        let Some(last) = page.last().cloned() else {
            return all;
        };
        for key in page {
            let packet = repo.object(&key).await.unwrap();
            all.push((key, packet.bytes));
        }
        cursor = Some(last);
    }
}

async fn codecs(repo: &LocalRepository) -> (i64, i64) {
    let rows: Vec<(i64, i64)> =
        sqlx::query_as("select codec,count(*) from objects group by codec order by codec")
            .fetch_all(&repo.pool)
            .await
            .unwrap();
    let count = |codec| {
        rows.iter()
            .find(|(c, _)| *c == codec)
            .map_or(0, |(_, n)| *n)
    };
    (count(0), count(1))
}

async fn recompute(repo: &LocalRepository) {
    let usage = repo.storage_usage().await.unwrap();
    let (objects, logical, stored): (i64, i64, i64) = sqlx::query_as(
        "select count(*),coalesce(sum(coalesce(size,length(bytes))),0),coalesce(sum(length(bytes)),0) from objects",
    )
    .fetch_one(&repo.pool)
    .await
    .unwrap();
    let (commits, commit_bytes): (i64, i64) =
        sqlx::query_as("select count(*),coalesce(sum(length(body)),0) from commits")
            .fetch_one(&repo.pool)
            .await
            .unwrap();
    assert_eq!(usage.objects_used, objects);
    assert_eq!(usage.commits_used, commits);
    assert_eq!(usage.bytes_used, logical + commit_bytes);
    assert_eq!(usage.stored_bytes, stored + commit_bytes);
    assert!(usage.stored_bytes <= usage.bytes_used);
}

/// Replace the stored bytes of one row the way only a deliberate repair could:
/// inside a transaction that holds the compaction row.
async fn repair(repo: &LocalRepository, address: &str, bytes: Vec<u8>) {
    let mut tx = repo.pool.begin().await.unwrap();
    sqlx::query("insert into local_compaction(singleton) values(1)")
        .execute(&mut *tx)
        .await
        .unwrap();
    let changed = sqlx::query("update objects set bytes=? where address=?")
        .bind(bytes)
        .bind(address)
        .execute(&mut *tx)
        .await
        .unwrap()
        .rows_affected();
    assert_eq!(changed, 1);
    sqlx::query("delete from local_compaction")
        .execute(&mut *tx)
        .await
        .unwrap();
    tx.commit().await.unwrap();
}

#[tokio::test]
async fn new_objects_are_compressed_and_addresses_bytes_and_quotas_are_unchanged() {
    let file = path();
    let project = uuid::Uuid::new_v4();
    let repo = LocalRepository::open(&file, project).await.unwrap();
    let id = doc();
    let state = document(48, 1);
    let (root, stats) = repo.write_state(id, &state).await.unwrap();

    // The address is the canonical one: a store that never compresses agrees.
    let (canonical_root, canonical) = typed::write_state(
        &mut MemoryStore::default(),
        id,
        &state,
        Limits::default(),
        Default::default(),
    )
    .await
    .unwrap();
    assert_eq!(root, canonical_root);
    assert_eq!(stats.new_bytes, canonical.new_bytes);
    assert_eq!(stats.new_objects, canonical.new_objects);

    // Logical counters equal the canonical bytes; physical ones are smaller.
    let usage = repo.storage_usage().await.unwrap();
    assert_eq!(usage.bytes_used, stats.new_bytes as i64);
    assert_eq!(usage.objects_used, stats.new_objects as i64);
    assert!(usage.stored_bytes < usage.bytes_used, "{usage:?}");
    recompute(&repo).await;

    // Mixed rows: small index/scalar objects stay identity, text chunks compress.
    let (identity, compressed) = codecs(&repo).await;
    assert!(identity > 0 && compressed > 0, "{identity} {compressed}");

    // Exported packets are the uncompressed canonical objects and verify.
    let all = packets(&repo).await;
    assert_eq!(all.len() as i64, usage.objects_used);
    let mut total = 0usize;
    for (key, bytes) in &all {
        shared_checkpoint::validate_stored(&key.address, bytes).unwrap();
        total += bytes.len();
    }
    assert_eq!(total as i64, usage.bytes_used);
    assert_eq!(repo.read_state(id, &root).await.unwrap(), state);

    // The same packets import into a repository that never compresses.
    let legacy_file = path();
    let legacy_project = uuid::Uuid::new_v4();
    legacy(&legacy_file, legacy_project, &[]).await;
    let other = LocalRepository::open(&legacy_file, legacy_project)
        .await
        .unwrap();
    for chunk in all.chunks(32) {
        let batch: Vec<ObjectPacket> = chunk
            .iter()
            .map(|(key, bytes)| ObjectPacket {
                key: key.clone(),
                bytes: bytes.clone(),
            })
            .collect();
        other.import_objects(&batch).await.unwrap();
    }
    assert_eq!(other.read_state(id, &root).await.unwrap(), state);
    // The other repository (opened from a schema-2 file) holds the same logical
    // bytes; its one empty commit is the only other charge.
    let held: i64 = sqlx::query_scalar("select sum(coalesce(size,length(bytes))) from objects")
        .fetch_one(&other.pool)
        .await
        .unwrap();
    assert_eq!(held, usage.bytes_used);

    // Reopening changes nothing.
    repo.close().await;
    let repo = LocalRepository::open(&file, project).await.unwrap();
    assert_eq!(repo.storage_usage().await.unwrap(), usage);
    assert_eq!(repo.read_state(id, &root).await.unwrap(), state);
    cleanup(other, &legacy_file).await;
    cleanup(repo, &file).await;
}

#[tokio::test]
async fn upgrade_from_schema_two_keeps_every_object_counter_and_plan_hash() {
    let file = path();
    let project = uuid::Uuid::new_v4();
    let (first, second) = (doc(), doc());
    let docs = [(first, document(40, 2)), (second, document(24, 3))];
    legacy(&file, project, &docs).await;

    // A read-only handle sees schema 2 and never upgrades the file.
    let reader = LocalRepository::open_read_only(&file, project)
        .await
        .unwrap();
    let before_plan = reader.gc_plan(GcPlanOptions::default()).await.unwrap();
    assert_eq!(before_plan.local_schema, 2);
    assert_eq!(before_plan.accounting.consistent, Some(true));
    assert_eq!(
        before_plan.physical.stored_bytes,
        before_plan.retained.bytes
    );
    assert_eq!(before_plan.physical.compressed_objects, 0);
    let before_usage = reader.storage_usage().await.unwrap();
    assert_eq!(before_usage.stored_bytes, before_usage.bytes_used);
    let before_packets = packets(&reader).await;
    reader.close().await;

    let repo = LocalRepository::open(&file, project).await.unwrap();
    let version: i64 = sqlx::query_scalar("select version from local_meta")
        .fetch_one(&repo.pool)
        .await
        .unwrap();
    assert_eq!(version, 3);
    assert_eq!(SCHEMA_VERSION, 3);
    // Nothing was rewritten: every row is identity with no declared size.
    let (identity, compressed) = codecs(&repo).await;
    assert_eq!(compressed, 0);
    assert_eq!(identity as usize, before_packets.len());
    let unsized_rows: i64 = sqlx::query_scalar("select count(*) from objects where size is null")
        .fetch_one(&repo.pool)
        .await
        .unwrap();
    assert_eq!(unsized_rows, identity);
    let usage = repo.storage_usage().await.unwrap();
    assert_eq!(usage.bytes_used, before_usage.bytes_used);
    assert_eq!(usage.stored_bytes, usage.bytes_used);
    assert_eq!(usage.objects_used, before_usage.objects_used);
    assert_eq!(usage.limits, before_usage.limits);
    assert_eq!(packets(&repo).await, before_packets);
    let plan = repo.gc_plan(GcPlanOptions::default()).await.unwrap();
    assert_eq!(plan.local_schema, 3);
    assert_eq!(plan.plan_hash, before_plan.plan_hash);
    let head = repo.head("main").await.unwrap().unwrap();
    let commit = repo.commit(&head.commit).await.unwrap();
    for entry in commit.commit.files.values() {
        let expected = docs.iter().find(|(d, _)| *d == entry.document).unwrap();
        assert_eq!(
            repo.read_state(entry.document, &entry.state).await.unwrap(),
            expected.1
        );
    }

    // Old rows and new rows coexist; new writes are compressed and charged
    // logically; then the whole history can be re-encoded.
    let third = doc();
    let extra = document(16, 4);
    let (extra_root, extra_stats) = repo.write_state(third, &extra).await.unwrap();
    let (identity_now, compressed_now) = codecs(&repo).await;
    assert!(compressed_now > 0 && identity_now > identity);
    let usage_mixed = repo.storage_usage().await.unwrap();
    assert_eq!(
        usage_mixed.bytes_used,
        usage.bytes_used + extra_stats.new_bytes as i64
    );
    recompute(&repo).await;

    let report = repo.compact(CompactOptions::default()).await.unwrap();
    assert!(report.complete);
    assert!(report.compacted > 0);
    assert!(report.stored_bytes_saved > 0);
    let usage_after = repo.storage_usage().await.unwrap();
    assert_eq!(usage_after.bytes_used, usage_mixed.bytes_used);
    assert_eq!(
        usage_after.stored_bytes,
        usage_mixed.stored_bytes - report.stored_bytes_saved as i64
    );
    recompute(&repo).await;
    // Exactly the same canonical objects, addresses and bytes as before.
    let after_packets = packets(&repo).await;
    for (key, bytes) in &before_packets {
        let found = after_packets.iter().find(|(k, _)| k == key).unwrap();
        assert_eq!(&found.1, bytes);
    }
    assert_eq!(repo.read_state(third, &extra_root).await.unwrap(), extra);
    let hash_after = repo.gc_plan(GcPlanOptions::default()).await.unwrap();
    assert_eq!(hash_after.accounting.consistent, Some(true));
    assert!(hash_after.physical.compressed_objects > 0);
    assert_eq!(
        hash_after.physical.stored_bytes as i64,
        usage_after.stored_bytes
    );
    assert!(hash_after.physical.stored_bytes < hash_after.retained.bytes);
    cleanup(repo, &file).await;
}

#[tokio::test]
async fn unknown_schemas_are_refused_by_every_open_mode() {
    let file = path();
    let project = uuid::Uuid::new_v4();
    let repo = LocalRepository::open(&file, project).await.unwrap();
    // The number a schema-2 client would reject is the one now written.
    let version: i64 = sqlx::query_scalar("select version from local_meta")
        .fetch_one(&repo.pool)
        .await
        .unwrap();
    assert_eq!(version, 3);
    assert!(!matches!(version, 1 | 2));
    sqlx::query("update local_meta set version=4")
        .execute(&repo.pool)
        .await
        .unwrap();
    assert!(matches!(
        LocalRepository::open(&file, project).await,
        Err(Error::Invalid("local schema/project"))
    ));
    assert!(matches!(
        LocalRepository::open_read_only(&file, project).await,
        Err(Error::Invalid("local schema/project"))
    ));
    sqlx::query("update local_meta set version=3")
        .execute(&repo.pool)
        .await
        .unwrap();
    cleanup(repo, &file).await;
}

#[tokio::test]
async fn limits_and_plans_count_logical_bytes_whatever_the_rows_store() {
    let project = uuid::Uuid::new_v4();
    let id = doc();
    let state = document(32, 5);
    // Baseline: what the previous release stored for the same state (identity rows).
    let base_file = path();
    legacy(&base_file, project, &[(id, state.clone())]).await;
    let base = LocalRepository::open(&base_file, project).await.unwrap();
    let (identity_rows, identity_bytes): (i64, i64) =
        sqlx::query_as("select count(*),sum(length(bytes)) from objects")
            .fetch_one(&base.pool)
            .await
            .unwrap();
    let source_file = path();
    let source = LocalRepository::open(&source_file, project).await.unwrap();
    let (root, stats) = source.write_state(id, &state).await.unwrap();
    let logical = source.storage_usage().await.unwrap().bytes_used;
    // The same bytes and object count the identity-only release held.
    assert_eq!(logical, identity_bytes);
    assert_eq!(
        source.storage_usage().await.unwrap().objects_used,
        identity_rows
    );
    assert_eq!(logical, stats.new_bytes as i64);
    assert!(source.storage_usage().await.unwrap().stored_bytes < logical);

    // A repository whose limit is exactly the logical size accepts it ...
    let exact_file = path();
    let exact = LocalRepository::open(&exact_file, project).await.unwrap();
    exact
        .set_storage_limits(StorageLimits {
            bytes: logical,
            ..Default::default()
        })
        .await
        .unwrap();
    exact.write_state(id, &state).await.unwrap();
    // ... and one byte less does not, although the stored bytes would fit.
    let short_file = path();
    let short = LocalRepository::open(&short_file, project).await.unwrap();
    short
        .set_storage_limits(StorageLimits {
            bytes: logical - 1,
            ..Default::default()
        })
        .await
        .unwrap();
    let error = short.write_state(id, &state).await.unwrap_err();
    assert!(
        matches!(
            error,
            Error::Limit(_) | Error::Object(shared_checkpoint::Error::Limit(_))
        ),
        "{error:?}"
    );
    assert_eq!(short.storage_usage().await.unwrap().bytes_used, 0);
    assert_eq!(short.storage_usage().await.unwrap().stored_bytes, 0);
    // Reading what a larger repository holds uses logical sizes for its budget.
    assert_eq!(exact.read_state(id, &root).await.unwrap(), state);
    cleanup(base, &base_file).await;
    cleanup(source, &source_file).await;
    cleanup(exact, &exact_file).await;
    cleanup(short, &short_file).await;
}

#[tokio::test]
async fn corrupt_and_hostile_rows_are_detected_before_their_bytes_are_used() {
    let file = path();
    let project = uuid::Uuid::new_v4();
    let repo = LocalRepository::open(&file, project).await.unwrap();
    let id = doc();
    let state = document(24, 6);
    let (root, _) = repo.write_state(id, &state).await.unwrap();
    let (document_text, address): (String, String) =
        sqlx::query_as("select document,address from objects where codec=1 limit 1")
            .fetch_one(&repo.pool)
            .await
            .unwrap();
    let key = ObjectKey {
        document: id,
        address: Address::try_from(address.clone()).unwrap(),
    };
    let original = repo.object(&key).await.unwrap().bytes;
    let (stored, size): (Vec<u8>, i64) =
        sqlx::query_as("select bytes,size from objects where document=? and address=?")
            .bind(&document_text)
            .bind(&address)
            .fetch_one(&repo.pool)
            .await
            .unwrap();

    // Outside compaction the immutability guard still refuses every update.
    assert!(sqlx::query("update objects set bytes=bytes")
        .execute(&repo.pool)
        .await
        .is_err());
    assert!(
        sqlx::query("update objects set codec=0,size=null where codec=1")
            .execute(&repo.pool)
            .await
            .is_err()
    );
    assert!(
        sqlx::query("insert into local_compaction(singleton) values(1)")
            .execute(&repo.pool)
            .await
            .is_ok()
    );
    // ... even with the row present, a different logical length is refused.
    assert!(sqlx::query("update objects set size=size-1 where codec=1")
        .execute(&repo.pool)
        .await
        .is_err());
    sqlx::query("delete from local_compaction")
        .execute(&repo.pool)
        .await
        .unwrap();

    // 1. A damaged deflate stream never decodes to the original object.
    let mut damaged = stored.clone();
    let middle = damaged.len() / 2;
    damaged[middle] ^= 0xA5;
    repair(&repo, &address, damaged).await;
    assert!(repo.object(&key).await.is_err());
    assert!(repo.read_state(id, &root).await.is_err());
    // 2. Truncated.
    let truncated = stored[..stored.len() - 4].to_vec();
    repair(&repo, &address, truncated).await;
    assert!(repo.object(&key).await.is_err());
    // 3. Restored: reads work again and accounting never moved.
    repair(&repo, &address, stored.clone()).await;
    assert_eq!(repo.object(&key).await.unwrap().bytes, original);
    assert_eq!(repo.read_state(id, &root).await.unwrap(), state);
    recompute(&repo).await;

    // 4. A declared length that is wrong: smaller than the stream, or larger.
    sqlx::query("drop trigger objects_no_update")
        .execute(&repo.pool)
        .await
        .unwrap();
    for wrong in [size - 1, size + 1, stored.len() as i64 + 1] {
        sqlx::query("update objects set size=? where codec=1 and address=?")
            .bind(wrong)
            .bind(&address)
            .execute(&repo.pool)
            .await
            .unwrap();
        assert!(repo.object(&key).await.is_err(), "declared {wrong}");
    }
    sqlx::query("update objects set size=? where codec=1 and address=?")
        .bind(size)
        .bind(&address)
        .execute(&repo.pool)
        .await
        .unwrap();
    assert_eq!(repo.object(&key).await.unwrap().bytes, original);
    // A codec this build does not know is refused on read (the table reserves the
    // number so a later codec needs no rebuild); the declared length is bounded.
    sqlx::query("update objects set codec=2 where address=?")
        .bind(&address)
        .execute(&repo.pool)
        .await
        .unwrap();
    let unknown = match repo.object(&key).await {
        Err(error) => error,
        Ok(_) => panic!("an unknown codec must not decode"),
    };
    assert!(
        unknown.to_string().contains("stored object codec"),
        "{unknown}"
    );
    sqlx::query("update objects set codec=1 where address=?")
        .bind(&address)
        .execute(&repo.pool)
        .await
        .unwrap();
    assert_eq!(repo.object(&key).await.unwrap().bytes, original);
    assert!(
        sqlx::query("update objects set size=1048577 where codec=1 and address=?")
            .bind(&address)
            .execute(&repo.pool)
            .await
            .is_err()
    );

    // 5. A decompression bomb: a ~1 KiB stream that inflates to 1 MiB. It is stored
    // under a made-up address; reading it must fail on the hash, after inflating at
    // most the declared size, and with a small declared size must fail on inflation.
    let bomb = miniz_oxide::deflate::compress_to_vec(&vec![0u8; 1_048_576], 9);
    assert!(bomb.len() < 2048);
    let fake = "ab".repeat(32);
    sqlx::query("insert into objects(document,address,bytes,codec,size) values(?,?,?,1,?)")
        .bind(id.0.to_string())
        .bind(&fake)
        .bind(&bomb)
        .bind(1_048_576i64)
        .execute(&repo.pool)
        .await
        .unwrap();
    let fake_key = ObjectKey {
        document: id,
        address: Address::try_from(fake.clone()).unwrap(),
    };
    assert!(repo.object(&fake_key).await.is_err());
    let hostile = "cd".repeat(32);
    sqlx::query("insert into objects(document,address,bytes,codec,size) values(?,?,?,1,?)")
        .bind(id.0.to_string())
        .bind(&hostile)
        .bind(&bomb)
        .bind(4096i64)
        .execute(&repo.pool)
        .await
        .unwrap();
    let error = match repo
        .object(&ObjectKey {
            document: id,
            address: Address::try_from(hostile).unwrap(),
        })
        .await
    {
        Err(error) => error,
        Ok(_) => panic!("a hostile row must not decode"),
    };
    assert!(
        error.to_string().contains("stored object compression"),
        "{error}"
    );
    cleanup(repo, &file).await;
}

#[tokio::test]
async fn compact_is_resumable_idempotent_and_changes_no_logical_byte() {
    let file = path();
    let project = uuid::Uuid::new_v4();
    let docs = [(doc(), document(40, 7)), (doc(), document(30, 8))];
    legacy(&file, project, &docs).await;
    let repo = LocalRepository::open(&file, project).await.unwrap();
    let before_packets = packets(&repo).await;
    let before = repo.storage_usage().await.unwrap();
    let plan = repo.gc_plan(GcPlanOptions::default()).await.unwrap();
    let total_rows = before_packets.len() as u64;

    // First call stops early and gives a cursor.
    let first = repo
        .compact(CompactOptions {
            batch_objects: 7,
            max_objects: Some(20),
            after: None,
            vacuum: false,
        })
        .await
        .unwrap();
    assert!(!first.complete);
    assert_eq!(first.examined, 20);
    assert_eq!(first.batches, 3);
    assert!(first.compacted > 0);
    let next = first.next.clone().unwrap();
    recompute(&repo).await;

    // Resume from it; the rest completes and nothing is examined twice.
    let rest = repo
        .compact(CompactOptions {
            batch_objects: 11,
            max_objects: None,
            after: Some(ObjectKey {
                document: DocumentId(next.document),
                address: Address::try_from(next.address).unwrap(),
            }),
            vacuum: false,
        })
        .await
        .unwrap();
    assert!(rest.complete);
    assert_eq!(first.examined + rest.examined, total_rows);
    assert_eq!(
        first.compacted + rest.compacted + first.kept_identity + rest.kept_identity,
        total_rows
    );

    let after = repo.storage_usage().await.unwrap();
    assert_eq!(after.bytes_used, before.bytes_used);
    assert_eq!(after.objects_used, before.objects_used);
    assert_eq!(after.limits, before.limits);
    assert_eq!(
        after.stored_bytes,
        before.stored_bytes - (first.stored_bytes_saved + rest.stored_bytes_saved) as i64
    );
    assert!(after.stored_bytes < before.stored_bytes);
    recompute(&repo).await;
    assert_eq!(packets(&repo).await, before_packets);
    let plan_after = repo.gc_plan(GcPlanOptions::default()).await.unwrap();
    assert_eq!(plan_after.plan_hash, plan.plan_hash);

    // A second full run examines everything and changes nothing.
    let again = repo.compact(CompactOptions::default()).await.unwrap();
    assert!(again.complete);
    assert_eq!(again.examined, total_rows);
    assert_eq!(again.compacted, 0);
    assert_eq!(again.stored_bytes_saved, 0);
    assert_eq!(again.already_compressed, first.compacted + rest.compacted);
    assert_eq!(repo.storage_usage().await.unwrap(), after);
    assert!(again.summary().contains("complete"));
    // Batch size bounds are enforced.
    for batch_objects in [0, 1025] {
        assert!(repo
            .compact(CompactOptions {
                batch_objects,
                ..Default::default()
            })
            .await
            .is_err());
    }
    // Staged objects collected afterwards report both byte kinds and stay exact.
    let staged_doc = doc();
    repo.write_state(staged_doc, &document(10, 9))
        .await
        .unwrap();
    let plan = repo.gc_plan(GcPlanOptions::default()).await.unwrap();
    let report = repo
        .gc_apply(&plan.plan_hash, GcApplyOptions::default())
        .await
        .unwrap();
    assert!(report.applied);
    assert!(report.deleted.stored_bytes > 0);
    assert!(report.deleted.stored_bytes <= report.deleted.object_bytes);
    assert_eq!(
        report.capacity_before.stored_bytes - report.capacity_after.stored_bytes,
        report.deleted.stored_bytes as i64
    );
    assert!(report.summary().contains("stored"));
    recompute(&repo).await;
    cleanup(repo, &file).await;
}

#[tokio::test]
async fn an_interrupted_compaction_commits_only_whole_verified_batches() {
    let file = path();
    let project = uuid::Uuid::new_v4();
    legacy(
        &file,
        project,
        &[(doc(), document(40, 10)), (doc(), document(20, 11))],
    )
    .await;
    let repo = LocalRepository::open(&file, project).await.unwrap();
    let packets_before = packets(&repo).await;
    let usage_before = repo.storage_usage().await.unwrap();
    let rows_before: Vec<(String, String, Vec<u8>, i64)> = sqlx::query_as(
        "select document,address,bytes,codec from objects order by document,address",
    )
    .fetch_all(&repo.pool)
    .await
    .unwrap();

    // Failing at every point of the first batch changes nothing at all.
    for stage in [Stage::Locked, Stage::Rewritten, Stage::Accounted] {
        let error = repo
            .compact_with(
                CompactOptions {
                    batch_objects: 16,
                    ..Default::default()
                },
                &mut |at| {
                    if at == stage {
                        Err(Error::Invalid("injected failure"))
                    } else {
                        Ok(())
                    }
                },
            )
            .await
            .unwrap_err();
        assert!(matches!(error, Error::Invalid("injected failure")));
        let rows: Vec<(String, String, Vec<u8>, i64)> = sqlx::query_as(
            "select document,address,bytes,codec from objects order by document,address",
        )
        .fetch_all(&repo.pool)
        .await
        .unwrap();
        assert_eq!(rows, rows_before, "{stage:?}");
        assert_eq!(repo.storage_usage().await.unwrap(), usage_before);
        let leftover: i64 = sqlx::query_scalar("select count(*) from local_compaction")
            .fetch_one(&repo.pool)
            .await
            .unwrap();
        assert_eq!(leftover, 0, "{stage:?}");
    }

    // Failing right after the third batch committed keeps three whole batches.
    let mut batches = 0;
    let error = repo
        .compact_with(
            CompactOptions {
                batch_objects: 16,
                ..Default::default()
            },
            &mut |at| {
                if at == Stage::Committed {
                    batches += 1;
                    if batches == 3 {
                        return Err(Error::Invalid("injected failure"));
                    }
                }
                Ok(())
            },
        )
        .await
        .unwrap_err();
    assert!(matches!(error, Error::Invalid("injected failure")));
    let (identity, compressed) = codecs(&repo).await;
    assert!(identity > 0 && compressed > 0);
    recompute(&repo).await;
    assert_eq!(packets(&repo).await, packets_before);
    let partial = repo.storage_usage().await.unwrap();
    assert!(partial.stored_bytes < usage_before.stored_bytes);
    assert_eq!(partial.bytes_used, usage_before.bytes_used);

    // Run again from the start: finishes the remainder, no row examined wrongly.
    let report = repo.compact(CompactOptions::default()).await.unwrap();
    assert!(report.complete);
    assert!(report.already_compressed > 0);
    recompute(&repo).await;
    assert_eq!(packets(&repo).await, packets_before);

    // A corrupt identity row stops the run with an error and rolls its batch back.
    let corrupt_file = path();
    legacy(&corrupt_file, project, &[(doc(), document(20, 12))]).await;
    let corrupt = LocalRepository::open(&corrupt_file, project).await.unwrap();
    let (address, bytes): (String, Vec<u8>) = sqlx::query_as(
        "select address,bytes from objects where length(bytes)>300 order by address limit 1",
    )
    .fetch_one(&corrupt.pool)
    .await
    .unwrap();
    let mut flipped = bytes.clone();
    let last = flipped.len() - 1;
    flipped[last] ^= 1;
    repair(&corrupt, &address, flipped).await;
    let usage = corrupt.storage_usage().await.unwrap();
    assert!(corrupt.compact(CompactOptions::default()).await.is_err());
    assert_eq!(corrupt.storage_usage().await.unwrap(), usage);
    let compressed: i64 = sqlx::query_scalar("select count(*) from objects where codec=1")
        .fetch_one(&corrupt.pool)
        .await
        .unwrap();
    assert_eq!(compressed, 0);
    cleanup(corrupt, &corrupt_file).await;
    cleanup(repo, &file).await;
}

#[tokio::test]
async fn writes_proceed_while_compaction_runs_and_stay_accounted() {
    let file = path();
    let project = uuid::Uuid::new_v4();
    legacy(&file, project, &[(doc(), document(60, 13))]).await;
    let repo = LocalRepository::open(&file, project).await.unwrap();
    let writer = repo.clone();
    let late = doc();
    let states: Vec<DocumentState> = (0..4).map(|i| document(6, 20 + i)).collect();
    let (report, roots) = tokio::join!(
        repo.compact(CompactOptions {
            batch_objects: 10,
            ..Default::default()
        }),
        async {
            let mut roots = Vec::new();
            for state in &states {
                roots.push(writer.write_state(late, state).await.unwrap().0);
            }
            roots
        }
    );
    let report = report.unwrap();
    assert!(report.complete);
    for (state, root) in states.iter().zip(&roots) {
        assert_eq!(&repo.read_state(late, root).await.unwrap(), state);
    }
    recompute(&repo).await;
    let usage = repo.storage_usage().await.unwrap();
    assert!(usage.stored_bytes < usage.bytes_used);
    cleanup(repo, &file).await;
}

#[tokio::test]
async fn twenty_thousand_objects_are_written_compacted_and_verified() {
    let file = path();
    let project = uuid::Uuid::new_v4();
    // Many small documents so the object count, not the object size, is large.
    let mut docs = Vec::new();
    let mut objects = 0usize;
    let started = Instant::now();
    let mut seed = 100u64;
    let probe = {
        let mut store = MemoryStore::default();
        typed::write_state(
            &mut store,
            doc(),
            &document(40, seed),
            Limits::default(),
            Default::default(),
        )
        .await
        .unwrap()
        .1
        .new_objects
    };
    while objects < 20_000 {
        docs.push((doc(), document(40, seed)));
        seed += 1;
        objects += probe;
    }
    legacy(&file, project, &docs).await;
    let built = started.elapsed();
    let repo = LocalRepository::open(&file, project).await.unwrap();
    let before = repo.storage_usage().await.unwrap();
    assert!(before.objects_used >= 20_000, "{}", before.objects_used);
    let started = Instant::now();
    let report = repo
        .compact(CompactOptions {
            batch_objects: 256,
            ..Default::default()
        })
        .await
        .unwrap();
    let compacted_in = started.elapsed();
    println!(
        "pack smoke: {} objects built in {built:?}; compact {} rows in {} batches, {} rewritten, {} -> {} stored bytes (logical {}) in {compacted_in:?}",
        before.objects_used,
        report.examined,
        report.batches,
        report.compacted,
        before.stored_bytes,
        report.capacity_after.stored_bytes,
        before.bytes_used,
    );
    assert!(report.complete);
    assert_eq!(report.examined as i64, before.objects_used);
    assert!(report.compacted > 0);
    assert_eq!(report.capacity_after.bytes_used, before.bytes_used);
    assert!(report.capacity_after.stored_bytes < before.stored_bytes);
    recompute(&repo).await;
    // Spot-check reads after compaction across the key space.
    let all = repo.inventory(None, 256).await.unwrap();
    for key in all.iter().step_by(17) {
        let packet = repo.object(key).await.unwrap();
        shared_checkpoint::validate_stored(&key.address, &packet.bytes).unwrap();
    }
    let head = repo.head("main").await.unwrap().unwrap();
    let commit = repo.commit(&head.commit).await.unwrap();
    for (index, (document, state)) in docs.iter().enumerate().step_by(97) {
        let entry = &commit.commit.files[&format!("File{index}.docx")];
        assert_eq!(entry.document, *document);
        assert_eq!(
            &repo.read_state(*document, &entry.state).await.unwrap(),
            state
        );
    }
    cleanup(repo, &file).await;
}

/// Paragraphs of this repository's own documentation (prose, 160 to 900
/// characters, de-duplicated, file-name order) for the measurement below.
fn documentation_paragraphs() -> Vec<String> {
    let docs = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../docs");
    let mut names: Vec<_> = std::fs::read_dir(docs)
        .unwrap()
        .filter_map(|e| e.ok().map(|e| e.path()))
        .filter(|p| p.extension().is_some_and(|x| x == "md"))
        .collect();
    names.sort();
    let mut seen = std::collections::BTreeSet::new();
    let mut found = Vec::new();
    for name in names {
        let text = std::fs::read_to_string(name)
            .unwrap_or_default()
            .replace("\r\n", "\n");
        for block in text.split("\n\n") {
            let paragraph = block.split_whitespace().collect::<Vec<_>>().join(" ");
            let first = paragraph.chars().next();
            if (160..=900).contains(&paragraph.len())
                && !matches!(first, Some('|' | '#' | '`' | '>' | '*' | '-' | '0'..='9'))
                && seen.insert(paragraph.clone())
            {
                found.push(paragraph);
            }
        }
    }
    found
}

/// Release-mode cost of compression in the write and read paths, for
/// docs/benchmarks/local-compression-20261014.json. The identity layout writes
/// rows exactly as the schema-2 release did; the packed layout is the new path.
/// Run: cargo test --release -p engine-core -- --ignored --nocapture measure_
#[tokio::test]
#[ignore = "measurement, not a regression test"]
async fn measure_write_and_read_cost_of_compression() {
    use std::time::Duration;
    let pool = documentation_paragraphs();
    let edits = 32usize;
    let mut results = Vec::new();
    for (fixture, nodes) in [
        ("repetitive", 128usize),
        ("repetitive", 512),
        ("docs", 128),
        ("docs", 512),
    ] {
        assert!(pool.len() >= 2 * nodes + 256 || fixture == "repetitive");
        let label = |i: usize, edit: usize| -> String {
            if fixture == "docs" {
                pool[if edit == 0 { i } else { nodes + edit * 7 + i } % pool.len()].clone()
            } else if edit == 0 {
                format!("Paragraph {i}: {}", "x".repeat(256))
            } else {
                format!("Edit {edit}: {}", "y".repeat(256))
            }
        };
        let mut layouts = serde_json::Map::new();
        for packed in [false, true] {
            let file = path();
            let project = uuid::Uuid::new_v4();
            let repo = LocalRepository::open(&file, project).await.unwrap();
            let id = doc();
            let mut state = DocumentState::default();
            for i in 0..nodes {
                let node_id = NodeId(format!("{:026}", i + 1));
                state.nodes.insert(
                    node_id.clone(),
                    MaterializedNode {
                        id: node_id,
                        parent_id: None,
                        node_type: "paragraph".into(),
                        pos: format!("p{i:06}"),
                        var_name: None,
                        deleted: false,
                        current_fields: json!({"label": label(i, 0)}),
                    },
                );
            }
            let keys: Vec<NodeId> = state.nodes.keys().cloned().collect();
            let mut write_times: Vec<Duration> = Vec::new();
            let mut roots = Vec::new();
            for revision in 0..=edits {
                if revision > 0 {
                    let index = (revision - 1) % nodes;
                    state.nodes.get_mut(&keys[index]).unwrap().current_fields =
                        json!({"label": label(index, revision)});
                }
                let started = Instant::now();
                let mut tx = repo.pool.begin().await.unwrap();
                let (root, _) = typed::write_state(
                    &mut Store {
                        connection: &mut tx,
                        read_budget: None,
                        packed,
                    },
                    id,
                    &state,
                    Limits::default(),
                    Default::default(),
                )
                .await
                .unwrap();
                tx.commit().await.unwrap();
                write_times.push(started.elapsed());
                roots.push(root);
            }
            let mut read_times = Vec::new();
            for root in [&roots[0], roots.last().unwrap()] {
                let started = Instant::now();
                repo.read_state(id, root).await.unwrap();
                read_times.push(started.elapsed());
            }
            // Codec cost on this repository's own objects, in isolation.
            let mut logical = Vec::new();
            let mut cursor = None;
            loop {
                let page = repo.inventory(cursor.as_ref(), 256).await.unwrap();
                let Some(last) = page.last().cloned() else {
                    break;
                };
                for key in page {
                    logical.push(repo.object(&key).await.unwrap().bytes);
                }
                cursor = Some(last);
            }
            let started = Instant::now();
            let packed_forms: Vec<Option<Vec<u8>>> =
                logical.iter().map(|bytes| pack::pack(bytes)).collect();
            let pack_us = started.elapsed().as_secs_f64() * 1e6 / logical.len() as f64;
            let started = Instant::now();
            let mut inflated = 0usize;
            for (bytes, form) in logical.iter().zip(&packed_forms) {
                if let Some(form) = form {
                    let back = pack::unpack(pack::DEFLATE, form.clone(), Some(bytes.len() as i64))
                        .unwrap();
                    inflated += back.len();
                }
            }
            let compressed_count = packed_forms.iter().flatten().count().max(1);
            let unpack_us = started.elapsed().as_secs_f64() * 1e6 / compressed_count as f64;
            assert!(inflated > 0 || !packed);
            let usage = repo.storage_usage().await.unwrap();
            let ms = |d: &Duration| d.as_secs_f64() * 1000.0;
            let mut sorted = write_times.clone();
            sorted.sort();
            layouts.insert(
                if packed { "packed" } else { "identity" }.into(),
                json!({
                    "bytes_used": usage.bytes_used,
                    "stored_bytes": usage.stored_bytes,
                    "objects": usage.objects_used,
                    "write_total_ms": write_times.iter().map(ms).sum::<f64>(),
                    "write_median_ms": ms(&sorted[sorted.len() / 2]),
                    "write_first_revision_ms": ms(&write_times[0]),
                    "read_first_root_ms": ms(&read_times[0]),
                    "read_last_root_ms": ms(&read_times[1]),
                    "pack_microseconds_per_object": pack_us,
                    "unpack_microseconds_per_compressed_object": unpack_us,
                }),
            );
            cleanup(repo, &file).await;
        }
        results
            .push(json!({"fixture": fixture, "nodes": nodes, "edits": edits, "layouts": layouts}));
    }
    println!(
        "LOCAL_COMPRESSION_MEASUREMENT {}",
        serde_json::to_string_pretty(&json!({"in_process_release_measurement": results})).unwrap()
    );
}
