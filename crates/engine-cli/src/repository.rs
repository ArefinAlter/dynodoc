//! Explicit local repository commands. These never contact a remote server or
//! apply content to a native editor. Signing keys are supplied by the operator.
use anyhow::Context;
use base64::{engine::general_purpose::STANDARD, Engine};
use clap::{Subcommand, ValueEnum};
use engine_core::{
    repository::{
        local::{
            GcPlanOptions, LocalRepository, ObjectKey, ObjectPacket, StorageLimits, SCHEMA_VERSION,
        },
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

#[derive(Clone, Copy, ValueEnum)]
pub enum PlanFormat {
    Json,
    Text,
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
    /// Read-only retention plan: what no branch head or recovery source reaches.
    ///
    /// Reports unreachable commits and objects (staged, orphaned, superseded)
    /// with encoded byte totals, and what stays pinned and why. Deletes nothing;
    /// there is no garbage collector. Opens the database read-only: it never
    /// creates, upgrades or writes it.
    GcPlan {
        /// Largest pinned reasons to include (1-100).
        #[arg(long, default_value_t = 10)]
        top: usize,
        /// Unreachable commits and staged objects to list as examples (0-1000).
        #[arg(long, default_value_t = 10)]
        list: usize,
        /// Deterministic JSON, or a plain-text summary of the same facts.
        #[arg(long, value_enum, default_value_t = PlanFormat::Json)]
        format: PlanFormat,
    },
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
    ///
    /// Paths edited on both sides stay whole-file conflicts. When both sides keep
    /// the same document, `same_file_convergence` also reports a dry-run
    /// node/field merge: clean or not, the address it would have, and conflicts.
    /// Nothing is stored or signed.
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

/// Project comparison: unique-base check, path-level plan and, only when a
/// conflicting path keeps one document identity, the report-only same-file merge.
async fn compare_value(
    repo: &LocalRepository,
    base: Address,
    left: Address,
    right: Address,
) -> anyhow::Result<serde_json::Value> {
    let base = repo.commit(&base).await?;
    let left = repo.commit(&left).await?;
    let right = repo.commit(&right).await?;
    let bases = repo.merge_bases(&left.verify()?, &right.verify()?).await?;
    anyhow::ensure!(bases.len() == 1 && bases[0] == base.verify()?,
        "comparison requires the unique best common ancestor; resolve disconnected or criss-cross history explicitly");
    let plan = engine_core::repository::compare::plan(&base.commit, &left.commit, &right.commit)?;
    let mut output = serde_json::to_value(&plan)?;
    if let Some(report) = repo.same_file_convergence(&plan).await? {
        output["same_file_convergence"] = serde_json::to_value(report)?;
    }
    Ok(output)
}

pub async fn run(path: PathBuf, project: uuid::Uuid, action: Action) -> anyhow::Result<()> {
    // The retention plan must not create, upgrade or write the database.
    let repo = if matches!(action, Action::GcPlan { .. }) {
        LocalRepository::open_read_only(&path, project).await?
    } else {
        LocalRepository::open(&path, project).await?
    };
    let address = |s: String| Address::try_from(s);
    let mut text = None;
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
        Action::GcPlan { top, list, format } => {
            let plan = repo
                .gc_plan(GcPlanOptions {
                    top_reasons: top,
                    list,
                })
                .await?;
            match format {
                PlanFormat::Json => serde_json::to_value(&plan)?,
                PlanFormat::Text => {
                    text = Some(plan.summary());
                    serde_json::Value::Null
                }
            }
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
            compare_value(&repo, address(base)?, address(left)?, address(right)?).await?
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
    match text {
        Some(text) => print!("{text}"),
        None => println!(
            "{}",
            serde_json::to_string_pretty(&output).context("encoding local repository result")?
        ),
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use clap::{Command, FromArgMatches};
    use engine_core::{
        materializer::{DocumentState, MaterializedNode},
        repository::FileEntry,
    };
    use engine_shared::NodeId;
    use ring::signature::Ed25519KeyPair;

    fn paragraphs(edits: &[(usize, &str)]) -> DocumentState {
        let mut state = DocumentState::default();
        for i in 0..4 {
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
                    current_fields: serde_json::json!({"label": label}),
                },
            );
        }
        state
    }

    struct Fixture {
        path: PathBuf,
        project: uuid::Uuid,
        repo: LocalRepository,
    }

    impl Fixture {
        async fn new() -> Self {
            let path = std::env::temp_dir().join(format!(
                "dynodoc-cli-repository-test-{}.sqlite",
                uuid::Uuid::new_v4()
            ));
            let project = uuid::Uuid::new_v4();
            let repo = LocalRepository::open(&path, project).await.unwrap();
            Self {
                path,
                project,
                repo,
            }
        }
        async fn file(&self, document: DocumentId, edits: &[(usize, &str)]) -> FileEntry {
            let (state, _) = self
                .repo
                .write_state(document, &paragraphs(edits))
                .await
                .unwrap();
            FileEntry { document, state }
        }
        async fn commit(
            &self,
            parents: &[&Address],
            files: Vec<(&str, FileEntry)>,
            message: &str,
        ) -> Address {
            let commit = Commit {
                version: 1,
                project: self.project,
                parents: parents.iter().map(|a| (*a).clone()).collect(),
                files: files
                    .into_iter()
                    .map(|(path, file)| (path.to_string(), file))
                    .collect(),
                author_key: String::new(),
                authored_at_ms: 1_700_000_000_000,
                message: message.into(),
                recovery: None,
            };
            let key = Ed25519KeyPair::from_seed_unchecked(&[7; 32]).unwrap();
            self.repo
                .import_commit(&SignedCommit::sign(commit, &key).unwrap())
                .await
                .unwrap()
        }
        async fn finish(self) {
            self.repo.close().await;
            for attempt in 0..20 {
                match std::fs::remove_file(&self.path) {
                    Ok(()) => return,
                    Err(_) if attempt < 19 => {
                        tokio::time::sleep(std::time::Duration::from_millis(25)).await
                    }
                    Err(error) => panic!("test database cleanup: {error}"),
                }
            }
        }
    }

    #[tokio::test]
    async fn compare_output_is_unchanged_without_same_document_conflicts() {
        let f = Fixture::new().await;
        let (report, budget) = (
            DocumentId(uuid::Uuid::new_v4()),
            DocumentId(uuid::Uuid::new_v4()),
        );
        let base = f
            .commit(
                &[],
                vec![
                    ("Report.docx", f.file(report, &[]).await),
                    ("Budget.xlsx", f.file(budget, &[]).await),
                ],
                "base",
            )
            .await;
        let left = f
            .commit(
                &[&base],
                vec![
                    ("Report.docx", f.file(report, &[(0, "left")]).await),
                    ("Budget.xlsx", f.file(budget, &[]).await),
                ],
                "left",
            )
            .await;
        let right = f
            .commit(
                &[&base],
                vec![
                    ("Report.docx", f.file(report, &[]).await),
                    ("Budget.xlsx", f.file(budget, &[(1, "right")]).await),
                ],
                "right",
            )
            .await;
        let output = compare_value(&f.repo, base.clone(), left.clone(), right.clone())
            .await
            .unwrap();
        let plan = engine_core::repository::compare::plan(
            &f.repo.commit(&base).await.unwrap().commit,
            &f.repo.commit(&left).await.unwrap().commit,
            &f.repo.commit(&right).await.unwrap().commit,
        )
        .unwrap();
        // Exactly the pre-existing planner output: same bytes, no extra key.
        let expected = serde_json::to_value(&plan).unwrap();
        assert_eq!(
            serde_json::to_string_pretty(&output).unwrap(),
            serde_json::to_string_pretty(&expected).unwrap()
        );
        assert!(output.get("same_file_convergence").is_none());
        assert_eq!(output["conflicts"].as_array().unwrap().len(), 0);
        f.finish().await;
    }

    #[tokio::test]
    async fn compare_adds_a_same_file_report_without_changing_the_conflict_list() {
        let f = Fixture::new().await;
        let report = DocumentId(uuid::Uuid::new_v4());
        let base = f
            .commit(
                &[],
                vec![("Report.docx", f.file(report, &[]).await)],
                "base",
            )
            .await;
        let left = f
            .commit(
                &[&base],
                vec![("Report.docx", f.file(report, &[(0, "left")]).await)],
                "left",
            )
            .await;
        let right = f
            .commit(
                &[&base],
                vec![("Report.docx", f.file(report, &[(3, "right")]).await)],
                "right",
            )
            .await;
        let forward = compare_value(&f.repo, base.clone(), left.clone(), right.clone())
            .await
            .unwrap();
        let reverse = compare_value(&f.repo, base.clone(), right.clone(), left.clone())
            .await
            .unwrap();
        assert_eq!(forward["conflicts"].as_array().unwrap().len(), 1);
        assert!(forward["files"].as_object().unwrap().is_empty());
        let file = &forward["same_file_convergence"]["files"][0];
        assert_eq!(file["path"], "Report.docx");
        assert_eq!(file["clean"], true);
        assert!(file["merged_state"].is_string());
        assert_eq!(
            file["merged_state"],
            reverse["same_file_convergence"]["files"][0]["merged_state"]
        );
        assert_eq!(forward["same_file_convergence"]["not_examined"], 0);
        f.finish().await;
    }

    #[tokio::test]
    async fn compare_still_requires_a_unique_best_common_ancestor() {
        let f = Fixture::new().await;
        let a = f.commit(&[], vec![], "a").await;
        let b = f.commit(&[&a], vec![], "b").await;
        let c = f.commit(&[&a], vec![], "c").await;
        let d = f.commit(&[&b, &c], vec![], "d").await;
        let e = f.commit(&[&c, &b], vec![], "e").await;
        // Criss-cross: two best bases.
        let error = compare_value(&f.repo, b.clone(), d.clone(), e.clone())
            .await
            .unwrap_err();
        assert!(error.to_string().contains("unique best common ancestor"));
        // Disconnected histories have none.
        let unrelated = f.commit(&[], vec![], "unrelated").await;
        assert!(compare_value(&f.repo, a.clone(), b.clone(), unrelated)
            .await
            .is_err());
        // A unique base still compares.
        assert!(compare_value(&f.repo, a, b, c).await.is_ok());
        f.finish().await;
    }

    #[tokio::test]
    async fn gc_plan_is_read_only_and_never_creates_a_database() {
        let f = Fixture::new().await;
        let document = DocumentId(uuid::Uuid::new_v4());
        f.file(document, &[]).await;
        let (path, project) = (f.path.clone(), f.project);
        for format in [PlanFormat::Json, PlanFormat::Text] {
            run(
                path.clone(),
                project,
                Action::GcPlan {
                    top: 3,
                    list: 2,
                    format,
                },
            )
            .await
            .unwrap();
        }
        let missing = path.with_extension("missing");
        let result = run(
            missing.clone(),
            project,
            Action::GcPlan {
                top: 10,
                list: 10,
                format: PlanFormat::Json,
            },
        )
        .await;
        assert!(result.is_err());
        assert!(!missing.exists());
        f.finish().await;
    }

    #[test]
    fn gc_plan_arguments_parse_with_documented_defaults() {
        let command = Action::augment_subcommands(Command::new("repository").no_binary_name(true));
        let parse = |args: &[&str]| {
            let matches = command.clone().try_get_matches_from(args).unwrap();
            Action::from_arg_matches(&matches).unwrap()
        };
        assert!(matches!(
            parse(&["gc-plan"]),
            Action::GcPlan {
                top: 10,
                list: 10,
                format: PlanFormat::Json
            }
        ));
        assert!(matches!(
            parse(&["gc-plan", "--top", "3", "--list", "0", "--format", "text"]),
            Action::GcPlan {
                top: 3,
                list: 0,
                format: PlanFormat::Text
            }
        ));
        assert!(command
            .clone()
            .try_get_matches_from(["gc-plan", "--format", "xml"])
            .is_err());
    }
}
