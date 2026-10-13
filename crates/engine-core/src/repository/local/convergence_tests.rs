use super::tests::{cleanup, path};
use super::*;
use crate::{
    materializer::MaterializedNode,
    repository::{
        compare,
        convergence::{state_address, MAX_REPORTED_FILES},
        tests::signed,
        FileEntry,
    },
};
use engine_shared::NodeId;
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

struct Trio {
    base: SignedCommit,
    left: SignedCommit,
    right: SignedCommit,
}

/// Base, left and right commits that each keep `Report.docx` under one document.
async fn trio(
    repo: &LocalRepository,
    document: DocumentId,
    left: &[(usize, &str)],
    right: &[(usize, &str)],
) -> Trio {
    let entry = |root: Address| FileEntry {
        document,
        state: root,
    };
    let (b, _) = repo.write_state(document, &paragraphs(&[])).await.unwrap();
    let (l, _) = repo.write_state(document, &paragraphs(left)).await.unwrap();
    let (r, _) = repo
        .write_state(document, &paragraphs(right))
        .await
        .unwrap();
    let project = repo.project();
    let make = |parents: Vec<Address>, root: Address, message: &str| {
        signed(
            project,
            parents,
            BTreeMap::from([("Report.docx".into(), entry(root))]),
            message,
        )
    };
    let base = make(vec![], b, "base");
    let base_id = repo.import_commit(&base).await.unwrap();
    let left = make(vec![base_id.clone()], l, "left");
    let right = make(vec![base_id], r, "right");
    repo.import_commit(&left).await.unwrap();
    repo.import_commit(&right).await.unwrap();
    Trio { base, left, right }
}

#[tokio::test]
async fn report_predicts_the_stored_address_and_writes_nothing() {
    let file = path();
    let repo = LocalRepository::open(&file, uuid::Uuid::new_v4())
        .await
        .unwrap();
    let document = DocumentId(uuid::Uuid::new_v4());
    let t = trio(&repo, document, &[(1, "left edit")], &[(4, "right edit")]).await;
    let plan = compare::plan(&t.base.commit, &t.left.commit, &t.right.commit).unwrap();
    // Path-level planning alone sees one whole-file conflict.
    assert_eq!(plan.conflicts.len(), 1);
    assert!(plan.files.is_empty());
    let usage = repo.storage_usage().await.unwrap();
    let objects = repo.inventory(None, 256).await.unwrap();
    let report = repo.same_file_convergence(&plan).await.unwrap().unwrap();
    assert_eq!(repo.storage_usage().await.unwrap(), usage);
    assert_eq!(repo.inventory(None, 256).await.unwrap(), objects);
    assert_eq!(report.not_examined, 0);
    let file_report = &report.files[0];
    assert_eq!(file_report.path, "Report.docx");
    assert!(file_report.clean && file_report.conflicts.is_empty());
    let expected = paragraphs(&[(1, "left edit"), (4, "right edit")]);
    let predicted = file_report.merged_state.clone().unwrap();
    assert_eq!(predicted, state_address(document, &expected).await.unwrap());
    // Writing the merged state later produces exactly the predicted root.
    let (stored, _) = repo.write_state(document, &expected).await.unwrap();
    assert_eq!(stored, predicted);
    // Left and right swapped: same address, still clean.
    let swapped = compare::plan(&t.base.commit, &t.right.commit, &t.left.commit).unwrap();
    let mirror = repo.same_file_convergence(&swapped).await.unwrap().unwrap();
    assert_eq!(mirror.files[0].merged_state, file_report.merged_state);
    assert_eq!(mirror.files[0].left, file_report.right);
    assert_eq!(mirror.files[0].right, file_report.left);
    cleanup(repo, &file).await;
}

#[tokio::test]
async fn overlapping_edits_list_stable_node_conflicts_and_no_address() {
    let file = path();
    let repo = LocalRepository::open(&file, uuid::Uuid::new_v4())
        .await
        .unwrap();
    let document = DocumentId(uuid::Uuid::new_v4());
    let t = trio(
        &repo,
        document,
        &[(2, "left wording"), (0, "left only")],
        &[(2, "right wording")],
    )
    .await;
    let plan = compare::plan(&t.base.commit, &t.left.commit, &t.right.commit).unwrap();
    let report = repo.same_file_convergence(&plan).await.unwrap().unwrap();
    let file_report = &report.files[0];
    assert!(!file_report.clean);
    assert_eq!(file_report.merged_state, None);
    assert_eq!(file_report.conflicts.len(), 1);
    let conflict = &file_report.conflicts[0];
    assert_eq!(conflict.node, Some(NodeId(format!("{:026}", 2))));
    assert_eq!(conflict.field, "label");
    assert_eq!(conflict.base, "paragraph 2");
    assert_eq!(conflict.left, "left wording");
    assert_eq!(conflict.right, "right wording");
    let swapped = compare::plan(&t.base.commit, &t.right.commit, &t.left.commit).unwrap();
    let mirror = repo.same_file_convergence(&swapped).await.unwrap().unwrap();
    let mirrored = &mirror.files[0].conflicts[0];
    assert_eq!(mirrored.node, conflict.node);
    assert_eq!(mirrored.left, conflict.right);
    assert_eq!(mirrored.right, conflict.left);
    cleanup(repo, &file).await;
}

#[tokio::test]
async fn only_conflicts_that_keep_one_document_identity_are_examined() {
    let file = path();
    let repo = LocalRepository::open(&file, uuid::Uuid::new_v4())
        .await
        .unwrap();
    let project = repo.project();
    let document = DocumentId(uuid::Uuid::new_v4());
    let other = DocumentId(uuid::Uuid::new_v4());
    let (b, _) = repo.write_state(document, &paragraphs(&[])).await.unwrap();
    let (l, _) = repo
        .write_state(document, &paragraphs(&[(0, "left")]))
        .await
        .unwrap();
    let (r, _) = repo.write_state(other, &paragraphs(&[])).await.unwrap();
    let file_of = |document, state| FileEntry { document, state };
    let base = signed(
        project,
        vec![],
        BTreeMap::from([("a".into(), file_of(document, b))]),
        "base",
    );
    let base_id = repo.import_commit(&base).await.unwrap();
    let left = signed(
        project,
        vec![base_id.clone()],
        BTreeMap::from([("a".into(), file_of(document, l))]),
        "left",
    );
    // Same path, but the right side replaced it with a different document.
    let replaced = signed(
        project,
        vec![base_id.clone()],
        BTreeMap::from([("a".into(), file_of(other, r))]),
        "replaced",
    );
    // Right deleted the file while left edited it.
    let deleted = signed(project, vec![base_id], BTreeMap::new(), "deleted");
    for right in [&replaced, &deleted] {
        repo.import_commit(&left).await.unwrap();
        repo.import_commit(right).await.unwrap();
        let plan = compare::plan(&base.commit, &left.commit, &right.commit).unwrap();
        assert_eq!(plan.conflicts.len(), 1);
        assert!(repo.same_file_convergence(&plan).await.unwrap().is_none());
    }
    // No conflicts at all: nothing to report.
    let plan = compare::plan(&base.commit, &left.commit, &base.commit).unwrap();
    assert!(plan.conflicts.is_empty());
    assert!(repo.same_file_convergence(&plan).await.unwrap().is_none());
    cleanup(repo, &file).await;
}

#[tokio::test]
async fn examined_files_are_bounded_and_the_remainder_is_counted() {
    let file = path();
    let repo = LocalRepository::open(&file, uuid::Uuid::new_v4())
        .await
        .unwrap();
    let project = repo.project();
    let total = MAX_REPORTED_FILES + 2;
    let mut maps = [BTreeMap::new(), BTreeMap::new(), BTreeMap::new()];
    for i in 0..total {
        let document = DocumentId(uuid::Uuid::new_v4());
        for (map, edits) in maps
            .iter_mut()
            .zip([&[][..], &[(0, "left")][..], &[(0, "right")][..]])
        {
            let (root, _) = repo
                .write_state(document, &paragraphs(edits))
                .await
                .unwrap();
            map.insert(
                format!("file-{i:03}.docx"),
                FileEntry {
                    document,
                    state: root,
                },
            );
        }
    }
    let [base_files, left_files, right_files] = maps;
    let base = signed(project, vec![], base_files, "base");
    let base_id = repo.import_commit(&base).await.unwrap();
    let left = signed(project, vec![base_id.clone()], left_files, "left");
    let right = signed(project, vec![base_id], right_files, "right");
    repo.import_commit(&left).await.unwrap();
    repo.import_commit(&right).await.unwrap();
    let plan = compare::plan(&base.commit, &left.commit, &right.commit).unwrap();
    assert_eq!(plan.conflicts.len(), total);
    let report = repo.same_file_convergence(&plan).await.unwrap().unwrap();
    assert_eq!(report.files.len(), MAX_REPORTED_FILES);
    assert_eq!(report.not_examined, 2);
    assert!(report.files.iter().all(|f| !f.clean));
    cleanup(repo, &file).await;
}
