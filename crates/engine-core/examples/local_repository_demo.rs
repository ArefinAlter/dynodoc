//! Synthetic, entirely local two-replica acceptance. No accounts/native documents
//! or production signing keys. Writes a fresh directory supplied by the caller.
use engine_core::{
    materializer::{DocumentState, MaterializedNode},
    repository::{self, local::LocalRepository, Commit, FileEntry, SignedCommit},
};
use engine_shared::{DocumentId, NodeId};
use ring::signature::Ed25519KeyPair;
use std::{collections::BTreeMap, path::PathBuf};

fn state(text: &str) -> DocumentState {
    let node = MaterializedNode {
        id: NodeId("01ARZ3NDEKTSV4RRFFQ69G5FAV".into()),
        parent_id: None,
        node_type: "paragraph".into(),
        pos: "a0".into(),
        var_name: None,
        deleted: false,
        current_fields: serde_json::json!({"label":text}),
    };
    DocumentState {
        nodes: BTreeMap::from([(node.id.clone(), node)]),
        ..Default::default()
    }
}
async fn transfer(
    source: &LocalRepository,
    target: &LocalRepository,
) -> anyhow::Result<(usize, usize)> {
    let mut cursor = None;
    let mut objects = 0;
    let mut bytes = 0;
    loop {
        let page = source.inventory(cursor.as_ref(), 64).await?;
        if page.is_empty() {
            break;
        }
        for key in target.missing(&page).await? {
            let packet = source.object(&key).await?;
            bytes += packet.bytes.len();
            objects += target.import_objects(&[packet]).await?;
        }
        cursor = page.last().cloned();
    }
    Ok((objects, bytes))
}
#[tokio::main]
async fn main() -> anyhow::Result<()> {
    let directory = PathBuf::from(
        std::env::args()
            .nth(1)
            .ok_or_else(|| anyhow::anyhow!("supply a new output directory"))?,
    );
    // create_dir intentionally rejects any existing directory.
    std::fs::create_dir(&directory)?;
    let project = uuid::Uuid::new_v4();
    let first = DocumentId(uuid::Uuid::new_v4());
    let second = DocumentId(uuid::Uuid::new_v4());
    let a_path = directory.join("alice.sqlite");
    let b_path = directory.join("bob.sqlite");
    let a = LocalRepository::open(&a_path, project).await?;
    let b = LocalRepository::open(&b_path, project).await?;
    // Publicly known fixture keys are ONLY for this synthetic example.
    let key_a = Ed25519KeyPair::from_seed_unchecked(&[7; 32]).unwrap();
    let key_b = Ed25519KeyPair::from_seed_unchecked(&[8; 32]).unwrap();
    let root_a = a.write_state(first, &state("Initial report")).await?.0;
    let root_b = a.write_state(second, &state("Initial budget")).await?.0;
    let initial = SignedCommit::sign(
        Commit {
            version: 1,
            project,
            parents: vec![],
            files: BTreeMap::from([
                (
                    "Report.docx".into(),
                    FileEntry {
                        document: first,
                        state: root_a.clone(),
                    },
                ),
                (
                    "Budget.xlsx".into(),
                    FileEntry {
                        document: second,
                        state: root_b,
                    },
                ),
            ]),
            author_key: String::new(),
            authored_at_ms: 1,
            message: "Initial two-file project".into(),
            recovery: None,
        },
        &key_a,
    )?;
    let initial_id = a.import_commit(&initial).await?;
    let initial_transfer = transfer(&a, &b).await?;
    b.import_commit(&initial).await?;
    let a_head = a.compare_and_swap("main", None, &initial_id).await?;
    let b_head = b.compare_and_swap("main", None, &initial_id).await?;
    let mut left = initial.commit.clone();
    left.parents = vec![initial_id.clone()];
    left.message = "Alice edits report offline".into();
    left.files.get_mut("Report.docx").unwrap().state =
        a.write_state(first, &state("Alice's report")).await?.0;
    let left = SignedCommit::sign(left, &key_a)?;
    let left_id = a.import_commit(&left).await?;
    let mut right = initial.commit.clone();
    right.parents = vec![initial_id.clone()];
    right.message = "Bob edits budget offline".into();
    right.files.get_mut("Budget.xlsx").unwrap().state =
        b.write_state(second, &state("Bob's budget")).await?.0;
    let right = SignedCommit::sign(right, &key_b)?;
    let right_id = b.import_commit(&right).await?;
    // Restart both devices before reconnecting. Commits and their content persist.
    a.close().await;
    b.close().await;
    let a = LocalRepository::open(&a_path, project).await?;
    let b = LocalRepository::open(&b_path, project).await?;
    let to_a = transfer(&b, &a).await?;
    let to_b = transfer(&a, &b).await?;
    a.import_commit(&right).await?;
    b.import_commit(&left).await?;
    anyhow::ensure!(a.merge_bases(&left_id, &right_id).await? == vec![initial_id.clone()]);
    let plan = repository::compare::plan(&initial.commit, &left.commit, &right.commit)?;
    anyhow::ensure!(plan.conflicts.is_empty());
    let reverse = repository::compare::plan(&initial.commit, &right.commit, &left.commit)?;
    anyhow::ensure!(plan.files == reverse.files);
    let merge = SignedCommit::sign(
        Commit {
            parents: vec![left_id, right_id],
            files: plan.files,
            message: "Reviewed project merge".into(),
            authored_at_ms: 2,
            ..initial.commit.clone()
        },
        &key_a,
    )?;
    let merged = a.import_commit(&merge).await?;
    b.import_commit(&merge).await?;
    let a_final = a.compare_and_swap("main", Some(&a_head), &merged).await?;
    let b_final = b.compare_and_swap("main", Some(&b_head), &merged).await?;
    anyhow::ensure!(a_final == b_final);
    anyhow::ensure!(a.read_state(first, &root_a).await? == state("Initial report"));
    let retry = transfer(&a, &b).await?;
    anyhow::ensure!(retry == (0, 0));
    a.close().await;
    b.close().await;
    println!(
        "{}",
        serde_json::to_string_pretty(&serde_json::json!({"project":project,"head":a_final,
        "initial_transfer":{"objects":initial_transfer.0,"bytes":initial_transfer.1},
        "alice_received":{"objects":to_a.0,"bytes":to_a.1},"bob_received":{"objects":to_b.0,"bytes":to_b.1},
        "retry_objects":retry.0,"old_revision_restored":true,"replica_heads_equal":true}))?
    );
    Ok(())
}
