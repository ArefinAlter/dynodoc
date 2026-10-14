use super::collector::{Deleted, Stage};
use super::tests::{cleanup, path, state};
use super::*;
use crate::{
    materializer::MaterializedNode,
    repository::{
        convergence::state_address, recovery::Kind, tests::signed, FileEntry, SignedCommit,
    },
};
use engine_shared::NodeId;
use ring::signature::Ed25519KeyPair;
use serde_json::json;
use sqlx::Connection;
use std::time::Instant;

const STALE_TEXT: &str = "retention plan hash does not match the repository now (a branch moved or objects changed); run gc-plan again";

fn entry(document: DocumentId, root: &Address) -> FileEntry {
    FileEntry {
        document,
        state: root.clone(),
    }
}

fn doc() -> DocumentId {
    DocumentId(uuid::Uuid::new_v4())
}

fn paragraphs(count: usize, tag: &str) -> DocumentState {
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
                current_fields: json!({"label": format!("paragraph {i} {tag}")}),
            },
        );
    }
    state
}

async fn import(
    repo: &LocalRepository,
    parents: Vec<Address>,
    files: &[(&str, FileEntry)],
    message: &str,
) -> Address {
    let commit = signed(
        repo.project(),
        parents,
        files
            .iter()
            .map(|(path, file)| ((*path).to_string(), file.clone()))
            .collect(),
        message,
    );
    repo.import_commit(&commit).await.unwrap()
}

async fn publish(repo: &LocalRepository, branch: &str, commit: &Address) {
    let head = repo.head(branch).await.unwrap();
    repo.compare_and_swap(branch, head.as_ref(), commit)
        .await
        .unwrap();
}

fn all() -> GcPlanOptions {
    GcPlanOptions {
        top_reasons: 100,
        list: 1000,
    }
}

/// Everything a collection must leave alone (or change only in the objects table
/// and counters): rows, schema objects and counters of the whole database.
#[derive(Debug, PartialEq, Eq)]
struct Snapshot {
    objects: Vec<(String, String, Vec<u8>)>,
    commits: Vec<(String, Vec<u8>, i64)>,
    refs: Vec<(String, String, i64)>,
    schema: Vec<(String, String)>,
    usage: StorageUsage,
}

async fn snapshot(repo: &LocalRepository) -> Snapshot {
    let objects =
        sqlx::query_as("select document,address,bytes from objects order by document,address")
            .fetch_all(&repo.pool)
            .await
            .unwrap();
    let commits = sqlx::query_as("select address,body,height from commits order by address")
        .fetch_all(&repo.pool)
        .await
        .unwrap();
    let refs = sqlx::query_as("select name,head,generation from refs order by name")
        .fetch_all(&repo.pool)
        .await
        .unwrap();
    let schema = sqlx::query_as(
        "select name,coalesce(sql,'') from sqlite_master where name not like 'sqlite_%' order by name",
    )
    .fetch_all(&repo.pool)
    .await
    .unwrap();
    Snapshot {
        objects,
        commits,
        refs,
        schema,
        usage: repo.storage_usage().await.unwrap(),
    }
}

fn object_keys(snapshot: &Snapshot) -> std::collections::BTreeSet<(String, String)> {
    snapshot
        .objects
        .iter()
        .map(|(d, a, _)| (d.clone(), a.clone()))
        .collect()
}

async fn usage_is_recomputed(repo: &LocalRepository) {
    let usage = repo.storage_usage().await.unwrap();
    let (objects, object_bytes, stored_object_bytes): (i64, i64, i64) = sqlx::query_as(
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
    let branches: i64 = sqlx::query_scalar("select count(*) from refs")
        .fetch_one(&repo.pool)
        .await
        .unwrap();
    assert_eq!(usage.objects_used, objects);
    assert_eq!(usage.bytes_used, object_bytes + commit_bytes);
    assert_eq!(usage.stored_bytes, stored_object_bytes + commit_bytes);
    assert_eq!(usage.commits_used, commits);
    assert_eq!(usage.branches_used, branches);
}

fn stale(error: &Error) -> bool {
    matches!(error, Error::Invalid(text) if *text == STALE_TEXT)
}

struct Fixture {
    file: std::path::PathBuf,
    repo: LocalRepository,
    report: DocumentId,
    main: Address,
    side: Address,
    orphan: Address,
    source: Address,
    recovered: Address,
}

/// A repository with a published line, a recovery commit that pins a source commit
/// no branch reaches, an unpublished side line, an orphan commit and staged objects.
async fn fixture() -> Fixture {
    let file = path();
    let project = uuid::Uuid::new_v4();
    let repo = LocalRepository::open(&file, project).await.unwrap();
    let (report, other) = (doc(), doc());
    let (r0, _) = repo.write_state(report, &state("base")).await.unwrap();
    let (r1, _) = repo
        .write_state(report, &state("source line"))
        .await
        .unwrap();
    let (r2, _) = repo
        .write_state(report, &state("current line"))
        .await
        .unwrap();
    let (resolved, _) = repo
        .write_state(report, &state("hand resolved"))
        .await
        .unwrap();
    let base = import(
        &repo,
        vec![],
        &[("Report.docx", entry(report, &r0))],
        "base",
    )
    .await;
    let current = import(
        &repo,
        vec![base.clone()],
        &[("Report.docx", entry(report, &r2))],
        "current",
    )
    .await;
    let source = import(
        &repo,
        vec![base.clone()],
        &[("Report.docx", entry(report, &r1))],
        "source",
    )
    .await;
    let plan = repo
        .plan_recovery(Kind::CherryPick, &source, &current, None)
        .await
        .unwrap();
    let commit = plan
        .resolve(
            &BTreeMap::from([("Report.docx".to_string(), Some(entry(report, &resolved)))]),
            5,
            "Recover reviewed change".into(),
        )
        .unwrap();
    let key = Ed25519KeyPair::from_seed_unchecked(&[7; 32]).unwrap();
    let recovered = repo
        .import_commit(&SignedCommit::sign(commit, &key).unwrap())
        .await
        .unwrap();
    publish(&repo, "main", &recovered).await;
    // Unpublished side line (superseded) and a disconnected orphan.
    let (side_root, _) = repo.write_state(report, &state("side line")).await.unwrap();
    let side = import(
        &repo,
        vec![base.clone()],
        &[("Report.docx", entry(report, &side_root))],
        "side",
    )
    .await;
    let (o1, _) = repo
        .write_state(other, &state("disconnected"))
        .await
        .unwrap();
    let orphan = import(
        &repo,
        vec![],
        &[("Other.docx", entry(other, &o1))],
        "orphan",
    )
    .await;
    // Staged only: never named by a commit, one under a document that also has
    // reachable states.
    repo.write_state(doc(), &state("never committed"))
        .await
        .unwrap();
    repo.write_state(report, &state("edited, never committed"))
        .await
        .unwrap();
    Fixture {
        file,
        repo,
        report,
        main: recovered.clone(),
        side,
        orphan,
        source,
        recovered,
    }
}

fn staged(plan: &GcPlan) -> &super::retention::Counts {
    &plan
        .unreachable
        .categories
        .iter()
        .find(|c| c.category == "staged")
        .unwrap()
        .counts
}

#[tokio::test]
async fn staged_objects_are_deleted_and_everything_a_commit_reaches_is_kept() {
    let f = fixture().await;
    let plan = f.repo.gc_plan(all()).await.unwrap();
    let staged_counts = staged(&plan).clone();
    assert!(staged_counts.objects > 0);
    assert!(plan.unreachable.counts.commits >= 2);
    let before = snapshot(&f.repo).await;
    let report = f
        .repo
        .gc_apply(&plan.plan_hash, GcApplyOptions::default())
        .await
        .unwrap();
    assert!(report.applied && !report.already_applied);
    assert_eq!(report.deleted.objects, staged_counts.objects);
    assert_eq!(report.deleted.object_bytes, staged_counts.object_bytes);
    assert_eq!(report.format, "dynodoc.local-retention-gc.v1");
    assert_eq!(report.capacity_before, before.usage);
    let after = snapshot(&f.repo).await;
    assert_eq!(report.capacity_after, after.usage);
    // Exactly the staged examples (listed in full) disappeared; nothing else did.
    let removed: std::collections::BTreeSet<_> = object_keys(&before)
        .difference(&object_keys(&after))
        .cloned()
        .collect();
    let listed: std::collections::BTreeSet<_> = plan
        .examples
        .staged_objects
        .iter()
        .map(|o| (o.document.to_string(), o.address.as_str().to_string()))
        .collect();
    assert_eq!(removed, listed);
    assert_eq!(removed.len() as u64, staged_counts.objects);
    // Commits, signatures and refs are byte-for-byte identical.
    assert_eq!(after.commits, before.commits);
    assert_eq!(after.refs, before.refs);
    for address in [&f.main, &f.side, &f.orphan, &f.source, &f.recovered] {
        f.repo.commit(address).await.unwrap();
        let signed = f.repo.commit(address).await.unwrap();
        assert_eq!(&signed.verify().unwrap(), address);
        for file in signed.commit.files.values() {
            // Reachable, superseded, orphaned and recovery-pinned states all read.
            f.repo.read_state(file.document, &file.state).await.unwrap();
        }
    }
    let plan_after = f.repo.gc_plan(all()).await.unwrap();
    assert_eq!(staged(&plan_after).objects, 0);
    assert_eq!(plan_after.reachable, plan.reachable);
    assert_eq!(
        plan_after.unreachable.categories[1..],
        plan.unreachable.categories[1..]
    );
    assert_eq!(plan_after.accounting.consistent, Some(true));
    assert_eq!(report.retained, plan_after.retained);
    assert_eq!(report.unreachable_retained, plan_after.unreachable.counts);
    usage_is_recomputed(&f.repo).await;
    cleanup(f.repo, &f.file).await;
}

#[tokio::test]
async fn the_immutability_guard_is_restored_and_unreachable_history_is_never_deleted() {
    let f = fixture().await;
    let guard: String = sqlx::query_scalar(
        "select sql from sqlite_master where type='trigger' and name='objects_no_delete'",
    )
    .fetch_one(&f.repo.pool)
    .await
    .unwrap();
    let hash = f.repo.gc_plan(all()).await.unwrap().plan_hash;
    f.repo
        .gc_apply(&hash, GcApplyOptions::default())
        .await
        .unwrap();
    let restored: String = sqlx::query_scalar(
        "select sql from sqlite_master where type='trigger' and name='objects_no_delete'",
    )
    .fetch_one(&f.repo.pool)
    .await
    .unwrap();
    assert_eq!(restored, guard);
    assert!(sqlx::query("delete from objects")
        .execute(&f.repo.pool)
        .await
        .is_err());
    assert!(sqlx::query("delete from commits")
        .execute(&f.repo.pool)
        .await
        .is_err());
    assert!(sqlx::query("update objects set bytes=bytes")
        .execute(&f.repo.pool)
        .await
        .is_err());
    // The append-only log records the run and refuses edits.
    let logged: (String, i64) =
        sqlx::query_as("select plan_hash,deleted_objects from local_retention_log")
            .fetch_one(&f.repo.pool)
            .await
            .unwrap();
    assert_eq!(logged.0, hash);
    assert!(logged.1 > 0);
    assert!(sqlx::query("delete from local_retention_log")
        .execute(&f.repo.pool)
        .await
        .is_err());
    // Unreachable commits keep every object they reach: still planned, not staged.
    let plan = f.repo.gc_plan(all()).await.unwrap();
    assert!(plan.unreachable.counts.commits >= 2);
    assert!(plan.unreachable.counts.objects > 0);
    assert_eq!(staged(&plan).objects, 0);
    cleanup(f.repo, &f.file).await;
}

#[tokio::test]
async fn a_stale_plan_refuses_with_no_change() {
    let f = fixture().await;
    let repo = &f.repo;
    let hash = repo.gc_plan(all()).await.unwrap().plan_hash;
    let before = snapshot(repo).await;

    // Malformed hashes never reach the database.
    for bad in ["", "abc", &"g".repeat(64), &"a".repeat(65)] {
        assert!(repo.gc_apply(bad, GcApplyOptions::default()).await.is_err());
    }
    // A hash the repository never produced.
    let error = repo
        .gc_apply(&"0".repeat(64), GcApplyOptions::default())
        .await
        .unwrap_err();
    assert!(stale(&error), "{error:?}");
    assert_eq!(snapshot(repo).await, before);

    // A branch moved (ref CAS): same objects, different heads and generation.
    let head = repo.head("main").await.unwrap();
    let moved = repo
        .compare_and_swap("main", head.as_ref(), &f.source)
        .await
        .unwrap();
    let error = repo
        .gc_apply(&hash, GcApplyOptions::default())
        .await
        .unwrap_err();
    assert!(stale(&error), "{error:?}");
    let moved_snapshot = snapshot(repo).await;
    assert_eq!(moved_snapshot.objects, before.objects);
    assert_eq!(moved_snapshot.usage, before.usage);
    // Moving it back changes the generation again, so the old plan stays stale.
    repo.compare_and_swap("main", Some(&moved), &f.main)
        .await
        .unwrap();
    assert!(stale(
        &repo
            .gc_apply(&hash, GcApplyOptions::default())
            .await
            .unwrap_err()
    ));

    // A new plan, then another writer stages an object before collection.
    let hash = repo.gc_plan(all()).await.unwrap().plan_hash;
    let before = snapshot(repo).await;
    repo.write_state(doc(), &state("arrived after the plan"))
        .await
        .unwrap();
    let arrived = snapshot(repo).await;
    assert!(arrived.objects.len() > before.objects.len());
    let error = repo
        .gc_apply(&hash, GcApplyOptions::default())
        .await
        .unwrap_err();
    assert!(stale(&error), "{error:?}");
    assert_eq!(snapshot(repo).await, arrived);

    // A new plan, then an import adds a commit and its objects.
    let hash = repo.gc_plan(all()).await.unwrap().plan_hash;
    let before = snapshot(repo).await;
    let (root, _) = repo
        .write_state(f.report, &state("imported late"))
        .await
        .unwrap();
    let hash_after_objects = repo.gc_plan(all()).await.unwrap().plan_hash;
    assert_ne!(hash, hash_after_objects);
    import(
        repo,
        vec![f.main.clone()],
        &[("Report.docx", entry(f.report, &root))],
        "late commit",
    )
    .await;
    assert!(stale(
        &repo
            .gc_apply(&hash_after_objects, GcApplyOptions::default())
            .await
            .unwrap_err()
    ));
    let after = snapshot(repo).await;
    assert!(after.objects.len() > before.objects.len());
    assert!(!after.schema.iter().any(|(n, _)| n == "local_retention_log"));
    cleanup(f.repo, &f.file).await;
}

#[tokio::test]
async fn repeating_an_applied_hash_changes_nothing() {
    let f = fixture().await;
    let repo = &f.repo;
    let hash = repo.gc_plan(all()).await.unwrap().plan_hash;
    let first = repo
        .gc_apply(&hash, GcApplyOptions::default())
        .await
        .unwrap();
    assert!(first.applied);
    let settled = snapshot(repo).await;
    let second = repo
        .gc_apply(&hash, GcApplyOptions::default())
        .await
        .unwrap();
    assert!(!second.applied && second.already_applied);
    assert_eq!(second.deleted, Deleted::default());
    assert_eq!(second.capacity_before, second.capacity_after);
    assert_eq!(snapshot(repo).await, settled);
    // Uppercase spelling is the same hash.
    let third = repo
        .gc_apply(&hash.to_ascii_uppercase(), GcApplyOptions::default())
        .await
        .unwrap();
    assert!(third.already_applied);
    // A fresh plan of the clean repository has nothing to delete and is a no-op.
    let clean = repo.gc_plan(all()).await.unwrap();
    assert_ne!(clean.plan_hash, hash);
    let noop = repo
        .gc_apply(&clean.plan_hash, GcApplyOptions::default())
        .await
        .unwrap();
    assert!(!noop.applied && !noop.already_applied);
    assert_eq!(snapshot(repo).await, settled);
    // The log holds the single deleting run.
    let runs: i64 = sqlx::query_scalar("select count(*) from local_retention_log")
        .fetch_one(&repo.pool)
        .await
        .unwrap();
    assert_eq!(runs, 1);
    cleanup(f.repo, &f.file).await;
}

#[tokio::test]
async fn every_branch_head_reads_verifies_and_exports_identically() {
    let f = fixture().await;
    let repo = &f.repo;
    // A second branch on the superseded line and one on the orphan.
    publish(repo, "review/side", &f.side).await;
    publish(repo, "review/orphan", &f.orphan).await;
    let mut before = Vec::new();
    for branch in ["main", "review/side", "review/orphan"] {
        let head = repo.head(branch).await.unwrap().unwrap();
        let commit = repo.commit(&head.commit).await.unwrap();
        for (path, file) in &commit.commit.files {
            let read = repo.read_state(file.document, &file.state).await.unwrap();
            // Re-encoding the state reproduces its full typed address.
            assert_eq!(
                state_address(file.document, &read).await.unwrap(),
                file.state
            );
            before.push((branch, path.clone(), file.clone(), read));
        }
    }
    let mut exported = Vec::new();
    let mut cursor = None;
    loop {
        let page = repo.inventory(cursor.as_ref(), 256).await.unwrap();
        let Some(last) = page.last().cloned() else {
            break;
        };
        for key in page {
            exported.push((key.clone(), repo.object(&key).await.unwrap().bytes));
        }
        cursor = Some(last);
    }
    let hash = repo.gc_plan(all()).await.unwrap().plan_hash;
    let report = repo
        .gc_apply(&hash, GcApplyOptions::default())
        .await
        .unwrap();
    assert!(report.applied);
    for (branch, path, file, read) in &before {
        let head = repo.head(branch).await.unwrap().unwrap();
        let commit = repo.commit(&head.commit).await.unwrap();
        assert_eq!(&commit.commit.files[path], file);
        let again = repo.read_state(file.document, &file.state).await.unwrap();
        assert_eq!(&again, read);
        assert_eq!(
            state_address(file.document, &again).await.unwrap(),
            file.state
        );
    }
    // Every surviving object re-exports byte-identically and verifies; the
    // survivors are exactly the objects that were not staged.
    let mut survivors = 0u64;
    for (key, bytes) in &exported {
        match repo.object(key).await {
            Ok(packet) => {
                assert_eq!(&packet.bytes, bytes);
                survivors += 1;
            }
            Err(Error::Object(shared_checkpoint::Error::Missing(_))) => {}
            Err(other) => panic!("{other:?}"),
        }
    }
    assert_eq!(survivors, report.retained.objects);
    assert_eq!(exported.len() as u64 - survivors, report.deleted.objects);
    cleanup(f.repo, &f.file).await;
}

#[tokio::test]
async fn capacity_counters_equal_a_fresh_recomputation_and_limits_are_untouched() {
    let f = fixture().await;
    let repo = &f.repo;
    let limits = StorageLimits {
        bytes: 64 * 1024 * 1024,
        objects: 50_000,
        commits: 500,
        branches: 64,
    };
    repo.set_storage_limits(limits).await.unwrap();
    let before = repo.storage_usage().await.unwrap();
    usage_is_recomputed(repo).await;
    let hash = repo.gc_plan(all()).await.unwrap().plan_hash;
    let report = repo
        .gc_apply(&hash, GcApplyOptions::default())
        .await
        .unwrap();
    usage_is_recomputed(repo).await;
    let after = repo.storage_usage().await.unwrap();
    assert_eq!(after.limits, limits);
    assert_eq!(after.commits_used, before.commits_used);
    assert_eq!(after.branches_used, before.branches_used);
    assert_eq!(
        after.objects_used,
        before.objects_used - report.deleted.objects as i64
    );
    assert_eq!(
        after.bytes_used,
        before.bytes_used - report.deleted.object_bytes as i64
    );
    assert_eq!(report.capacity_after, after);
    assert!(repo.gc_plan(all()).await.unwrap().accounting.consistent == Some(true));
    // Capacity freed by the collection is usable again: a write that stages the
    // same bytes succeeds and is charged exactly once.
    repo.write_state(doc(), &state("never committed"))
        .await
        .unwrap();
    usage_is_recomputed(repo).await;
    cleanup(f.repo, &f.file).await;
}

#[tokio::test]
async fn read_only_and_unsupported_repositories_cannot_collect() {
    let f = fixture().await;
    let hash = f.repo.gc_plan(all()).await.unwrap().plan_hash;
    let before = snapshot(&f.repo).await;
    let reader = LocalRepository::open_read_only(&f.file, f.repo.project())
        .await
        .unwrap();
    assert!(reader
        .gc_apply(&hash, GcApplyOptions::default())
        .await
        .is_err());
    reader.close().await;
    assert_eq!(snapshot(&f.repo).await, before);
    cleanup(f.repo, &f.file).await;
}

#[tokio::test]
async fn a_writer_in_another_connection_makes_gc_wait_or_fail_cleanly() {
    let f = fixture().await;
    let repo = f.repo.clone();
    let hash = repo.gc_plan(all()).await.unwrap().plan_hash;
    let before = snapshot(&repo).await;

    // 1. The other writer finishes without changing anything: gc waits, then runs.
    let mut other = SqliteConnection::connect_with(&repo.options).await.unwrap();
    sqlx::query("begin immediate")
        .execute(&mut other)
        .await
        .unwrap();
    let released = std::cell::Cell::new(false);
    let ((report, waited), ()) = tokio::join!(
        async {
            let result = repo.gc_apply(&hash, GcApplyOptions::default()).await;
            (result, released.get())
        },
        async {
            tokio::time::sleep(std::time::Duration::from_millis(300)).await;
            assert_eq!(snapshot(&repo).await.objects, before.objects);
            released.set(true);
            sqlx::query("commit").execute(&mut other).await.unwrap();
        }
    );
    assert!(waited, "gc must wait for the writer lock, not skip it");
    assert!(report.unwrap().applied);
    usage_is_recomputed(&repo).await;

    // 2. The other writer stages an object while gc waits: the plan is stale when
    // gc finally gets the lock, so nothing is deleted and nothing is corrupted.
    repo.write_state(doc(), &state("staged for deletion"))
        .await
        .unwrap();
    let hash = repo.gc_plan(all()).await.unwrap().plan_hash;
    let settled = snapshot(&repo).await;
    sqlx::query("begin immediate")
        .execute(&mut other)
        .await
        .unwrap();
    let late = doc();
    let (error, root) = tokio::join!(repo.gc_apply(&hash, GcApplyOptions::default()), async {
        tokio::time::sleep(std::time::Duration::from_millis(200)).await;
        let (root, _) = typed::write_state(
            &mut Store {
                connection: &mut other,
                read_budget: None,
                packed: true,
            },
            late,
            &state("written by the other connection"),
            Limits::default(),
            Default::default(),
        )
        .await
        .unwrap();
        sqlx::query("commit").execute(&mut other).await.unwrap();
        root
    });
    let error = error.unwrap_err();
    assert!(stale(&error), "{error:?}");
    let after = snapshot(&repo).await;
    assert!(object_keys(&after).is_superset(&object_keys(&settled)));
    assert!(after.objects.len() > settled.objects.len());
    repo.read_state(late, &root).await.unwrap();
    usage_is_recomputed(&repo).await;

    // 3. A writer that outlasts the busy timeout: gc fails cleanly, changes
    // nothing and succeeds once the lock is released.
    let patient = LocalRepository {
        pool: SqlitePoolOptions::new()
            .max_connections(2)
            .connect_with(
                repo.options
                    .clone()
                    .busy_timeout(std::time::Duration::from_millis(150)),
            )
            .await
            .unwrap(),
        project: repo.project,
        schema: repo.schema,
        options: repo.options.clone(),
    };
    let hash = repo.gc_plan(all()).await.unwrap().plan_hash;
    let settled = snapshot(&repo).await;
    sqlx::query("begin immediate")
        .execute(&mut other)
        .await
        .unwrap();
    let error = patient
        .gc_apply(&hash, GcApplyOptions::default())
        .await
        .unwrap_err();
    assert!(matches!(error, Error::Db(_)), "{error:?}");
    sqlx::query("rollback").execute(&mut other).await.unwrap();
    assert_eq!(snapshot(&repo).await, settled);
    let report = patient
        .gc_apply(&hash, GcApplyOptions::default())
        .await
        .unwrap();
    assert!(report.applied);
    usage_is_recomputed(&repo).await;
    other.close().await.unwrap();
    patient.close().await;
    cleanup(repo, &f.file).await;
}

#[tokio::test]
async fn a_failure_or_crash_at_any_point_deletes_nothing() {
    let f = fixture().await;
    let repo = &f.repo;
    let hash = repo.gc_plan(all()).await.unwrap().plan_hash;
    let before = snapshot(repo).await;
    for stage in [
        Stage::Locked,
        Stage::Verified,
        Stage::Deleted,
        Stage::Accounted,
    ] {
        let error = repo
            .gc_apply_with(&hash, GcApplyOptions::default(), &mut |at| {
                if at == stage {
                    Err(Error::Invalid("injected failure"))
                } else {
                    Ok(())
                }
            })
            .await
            .unwrap_err();
        assert!(matches!(error, Error::Invalid("injected failure")));
        // Rows, guard trigger, counters and the absent log are all untouched.
        assert_eq!(snapshot(repo).await, before, "after failure at {stage:?}");
    }
    // A hard crash: another connection takes the writer lock, removes the guard
    // and deletes rows, then disappears before commit.
    let mut crashing = SqliteConnection::connect_with(&repo.options).await.unwrap();
    sqlx::query("begin immediate")
        .execute(&mut crashing)
        .await
        .unwrap();
    sqlx::query("drop trigger objects_no_delete")
        .execute(&mut crashing)
        .await
        .unwrap();
    let removed = sqlx::query("delete from objects")
        .execute(&mut crashing)
        .await
        .unwrap()
        .rows_affected();
    assert!(removed > 0);
    drop(crashing);
    // Readers never saw the uncommitted work; the collector below also has to wait
    // for the abandoned connection's lock to disappear before it can start.
    assert_eq!(snapshot(repo).await, before);
    // After all of that, the same plan still applies.
    let report = repo
        .gc_apply(&hash, GcApplyOptions::default())
        .await
        .unwrap();
    assert!(report.applied);
    usage_is_recomputed(repo).await;
    cleanup(f.repo, &f.file).await;
}

#[tokio::test]
async fn a_large_synthetic_repository_collects_exactly_the_staged_objects() {
    let file = path();
    let project = uuid::Uuid::new_v4();
    let repo = LocalRepository::open(&file, project).await.unwrap();
    repo.set_storage_limits(StorageLimits {
        bytes: 1 << 30,
        objects: 200_000,
        commits: 10_000,
        branches: 64,
    })
    .await
    .unwrap();
    let documents: Vec<DocumentId> = (0..4).map(|_| doc()).collect();
    // Reachable history: 30 commits over 4 documents.
    let mut roots: Vec<Address> = Vec::new();
    for (i, document) in documents.iter().enumerate() {
        roots.push(
            repo.write_state(*document, &paragraphs(40, &format!("base{i}")))
                .await
                .unwrap()
                .0,
        );
    }
    let files = |roots: &[Address]| -> Vec<(&str, FileEntry)> {
        ["a.docx", "b.xlsx", "c.pptx", "d.docx"]
            .into_iter()
            .zip(&documents)
            .zip(roots)
            .map(|((name, document), root)| (name, entry(*document, root)))
            .collect()
    };
    let mut head = import(&repo, vec![], &files(&roots), "initial").await;
    for round in 0..30usize {
        let which = round % 4;
        roots[which] = repo
            .write_state(documents[which], &paragraphs(40, &format!("r{round}")))
            .await
            .unwrap()
            .0;
        head = import(&repo, vec![head], &files(&roots), &format!("r{round}")).await;
    }
    publish(&repo, "main", &head).await;
    // A superseded line, so unreachable commits and their objects exist too.
    let dead = repo
        .write_state(documents[0], &paragraphs(40, "dead end"))
        .await
        .unwrap()
        .0;
    let mut dead_files = roots.clone();
    dead_files[0] = dead;
    import(&repo, vec![head.clone()], &files(&dead_files), "dead end").await;

    // Bulk staging in one transaction so the fixture itself is fast.
    let started = Instant::now();
    let mut staged_objects = 0usize;
    let mut tx = repo.pool.begin().await.unwrap();
    let mut i = 0usize;
    while staged_objects < 20_000 {
        let (_, stats) = typed::write_state(
            &mut Store {
                connection: &mut tx,
                read_budget: None,
                packed: true,
            },
            documents[i % 4],
            &paragraphs(40, &format!("staged{i}")),
            Limits::default(),
            Default::default(),
        )
        .await
        .unwrap();
        staged_objects += stats.new_objects;
        i += 1;
    }
    tx.commit().await.unwrap();
    println!(
        "synthetic fixture: {staged_objects} staged objects from {i} states in {:?}",
        started.elapsed()
    );
    let started = Instant::now();
    let plan = repo
        .gc_plan(GcPlanOptions {
            top_reasons: 5,
            list: 3,
        })
        .await
        .unwrap();
    let plan_time = started.elapsed();
    assert_eq!(staged(&plan).objects as usize, staged_objects);
    assert!(plan.retained.objects >= 20_000);
    assert_eq!(plan.unreachable.counts.commits, 1);
    let usage_before = repo.storage_usage().await.unwrap();
    let started = Instant::now();
    let report = repo
        .gc_apply(&plan.plan_hash, GcApplyOptions { vacuum: true })
        .await
        .unwrap();
    println!(
        "synthetic collection: plan {plan_time:?}, apply+vacuum {:?}, deleted {} objects / {} bytes",
        started.elapsed(),
        report.deleted.objects,
        report.deleted.object_bytes
    );
    assert!(report.applied);
    assert_eq!(report.deleted.objects as usize, staged_objects);
    assert_eq!(
        report.capacity_after.objects_used,
        usage_before.objects_used - staged_objects as i64
    );
    let vacuum = report.vacuum.as_ref().unwrap();
    assert!(vacuum.error.is_none(), "{:?}", vacuum.error);
    assert!(
        vacuum.file_bytes_after < vacuum.file_bytes_before,
        "{vacuum:?}"
    );
    usage_is_recomputed(&repo).await;
    let after = repo.gc_plan(all()).await.unwrap();
    assert_eq!(staged(&after).objects, 0);
    assert_eq!(after.reachable, plan.reachable);
    assert_eq!(after.accounting.consistent, Some(true));
    // The head still reconstructs every file.
    let head_commit = repo.commit(&head).await.unwrap();
    for file in head_commit.commit.files.values() {
        repo.read_state(file.document, &file.state).await.unwrap();
    }
    cleanup(repo, &file).await;
}

#[tokio::test]
async fn a_plan_taken_on_a_schema_one_database_survives_the_upgrade_on_open() {
    let file = path();
    let project = uuid::Uuid::new_v4();
    let (kept, staged_doc) = (doc(), doc());
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
    let mut write = async |document: DocumentId, text: &str| {
        typed::write_state(
            &mut Store {
                connection: &mut tx,
                read_budget: None,
                packed: false,
            },
            document,
            &state(text),
            Limits::default(),
            Default::default(),
        )
        .await
        .unwrap()
    };
    let (root, _) = write(kept, "kept").await;
    let (_, staged_stats) = write(staged_doc, "staged only").await;
    let commit = signed(
        project,
        vec![],
        BTreeMap::from([("a".into(), entry(kept, &root))]),
        "legacy",
    );
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

    let reader = LocalRepository::open_read_only(&file, project)
        .await
        .unwrap();
    let plan = reader.gc_plan(all()).await.unwrap();
    assert_eq!(plan.local_schema, 1);
    assert_eq!(staged(&plan).objects, staged_stats.new_objects as u64);
    // A read-only handle can never collect, and does not upgrade the file.
    assert!(reader
        .gc_apply(&plan.plan_hash, GcApplyOptions::default())
        .await
        .is_err());
    reader.close().await;
    // A writable open upgrades (1 -> 2 -> 3); the same reviewed plan still applies.
    let repo = LocalRepository::open(&file, project).await.unwrap();
    let fresh = repo.gc_plan(all()).await.unwrap();
    assert_eq!(fresh.local_schema, 3);
    assert_eq!(fresh.plan_hash, plan.plan_hash);
    let report = repo
        .gc_apply(&plan.plan_hash, GcApplyOptions::default())
        .await
        .unwrap();
    assert!(report.applied);
    assert_eq!(report.deleted.objects, staged_stats.new_objects as u64);
    usage_is_recomputed(&repo).await;
    repo.read_state(kept, &root).await.unwrap();
    cleanup(repo, &file).await;
}
