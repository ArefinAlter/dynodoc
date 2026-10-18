//! `LocalRepository::horizon_plan` against real SQLite repositories: the loader
//! sees what is stored, plans change nothing, and the plan's numbers agree with
//! computations that do not use the planner (merge bases from the repository's own
//! `merge_bases`, objects by re-materializing the states that would stay).

use super::tests::{cleanup, path};
use super::*;
use crate::{
    materializer::MaterializedNode,
    repository::{
        horizon::{
            plan::{HorizonPlan, PlanOptions},
            policy::{LoadOptions, Policy},
            Error as HorizonError,
        },
        project_merge::SameFile,
        recovery::Kind,
        tests::signed,
        FileEntry, SignedCommit,
    },
};
use engine_shared::NodeId;
use ring::signature::Ed25519KeyPair;
use serde_json::json;
use sqlx::{ConnectOptions, Connection};

fn paragraphs(tag: &str, edits: &[(usize, &str)]) -> DocumentState {
    let mut state = DocumentState::default();
    for i in 0..6 {
        let id = NodeId(format!("{i:026}"));
        let label = edits
            .iter()
            .find(|(index, _)| *index == i)
            .map_or(format!("paragraph {i} {tag}"), |(_, text)| {
                (*text).to_string()
            });
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

fn key() -> Ed25519KeyPair {
    Ed25519KeyPair::from_seed_unchecked(&[7; 32]).unwrap()
}

fn policy(json: &str) -> Policy {
    Policy::load(json.as_bytes(), LoadOptions::default()).unwrap()
}

struct Repo {
    repo: LocalRepository,
    file: std::path::PathBuf,
    report: DocumentId,
    budget: DocumentId,
    /// Every commit in creation order, named.
    commits: Vec<(String, Address)>,
}

impl Repo {
    fn id(&self, name: &str) -> Address {
        self.commits
            .iter()
            .find(|(n, _)| n == name)
            .unwrap()
            .1
            .clone()
    }

    async fn files(
        &self,
        report: &[(usize, &str)],
        tag: &str,
        budget: &str,
    ) -> BTreeMap<String, FileEntry> {
        let (r, _) = self
            .repo
            .write_state(self.report, &paragraphs(tag, report))
            .await
            .unwrap();
        let (b, _) = self
            .repo
            .write_state(self.budget, &paragraphs(budget, &[]))
            .await
            .unwrap();
        BTreeMap::from([
            (
                "Report.docx".into(),
                FileEntry {
                    document: self.report,
                    state: r,
                },
            ),
            (
                "Budget.xlsx".into(),
                FileEntry {
                    document: self.budget,
                    state: b,
                },
            ),
        ])
    }

    async fn commit(
        &mut self,
        name: &str,
        parents: &[&str],
        files: BTreeMap<String, FileEntry>,
    ) -> Address {
        let parents = parents.iter().map(|p| self.id(p)).collect();
        let address = self
            .repo
            .import_commit(&signed(self.repo.project(), parents, files, name))
            .await
            .unwrap();
        self.commits.push((name.into(), address.clone()));
        address
    }

    async fn publish(&self, branch: &str, name: &str) {
        let head = self.repo.head(branch).await.unwrap();
        self.repo
            .compare_and_swap(branch, head.as_ref(), &self.id(name))
            .await
            .unwrap();
    }
}

/// c0..c5, then left and right editing Report differently from c5, a clean v3
/// merge `m`, a Budget-only commit `s`, `n2`, `n3`, a v2 revert `r` of `s`, and
/// `n4`, `n5`. `main` is at `n5`, `feature` at `right`.
async fn history() -> Repo {
    let file = path();
    let repo = LocalRepository::open(&file, uuid::Uuid::new_v4())
        .await
        .unwrap();
    let mut h = Repo {
        repo,
        file,
        report: DocumentId(uuid::Uuid::new_v4()),
        budget: DocumentId(uuid::Uuid::new_v4()),
        commits: Vec::new(),
    };
    for i in 0..6 {
        let files = h.files(&[], &format!("c{i}"), &format!("b{i}")).await;
        let parents = if i == 0 {
            vec![]
        } else {
            vec![format!("c{}", i - 1)]
        };
        let parents: Vec<&str> = parents.iter().map(String::as_str).collect();
        h.commit(&format!("c{i}"), &parents, files).await;
    }
    // Same tag as c5 so only the listed paragraphs differ from it.
    let left_files = h.files(&[(1, "left edit")], "c5", "b5").await;
    let right_files = h.files(&[(4, "right edit")], "c5", "b5").await;
    h.commit("left", &["c5"], left_files).await;
    h.commit("right", &["c5"], right_files).await;
    let merge = h
        .repo
        .prepare_merge(
            &h.id("left"),
            &h.id("right"),
            SameFile::Clean,
            &BTreeMap::new(),
            1_700_000_000_500,
            "merge".into(),
        )
        .await
        .unwrap();
    let merged = SignedCommit::sign(merge.commit.unwrap(), &key()).unwrap();
    assert_eq!(merged.commit.version, 3);
    let address = h.repo.import_commit(&merged).await.unwrap();
    h.commits.push(("m".into(), address));
    let mut files = h.repo.commit(&h.id("m")).await.unwrap().commit.files;
    let (budget, _) = h
        .repo
        .write_state(h.budget, &paragraphs("budget after merge", &[]))
        .await
        .unwrap();
    files.get_mut("Budget.xlsx").unwrap().state = budget;
    h.commit("s", &["m"], files.clone()).await;
    for name in ["n2", "n3"] {
        let parent = if name == "n2" { "s" } else { "n2" };
        let mut next = files.clone();
        let (root, _) = h
            .repo
            .write_state(
                h.report,
                &paragraphs(name, &[(1, "left edit"), (4, "right edit")]),
            )
            .await
            .unwrap();
        next.get_mut("Report.docx").unwrap().state = root;
        files = next;
        h.commit(name, &[parent], files.clone()).await;
    }
    let plan = h
        .repo
        .plan_recovery(Kind::Revert, &h.id("s"), &h.id("n3"), None)
        .await
        .unwrap();
    let recovered = SignedCommit::sign(
        plan.resolve(&BTreeMap::new(), 1_700_000_001_000, "revert s".into())
            .unwrap(),
        &key(),
    )
    .unwrap();
    assert_eq!(recovered.commit.version, 2);
    let address = h.repo.import_commit(&recovered).await.unwrap();
    h.commits.push(("r".into(), address));
    let mut files = recovered.commit.files.clone();
    for (name, parent) in [("n4", "r"), ("n5", "n4")] {
        let (root, _) = h
            .repo
            .write_state(h.report, &paragraphs(name, &[]))
            .await
            .unwrap();
        files.get_mut("Report.docx").unwrap().state = root;
        h.commit(name, &[parent], files.clone()).await;
    }
    h.publish("main", "n5").await;
    h.publish("feature", "right").await;
    h
}

/// Changes only when another connection commits to the database file.
async fn data_version(connection: &mut sqlx::SqliteConnection) -> i64 {
    sqlx::query_scalar("pragma data_version")
        .fetch_one(connection)
        .await
        .unwrap()
}

fn naive_bases(
    parents: &BTreeMap<Address, Vec<Address>>,
    a: &Address,
    b: &Address,
) -> Vec<Address> {
    let ancestors = |start: &Address| {
        let mut seen = BTreeSet::from([start.clone()]);
        let mut stack = vec![start.clone()];
        while let Some(node) = stack.pop() {
            for p in &parents[&node] {
                if seen.insert(p.clone()) {
                    stack.push(p.clone());
                }
            }
        }
        seen
    };
    let (left, right) = (ancestors(a), ancestors(b));
    let common: BTreeSet<Address> = left.intersection(&right).cloned().collect();
    let dominated: BTreeSet<Address> = common
        .iter()
        .flat_map(|c| parents[c].iter().cloned())
        .collect();
    common.difference(&dominated).cloned().collect()
}

async fn object_count(repo: &LocalRepository) -> usize {
    let (mut count, mut cursor) = (0, None);
    loop {
        let page = repo.inventory(cursor.as_ref(), 256).await.unwrap();
        let Some(last) = page.last().cloned() else {
            return count;
        };
        count += page.len();
        cursor = Some(last);
    }
}

async fn object_bytes(repo: &LocalRepository) -> u64 {
    repo.gc_plan(GcPlanOptions::default())
        .await
        .unwrap()
        .retained
        .object_bytes
}

/// A window of three commits, the merge-base rule, and the v3 merge commit pinned so
/// its provenance closure is exercised.
fn pin(h: &Repo, tier: &str) -> Policy {
    policy(&format!(
        r#"{{"version":1,"id":"pin","tier":"{tier}","window":{{"keep_last_commits":3}},
        "protect":{{"branch_heads":true,"commits":["{}"],
        "merge_bases":{{"against":"main","branches":["*"]}}}}}}"#,
        h.id("m").as_str()
    ))
}

#[tokio::test]
async fn a_real_history_keeps_heads_bases_and_closure_and_its_merge_bases_survive() {
    let h = history().await;
    let plan = h
        .repo
        .horizon_plan(&pin(&h, "history"), &PlanOptions::default())
        .await
        .unwrap();
    let keep: BTreeSet<Address> = plan.sets.retained.iter().cloned().collect();
    let name_of = |a: &Address| h.commits.iter().find(|(_, x)| x == a).unwrap().0.clone();
    let kept: BTreeSet<String> = keep.iter().map(name_of).collect();
    // heads (n5, right); a window of 3 along first parents from each head (n5, n4, r
    // and right, c5, c4); the pinned merge commit m; the fork point of feature against
    // main (right itself); the v2 closure of r (source s, mainline m, first parent n3)
    // and the v3 closure of m (left, right, base c5).
    let want: BTreeSet<String> = ["n5", "n4", "r", "right", "c5", "c4", "m", "s", "n3", "left"]
        .into_iter()
        .map(String::from)
        .collect();
    assert_eq!(kept, want);
    assert_eq!(plan.commits.total, h.commits.len() as u64);
    assert_eq!(plan.commits.released as usize, h.commits.len() - want.len());

    // Heights in the skeleton are the stored heights.
    for entry in &plan.sets.skeleton {
        let stored: i64 = sqlx::query_scalar("select height from commits where address=?")
            .bind(entry.address.as_str())
            .fetch_one(&h.repo.pool)
            .await
            .unwrap();
        assert_eq!(entry.height, stored as u64);
    }

    // I3 against the repository's own merge_bases, for every pair of stored commits.
    let mut full: BTreeMap<Address, Vec<Address>> = BTreeMap::new();
    for (_, address) in &h.commits {
        full.insert(
            address.clone(),
            h.repo.commit(address).await.unwrap().commit.parents,
        );
    }
    let mut retained_graph: BTreeMap<Address, Vec<Address>> = BTreeMap::new();
    for address in &keep {
        retained_graph.insert(address.clone(), full[address].clone());
    }
    for entry in &plan.sets.skeleton {
        retained_graph.insert(entry.address.clone(), entry.parents.clone());
    }
    assert_eq!(retained_graph.len(), h.commits.len());
    for (_, x) in &h.commits {
        for (_, y) in &h.commits {
            let real = h.repo.merge_bases(x, y).await.unwrap();
            assert_eq!(
                naive_bases(&retained_graph, x, y),
                real,
                "{} {}",
                name_of(x),
                name_of(y)
            );
        }
    }
    // The fork point of the two live branches is retained.
    let fork = h
        .repo
        .merge_bases(&h.id("n5"), &h.id("right"))
        .await
        .unwrap();
    assert!(fork.iter().all(|b| keep.contains(b)));

    // Objects, against re-materializing exactly the states that would stay.
    let released: BTreeSet<(uuid::Uuid, Address)> = plan
        .sets
        .released_states
        .iter()
        .map(|s| (s.document, s.state_root.clone()))
        .collect();
    let scratch_path = path();
    let scratch = LocalRepository::open(&scratch_path, h.repo.project())
        .await
        .unwrap();
    let mut all_states = BTreeSet::new();
    for (_, address) in &h.commits {
        for file in h.repo.commit(address).await.unwrap().commit.files.values() {
            all_states.insert((file.document.0, file.state.clone()));
        }
    }
    let mut live = 0;
    for (document, root) in &all_states {
        if released.contains(&(*document, root.clone())) {
            continue;
        }
        live += 1;
        let state = h
            .repo
            .read_state(DocumentId(*document), root)
            .await
            .unwrap();
        let (again, _) = scratch
            .write_state(DocumentId(*document), &state)
            .await
            .unwrap();
        assert_eq!(&again, root);
    }
    assert!(live > 0 && live < all_states.len());
    assert_eq!(
        plan.counts.states_released as usize,
        all_states.len() - live
    );
    assert_eq!(
        plan.counts.objects_released as usize,
        object_count(&h.repo).await - object_count(&scratch).await
    );
    assert_eq!(
        plan.counts.object_bytes_released,
        object_bytes(&h.repo).await - object_bytes(&scratch).await
    );
    assert!(plan.objects_measured);
    // Commit bytes are the stored lengths.
    let mut commit_bytes = 0u64;
    for address in &plan.sets.released {
        let len: i64 = sqlx::query_scalar("select length(body) from commits where address=?")
            .bind(address.as_str())
            .fetch_one(&h.repo.pool)
            .await
            .unwrap();
        commit_bytes += len as u64;
    }
    assert_eq!(plan.counts.commit_bytes_released, commit_bytes);
    cleanup(scratch, &scratch_path).await;
    cleanup(h.repo, &h.file).await;
}

#[tokio::test]
async fn the_content_tier_keeps_every_commit_and_releases_only_unneeded_states() {
    let h = history().await;
    let plan = h
        .repo
        .horizon_plan(&pin(&h, "content"), &PlanOptions::default())
        .await
        .unwrap();
    assert_eq!(plan.commits.released, 0);
    assert_eq!(plan.counts.commits_released, 0);
    assert!(plan.sets.skeleton.is_empty() && plan.sets.frontier.is_empty());
    assert_eq!(plan.sets.retained.len(), h.commits.len());
    assert!(plan.counts.states_released > 0);
    // Heads' states stay, whatever the tier.
    let released: BTreeSet<_> = plan
        .sets
        .released_states
        .iter()
        .map(|s| (s.document, s.state_root.clone()))
        .collect();
    for name in ["n5", "right"] {
        for file in h
            .repo
            .commit(&h.id(name))
            .await
            .unwrap()
            .commit
            .files
            .values()
        {
            assert!(!released.contains(&(file.document.0, file.state.clone())));
        }
    }
    // A content horizon is a record in waiting too.
    plan.draft_record(1, None, 1_790_000_000_000).unwrap();
    cleanup(h.repo, &h.file).await;
}

#[tokio::test]
async fn planning_changes_nothing_and_a_read_only_handle_plans_identically() {
    let h = history().await;
    let wal = std::path::PathBuf::from(format!("{}-wal", h.file.display()));
    let snapshot = || {
        (
            std::fs::read(&h.file).unwrap(),
            std::fs::read(&wal).unwrap_or_default(),
        )
    };
    let mut observer = SqliteConnectOptions::new()
        .filename(&h.file)
        .connect()
        .await
        .unwrap();
    let version_before = data_version(&mut observer).await;
    let before = snapshot();
    let policy = pin(&h, "history");
    let first = h
        .repo
        .horizon_plan(&policy, &PlanOptions::default())
        .await
        .unwrap();
    let second = h
        .repo
        .horizon_plan(
            &policy,
            &PlanOptions {
                list: 0,
                ..PlanOptions::default()
            },
        )
        .await
        .unwrap();
    assert_eq!(first.plan_hash, second.plan_hash, "examples are not hashed");
    assert!(snapshot() == before, "database or WAL bytes changed");
    assert_eq!(data_version(&mut observer).await, version_before);
    let reader = LocalRepository::open_read_only(&h.file, h.repo.project())
        .await
        .unwrap();
    let third = reader
        .horizon_plan(&policy, &PlanOptions::default())
        .await
        .unwrap();
    assert_eq!(
        first.canonical_bytes().unwrap(),
        third.canonical_bytes().unwrap()
    );
    assert!(snapshot() == before);
    reader.close().await;
    observer.close().await.unwrap();
    // The writable handle can still write after being planned through.
    h.repo
        .write_state(h.report, &paragraphs("after", &[]))
        .await
        .unwrap();
    // Reopening, as another process would, yields the same plan.
    let file = h.file.clone();
    let project = h.repo.project();
    h.repo.close().await;
    let reopened = LocalRepository::open(&file, project).await.unwrap();
    let fourth = reopened
        .horizon_plan(&policy, &PlanOptions::default())
        .await
        .unwrap();
    // A staged object changed the database but not what the horizon concerns.
    assert_eq!(first.plan_hash, fourth.plan_hash);
    cleanup(reopened, &file).await;
}

#[tokio::test]
async fn a_plan_follows_the_repository_when_a_branch_moves() {
    let h = history().await;
    let policy = pin(&h, "history");
    let before = h
        .repo
        .horizon_plan(&policy, &PlanOptions::default())
        .await
        .unwrap();
    // Moving `feature` to n4 changes the heads, so it changes the plan hash.
    let head = h.repo.head("feature").await.unwrap();
    h.repo
        .compare_and_swap("feature", head.as_ref(), &h.id("n4"))
        .await
        .unwrap();
    let after = h
        .repo
        .horizon_plan(&policy, &PlanOptions::default())
        .await
        .unwrap();
    assert_ne!(before.plan_hash, after.plan_hash);
    assert_ne!(before.heads_digest, after.heads_digest);
    // And a time rule without a reference instant is refused, not read from a clock.
    let timed = policy_json_days();
    assert!(matches!(
        h.repo.horizon_plan(&timed, &PlanOptions::default()).await,
        Err(HorizonError::Plan(_))
    ));
    let planned = h
        .repo
        .horizon_plan(
            &timed,
            &PlanOptions {
                as_of_ms: Some(1_700_000_000_000),
                list: 1,
            },
        )
        .await
        .unwrap();
    // Every commit here was authored at one instant, so all are recent.
    assert_eq!(planned.commits.released, 0);
    assert_eq!(planned.as_of_ms, Some(1_700_000_000_000));
    cleanup(h.repo, &h.file).await;
}

fn policy_json_days() -> Policy {
    policy(
        r#"{"version":1,"id":"days","tier":"history","window":{"keep_within_days":30},"protect":{"branch_heads":true}}"#,
    )
}

#[tokio::test]
async fn an_empty_repository_and_a_repository_without_branches_plan_without_error() {
    let file = path();
    let repo = LocalRepository::open(&file, uuid::Uuid::new_v4())
        .await
        .unwrap();
    let floor = policy(
        r#"{"version":1,"id":"floor","tier":"history","window":{"min_commits_kept":1},"protect":{"branch_heads":true}}"#,
    );
    let empty = repo
        .horizon_plan(&floor, &PlanOptions::default())
        .await
        .unwrap();
    assert_eq!(empty.commits.total, 0);
    assert!(empty.hash_is_valid() && empty.heads.is_empty());
    // Commits without any branch: nothing is a head, so a history policy releases them all.
    let a = repo
        .import_commit(&signed(repo.project(), vec![], BTreeMap::new(), "a"))
        .await
        .unwrap();
    let b = repo
        .import_commit(&signed(
            repo.project(),
            vec![a.clone()],
            BTreeMap::new(),
            "b",
        ))
        .await
        .unwrap();
    let plan = repo
        .horizon_plan(&floor, &PlanOptions::default())
        .await
        .unwrap();
    assert_eq!(plan.sets.released.len(), 2);
    assert_eq!(plan.sets.skeleton.len(), 2);
    assert!(plan.sets.retained.is_empty());
    let b_row = plan.sets.skeleton.iter().find(|e| e.address == b).unwrap();
    assert_eq!((b_row.parents.clone(), b_row.height), (vec![a], 1));
    cleanup(repo, &file).await;
}

#[tokio::test]
async fn corrupt_or_missing_content_fails_closed() {
    let h = history().await;
    // An owner-level edit is detected, not planned around: drop the guard in this
    // disposable database and remove an object a commit needs.
    sqlx::query("drop trigger objects_no_delete")
        .execute(&h.repo.pool)
        .await
        .unwrap();
    let head = h.repo.commit(&h.id("n5")).await.unwrap();
    let root = &head.commit.files["Report.docx"].state;
    sqlx::query("delete from objects where address=?")
        .bind(root.as_str())
        .execute(&h.repo.pool)
        .await
        .unwrap();
    let result = h
        .repo
        .horizon_plan(&pin(&h, "history"), &PlanOptions::default())
        .await;
    assert!(
        matches!(&result, Err(HorizonError::Repository(message)) if message.contains("missing")),
        "{result:?}"
    );
    cleanup(h.repo, &h.file).await;
}

#[tokio::test]
async fn a_schema_one_repository_is_planned_without_being_upgraded() {
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
    let (root, stats) = typed::write_state(
        &mut Store {
            connection: &mut tx,
            read_budget: None,
            packed: false,
        },
        document,
        &super::tests::state("schema one"),
        Limits::default(),
        Default::default(),
    )
    .await
    .unwrap();
    let commit = signed(
        project,
        vec![],
        BTreeMap::from([(
            "a".into(),
            FileEntry {
                document,
                state: root,
            },
        )]),
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
    let floor =
        policy(r#"{"version":1,"id":"floor","tier":"content","window":{"min_commits_kept":1}}"#);
    let plan: HorizonPlan = reader
        .horizon_plan(&floor, &PlanOptions::default())
        .await
        .unwrap();
    assert_eq!(plan.commits.total, 1);
    assert_eq!(plan.counts.states_released, 0);
    assert!(stats.new_objects > 0);
    let upgraded: i64 =
        sqlx::query_scalar("select count(*) from sqlite_master where name='local_storage'")
            .fetch_one(&reader.pool)
            .await
            .unwrap();
    assert_eq!(upgraded, 0, "planning must not upgrade the schema");
    cleanup(reader, &file).await;
}
