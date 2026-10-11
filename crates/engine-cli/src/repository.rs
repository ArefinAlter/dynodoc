//! Explicit local repository commands. These never contact a remote server or
//! apply content to a native editor. Signing keys are supplied by the operator.
use anyhow::Context;
use clap::Subcommand;
use engine_core::{
    repository::{local::LocalRepository, Commit, Head, SignedCommit},
    shared_checkpoint::Address,
};
use engine_shared::DocumentId;
use ring::signature::Ed25519KeyPair;
use std::{
    io::Read,
    path::{Path, PathBuf},
};

#[derive(Subcommand)]
pub enum Action {
    /// Initialize or verify the local project database.
    Init,
    /// Store a DocumentState JSON graph; returns its typed root without publishing.
    StoreState {
        document: uuid::Uuid,
        input: PathBuf,
    },
    /// Reconstruct an old or current state root as JSON.
    ReadState { document: uuid::Uuid, root: String },
    /// Sign and store a Commit JSON manifest with an existing Ed25519 PKCS#8 key.
    Commit { manifest: PathBuf, key: PathBuf },
    /// Verify/import a signed commit after its parents and objects are available.
    ImportCommit { input: PathBuf },
    /// Print a verified signed commit for review or transfer.
    Show { commit: String },
    /// Read one project branch. Missing branches print null.
    Head { branch: String },
    /// Publish a project commit using an exact expected address AND generation.
    SetHead {
        branch: String,
        commit: String,
        #[arg(long, requires = "expected_generation")]
        expected: Option<String>,
        #[arg(long, requires = "expected")]
        expected_generation: Option<i64>,
    },
    /// Return all best common ancestors of two commits.
    MergeBases { left: String, right: String },
    /// Compare three stored project commits without changing a branch or file.
    Compare {
        base: String,
        left: String,
        right: String,
    },
}

fn read_json<T: serde::de::DeserializeOwned>(path: &Path, limit: usize) -> anyhow::Result<T> {
    let mut bytes = Vec::new();
    std::fs::File::open(path)?
        .take(limit as u64 + 1)
        .read_to_end(&mut bytes)?;
    anyhow::ensure!(
        bytes.len() <= limit,
        "input exceeds the local command's byte limit"
    );
    Ok(serde_json::from_slice(&bytes)?)
}

pub async fn run(path: PathBuf, project: uuid::Uuid, action: Action) -> anyhow::Result<()> {
    let repo = LocalRepository::open(&path, project).await?;
    let address = |s: String| Address::try_from(s);
    let output = match action {
        Action::Init => serde_json::json!({"project":project,"local_schema":1}),
        Action::StoreState { document, input } => {
            let state = read_json(&input, 16 * 1024 * 1024)?;
            let (root, stats) = repo.write_state(DocumentId(document), &state).await?;
            serde_json::json!({"state":root,"new_objects":stats.new_objects,"new_bytes":stats.new_bytes})
        }
        Action::ReadState { document, root } => serde_json::to_value(
            repo.read_state(DocumentId(document), &address(root)?)
                .await?,
        )?,
        Action::Commit { manifest, key } => {
            let commit: Commit = read_json(&manifest, engine_core::repository::MAX_COMMIT_BYTES)?;
            let mut bytes = Vec::new();
            std::fs::File::open(key)?
                .take(4097)
                .read_to_end(&mut bytes)?;
            anyhow::ensure!(bytes.len() <= 4096, "signing key file too large");
            let key = Ed25519KeyPair::from_pkcs8(&bytes)
                .map_err(|_| anyhow::anyhow!("invalid Ed25519 PKCS#8 key"))?;
            let signed = SignedCommit::sign(commit, &key)?;
            serde_json::json!({"commit":repo.import_commit(&signed).await?})
        }
        Action::ImportCommit { input } => {
            serde_json::json!({"commit":repo.import_commit(&read_json(&input, engine_core::repository::MAX_COMMIT_BYTES + 1024)?).await?})
        }
        Action::Show { commit } => serde_json::to_value(repo.commit(&address(commit)?).await?)?,
        Action::Head { branch } => serde_json::to_value(repo.head(&branch).await?)?,
        Action::SetHead {
            branch,
            commit,
            expected,
            expected_generation,
        } => {
            let expected = match (expected, expected_generation) {
                (Some(commit), Some(generation)) => Some(Head {
                    commit: address(commit)?,
                    generation,
                }),
                (None, None) => None,
                _ => anyhow::bail!("expected address and generation must be supplied together"),
            };
            serde_json::to_value(
                repo.compare_and_swap(&branch, expected.as_ref(), &address(commit)?)
                    .await?,
            )?
        }
        Action::MergeBases { left, right } => {
            serde_json::to_value(repo.merge_bases(&address(left)?, &address(right)?).await?)?
        }
        Action::Compare { base, left, right } => {
            let base = repo.commit(&address(base)?).await?;
            let left = repo.commit(&address(left)?).await?;
            let right = repo.commit(&address(right)?).await?;
            let bases = repo.merge_bases(&left.verify()?, &right.verify()?).await?;
            anyhow::ensure!(bases.len() == 1 && bases[0] == base.verify()?,
                "comparison requires the unique best common ancestor; resolve disconnected or criss-cross history explicitly");
            serde_json::to_value(engine_core::repository::compare::plan(
                &base.commit,
                &left.commit,
                &right.commit,
            )?)?
        }
    };
    repo.close().await;
    println!(
        "{}",
        serde_json::to_string_pretty(&output).context("encoding local repository result")?
    );
    Ok(())
}
