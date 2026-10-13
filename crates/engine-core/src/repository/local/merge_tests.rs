use super::tests::{cleanup, path};
use super::*;
use crate::{
    materializer::MaterializedNode,
    repository::{
        compare,
        convergence::{state_address, STRATEGY},
        project_merge::{Merge, SameFile},
        recovery::Kind,
        tests::signed,
        Commit, FileEntry, SignedCommit,
    },
};
use engine_shared::NodeId;
use ring::signature::Ed25519KeyPair;
use serde_json::json;

fn paragraphs(edits: &[(usize, &str)]) -> DocumentState {
    let mut state = DocumentState::default();
    for i in 0..6 {
        let id = NodeId(format!("{i:026}"));
        let label = edits
            .iter()
            .find(|(index, _)| *index == i)
            .map_or(format!("paragraph {i}"), |(_, text)| (*text).to_string());
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

struct Setup {
    repo: LocalRepository,
    file: std::path::PathBuf,
    report: DocumentId,
    base: Address,
    left: Address,
    right: Address,
    base_commit: SignedCommit,
    left_commit: SignedCommit,
    right_commit: SignedCommit,
}

/// Base, left and right commits of Report.docx (one identity throughout, edited
/// on both sides) and Budget.xlsx (edited only on the right).
async fn setup(left_edits: &[(usize, &str)], right_edits: &[(usize, &str)]) -> Setup {
    let file = path();
    let repo = LocalRepository::open(&file, uuid::Uuid::new_v4())
        .await
        .unwrap();
    let (report, budget) = (
        DocumentId(uuid::Uuid::new_v4()),
        DocumentId(uuid::Uuid::new_v4()),
    );
    let mut roots = BTreeMap::new();
    for (name, document, edits) in [
        ("base-report", report, &[][..]),
        ("left-report", report, left_edits),
        ("right-report", report, right_edits),
        ("base-budget", budget, &[][..]),
        ("right-budget", budget, &[(1, "budget edited")][..]),
    ] {
        let (root, _) = repo
            .write_state(document, &paragraphs(edits))
            .await
            .unwrap();
        roots.insert(name, root);
    }
    let files = |report_root: &str, budget_root: &str| {
        BTreeMap::from([
            (
                "Report.docx".to_string(),
                FileEntry {
                    document: report,
                    state: roots[report_root].clone(),
                },
            ),
            (
                "Budget.xlsx".to_string(),
                FileEntry {
                    document: budget,
                    state: roots[budget_root].clone(),
                },
            ),
        ])
    };
    let project = repo.project();
    let base_commit = signed(project, vec![], files("base-report", "base-budget"), "base");
    let base = repo.import_commit(&base_commit).await.unwrap();
    let left_commit = signed(
        project,
        vec![base.clone()],
        files("left-report", "base-budget"),
        "left",
    );
    let right_commit = signed(
        project,
        vec![base.clone()],
        files("right-report", "right-budget"),
        "right",
    );
    let left = repo.import_commit(&left_commit).await.unwrap();
    let right = repo.import_commit(&right_commit).await.unwrap();
    Setup {
        repo,
        file,
        report,
        base,
        left,
        right,
        base_commit,
        left_commit,
        right_commit,
    }
}

async fn merge(
    s: &Setup,
    left: &Address,
    right: &Address,
    same_file: SameFile,
    resolutions: &BTreeMap<String, Option<FileEntry>>,
) -> Result<Merge, Error> {
    s.repo
        .prepare_merge(
            left,
            right,
            same_file,
            resolutions,
            1_700_000_000_500,
            "merge".into(),
        )
        .await
}

async fn object_count(repo: &LocalRepository) -> usize {
    let mut count = 0;
    let mut cursor = None;
    loop {
        let page = repo.inventory(cursor.as_ref(), 256).await.unwrap();
        let Some(last) = page.last().cloned() else {
            return count;
        };
        count += page.len();
        cursor = Some(last);
    }
}

#[tokio::test]
async fn clean_same_file_merge_is_signed_stored_and_verifiable() {
    let s = setup(&[(1, "left edit")], &[(4, "right edit")]).await;
    let merged = merge(&s, &s.left, &s.right, SameFile::Clean, &BTreeMap::new())
        .await
        .unwrap();
    assert_eq!(merged.base, s.base);
    assert!(merged.unresolved.is_empty() && merged.explicit.is_empty());
    let commit = merged.commit.unwrap();
    assert_eq!(commit.version, 3);
    assert_eq!(commit.parents, vec![s.left.clone(), s.right.clone()]);
    let expected = paragraphs(&[(1, "left edit"), (4, "right edit")]);
    let expected_root = state_address(s.report, &expected).await.unwrap();
    // The path-level result for the independent file is untouched.
    assert_eq!(
        commit.files["Budget.xlsx"],
        s.right_commit.commit.files["Budget.xlsx"]
    );
    assert_eq!(commit.files["Report.docx"].state, expected_root);
    assert_eq!(commit.files["Report.docx"].document, s.report);
    let [record] = commit.same_file_resolutions.as_slice() else {
        panic!("one record expected");
    };
    assert_eq!(record.path, "Report.docx");
    assert_eq!(record.document, s.report);
    assert_eq!(record.strategy, STRATEGY);
    assert_eq!(record.merged, expected_root);
    assert_eq!(
        (&record.base, &record.left, &record.right),
        (
            &s.base_commit.commit.files["Report.docx"].state,
            &s.left_commit.commit.files["Report.docx"].state,
            &s.right_commit.commit.files["Report.docx"].state,
        )
    );
    // The merged state was stored before the commit; sign, import, verify.
    assert_eq!(
        s.repo.read_state(s.report, &expected_root).await.unwrap(),
        expected
    );
    let signed_merge = SignedCommit::sign(commit, &key()).unwrap();
    let id = s.repo.import_commit(&signed_merge).await.unwrap();
    assert_eq!(signed_merge.verify().unwrap(), id);
    assert_eq!(s.repo.verify_same_file_resolutions(&id).await.unwrap(), 1);
    // Ancestry stays the two parents; the base is still their unique best ancestor.
    assert_eq!(
        s.repo.merge_bases(&s.left, &s.right).await.unwrap(),
        vec![s.base.clone()]
    );
    // An ordinary two-parent commit has nothing to verify.
    assert_eq!(
        s.repo.verify_same_file_resolutions(&s.left).await.unwrap(),
        0
    );
    cleanup(s.repo, &s.file).await;
}

#[tokio::test]
async fn swapping_left_and_right_gives_the_same_merged_state_address() {
    let s = setup(
        &[(1, "left edit"), (3, "left only")],
        &[(4, "right edit"), (3, "left only")],
    )
    .await;
    let forward = merge(&s, &s.left, &s.right, SameFile::Clean, &BTreeMap::new())
        .await
        .unwrap();
    let reverse = merge(&s, &s.right, &s.left, SameFile::Clean, &BTreeMap::new())
        .await
        .unwrap();
    let (forward, reverse) = (forward.commit.unwrap(), reverse.commit.unwrap());
    assert_eq!(forward.parents, vec![s.left.clone(), s.right.clone()]);
    assert_eq!(reverse.parents, vec![s.right.clone(), s.left.clone()]);
    assert_eq!(forward.files, reverse.files);
    let (f, r) = (
        &forward.same_file_resolutions[0],
        &reverse.same_file_resolutions[0],
    );
    assert_eq!(f.merged, r.merged);
    // Only the side labels follow the parent order.
    assert_eq!((&f.left, &f.right), (&r.right, &r.left));
    // Both are importable and verifiable; they are two different commits.
    let a = s
        .repo
        .import_commit(&SignedCommit::sign(forward, &key()).unwrap())
        .await
        .unwrap();
    let b = s
        .repo
        .import_commit(&SignedCommit::sign(reverse, &key()).unwrap())
        .await
        .unwrap();
    assert_ne!(a, b);
    assert_eq!(s.repo.verify_same_file_resolutions(&a).await.unwrap(), 1);
    assert_eq!(s.repo.verify_same_file_resolutions(&b).await.unwrap(), 1);
    cleanup(s.repo, &s.file).await;
}

#[tokio::test]
async fn conflicting_files_stay_explicit_even_with_the_flag() {
    let s = setup(
        &[(2, "left wording"), (0, "left only")],
        &[(2, "right wording")],
    )
    .await;
    let before = object_count(&s.repo).await;
    for same_file in [SameFile::Off, SameFile::Clean] {
        let refused = merge(&s, &s.left, &s.right, same_file, &BTreeMap::new())
            .await
            .unwrap();
        assert_eq!(refused.unresolved, vec!["Report.docx".to_string()]);
        assert!(refused.commit.is_none() && refused.applied.is_empty());
        // Nothing was stored while a conflict is open.
        assert_eq!(object_count(&s.repo).await, before);
    }
    // An explicit resolution is accepted exactly as before and carries no record.
    let choice = s.left_commit.commit.files["Report.docx"].clone();
    let resolutions = BTreeMap::from([("Report.docx".to_string(), Some(choice.clone()))]);
    let resolved = merge(&s, &s.left, &s.right, SameFile::Clean, &resolutions)
        .await
        .unwrap();
    assert_eq!(resolved.explicit, vec!["Report.docx".to_string()]);
    let commit = resolved.commit.unwrap();
    assert_eq!(commit.version, 1);
    assert!(commit.same_file_resolutions.is_empty());
    assert_eq!(commit.files["Report.docx"], choice);
    // Deleting through an explicit `None` also works; unknown paths are refused.
    let deleted = BTreeMap::from([("Report.docx".to_string(), None)]);
    let commit = merge(&s, &s.left, &s.right, SameFile::Off, &deleted)
        .await
        .unwrap()
        .commit
        .unwrap();
    assert!(!commit.files.contains_key("Report.docx"));
    let unknown = BTreeMap::from([("Budget.xlsx".to_string(), None)]);
    assert!(merge(&s, &s.left, &s.right, SameFile::Off, &unknown)
        .await
        .is_err());
    cleanup(s.repo, &s.file).await;
}

#[tokio::test]
async fn without_the_flag_the_commit_is_the_manual_manifest_and_explicit_choices_win() {
    let s = setup(&[(1, "left edit")], &[(4, "right edit")]).await;
    // Off: the clean-mergeable path is still a conflict needing a choice.
    let off = merge(&s, &s.left, &s.right, SameFile::Off, &BTreeMap::new())
        .await
        .unwrap();
    assert_eq!(off.unresolved, vec!["Report.docx".to_string()]);
    // With a choice the result is byte-identical to composing the manifest by hand.
    let choice = s.right_commit.commit.files["Report.docx"].clone();
    let resolutions = BTreeMap::from([("Report.docx".to_string(), Some(choice.clone()))]);
    let auto = merge(&s, &s.left, &s.right, SameFile::Off, &resolutions)
        .await
        .unwrap();
    let plan = compare::plan(
        &s.base_commit.commit,
        &s.left_commit.commit,
        &s.right_commit.commit,
    )
    .unwrap();
    let mut files = plan.files;
    files.insert("Report.docx".into(), choice);
    let manual = Commit {
        version: 1,
        project: s.repo.project(),
        parents: vec![s.left.clone(), s.right.clone()],
        files,
        author_key: String::new(),
        authored_at_ms: 1_700_000_000_500,
        message: "merge".into(),
        recovery: None,
        same_file_resolutions: Vec::new(),
    };
    let signed_auto = SignedCommit::sign(auto.commit.unwrap(), &key()).unwrap();
    let signed_manual = SignedCommit::sign(manual, &key()).unwrap();
    assert_eq!(
        serde_json::to_vec(&signed_auto).unwrap(),
        serde_json::to_vec(&signed_manual).unwrap()
    );
    // With the flag, an explicit choice for a mergeable path still wins.
    let stored = object_count(&s.repo).await;
    let clean = merge(&s, &s.left, &s.right, SameFile::Clean, &resolutions)
        .await
        .unwrap();
    let commit = clean.commit.unwrap();
    assert_eq!(commit.version, 1);
    assert!(clean.applied.is_empty());
    assert_eq!(object_count(&s.repo).await, stored);
    cleanup(s.repo, &s.file).await;
}

#[tokio::test]
async fn a_merge_commit_imports_into_a_second_repository() {
    let s = setup(&[(1, "left edit")], &[(4, "right edit")]).await;
    let commit = merge(&s, &s.left, &s.right, SameFile::Clean, &BTreeMap::new())
        .await
        .unwrap()
        .commit
        .unwrap();
    let signed_merge = SignedCommit::sign(commit, &key()).unwrap();
    let id = s.repo.import_commit(&signed_merge).await.unwrap();

    let other_file = path();
    let other = LocalRepository::open(&other_file, s.repo.project())
        .await
        .unwrap();
    // The merge cannot arrive before its parents or content.
    assert!(other.import_commit(&signed_merge).await.is_err());
    let mut cursor = None;
    loop {
        let page = s.repo.inventory(cursor.as_ref(), 64).await.unwrap();
        let Some(last) = page.last().cloned() else {
            break;
        };
        for key in other.missing(&page).await.unwrap() {
            let packet = s.repo.object(&key).await.unwrap();
            other.import_objects(&[packet]).await.unwrap();
        }
        cursor = Some(last);
    }
    // Parents must arrive first: a commit whose parent is missing is refused.
    other.import_commit(&s.left_commit).await.unwrap_err();
    other.import_commit(&s.base_commit).await.unwrap();
    other.import_commit(&s.left_commit).await.unwrap();
    other.import_commit(&s.right_commit).await.unwrap();
    assert_eq!(other.import_commit(&signed_merge).await.unwrap(), id);
    assert_eq!(other.verify_same_file_resolutions(&id).await.unwrap(), 1);
    let head = other.compare_and_swap("main", None, &id).await.unwrap();
    assert_eq!(head.commit, id);
    let file = &other.commit(&id).await.unwrap().commit.files["Report.docx"];
    assert_eq!(
        other.read_state(file.document, &file.state).await.unwrap(),
        paragraphs(&[(1, "left edit"), (4, "right edit")])
    );
    cleanup(other, &other_file).await;
    cleanup(s.repo, &s.file).await;
}

#[tokio::test]
async fn provenance_that_does_not_describe_the_real_parents_is_rejected() {
    let s = setup(&[(1, "left edit")], &[(4, "right edit")]).await;
    let honest = merge(&s, &s.left, &s.right, SameFile::Clean, &BTreeMap::new())
        .await
        .unwrap()
        .commit
        .unwrap();
    let unrelated = s
        .repo
        .write_state(s.report, &paragraphs(&[(5, "unrelated")]))
        .await
        .unwrap()
        .0;
    let forged = |f: &dyn Fn(&mut Commit)| {
        let mut commit = honest.clone();
        f(&mut commit);
        SignedCommit::sign(commit, &key()).unwrap()
    };
    let cases = [
        // A base that is not the unique merge base's state.
        forged(&|c| c.same_file_resolutions[0].base = unrelated.clone()),
        // Inputs that are not the parents' states.
        forged(&|c| c.same_file_resolutions[0].left = unrelated.clone()),
        forged(&|c| c.same_file_resolutions[0].right = unrelated.clone()),
        // Parents in the other order than the record claims.
        forged(&|c| c.parents.reverse()),
        // A record for a path that no parent has.
        forged(&|c| {
            let file = c.files.remove("Report.docx").unwrap();
            c.files.insert("Renamed.docx".into(), file);
            c.same_file_resolutions[0].path = "Renamed.docx".into();
        }),
    ];
    for case in cases {
        assert!(s.repo.import_commit(&case).await.is_err());
    }
    // Importing checks provenance against the graph; re-running the strategy is a
    // separate verification. A well-formed record for a hand-picked state imports
    // but does not reproduce.
    let mut handpicked = honest.clone();
    handpicked.same_file_resolutions[0].merged = unrelated.clone();
    handpicked.files.get_mut("Report.docx").unwrap().state = unrelated;
    let handpicked = s
        .repo
        .import_commit(&SignedCommit::sign(handpicked, &key()).unwrap())
        .await
        .unwrap();
    assert!(s
        .repo
        .verify_same_file_resolutions(&handpicked)
        .await
        .is_err());
    // The honest commit is unaffected by any of that.
    let honest = s
        .repo
        .import_commit(&SignedCommit::sign(honest, &key()).unwrap())
        .await
        .unwrap();
    assert_eq!(
        s.repo.verify_same_file_resolutions(&honest).await.unwrap(),
        1
    );
    cleanup(s.repo, &s.file).await;
}

#[tokio::test]
async fn reverting_the_merge_restores_either_parent_and_keeps_both_histories() {
    let s = setup(&[(1, "left edit")], &[(4, "right edit")]).await;
    let commit = merge(&s, &s.left, &s.right, SameFile::Clean, &BTreeMap::new())
        .await
        .unwrap()
        .commit
        .unwrap();
    let id = s
        .repo
        .import_commit(&SignedCommit::sign(commit, &key()).unwrap())
        .await
        .unwrap();
    for (mainline, parent) in [(&s.left, &s.left_commit), (&s.right, &s.right_commit)] {
        let plan = s
            .repo
            .plan_recovery(Kind::Revert, &id, &id, Some(mainline))
            .await
            .unwrap();
        assert!(plan.changes.conflicts.is_empty());
        let reverted = SignedCommit::sign(
            plan.resolve(&BTreeMap::new(), 1_700_000_001_000, "Revert merge".into())
                .unwrap(),
            &key(),
        )
        .unwrap();
        // The files are exactly the chosen parent's: its Report.docx state and
        // its Budget.xlsx state, not the merged ones.
        assert_eq!(reverted.commit.files, parent.commit.files);
        assert_eq!(reverted.commit.parents, vec![id.clone()]);
        let reverted_id = s.repo.import_commit(&reverted).await.unwrap();
        for file in s
            .repo
            .commit(&reverted_id)
            .await
            .unwrap()
            .commit
            .files
            .values()
        {
            s.repo.read_state(file.document, &file.state).await.unwrap();
        }
    }
    // The merge, its parents and the reverts are all still readable.
    for address in [&id, &s.left, &s.right, &s.base] {
        s.repo.commit(address).await.unwrap();
    }
    let expected = state_address(
        s.report,
        &paragraphs(&[(1, "left edit"), (4, "right edit")]),
    )
    .await
    .unwrap();
    assert_eq!(
        s.repo.commit(&id).await.unwrap().commit.files["Report.docx"].state,
        expected
    );
    cleanup(s.repo, &s.file).await;
}

#[tokio::test]
async fn criss_cross_disconnected_and_identical_parents_are_refused() {
    let file = path();
    let repo = LocalRepository::open(&file, uuid::Uuid::new_v4())
        .await
        .unwrap();
    let project = repo.project();
    let import = |parents: Vec<Address>, message: &'static str| {
        let repo = repo.clone();
        async move {
            repo.import_commit(&signed(project, parents, BTreeMap::new(), message))
                .await
                .unwrap()
        }
    };
    let a = import(vec![], "a").await;
    let b = import(vec![a.clone()], "b").await;
    let c = import(vec![a.clone()], "c").await;
    let d = import(vec![b.clone(), c.clone()], "d").await;
    let e = import(vec![c.clone(), b.clone()], "e").await;
    let unrelated = import(vec![], "unrelated").await;
    for (left, right) in [(&d, &e), (&a, &unrelated), (&b, &b)] {
        for same_file in [SameFile::Off, SameFile::Clean] {
            let result = repo
                .prepare_merge(left, right, same_file, &BTreeMap::new(), 1, "x".into())
                .await;
            assert!(matches!(result, Err(Error::Invalid(_))), "{result:?}");
        }
    }
    // A unique base still merges (an empty project merges trivially).
    assert!(repo
        .prepare_merge(&b, &c, SameFile::Clean, &BTreeMap::new(), 1, "ok".into())
        .await
        .unwrap()
        .commit
        .is_some());
    cleanup(repo, &file).await;
}
