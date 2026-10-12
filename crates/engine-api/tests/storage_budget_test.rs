//! Budgets charge retained objects once, serialize concurrent writers and release
//! only through the existing audited erasure path. Runtime cannot adjust counters.
use engine_core::shared_checkpoint::{content, postgres::PgStore, Error, Limits};
use engine_shared::DocumentId;
use sqlx::PgPool;
use uuid::Uuid;
mod common;

#[sqlx::test(migrations = false)]
async fn migration_accounts_for_existing_shared_objects_without_rewriting_them(pool: PgPool) {
    let all = sqlx::migrate!("../../migrations");
    let old = sqlx::migrate::Migrator {
        migrations: std::borrow::Cow::Owned(
            all.iter().filter(|m| m.version <= 33).cloned().collect(),
        ),
        ..sqlx::migrate::Migrator::DEFAULT
    };
    old.run(&pool).await.unwrap();
    let actor: Uuid = sqlx::query_scalar(
        "insert into identity(email) values('backfill@example.test') returning id",
    )
    .fetch_one(&pool)
    .await
    .unwrap();
    let first = document(&pool, actor).await;
    let second = document(&pool, actor).await;
    store(&pool, first, b"before quota").await.unwrap();
    store(&pool, second, b"another file").await.unwrap();
    let before: Vec<(Uuid, String, Vec<u8>)> = sqlx::query_as(
        "select document_id,address,bytes from checkpoint_object order by document_id,address",
    )
    .fetch_all(&pool)
    .await
    .unwrap();
    let expected = (
        before.iter().map(|(_, _, bytes)| bytes.len() as i64).sum(),
        before.len() as i64,
    );
    all.run(&pool).await.unwrap();
    let runtime = common::RuntimeDatabase::new(pool.clone()).await;
    assert_eq!(usage(&runtime.pool, actor).await, expected);
    let after: Vec<(Uuid, String, Vec<u8>)> = sqlx::query_as(
        "select document_id,address,bytes from checkpoint_object order by document_id,address",
    )
    .fetch_all(&pool)
    .await
    .unwrap();
    assert_eq!(before, after);
    store(&runtime.pool, first, b"before quota").await.unwrap();
    assert_eq!(usage(&runtime.pool, actor).await, expected);
    runtime.close().await;
}

async fn document(pool: &PgPool, actor: Uuid) -> DocumentId {
    DocumentId(
        sqlx::query_scalar(
            "insert into document(title,created_by) values('Quota test',$1) returning id",
        )
        .bind(actor)
        .fetch_one(pool)
        .await
        .unwrap(),
    )
}
async fn store(pool: &PgPool, doc: DocumentId, bytes: &[u8]) -> Result<(), Error> {
    let mut tx = pool.begin().await?;
    content::write(
        &mut PgStore::new(&mut tx),
        doc,
        &mut &bytes[..],
        Limits::default(),
    )
    .await?;
    tx.commit().await?;
    Ok(())
}
async fn usage(pool: &PgPool, actor: Uuid) -> (i64, i64) {
    sqlx::query_as(
        "select bytes_used,objects_used from checkpoint_storage_account where identity_id=$1",
    )
    .bind(actor)
    .fetch_one(pool)
    .await
    .unwrap()
}
fn quota(error: &Error) -> bool {
    matches!(error, Error::Db(sqlx::Error::Database(e)) if e.code().as_deref()==Some("23514") && e.constraint()==Some("checkpoint_storage_quota"))
}

#[sqlx::test(migrations = "../../migrations")]
async fn shared_budget_counts_unique_bytes_rolls_back_and_releases_on_audited_erasure(
    pool: PgPool,
) {
    let runtime = common::RuntimeDatabase::new(pool.clone()).await;
    let actor: Uuid =
        sqlx::query_scalar("insert into identity(email) values('quota@example.test') returning id")
            .fetch_one(&runtime.pool)
            .await
            .unwrap();
    let doc = document(&runtime.pool, actor).await;
    store(&runtime.pool, doc, b"baseline").await.unwrap();
    let original = usage(&runtime.pool, actor).await;
    assert!(original.0 > 8 && original.1 > 0);
    let rows: (i64,i64) = sqlx::query_as("select sum(octet_length(bytes))::bigint,count(*) from checkpoint_object where document_id=$1").bind(doc.0).fetch_one(&pool).await.unwrap();
    assert_eq!(rows, original);
    for table in ["checkpoint_storage_account", "checkpoint_storage_document"] {
        assert!(
            sqlx::query(&format!("update {table} set byte_limit=byte_limit+1"))
                .execute(&runtime.pool)
                .await
                .is_err()
        );
        assert!(sqlx::query(&format!("delete from {table}"))
            .execute(&runtime.pool)
            .await
            .is_err());
    }
    // The operator may reduce a budget to current use, never below it.
    sqlx::query(
        "update checkpoint_storage_document set byte_limit=bytes_used where document_id=$1",
    )
    .bind(doc.0)
    .execute(&pool)
    .await
    .unwrap();
    store(&runtime.pool, doc, b"baseline").await.unwrap();
    assert_eq!(usage(&runtime.pool, actor).await, original);
    let failed = store(&runtime.pool, doc, b"different").await.unwrap_err();
    assert!(quota(&failed), "{failed}");
    let response: engine_api::error::ApiError = failed.into();
    assert!(matches!(
        response,
        engine_api::error::ApiError::StorageLimit
    ));
    assert_eq!(usage(&runtime.pool, actor).await, original);
    let count: i64 =
        sqlx::query_scalar("select count(*) from checkpoint_object where document_id=$1")
            .bind(doc.0)
            .fetch_one(&pool)
            .await
            .unwrap();
    assert_eq!(count, original.1);
    // Exercise the object-count limit independently of the byte limit.
    sqlx::query("update checkpoint_storage_document set byte_limit=1073741824,object_limit=objects_used where document_id=$1").bind(doc.0).execute(&pool).await.unwrap();
    assert!(quota(
        &store(&runtime.pool, doc, b"count limited")
            .await
            .unwrap_err()
    ));
    assert_eq!(usage(&runtime.pool, actor).await, original);
    assert!(
        sqlx::query("delete from checkpoint_object where document_id=$1")
            .bind(doc.0)
            .execute(&runtime.pool)
            .await
            .is_err()
    );
    let mut tx = runtime.pool.begin().await.unwrap();
    sqlx::query("update document set deleted_at=now() where id=$1")
        .bind(doc.0)
        .execute(&mut *tx)
        .await
        .unwrap();
    sqlx::query("insert into erasure_receipt(resource_id,kind,actor_id,reason,counts) values($1,'documents',$2,'owner_request','{}')").bind(doc.0).bind(actor).execute(&mut *tx).await.unwrap();
    sqlx::query("delete from document where id=$1")
        .bind(doc.0)
        .execute(&mut *tx)
        .await
        .unwrap();
    tx.commit().await.unwrap();
    assert_eq!(usage(&runtime.pool, actor).await, (0, 0));
    let documents: i64 =
        sqlx::query_scalar("select count(*) from checkpoint_storage_document where document_id=$1")
            .bind(doc.0)
            .fetch_one(&pool)
            .await
            .unwrap();
    assert_eq!(documents, 0);
    runtime.close().await;
}

#[sqlx::test(migrations = "../../migrations")]
async fn same_account_concurrent_documents_cannot_spend_the_same_remaining_budget(pool: PgPool) {
    let runtime = common::RuntimeDatabase::new(pool.clone()).await;
    let actor: Uuid = sqlx::query_scalar(
        "insert into identity(email) values('parallel@example.test') returning id",
    )
    .fetch_one(&runtime.pool)
    .await
    .unwrap();
    let baseline = document(&runtime.pool, actor).await;
    store(&runtime.pool, baseline, b"baseline").await.unwrap();
    let one = usage(&runtime.pool, actor).await;
    sqlx::query(
        "update checkpoint_storage_account set byte_limit=bytes_used*2 where identity_id=$1",
    )
    .bind(actor)
    .execute(&pool)
    .await
    .unwrap();
    let left = document(&runtime.pool, actor).await;
    let right = document(&runtime.pool, actor).await;
    let (left, right) = tokio::join!(
        store(&runtime.pool, left, b"baseline"),
        store(&runtime.pool, right, b"baseline")
    );
    assert_ne!(left.is_ok(), right.is_ok(), "{left:?} {right:?}");
    assert!(quota(&left.err().or(right.err()).unwrap()));
    assert_eq!(usage(&runtime.pool, actor).await, (one.0 * 2, one.1 * 2));
    // Exhaust the account object budget while leaving both document budgets and
    // the account byte budget free. Another document cannot bypass this bound.
    sqlx::query("update checkpoint_storage_account set byte_limit=4294967296,object_limit=objects_used where identity_id=$1").bind(actor).execute(&pool).await.unwrap();
    let third = document(&runtime.pool, actor).await;
    assert!(quota(
        &store(&runtime.pool, third, b"baseline").await.unwrap_err()
    ));
    runtime.close().await;
}
