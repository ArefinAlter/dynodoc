//! Explicit local repository commands. These never contact a remote server or
//! apply content to a native editor. Signing keys are supplied by the operator.
use anyhow::Context;
use base64::{engine::general_purpose::STANDARD, Engine};
use clap::{Subcommand, ValueEnum};
use engine_core::{
    repository::{
        local::{LocalRepository, ObjectKey, ObjectPacket, StorageLimits, SCHEMA_VERSION},
        recovery, Commit, Head, SignedCommit,
    },
    shared_checkpoint::Address,
};
use engine_shared::DocumentId;
use std::{
    collections::BTreeMap,
    io::Read,
    path::{Path, PathBuf},
};

#[derive(Clone, Copy, ValueEnum)]
pub enum RecoveryKind {
    CherryPick,
    Revert,
}

impl From<RecoveryKind> for recovery::Kind {
    fn from(value: RecoveryKind) -> Self {
        match value {
            RecoveryKind::CherryPick => Self::CherryPick,
            RecoveryKind::Revert => Self::Revert,
        }
    }
}

#[derive(Subcommand)]
pub enum Action {
    /// Initialize or verify the local project database.
    Init,
    /// Inspect retained encoded bytes/object/commit/branch counts and limits.
    Storage,
    /// Local private inventory; do not publish it as a shared remote listing.
    Objects {
        #[arg(long, requires = "after_address")]
        after_document: Option<uuid::Uuid>,
        #[arg(long, requires = "after_document")]
        after_address: Option<String>,
        #[arg(long, default_value_t = 64)]
        limit: usize,
    },
    /// Export one verified object as a base64 transfer packet.
    ExportObject {
        document: uuid::Uuid,
        address: String,
    },
    /// Import a private staging upload JSON batch, without moving any branch.
    ImportObjects { input: PathBuf },
    /// Explicitly set persistent capacity; never deletes retained history.
    StorageLimits {
        #[arg(long)]
        bytes: i64,
        #[arg(long)]
        objects: i64,
        #[arg(long)]
        commits: i64,
        #[arg(long)]
        branches: i64,
    },
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
    /// Preview a source commit's changes (or inverse) on an exact current commit.
    RecoveryPlan {
        source: String,
        onto: String,
        #[arg(long, value_enum)]
        kind: RecoveryKind,
        /// Required for a merge source; must name one of its parents.
        #[arg(long)]
        mainline: Option<String>,
    },
    /// Sign/store a reviewed recovery commit. Publish separately with set-head.
    Recover {
        source: String,
        onto: String,
        #[arg(long, value_enum)]
        kind: RecoveryKind,
        #[arg(long)]
        mainline: Option<String>,
        /// JSON object mapping EVERY conflict path to FileEntry or null (delete).
        #[arg(long)]
        resolutions: Option<PathBuf>,
        #[arg(long)]
        key: PathBuf,
        #[arg(long)]
        message: String,
        #[arg(long)]
        authored_at_ms: i64,
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

#[derive(serde::Deserialize)]
#[serde(deny_unknown_fields)]
struct TransferBatch {
    document: uuid::Uuid,
    objects: Vec<TransferObject>,
}
#[derive(serde::Deserialize)]
#[serde(deny_unknown_fields)]
struct TransferObject {
    address: Address,
    bytes: String,
}

pub async fn run(path: PathBuf, project: uuid::Uuid, action: Action) -> anyhow::Result<()> {
    let repo = LocalRepository::open(&path, project).await?;
    let address = |s: String| Address::try_from(s);
    let output = match action {
        Action::Init => serde_json::json!({"project":project,"local_schema":SCHEMA_VERSION}),
        Action::Storage => serde_json::to_value(repo.storage_usage().await?)?,
        Action::Objects {
            after_document,
            after_address,
            limit,
        } => {
            let after = match (after_document, after_address) {
                (Some(document), Some(value)) => Some(ObjectKey {
                    document: DocumentId(document),
                    address: address(value)?,
                }),
                _ => None,
            };
            let keys = repo.inventory(after.as_ref(), limit).await?;
            serde_json::json!({"objects":keys.iter().map(|k|serde_json::json!({"document":k.document,"address":k.address})).collect::<Vec<_>>()})
        }
        Action::ExportObject {
            document,
            address: value,
        } => {
            let packet = repo
                .object(&ObjectKey {
                    document: DocumentId(document),
                    address: address(value)?,
                })
                .await?;
            serde_json::json!({"address":packet.key.address,"bytes":STANDARD.encode(packet.bytes)})
        }
        Action::ImportObjects { input } => {
            let batch: TransferBatch = read_json(&input, 1_500_000)?;
            anyhow::ensure!(batch.objects.len() <= 64, "import at most 64 objects");
            let mut bytes = 0usize;
            let packets = batch
                .objects
                .into_iter()
                .map(|object| {
                    let decoded = STANDARD.decode(object.bytes)?;
                    bytes += decoded.len();
                    anyhow::ensure!(bytes <= 1024 * 1024, "import at most 1 MiB");
                    Ok(ObjectPacket {
                        key: ObjectKey {
                            document: DocumentId(batch.document),
                            address: object.address,
                        },
                        bytes: decoded,
                    })
                })
                .collect::<anyhow::Result<Vec<_>>>()?;
            serde_json::json!({"new_objects":repo.import_objects(&packets).await?,"published":false})
        }
        Action::StorageLimits {
            bytes,
            objects,
            commits,
            branches,
        } => serde_json::to_value(
            repo.set_storage_limits(StorageLimits {
                bytes,
                objects,
                commits,
                branches,
            })
            .await?,
        )?,
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
            let key = crate::signing::load(&key)?;
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
        Action::RecoveryPlan {
            source,
            onto,
            kind,
            mainline,
        } => {
            let mainline = mainline.map(address).transpose()?;
            serde_json::to_value(
                repo.plan_recovery(
                    kind.into(),
                    &address(source)?,
                    &address(onto)?,
                    mainline.as_ref(),
                )
                .await?,
            )?
        }
        Action::Recover {
            source,
            onto,
            kind,
            mainline,
            resolutions,
            key,
            message,
            authored_at_ms,
        } => {
            let mainline = mainline.map(address).transpose()?;
            let plan = repo
                .plan_recovery(
                    kind.into(),
                    &address(source)?,
                    &address(onto)?,
                    mainline.as_ref(),
                )
                .await?;
            let resolutions = match resolutions {
                Some(path) => read_json(&path, engine_core::repository::MAX_COMMIT_BYTES)?,
                None => BTreeMap::new(),
            };
            let commit = plan.resolve(&resolutions, authored_at_ms, message)?;
            let signed = SignedCommit::sign(commit, &crate::signing::load(&key)?)?;
            serde_json::json!({"commit":repo.import_commit(&signed).await?,"recovery":signed.commit.recovery,"parents":signed.commit.parents})
        }
    };
    repo.close().await;
    println!(
        "{}",
        serde_json::to_string_pretty(&output).context("encoding local repository result")?
    );
    Ok(())
}
