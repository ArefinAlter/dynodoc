use super::tests::{cleanup, path, state};
use super::*;
use crate::repository::{tests::signed, FileEntry};

fn capacity(error: Error) {
    let message = match error {
        Error::Limit(message) | Error::Object(shared_checkpoint::Error::Limit(message)) => message,
        other => panic!("expected capacity limit, got {other:?}"),
    };
    assert!(message.starts_with("local storage capacity"));
}

async fn packets(repo: &LocalRepository) -> Vec<ObjectPacket> {
    let mut result = Vec::new();
    for key in repo.inventory(None, 256).await.unwrap() {
        result.push(repo.object(&key).await.unwrap());
    }
    result
}

#[tokio::test]
async fn upgrade_preserves_v1_graphs_commits_refs_and_above_default_usage() {
    let file = path();
    let project = uuid::Uuid::new_v4();
    let document = DocumentId(uuid::Uuid::new_v4());
    let pool = SqlitePoolOptions::new()
        .max_connections(1)
        .connect_with(
            SqliteConnectOptions::new()
                .filename(&file)
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
    let original = state("Existing offline paragraph");
    let (root, stats) = typed::write_state(
        &mut Store {
            connection: &mut tx,
            read_budget: None,
        },
        document,
        &original,
        Limits::default(),
        Default::default(),
    )
    .await
    .unwrap();
    let commit = signed(
        project,
        vec![],
        BTreeMap::from([(
            "Report.docx".into(),
            FileEntry {
                document,
                state: root.clone(),
            },
        )]),
        "Before quotas",
    );
    let address = commit.verify().unwrap();
    let body = serde_json::to_vec(&commit).unwrap();
    sqlx::query("insert into commits values(?,?,0)")
        .bind(address.as_str())
        .bind(&body)
        .execute(&mut *tx)
        .await
        .unwrap();
    for i in 0..4097 {
        sqlx::query("insert into refs values(?,?,1)")
            .bind(format!("retained-{i}"))
            .bind(address.as_str())
            .execute(&mut *tx)
            .await
            .unwrap();
    }
    tx.commit().await.unwrap();
    // A wrong project must not partially upgrade another repository.
    assert!(LocalRepository::open(&file, uuid::Uuid::new_v4())
        .await
        .is_err());
    assert_eq!(
        sqlx::query_scalar::<_, i64>("select version from local_meta")
            .fetch_one(&pool)
            .await
            .unwrap(),
        1
    );
    pool.close().await;

    let repo = LocalRepository::open(&file, project).await.unwrap();
    let usage = repo.storage_usage().await.unwrap();
    assert_eq!(usage.bytes_used, stats.new_bytes as i64 + body.len() as i64);
    assert_eq!(usage.objects_used, stats.new_objects as i64);
    assert_eq!(usage.commits_used, 1);
    assert_eq!(usage.branches_used, 4097);
    assert_eq!(usage.limits.branches, 4097);
    assert_eq!(repo.read_state(document, &root).await.unwrap(), original);
    assert_eq!(
        repo.commit(&address).await.unwrap().signature,
        commit.signature
    );
    capacity(
        repo.compare_and_swap("one-more", None, &address)
            .await
            .unwrap_err(),
    );
    let head = repo.head("retained-0").await.unwrap().unwrap();
    repo.compare_and_swap("retained-0", Some(&head), &address)
        .await
        .unwrap();
    assert!(matches!(
        repo.set_storage_limits(StorageLimits::default()).await,
        Err(Error::Limit(_))
    ));
    repo.close().await;
    let reopened = LocalRepository::open(&file, project).await.unwrap();
    assert_eq!(reopened.storage_usage().await.unwrap(), usage);
    assert_eq!(
        reopened
            .head("retained-0")
            .await
            .unwrap()
            .unwrap()
            .generation,
        2
    );
    cleanup(reopened, &file).await;
}

#[tokio::test]
async fn failed_graph_and_transfer_batches_roll_back_bytes_and_objects() {
    let file = path();
    let source_file = path();
    let project = uuid::Uuid::new_v4();
    let source = LocalRepository::open(&source_file, project).await.unwrap();
    let document = DocumentId(uuid::Uuid::new_v4());
    let original = state("Local history is retained, including staged transfer objects");
    let root = source.write_state(document, &original).await.unwrap().0;
    let objects = packets(&source).await;
    assert!(objects.len() > 2);
    let repo = LocalRepository::open(&file, project).await.unwrap();
    // Object-count exhaustion happens after the first insertion in the graph.
    repo.set_storage_limits(StorageLimits {
        objects: 1,
        ..Default::default()
    })
    .await
    .unwrap();
    let empty = repo.storage_usage().await.unwrap();
    capacity(repo.write_state(document, &original).await.unwrap_err());
    assert_eq!(repo.storage_usage().await.unwrap(), empty);
    capacity(repo.import_objects(&objects[..2]).await.unwrap_err());
    assert!(repo.inventory(None, 256).await.unwrap().is_empty());
    assert_eq!(repo.storage_usage().await.unwrap(), empty);

    // Plenty of object slots, but only enough bytes for the first packet.
    repo.set_storage_limits(StorageLimits {
        bytes: objects[0].bytes.len() as i64,
        ..Default::default()
    })
    .await
    .unwrap();
    let empty = repo.storage_usage().await.unwrap();
    capacity(repo.import_objects(&objects[..2]).await.unwrap_err());
    assert_eq!(repo.storage_usage().await.unwrap(), empty);
    assert!(repo.inventory(None, 256).await.unwrap().is_empty());

    // An exact-capacity write and every later retry charge each stored row once.
    let total = objects.iter().map(|o| o.bytes.len() as i64).sum();
    repo.set_storage_limits(StorageLimits {
        bytes: total,
        objects: objects.len() as i64,
        ..Default::default()
    })
    .await
    .unwrap();
    assert_eq!(repo.import_objects(&objects).await.unwrap(), objects.len());
    let full = repo.storage_usage().await.unwrap();
    assert_eq!(full.bytes_used, total);
    assert_eq!(repo.import_objects(&objects).await.unwrap(), 0);
    assert_eq!(
        repo.write_state(document, &original)
            .await
            .unwrap()
            .1
            .new_objects,
        0
    );
    assert_eq!(repo.read_state(document, &root).await.unwrap(), original);
    assert_eq!(repo.storage_usage().await.unwrap(), full);
    assert!(matches!(
        repo.set_storage_limits(StorageLimits {
            bytes: total - 1,
            ..full.limits
        })
        .await,
        Err(Error::Limit(_))
    ));
    assert!(matches!(
        repo.set_storage_limits(StorageLimits {
            objects: -1,
            ..full.limits
        })
        .await,
        Err(Error::Invalid(_))
    ));
    repo.close().await;
    let reopened = LocalRepository::open(&file, project).await.unwrap();
    assert_eq!(reopened.storage_usage().await.unwrap(), full);
    capacity(
        reopened
            .write_state(document, &state("More data"))
            .await
            .unwrap_err(),
    );
    assert_eq!(reopened.storage_usage().await.unwrap(), full);
    cleanup(reopened, &file).await;
    cleanup(source, &source_file).await;
}

#[tokio::test]
async fn commit_and_branch_budgets_preserve_retries_and_existing_ref_updates() {
    let file = path();
    let project = uuid::Uuid::new_v4();
    let repo = LocalRepository::open(&file, project).await.unwrap();
    let first = signed(project, vec![], BTreeMap::new(), "First");
    let first_size = serde_json::to_vec(&first).unwrap().len() as i64;
    repo.set_storage_limits(StorageLimits {
        bytes: first_size - 1,
        ..Default::default()
    })
    .await
    .unwrap();
    capacity(repo.import_commit(&first).await.unwrap_err());
    assert_eq!(repo.storage_usage().await.unwrap().commits_used, 0);
    repo.set_storage_limits(StorageLimits {
        bytes: first_size,
        ..Default::default()
    })
    .await
    .unwrap();
    let id = repo.import_commit(&first).await.unwrap();
    let usage = repo.storage_usage().await.unwrap();
    assert_eq!(usage.bytes_used, first_size);
    assert_eq!(repo.import_commit(&first).await.unwrap(), id);
    assert_eq!(repo.storage_usage().await.unwrap(), usage);
    let second = signed(project, vec![id.clone()], BTreeMap::new(), "Second");
    capacity(repo.import_commit(&second).await.unwrap_err());
    // Separate count limit, while bytes are available.
    repo.set_storage_limits(StorageLimits {
        commits: 1,
        branches: 1,
        ..Default::default()
    })
    .await
    .unwrap();
    capacity(repo.import_commit(&second).await.unwrap_err());
    assert_eq!(repo.import_commit(&first).await.unwrap(), id);
    let mut head = repo.compare_and_swap("main", None, &id).await.unwrap();
    capacity(repo.compare_and_swap("topic", None, &id).await.unwrap_err());
    assert!(matches!(
        repo.compare_and_swap("main", None, &id).await,
        Err(Error::StaleRef)
    ));
    head = repo
        .compare_and_swap("main", Some(&head), &id)
        .await
        .unwrap();
    assert_eq!(head.generation, 2);
    let usage = repo.storage_usage().await.unwrap();
    assert_eq!(usage.commits_used, 1);
    assert_eq!(usage.branches_used, 1);
    // Explicit capacity increase resumes progress without discarding history.
    repo.set_storage_limits(StorageLimits {
        commits: 2,
        branches: 2,
        ..usage.limits
    })
    .await
    .unwrap();
    let next = repo.import_commit(&second).await.unwrap();
    repo.compare_and_swap("topic", None, &next).await.unwrap();
    repo.compare_and_swap("main", Some(&head), &next)
        .await
        .unwrap();
    assert_eq!(repo.commit(&id).await.unwrap().commit, first.commit);
    assert_eq!(repo.storage_usage().await.unwrap().branches_used, 2);
    cleanup(repo, &file).await;
}

#[tokio::test]
async fn independent_connections_cannot_spend_the_same_remaining_capacity() {
    let file = path();
    let source_file = path();
    let project = uuid::Uuid::new_v4();
    let source = LocalRepository::open(&source_file, project).await.unwrap();
    source
        .write_state(
            DocumentId(uuid::Uuid::new_v4()),
            &state("Concurrent transfer"),
        )
        .await
        .unwrap();
    let objects = packets(&source).await;
    let left = LocalRepository::open(&file, project).await.unwrap();
    let right = LocalRepository::open(&file, project).await.unwrap();
    left.set_storage_limits(StorageLimits {
        objects: 1,
        ..Default::default()
    })
    .await
    .unwrap();
    let (a, b) = tokio::join!(
        left.import_objects(&objects[..1]),
        right.import_objects(&objects[1..2])
    );
    let winner = match (a, b) {
        (Ok(1), Err(error)) => {
            capacity(error);
            &objects[0]
        }
        (Err(error), Ok(1)) => {
            capacity(error);
            &objects[1]
        }
        other => panic!("expected one admitted writer, got {other:?}"),
    };
    let usage = right.storage_usage().await.unwrap();
    assert_eq!(usage.objects_used, 1);
    assert_eq!(usage.bytes_used, winner.bytes.len() as i64);
    assert_eq!(left.storage_usage().await.unwrap(), usage);
    right.close().await;
    cleanup(left, &file).await;
    cleanup(source, &source_file).await;
}

#[tokio::test]
async fn newer_schema_and_missing_accounting_fail_closed() {
    let file = path();
    let project = uuid::Uuid::new_v4();
    let repo = LocalRepository::open(&file, project).await.unwrap();
    sqlx::query("update local_meta set version=3")
        .execute(&repo.pool)
        .await
        .unwrap();
    assert!(matches!(
        LocalRepository::open(&file, project).await,
        Err(Error::Invalid("local schema/project"))
    ));
    sqlx::query("update local_meta set version=2")
        .execute(&repo.pool)
        .await
        .unwrap();
    sqlx::query("delete from local_storage")
        .execute(&repo.pool)
        .await
        .unwrap();
    assert!(LocalRepository::open(&file, project).await.is_err());
    capacity(
        repo.write_state(
            DocumentId(uuid::Uuid::new_v4()),
            &state("No accounting row"),
        )
        .await
        .unwrap_err(),
    );
    assert!(repo.inventory(None, 256).await.unwrap().is_empty());
    cleanup(repo, &file).await;
}
