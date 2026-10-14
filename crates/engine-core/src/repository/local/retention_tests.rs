use super::retention::Counts;
use super::tests::{cleanup, path, state};
use super::*;
use crate::{
    materializer::MaterializedNode,
    repository::{recovery::Kind, tests::signed, FileEntry, SignedCommit},
};
use engine_shared::NodeId;
use ring::signature::Ed25519KeyPair;
use serde_json::json;
use sqlx::{ConnectOptions, Connection};

fn entry(document: DocumentId, root: &Address) -> FileEntry {
    FileEntry {
        document,
        state: root.clone(),
    }
}

fn doc() -> DocumentId {
    DocumentId(uuid::Uuid::new_v4())
}

/// A state with `count` paragraphs; `tag` changes only the paragraphs listed.
fn paragraphs(count: usize, tag: &str, changed: &[usize]) -> DocumentState {
    let mut state = DocumentState::default();
    for i in 0..count {
        let id = NodeId(format!("{i:026}"));
        let label = if changed.contains(&i) {
            format!("paragraph {i} edited {tag}")
        } else {
            format!("paragraph {i}")
        };
        state.nodes.insert(
            id.clone(),
            MaterializedNode {
                id,
                parent_id: None,
                node_type: "paragraph".into(),
                pos: format!("p{i:06}"),
                var_name: None,
                deleted: false,
                current_fields: json!({"label": label}),
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
) -> (Address, u64) {
    let commit = signed(
        repo.project(),
        parents,
        files
            .iter()
            .map(|(path, file)| ((*path).to_string(), file.clone()))
            .collect(),
        message,
    );
    let bytes = serde_json::to_vec(&commit).unwrap().len() as u64;
    (repo.import_commit(&commit).await.unwrap(), bytes)
}

async fn publish(repo: &LocalRepository, branch: &str, commit: &Address) {
    let head = repo.head(branch).await.unwrap();
    repo.compare_and_swap(branch, head.as_ref(), commit)
        .await
        .unwrap();
}

fn category<'a>(plan: &'a GcPlan, name: &str) -> &'a Counts {
    &plan
        .unreachable
        .categories
        .iter()
        .find(|c| c.category == name)
        .unwrap()
        .counts
}

fn reason<'a>(plan: &'a GcPlan, name: &str) -> Option<&'a Counts> {
    plan.pinned
        .reasons
        .iter()
        .find(|r| r.reason == name)
        .map(|r| &r.counts)
}

/// Changes only when another connection commits to the database file.
async fn data_version(connection: &mut sqlx::SqliteConnection) -> i64 {
    sqlx::query_scalar("pragma data_version")
        .fetch_one(connection)
        .await
        .unwrap()
}

fn json_bytes(plan: &GcPlan) -> Vec<u8> {
    serde_json::to_vec(plan).unwrap()
}

fn assert_consistent(plan: &GcPlan) {
    let mut sum = plan.reachable.clone();
    sum.objects += plan.unreachable.counts.objects;
    sum.object_bytes += plan.unreachable.counts.object_bytes;
    sum.commits += plan.unreachable.counts.commits;
    sum.commit_bytes += plan.unreachable.counts.commit_bytes;
    sum.bytes += plan.unreachable.counts.bytes;
    assert_eq!(sum, plan.retained);
    assert_eq!(
        plan.retained.bytes,
        plan.retained.object_bytes + plan.retained.commit_bytes
    );
    assert_eq!(
        plan.reclaimable_bytes_estimate,
        plan.unreachable.counts.bytes
    );
    assert_eq!(plan.accounting.consistent, Some(true));
    assert!(plan.read_only);
}

#[tokio::test]
async fn empty_repository_reports_nothing_and_is_stable() {
    let file = path();
    let repo = LocalRepository::open(&file, uuid::Uuid::new_v4())
        .await
        .unwrap();
    let plan = repo.gc_plan(GcPlanOptions::default()).await.unwrap();
    assert_eq!(plan.retained, Counts::default());
    assert_eq!(plan.reachable, Counts::default());
    assert_eq!(plan.unreachable.counts, Counts::default());
    assert_eq!(plan.reclaimable_bytes_estimate, 0);
    assert!(plan.branches.is_empty());
    assert!(plan.pinned.reasons.is_empty());
    assert_eq!(plan.unreachable.categories.len(), 3);
    assert_consistent(&plan);
    assert_eq!(
        json_bytes(&plan),
        json_bytes(&repo.gc_plan(GcPlanOptions::default()).await.unwrap())
    );
    assert!(plan.summary().contains("read-only"));
    cleanup(repo, &file).await;
}

#[tokio::test]
async fn branch_history_stays_reachable_and_staged_orphaned_superseded_are_split() {
    let file = path();
    let project = uuid::Uuid::new_v4();
    let repo = LocalRepository::open(&file, project).await.unwrap();
    let (report, budget) = (doc(), doc());
    let (r1, _) = repo.write_state(report, &state("one")).await.unwrap();
    let (r2, _) = repo.write_state(report, &state("two")).await.unwrap();
    let (r3, _) = repo.write_state(report, &state("three")).await.unwrap();
    let (b1, _) = repo.write_state(budget, &state("budget")).await.unwrap();
    let files = |root: &Address| {
        [
            ("Report.docx", entry(report, root)),
            ("Budget.xlsx", entry(budget, &b1)),
        ]
    };
    let (c1, _) = import(&repo, vec![], &files(&r1), "one").await;
    let (c2, _) = import(&repo, vec![c1.clone()], &files(&r2), "two").await;
    let (c3, _) = import(&repo, vec![c2.clone()], &files(&r3), "three").await;
    for commit in [&c1, &c2, &c3] {
        publish(&repo, "main", commit).await;
    }
    // Every object and commit is reachable: old commits stay through parents.
    let plan = repo.gc_plan(GcPlanOptions::default()).await.unwrap();
    assert_consistent(&plan);
    assert_eq!(plan.branches.len(), 1);
    assert_eq!(plan.branches[0].generation, 3);
    assert_eq!(plan.reachable, plan.retained);
    assert_eq!(plan.unreachable.counts, Counts::default());
    assert_eq!(reason(&plan, "branch-head:main").unwrap().commits, 1);
    let history = reason(&plan, "branch-history:main").unwrap();
    assert_eq!(history.commits, 2);
    assert!(history.objects > 0, "old states are pinned by ancestry");
    assert!(reason(&plan, "recovery-source").is_none());

    // Staged only: written, but no commit names it.
    let (_, staged) = repo
        .write_state(doc(), &state("never committed"))
        .await
        .unwrap();
    // Imported, linked to retained history, but no branch reaches it.
    let (side, side_stats) = repo.write_state(report, &state("side line")).await.unwrap();
    let (side_commit, side_bytes) = import(&repo, vec![c1.clone()], &files(&side), "side").await;
    // Imported with no ancestry in common with retained history.
    let other = doc();
    let (o1, other_stats) = repo
        .write_state(other, &state("disconnected"))
        .await
        .unwrap();
    let (orphan, orphan_bytes) = import(
        &repo,
        vec![],
        &[("Other.docx", entry(other, &o1))],
        "orphan",
    )
    .await;

    let plan = repo.gc_plan(GcPlanOptions::default()).await.unwrap();
    assert_consistent(&plan);
    let staged_counts = category(&plan, "staged");
    assert_eq!(staged_counts.objects, staged.new_objects as u64);
    assert_eq!(staged_counts.object_bytes, staged.new_bytes as u64);
    assert_eq!(staged_counts.commits, 0);
    let superseded = category(&plan, "superseded");
    assert_eq!(superseded.commits, 1);
    assert_eq!(superseded.commit_bytes, side_bytes);
    assert_eq!(superseded.objects, side_stats.new_objects as u64);
    assert_eq!(superseded.object_bytes, side_stats.new_bytes as u64);
    let orphaned = category(&plan, "orphaned");
    assert_eq!(orphaned.commits, 1);
    assert_eq!(orphaned.commit_bytes, orphan_bytes);
    assert_eq!(orphaned.objects, other_stats.new_objects as u64);
    assert_eq!(
        plan.reclaimable_bytes_estimate,
        staged.new_bytes as u64
            + side_stats.new_bytes as u64
            + side_bytes
            + other_stats.new_bytes as u64
            + orphan_bytes
    );
    let listed: Vec<_> = plan
        .examples
        .commits
        .iter()
        .map(|c| (c.commit.clone(), c.category))
        .collect();
    let mut expected = vec![
        (side_commit.clone(), "superseded"),
        (orphan.clone(), "orphaned"),
    ];
    expected.sort();
    assert_eq!(listed, expected);
    assert_eq!(
        plan.examples.staged_objects.len(),
        staged.new_objects.min(10)
    );

    // Publishing the side commit as a fast-forward of nothing: a new branch
    // makes it reachable; its objects leave the unreachable totals.
    publish(&repo, "side", &side_commit).await;
    let plan = repo.gc_plan(GcPlanOptions::default()).await.unwrap();
    assert_consistent(&plan);
    assert_eq!(category(&plan, "superseded"), &Counts::default());
    assert!(reason(&plan, "branch-head:side").is_some());
    assert_eq!(category(&plan, "orphaned").commits, 1);
    cleanup(repo, &file).await;
}

#[tokio::test]
async fn moving_a_branch_away_supersedes_only_what_no_ancestor_keeps() {
    let file = path();
    let project = uuid::Uuid::new_v4();
    let repo = LocalRepository::open(&file, project).await.unwrap();
    let report = doc();
    let mut roots = Vec::new();
    let mut stats = Vec::new();
    for text in ["base", "later one", "later two"] {
        let (root, s) = repo.write_state(report, &state(text)).await.unwrap();
        roots.push(root);
        stats.push(s);
    }
    let (c1, _) = import(&repo, vec![], &[("a", entry(report, &roots[0]))], "1").await;
    let (c2, b2) = import(
        &repo,
        vec![c1.clone()],
        &[("a", entry(report, &roots[1]))],
        "2",
    )
    .await;
    let (c3, b3) = import(
        &repo,
        vec![c2.clone()],
        &[("a", entry(report, &roots[2]))],
        "3",
    )
    .await;
    publish(&repo, "main", &c3).await;
    let before = repo.gc_plan(GcPlanOptions::default()).await.unwrap();
    assert_eq!(before.unreachable.counts, Counts::default());
    // A reset to the first commit leaves two commits reachable from nowhere.
    publish(&repo, "main", &c1).await;
    let after = repo.gc_plan(GcPlanOptions::default()).await.unwrap();
    assert_consistent(&after);
    let superseded = category(&after, "superseded");
    assert_eq!(superseded.commits, 2);
    assert_eq!(superseded.commit_bytes, b2 + b3);
    // The shared empty comment/suggestion maps stay pinned by the first state;
    // only objects exclusive to the later states are superseded.
    assert_eq!(
        superseded.objects,
        (stats[1].new_objects + stats[2].new_objects) as u64
    );
    assert_eq!(reason(&after, "branch-head:main").unwrap().commits, 1);
    assert!(reason(&after, "branch-history:main").is_none());
    // Moving it forward again restores reachability through the retained commits.
    publish(&repo, "main", &c3).await;
    let again = repo.gc_plan(GcPlanOptions::default()).await.unwrap();
    assert_eq!(again.unreachable.counts, Counts::default());
    assert_eq!(again.retained, before.retained);
    assert_eq!(again.reachable, before.reachable);
    assert_eq!(again.pinned, before.pinned);
    cleanup(repo, &file).await;
}

#[tokio::test]
async fn recovery_sources_are_pinned_until_nothing_retained_names_them() {
    let file = path();
    let project = uuid::Uuid::new_v4();
    let repo = LocalRepository::open(&file, project).await.unwrap();
    let report = doc();
    let (r0, _) = repo.write_state(report, &state("base")).await.unwrap();
    let (r1, source_stats) = repo
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
    let (base, _) = import(
        &repo,
        vec![],
        &[("Report.docx", entry(report, &r0))],
        "base",
    )
    .await;
    let (current, _) = import(
        &repo,
        vec![base.clone()],
        &[("Report.docx", entry(report, &r2))],
        "current",
    )
    .await;
    let (source, _) = import(
        &repo,
        vec![base.clone()],
        &[("Report.docx", entry(report, &r1))],
        "source",
    )
    .await;
    publish(&repo, "main", &current).await;
    let unpinned = repo.gc_plan(GcPlanOptions::default()).await.unwrap();
    assert_eq!(category(&unpinned, "superseded").commits, 1);

    // A conflicting cherry-pick resolved to different content: the signed
    // provenance reference alone keeps the source commit and its state.
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
    let pinned = repo.gc_plan(GcPlanOptions::default()).await.unwrap();
    assert_consistent(&pinned);
    assert_eq!(pinned.unreachable.counts, Counts::default());
    let by_source = reason(&pinned, "recovery-source").unwrap();
    assert_eq!(by_source.commits, 1);
    assert_eq!(by_source.objects, source_stats.new_objects as u64);
    // Ancestry still explains the rest.
    assert_eq!(reason(&pinned, "branch-head:main").unwrap().commits, 1);
    assert_eq!(reason(&pinned, "branch-history:main").unwrap().commits, 2);
    cleanup(repo, &file).await;
}

#[tokio::test]
async fn top_reasons_are_ranked_bounded_and_counted() {
    let file = path();
    let project = uuid::Uuid::new_v4();
    let repo = LocalRepository::open(&file, project).await.unwrap();
    for (i, size) in [2usize, 20, 8, 4].into_iter().enumerate() {
        let document = doc();
        let (root, _) = repo
            .write_state(document, &paragraphs(size, "x", &[]))
            .await
            .unwrap();
        let (commit, _) = import(
            &repo,
            vec![],
            &[("f", entry(document, &root))],
            &format!("c{i}"),
        )
        .await;
        publish(&repo, &format!("b{i}"), &commit).await;
    }
    let full = repo
        .gc_plan(GcPlanOptions {
            top_reasons: 100,
            list: 0,
        })
        .await
        .unwrap();
    assert_eq!(full.pinned.reasons.len(), 4);
    assert_eq!(full.pinned.reasons[0].reason, "branch-head:b1");
    assert!(full
        .pinned
        .reasons
        .windows(2)
        .all(|w| w[0].counts.bytes >= w[1].counts.bytes));
    assert!(full.examples.commits.is_empty());
    let top = repo
        .gc_plan(GcPlanOptions {
            top_reasons: 2,
            list: 0,
        })
        .await
        .unwrap();
    assert_eq!(top.pinned.reasons, full.pinned.reasons[..2].to_vec());
    assert_eq!(top.pinned.omitted_reasons, 2);
    assert!(repo
        .gc_plan(GcPlanOptions {
            top_reasons: 0,
            list: 1
        })
        .await
        .is_err());
    assert!(repo
        .gc_plan(GcPlanOptions {
            top_reasons: 1,
            list: 1001
        })
        .await
        .is_err());
    cleanup(repo, &file).await;
}

#[tokio::test]
async fn planning_is_deterministic_across_connections_and_performs_no_write() {
    let file = path();
    let project = uuid::Uuid::new_v4();
    let repo = LocalRepository::open(&file, project).await.unwrap();
    let report = doc();
    let (r1, _) = repo
        .write_state(report, &paragraphs(40, "a", &[]))
        .await
        .unwrap();
    let (r2, _) = repo
        .write_state(report, &paragraphs(40, "b", &[3, 9]))
        .await
        .unwrap();
    let (c1, _) = import(&repo, vec![], &[("Report.docx", entry(report, &r1))], "1").await;
    let (c2, _) = import(
        &repo,
        vec![c1.clone()],
        &[("Report.docx", entry(report, &r2))],
        "2",
    )
    .await;
    publish(&repo, "main", &c2).await;
    repo.write_state(doc(), &state("staged")).await.unwrap();
    import(&repo, vec![], &[], "empty orphan").await;

    let wal = std::path::PathBuf::from(format!("{}-wal", file.display()));
    let snapshot = || {
        (
            std::fs::read(&file).unwrap(),
            std::fs::read(&wal).unwrap_or_default(),
        )
    };
    let mut observer = SqliteConnectOptions::new()
        .filename(&file)
        .connect()
        .await
        .unwrap();
    let version_before = data_version(&mut observer).await;
    let files_before = snapshot();
    let first = repo.gc_plan(GcPlanOptions::default()).await.unwrap();
    let second = repo.gc_plan(GcPlanOptions::default()).await.unwrap();
    assert_eq!(json_bytes(&first), json_bytes(&second));
    assert_eq!(data_version(&mut observer).await, version_before);
    assert!(snapshot() == files_before, "database or WAL bytes changed");
    // A read-only handle on the same file plans identically.
    let reader = LocalRepository::open_read_only(&file, project)
        .await
        .unwrap();
    let third = reader.gc_plan(GcPlanOptions::default()).await.unwrap();
    assert_eq!(json_bytes(&first), json_bytes(&third));
    assert!(snapshot() == files_before);
    reader.close().await;
    observer.close().await.unwrap();
    repo.close().await;
    // Reopening, as another process would, yields the same bytes.
    let reopened = LocalRepository::open(&file, project).await.unwrap();
    let fourth = reopened.gc_plan(GcPlanOptions::default()).await.unwrap();
    assert_eq!(json_bytes(&first), json_bytes(&fourth));
    assert_consistent(&fourth);
    cleanup(reopened, &file).await;
}

#[tokio::test]
async fn read_only_open_never_creates_upgrades_or_writes() {
    let missing = path();
    assert!(
        LocalRepository::open_read_only(&missing, uuid::Uuid::new_v4())
            .await
            .is_err()
    );
    assert!(
        !missing.exists(),
        "a missing repository must not be created"
    );

    let file = path();
    let project = uuid::Uuid::new_v4();
    let repo = LocalRepository::open(&file, project).await.unwrap();
    let document = doc();
    let (root, _) = repo.write_state(document, &state("kept")).await.unwrap();
    let (commit, _) = import(&repo, vec![], &[("a", entry(document, &root))], "c").await;
    publish(&repo, "main", &commit).await;
    repo.close().await;

    let reader = LocalRepository::open_read_only(&file, project)
        .await
        .unwrap();
    assert_eq!(reader.head("main").await.unwrap().unwrap().commit, commit);
    assert_eq!(
        reader.read_state(document, &root).await.unwrap(),
        state("kept")
    );
    assert!(reader
        .write_state(document, &state("rejected"))
        .await
        .is_err());
    assert!(reader
        .compare_and_swap("other", None, &commit)
        .await
        .is_err());
    assert!(reader
        .set_storage_limits(StorageLimits::default())
        .await
        .is_err());
    assert!(LocalRepository::open_read_only(&file, uuid::Uuid::new_v4())
        .await
        .is_err());
    let plan = reader.gc_plan(GcPlanOptions::default()).await.unwrap();
    assert_eq!(plan.unreachable.counts, Counts::default());
    cleanup(reader, &file).await;
}

#[tokio::test]
async fn schema_one_repository_is_planned_without_being_upgraded() {
    let file = path();
    let project = uuid::Uuid::new_v4();
    let document = doc();
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
    let (root, stats) = typed::write_state(
        &mut Store {
            connection: &mut tx,
            read_budget: None,
            packed: false,
        },
        document,
        &state("schema one"),
        Limits::default(),
        Default::default(),
    )
    .await
    .unwrap();
    let commit = signed(
        project,
        vec![],
        BTreeMap::from([("a".into(), entry(document, &root))]),
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
    let plan = reader.gc_plan(GcPlanOptions::default()).await.unwrap();
    assert_eq!(plan.local_schema, 1);
    assert_eq!(plan.reachable.objects, stats.new_objects as u64);
    assert_eq!(plan.unreachable.counts, Counts::default());
    assert_eq!(plan.accounting.recorded, None);
    assert_eq!(plan.accounting.consistent, None);
    let version: i64 = sqlx::query_scalar("select version from local_meta")
        .fetch_one(&reader.pool)
        .await
        .unwrap();
    assert_eq!(version, 1);
    let upgraded: i64 =
        sqlx::query_scalar("select count(*) from sqlite_master where name='local_storage'")
            .fetch_one(&reader.pool)
            .await
            .unwrap();
    assert_eq!(upgraded, 0);
    cleanup(reader, &file).await;
}

#[tokio::test]
async fn corrupt_or_missing_content_fails_closed() {
    let file = path();
    let project = uuid::Uuid::new_v4();
    let repo = LocalRepository::open(&file, project).await.unwrap();
    let document = doc();
    let (root, _) = repo.write_state(document, &state("x")).await.unwrap();
    let (commit, _) = import(&repo, vec![], &[("a", entry(document, &root))], "c").await;
    publish(&repo, "main", &commit).await;
    assert_consistent(&repo.gc_plan(GcPlanOptions::default()).await.unwrap());
    // Triggers protect rows from the API; an owner-level edit is detected, not
    // silently planned around. Drop the guard in this disposable test database.
    sqlx::query("drop trigger objects_no_delete")
        .execute(&repo.pool)
        .await
        .unwrap();
    sqlx::query("delete from objects where address=?")
        .bind(root.as_str())
        .execute(&repo.pool)
        .await
        .unwrap();
    assert!(matches!(
        repo.gc_plan(GcPlanOptions::default()).await,
        Err(Error::Object(shared_checkpoint::Error::Missing(_)))
    ));
    cleanup(repo, &file).await;
}

#[tokio::test]
async fn synthetic_history_plans_within_bounds_and_matches_independent_counts() {
    let file = path();
    let project = uuid::Uuid::new_v4();
    let repo = LocalRepository::open(&file, project).await.unwrap();
    let documents: Vec<DocumentId> = (0..3).map(|_| doc()).collect();
    let mut reachable_objects = 0u64;
    let mut roots: Vec<Address> = Vec::new();
    for (i, document) in documents.iter().enumerate() {
        let (root, stats) = repo
            .write_state(*document, &paragraphs(48, &format!("d{i}"), &[]))
            .await
            .unwrap();
        reachable_objects += stats.new_objects as u64;
        roots.push(root);
    }
    let names = ["a.docx", "b.xlsx", "c.pptx"];
    let files = |roots: &[Address]| -> Vec<(&str, FileEntry)> {
        names
            .iter()
            .zip(&documents)
            .zip(roots)
            .map(|((name, document), root)| (*name, entry(*document, root)))
            .collect()
    };
    let (mut head, _) = import(&repo, vec![], &files(&roots), "initial").await;
    let mut commits = 1u64;
    for round in 0..40usize {
        let which = round % 3;
        let (root, stats) = repo
            .write_state(
                documents[which],
                &paragraphs(48, &format!("r{round}"), &[round % 48]),
            )
            .await
            .unwrap();
        reachable_objects += stats.new_objects as u64;
        roots[which] = root;
        let (next, _) = import(
            &repo,
            vec![head.clone()],
            &files(&roots),
            &format!("r{round}"),
        )
        .await;
        head = next;
        commits += 1;
    }
    publish(&repo, "main", &head).await;
    // Dead ends: one line per document, never published.
    let mut dead_objects = 0u64;
    for (i, document) in documents.iter().enumerate() {
        let (root, stats) = repo
            .write_state(*document, &paragraphs(48, "dead", &[i, i + 10, i + 20]))
            .await
            .unwrap();
        dead_objects += stats.new_objects as u64;
        let mut dead = roots.clone();
        dead[i] = root;
        import(
            &repo,
            vec![head.clone()],
            &files(&dead),
            &format!("dead {i}"),
        )
        .await;
    }
    let plan = repo
        .gc_plan(GcPlanOptions {
            top_reasons: 5,
            list: 100,
        })
        .await
        .unwrap();
    assert_consistent(&plan);
    assert_eq!(plan.reachable.commits, commits);
    assert_eq!(plan.reachable.objects, reachable_objects);
    assert_eq!(category(&plan, "superseded").commits, 3);
    assert_eq!(category(&plan, "superseded").objects, dead_objects);
    assert_eq!(category(&plan, "orphaned"), &Counts::default());
    assert_eq!(category(&plan, "staged"), &Counts::default());
    assert_eq!(plan.examples.commits.len(), 3);
    assert_eq!(reason(&plan, "branch-head:main").unwrap().commits, 1);
    assert_eq!(
        reason(&plan, "branch-history:main").unwrap().commits,
        commits - 1
    );
    let usage = repo.storage_usage().await.unwrap();
    assert_eq!(plan.retained.bytes as i64, usage.bytes_used);
    assert_eq!(plan.retained.objects as i64, usage.objects_used);
    cleanup(repo, &file).await;
}
