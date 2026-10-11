//! Local synthetic authority/client demonstration. Fixture signing keys are
//! publicly known. No production accounts, remote service or external witness.
use engine_core::repository::{
    attestation::{Receipt, Scope, SignedReceipt, SignedRotation},
    local::LocalRepository,
    receipt_journal::ReceiptJournal,
    Commit, SignedCommit,
};
use ring::signature::{Ed25519KeyPair, KeyPair};
use std::{collections::BTreeMap, path::PathBuf};

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    let directory = PathBuf::from(
        std::env::args()
            .nth(1)
            .ok_or_else(|| anyhow::anyhow!("supply a new output directory"))?,
    );
    std::fs::create_dir(&directory)?;
    let project = uuid::Uuid::new_v4();
    let scope = Scope {
        authority: uuid::Uuid::new_v4(),
        project,
        branch: "main".into(),
    };
    let actor = Ed25519KeyPair::from_seed_unchecked(&[7; 32]).unwrap();
    let authority = Ed25519KeyPair::from_seed_unchecked(&[8; 32]).unwrap();
    let replacement = Ed25519KeyPair::from_seed_unchecked(&[9; 32]).unwrap();
    let trusted: String = authority
        .public_key()
        .as_ref()
        .iter()
        .map(|b| format!("{b:02x}"))
        .collect();
    let repo = LocalRepository::open(&directory.join("project.sqlite"), project).await?;
    let journal_path = directory.join("client-trust.sqlite");
    let journal = ReceiptJournal::open(&journal_path).await?;
    let commit = |parents, message: &str| {
        SignedCommit::sign(
            Commit {
                version: 1,
                project,
                parents,
                files: BTreeMap::new(),
                author_key: String::new(),
                authored_at_ms: 1_700_000_000_000,
                message: message.into(),
                recovery: None,
            },
            &actor,
        )
    };
    let initial = repo
        .import_commit(&commit(vec![], "Synthetic initial project")?)
        .await?;
    let head = repo.compare_and_swap("main", None, &initial).await?;
    let first = SignedReceipt::sign(
        Receipt {
            version: 1,
            scope: scope.clone(),
            head: head.clone(),
            previous: None,
            issued_at_ms: 1_700_000_000_000,
            issuer_key: String::new(),
        },
        &authority,
    )?;
    journal
        .trust_checkpoint(&scope, &trusted, first.clone())
        .await?;
    let changed = repo
        .import_commit(&commit(vec![initial.clone()], "Synthetic next commit")?)
        .await?;
    let head = repo.compare_and_swap("main", Some(&head), &changed).await?;
    let second = SignedReceipt::sign(
        Receipt {
            head: head.clone(),
            previous: Some(first.verify()?),
            ..first.receipt.clone()
        },
        &authority,
    )?;
    let pin = journal
        .advance(&scope, std::slice::from_ref(&second))
        .await?;
    anyhow::ensure!(
        journal
            .advance(&scope, &[first.clone(), second.clone()])
            .await?
            == pin,
        "retry changed pin"
    );
    let mut fork = second.receipt.clone();
    fork.head.commit = initial;
    let fork = SignedReceipt::sign(fork, &authority)?;
    anyhow::ensure!(
        journal.advance(&scope, &[fork]).await.is_err(),
        "fork accepted"
    );
    anyhow::ensure!(
        journal.advance(&scope, &[first]).await.is_err(),
        "rollback accepted"
    );
    let transition = SignedRotation::sign(&pin, &authority, &replacement)?;
    let head = repo.compare_and_swap("main", Some(&head), &changed).await?;
    let third = SignedReceipt::sign(
        Receipt {
            head: head.clone(),
            previous: Some(second.verify()?),
            ..second.receipt.clone()
        },
        &replacement,
    )?;
    let rotated = journal.rotate(&scope, &transition, &third).await?;
    journal.close().await;
    let reopened = ReceiptJournal::open(&journal_path).await?;
    anyhow::ensure!(
        reopened.pin(&scope).await? == Some(rotated),
        "rotation lost after reopen"
    );
    reopened.revoke(&scope).await?;
    anyhow::ensure!(
        reopened.rotate(&scope, &transition, &third).await.is_err(),
        "revocation undone by retry"
    );
    let report = serde_json::json!({
        "fixture":"local synthetic authority and client; not production/witness/host acceptance",
        "scope":scope, "generation":head.generation, "commit":head.commit,
        "initial_public_key":trusted, "fork_rejected":true, "rollback_rejected":true,
        "proof_retry_idempotent":true, "rotation_survives_reopen":true,
        "revocation_rejects_rotation_retry":true,
    });
    std::fs::write(
        directory.join("last-receipt.json"),
        serde_json::to_vec_pretty(&third)?,
    )?;
    std::fs::write(
        directory.join("report.json"),
        serde_json::to_vec_pretty(&report)?,
    )?;
    println!("{}", serde_json::to_string_pretty(&report)?);
    reopened.close().await;
    repo.close().await;
    Ok(())
}
