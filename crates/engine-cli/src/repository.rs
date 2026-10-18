//! Explicit local repository commands. These never contact a remote server or
//! apply content to a native editor. Signing keys are supplied by the operator.
use anyhow::Context;
use base64::{engine::general_purpose::STANDARD, Engine};
use clap::{Subcommand, ValueEnum};
use engine_core::{
    repository::{
        horizon::{
            plan::PlanOptions,
            policy::{LoadOptions, Policy, MAX_POLICY_BYTES},
        },
        local::{
            CompactOptions, GcApplyOptions, GcPlanOptions, LocalRepository, ObjectKey,
            ObjectPacket, StorageLimits, SCHEMA_VERSION,
        },
        project_merge::SameFile,
        recovery, Commit, FileEntry, Head, SignedCommit,
    },
    shared_checkpoint::Address,
};
use engine_shared::DocumentId;
use ring::signature::Ed25519KeyPair;
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

/// Opt-in same-file convergence for `merge`; omit the flag for explicit-only.
#[derive(Clone, Copy, ValueEnum)]
pub enum SameFileMerge {
    /// Merge a path edited on both sides at node/field level when that merge is
    /// clean in both orientations.
    Clean,
}

#[derive(Subcommand)]
pub enum Action {
    /// Initialize or verify the local project database.
    Init,
    /// Inspect retained encoded bytes/object/commit/branch counts and limits.
    ///
    /// `bytes_used` and the limit are logical (canonical encoded bytes);
    /// `stored_bytes` is what the rows physically hold after compression at rest.
    Storage,
    /// Compress existing object rows in place (local schema 3). Safe to repeat.
    ///
    /// New objects are compressed when written; this one-time command re-encodes
    /// objects stored by earlier releases. Addresses, quotas and exported packets
    /// do not change. Each batch is a short verified transaction: an object must
    /// hash to its address and inflate back to the same bytes before its row is
    /// replaced, and the persistent counters are checked against the batch. An
    /// interrupted run keeps every committed batch; run it again to finish. It
    /// does not shrink the database file by itself: add --vacuum for that. Needs a
    /// writable repository.
    Compact {
        /// Rows examined per transaction (1-1024).
        #[arg(long, default_value_t = 256)]
        batch: usize,
        /// Stop after about this many rows (use the printed `next` to resume).
        #[arg(long)]
        max_objects: Option<u64>,
        #[arg(long, requires = "after_address")]
        after_document: Option<uuid::Uuid>,
        #[arg(long, requires = "after_document")]
        after_address: Option<String>,
        /// Return freed pages to the filesystem afterwards (checkpoint the
        /// write-ahead log, then VACUUM); needs free disk space about the size
        /// of the database file.
        #[arg(long)]
        vacuum: bool,
        /// JSON report, or a plain-text summary of the same facts.
        #[arg(long, value_enum, default_value_t = PlanFormat::Json)]
        format: PlanFormat,
    },
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
    /// with encoded (logical) byte totals, the physical bytes stored after
    /// compression, and what stays pinned and why. Deletes nothing.
    /// Prints a deterministic `plan_hash` that `gc --plan-hash` requires. Opens
    /// the database read-only: it never creates, upgrades or writes it.
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
    /// Read-only horizon plan: what a history horizon under a retention policy
    /// would release (docs/HORIZON-RECORD-V1.md). Deletes, signs and writes nothing.
    ///
    /// Heads, held and policy-pinned commits, merge bases, the recent window and the
    /// recovery (v2) and same-file merge (v3) provenance they need stay; under tier
    /// `history` every other commit would become a skeleton entry, under tier
    /// `content` every commit stays and only unneeded file states are released.
    /// Prints the canonical plan JSON on stdout and a short summary on stderr
    /// (`--format text` prints only the summary). The plan hash binds the policy,
    /// the branch heads and the three horizon roots, never the example count. Opens
    /// the database read-only: it never creates, upgrades or writes it.
    HorizonPlan {
        /// Retention policy JSON (`dynodoc.retention-policy.v1`).
        #[arg(long)]
        policy: PathBuf,
        /// Released and pinned commits to list as examples (0-1000).
        #[arg(long, default_value_t = 10)]
        list: usize,
        /// Instant, in milliseconds since the epoch, that `keep_within_days` counts
        /// back from. Defaults to the current time rounded down to a UTC day, so a
        /// plan repeated on the same day is the same plan. Echoed in the plan.
        #[arg(long)]
        as_of_ms: Option<i64>,
        /// Also write the canonical plan JSON to this new file (never overwrites).
        #[arg(long)]
        out: Option<PathBuf>,
        /// Canonical JSON plus a stderr summary, or the plain-text summary alone.
        #[arg(long, value_enum, default_value_t = PlanFormat::Json)]
        format: PlanFormat,
    },
    /// Delete the staged objects of a reviewed gc-plan; nothing else.
    ///
    /// Recomputes the plan inside one write transaction and refuses unless its
    /// hash equals --plan-hash (a branch moved or objects changed since the plan).
    /// Deletes only stored objects that no stored commit references. Never deletes
    /// commits, signatures, branch refs or any object a commit or recovery
    /// source reaches. Repeating an applied hash changes nothing. --vacuum then
    /// truncates the write-ahead log and runs VACUUM, which needs free disk space
    /// about the size of the database file.
    Gc {
        /// The `plan_hash` printed by `gc-plan` for the state you reviewed.
        #[arg(long)]
        plan_hash: String,
        /// Return freed pages to the filesystem after the deletion commits.
        #[arg(long)]
        vacuum: bool,
        /// JSON report, or a plain-text summary of the same facts.
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
    /// Sign/store a two-parent merge of LEFT (first parent) and RIGHT.
    ///
    /// The base is the unique best common ancestor; criss-cross and disconnected
    /// histories are refused. Independent changes combine as in `compare`; every
    /// conflicting path needs an explicit choice in --resolutions (FileEntry, or
    /// null to delete). With --same-file-merge clean, a path edited on both sides
    /// that keeps one document identity is merged node by node and field by field
    /// when that is clean in both orientations: the merged state is stored first
    /// and recorded in the signed commit (format v3). Paths with node or field
    /// conflicts still need an explicit choice. Publish separately with set-head.
    Merge {
        left: String,
        right: String,
        /// Merge clean same-file edits automatically (off when omitted).
        #[arg(long, value_enum)]
        same_file_merge: Option<SameFileMerge>,
        /// JSON object mapping each remaining conflict path to FileEntry or null.
        #[arg(long)]
        resolutions: Option<PathBuf>,
        #[arg(long)]
        key: PathBuf,
        #[arg(long)]
        message: String,
        #[arg(long)]
        authored_at_ms: i64,
    },
    /// Re-run the recorded strategy of a stored merge commit's same-file records.
    VerifyMerge { commit: String },
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

struct MergeArgs {
    left: Address,
    right: Address,
    same_file: SameFile,
    resolutions: BTreeMap<String, Option<FileEntry>>,
    message: String,
    authored_at_ms: i64,
}

/// Prepare, sign and store one merge. Nothing is stored while a conflict is open.
async fn merge_value(
    repo: &LocalRepository,
    args: MergeArgs,
    key: &Ed25519KeyPair,
) -> anyhow::Result<serde_json::Value> {
    let merge = repo
        .prepare_merge(
            &args.left,
            &args.right,
            args.same_file,
            &args.resolutions,
            args.authored_at_ms,
            args.message,
        )
        .await?;
    let Some(commit) = merge.commit else {
        let hint = if args.same_file == SameFile::Off {
            " (or use --same-file-merge clean for edits to one document that merge cleanly)"
        } else {
            ""
        };
        anyhow::bail!(
            "{} conflicting path(s) need an explicit resolution in --resolutions: {}{hint}",
            merge.unresolved.len(),
            merge.unresolved.join(", ")
        );
    };
    let signed = SignedCommit::sign(commit, key)?;
    let address = repo.import_commit(&signed).await?;
    Ok(serde_json::json!({
        "commit": address,
        "version": signed.commit.version,
        "parents": signed.commit.parents,
        "base": merge.base,
        "same_file_resolutions": signed.commit.same_file_resolutions,
        "explicit_resolutions": merge.explicit,
    }))
}

/// A policy file, parsed and validated; every problem is listed in the error.
fn load_policy(path: &Path) -> anyhow::Result<Policy> {
    let mut bytes = Vec::new();
    std::fs::File::open(path)
        .with_context(|| format!("opening {}", path.display()))?
        .take(MAX_POLICY_BYTES as u64 + 1)
        .read_to_end(&mut bytes)?;
    Policy::load(&bytes, LoadOptions::default())
        .map_err(|errors| anyhow::anyhow!("invalid retention policy {}:\n{errors}", path.display()))
}

/// The text `horizon-plan` prints on stdout; the other rendering goes to stderr.
async fn horizon_plan(
    repo: &LocalRepository,
    policy: &Path,
    list: usize,
    as_of_ms: Option<i64>,
    out: Option<PathBuf>,
    format: PlanFormat,
) -> anyhow::Result<String> {
    let policy = load_policy(policy)?;
    // A time rule needs an instant; the plan carries it, never an ambient clock.
    let as_of_ms = match (policy.window.keep_within_days, as_of_ms) {
        (Some(_), None) => Some(today_start_ms()?),
        (_, given) => given,
    };
    let plan = repo
        .horizon_plan(&policy, &PlanOptions { as_of_ms, list })
        .await?;
    let canonical = String::from_utf8(plan.canonical_bytes()?)?;
    if let Some(out) = out {
        use std::io::Write;
        std::fs::OpenOptions::new()
            .write(true)
            .create_new(true)
            .open(&out)
            .with_context(|| format!("creating {}", out.display()))?
            .write_all(canonical.as_bytes())?;
    }
    Ok(match format {
        PlanFormat::Json => {
            eprint!("{}", plan.summary());
            format!(
                "{canonical}
"
            )
        }
        PlanFormat::Text => plan.summary(),
    })
}

/// The current time in milliseconds since the epoch, rounded down to a UTC day.
fn today_start_ms() -> anyhow::Result<i64> {
    const DAY_MS: i64 = 86_400_000;
    let now = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .context("system clock before 1970")?;
    Ok(now.as_millis() as i64 / DAY_MS * DAY_MS)
}

pub async fn run(path: PathBuf, project: uuid::Uuid, action: Action) -> anyhow::Result<()> {
    // The retention and horizon plans must not create, upgrade or write the database.
    let repo = if matches!(action, Action::GcPlan { .. } | Action::HorizonPlan { .. }) {
        LocalRepository::open_read_only(&path, project).await?
    } else {
        // A destructive command never creates the database it would collect.
        if matches!(action, Action::Gc { .. }) {
            anyhow::ensure!(
                path.is_file(),
                "no repository database at {}",
                path.display()
            );
        }
        LocalRepository::open(&path, project).await?
    };
    let address = |s: String| Address::try_from(s);
    let mut text = None;
    let mut vacuum_failure = None;
    let output = match action {
        Action::Init => serde_json::json!({"project":project,"local_schema":SCHEMA_VERSION}),
        Action::Storage => serde_json::to_value(repo.storage_usage().await?)?,
        Action::Compact {
            batch,
            max_objects,
            after_document,
            after_address,
            vacuum,
            format,
        } => {
            let after = match (after_document, after_address) {
                (Some(document), Some(value)) => Some(ObjectKey {
                    document: DocumentId(document),
                    address: address(value)?,
                }),
                _ => None,
            };
            let report = repo
                .compact(CompactOptions {
                    batch_objects: batch,
                    max_objects,
                    after,
                    vacuum,
                })
                .await?;
            vacuum_failure = report
                .vacuum
                .as_ref()
                .and_then(|v| v.error.clone())
                .map(|e| format!("compaction committed but VACUUM failed: {e}"));
            match format {
                PlanFormat::Json => serde_json::to_value(&report)?,
                PlanFormat::Text => {
                    text = Some(report.summary());
                    serde_json::Value::Null
                }
            }
        }
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
        Action::HorizonPlan {
            policy,
            list,
            as_of_ms,
            out,
            format,
        } => {
            text = Some(horizon_plan(&repo, &policy, list, as_of_ms, out, format).await?);
            serde_json::Value::Null
        }
        Action::Gc {
            plan_hash,
            vacuum,
            format,
        } => {
            let report = repo.gc_apply(&plan_hash, GcApplyOptions { vacuum }).await?;
            // A failed shrink does not hide the deletion: print it, then fail.
            vacuum_failure = report
                .vacuum
                .as_ref()
                .and_then(|v| v.error.clone())
                .map(|e| format!("deletion committed but VACUUM failed: {e}"));
            match format {
                PlanFormat::Json => serde_json::to_value(&report)?,
                PlanFormat::Text => {
                    text = Some(report.summary());
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
        Action::Merge {
            left,
            right,
            same_file_merge,
            resolutions,
            key,
            message,
            authored_at_ms,
        } => {
            let resolutions = match resolutions {
                Some(path) => read_json(&path, engine_core::repository::MAX_COMMIT_BYTES)?,
                None => BTreeMap::new(),
            };
            // Load the key before anything is stored.
            let key = crate::signing::load(&key)?;
            let args = MergeArgs {
                left: address(left)?,
                right: address(right)?,
                same_file: match same_file_merge {
                    Some(SameFileMerge::Clean) => SameFile::Clean,
                    None => SameFile::Off,
                },
                resolutions,
                message,
                authored_at_ms,
            };
            merge_value(&repo, args, &key).await?
        }
        Action::VerifyMerge { commit } => {
            let commit = address(commit)?;
            let verified = repo.verify_same_file_resolutions(&commit).await?;
            serde_json::json!({"commit":commit,"verified_same_file_resolutions":verified})
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
    match vacuum_failure {
        Some(message) => anyhow::bail!(message),
        None => Ok(()),
    }
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
                same_file_resolutions: Vec::new(),
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

    #[tokio::test]
    async fn gc_collects_only_the_reviewed_plan_and_refuses_a_stale_one() {
        let f = Fixture::new().await;
        let (kept, staged) = (
            DocumentId(uuid::Uuid::new_v4()),
            DocumentId(uuid::Uuid::new_v4()),
        );
        let file = f.file(kept, &[]).await;
        let head = f.commit(&[], vec![("Report.docx", file)], "base").await;
        f.repo.compare_and_swap("main", None, &head).await.unwrap();
        f.file(staged, &[(0, "never committed")]).await;
        let (path, project) = (f.path.clone(), f.project);
        let plan = f
            .repo
            .gc_plan(GcPlanOptions {
                top_reasons: 1,
                list: 0,
            })
            .await
            .unwrap();
        assert!(plan.unreachable.counts.objects > 0);
        let run_gc = |hash: String, vacuum: bool, format: PlanFormat| {
            run(
                path.clone(),
                project,
                Action::Gc {
                    plan_hash: hash,
                    vacuum,
                    format,
                },
            )
        };
        // A hash that is not the current plan changes nothing.
        assert!(run_gc("0".repeat(64), false, PlanFormat::Json)
            .await
            .is_err());
        // A missing database is an error and is never created by `gc`.
        let missing = path.with_extension("absent");
        assert!(run(
            missing.clone(),
            project,
            Action::Gc {
                plan_hash: plan.plan_hash.clone(),
                vacuum: false,
                format: PlanFormat::Json,
            },
        )
        .await
        .is_err());
        assert!(!missing.exists());
        let before = f.repo.storage_usage().await.unwrap();
        assert_eq!(before.objects_used as u64, plan.retained.objects);
        run_gc(plan.plan_hash.clone(), true, PlanFormat::Text)
            .await
            .unwrap();
        let after = f.repo.storage_usage().await.unwrap();
        assert_eq!(
            after.objects_used as u64,
            plan.retained.objects - plan.unreachable.counts.objects
        );
        // Replaying the same reviewed hash is a no-op, in either format.
        run_gc(plan.plan_hash.clone(), false, PlanFormat::Json)
            .await
            .unwrap();
        assert_eq!(f.repo.storage_usage().await.unwrap(), after);
        // The kept state and the branch are intact.
        let kept_file = &f.repo.commit(&head).await.unwrap().commit.files["Report.docx"];
        assert_eq!(
            f.repo
                .read_state(kept_file.document, &kept_file.state)
                .await
                .unwrap(),
            paragraphs(&[])
        );
        f.finish().await;
    }

    #[test]
    fn gc_arguments_require_a_plan_hash_and_parse_options() {
        let command = Action::augment_subcommands(Command::new("repository").no_binary_name(true));
        let parse = |args: &[&str]| {
            let matches = command.clone().try_get_matches_from(args).unwrap();
            Action::from_arg_matches(&matches).unwrap()
        };
        assert!(command.clone().try_get_matches_from(["gc"]).is_err());
        let hash = "ab".repeat(32);
        match parse(&["gc", "--plan-hash", &hash]) {
            Action::Gc {
                plan_hash,
                vacuum,
                format,
            } => {
                assert_eq!(plan_hash, hash);
                assert!(!vacuum);
                assert!(matches!(format, PlanFormat::Json));
            }
            _ => panic!("expected gc"),
        }
        assert!(matches!(
            parse(&["gc", "--plan-hash", &hash, "--vacuum", "--format", "text"]),
            Action::Gc {
                vacuum: true,
                format: PlanFormat::Text,
                ..
            }
        ));
    }

    async fn object_total(repo: &LocalRepository) -> usize {
        repo.storage_usage().await.unwrap().objects_used as usize
    }

    fn merge_args(
        left: &Address,
        right: &Address,
        same_file: SameFile,
        resolutions: BTreeMap<String, Option<FileEntry>>,
    ) -> MergeArgs {
        MergeArgs {
            left: left.clone(),
            right: right.clone(),
            same_file,
            resolutions,
            message: "merge".into(),
            authored_at_ms: 1_700_000_000_500,
        }
    }

    #[tokio::test]
    async fn merge_needs_the_flag_for_same_file_edits_and_records_the_resolution() {
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
        let key = Ed25519KeyPair::from_seed_unchecked(&[7; 32]).unwrap();
        // Without the flag the path is a conflict: an error and no new content.
        let stored = object_total(&f.repo).await;
        let error = merge_value(
            &f.repo,
            merge_args(&left, &right, SameFile::Off, BTreeMap::new()),
            &key,
        )
        .await
        .unwrap_err();
        assert!(error.to_string().contains("Report.docx"));
        assert!(error.to_string().contains("--same-file-merge clean"));
        assert_eq!(object_total(&f.repo).await, stored);
        // With the flag: stored, signed as v3, recorded and independently verifiable.
        let output = merge_value(
            &f.repo,
            merge_args(&left, &right, SameFile::Clean, BTreeMap::new()),
            &key,
        )
        .await
        .unwrap();
        assert_eq!(output["version"], 3);
        assert_eq!(output["base"], serde_json::to_value(&base).unwrap());
        assert_eq!(output["parents"], serde_json::json!([left, right]));
        assert_eq!(output["explicit_resolutions"], serde_json::json!([]));
        let record = &output["same_file_resolutions"][0];
        assert_eq!(record["path"], "Report.docx");
        assert_eq!(record["strategy"], "dynodoc.same-file.node-field.v1");
        let commit: Address = serde_json::from_value(output["commit"].clone()).unwrap();
        let stored = f.repo.commit(&commit).await.unwrap();
        assert_eq!(stored.verify().unwrap(), commit);
        assert_eq!(
            stored.commit.files["Report.docx"].state,
            serde_json::from_value::<Address>(record["merged"].clone()).unwrap()
        );
        assert_eq!(
            f.repo.verify_same_file_resolutions(&commit).await.unwrap(),
            1
        );
        // The swapped merge produces the same merged state.
        let swapped = merge_value(
            &f.repo,
            merge_args(&right, &left, SameFile::Clean, BTreeMap::new()),
            &key,
        )
        .await
        .unwrap();
        assert_eq!(
            swapped["same_file_resolutions"][0]["merged"],
            record["merged"]
        );
        assert_ne!(swapped["commit"], output["commit"]);
        f.finish().await;
    }

    #[tokio::test]
    async fn merge_keeps_overlapping_edits_explicit_and_uses_given_resolutions() {
        let f = Fixture::new().await;
        let report = DocumentId(uuid::Uuid::new_v4());
        let base = f
            .commit(
                &[],
                vec![("Report.docx", f.file(report, &[]).await)],
                "base",
            )
            .await;
        let left_file = f.file(report, &[(1, "left wording")]).await;
        let left = f
            .commit(&[&base], vec![("Report.docx", left_file.clone())], "left")
            .await;
        let right = f
            .commit(
                &[&base],
                vec![("Report.docx", f.file(report, &[(1, "right wording")]).await)],
                "right",
            )
            .await;
        let key = Ed25519KeyPair::from_seed_unchecked(&[7; 32]).unwrap();
        for same_file in [SameFile::Off, SameFile::Clean] {
            assert!(merge_value(
                &f.repo,
                merge_args(&left, &right, same_file, BTreeMap::new()),
                &key
            )
            .await
            .is_err());
        }
        let output = merge_value(
            &f.repo,
            merge_args(
                &left,
                &right,
                SameFile::Clean,
                BTreeMap::from([("Report.docx".to_string(), Some(left_file.clone()))]),
            ),
            &key,
        )
        .await
        .unwrap();
        assert_eq!(output["version"], 1);
        assert_eq!(output["same_file_resolutions"], serde_json::json!([]));
        assert_eq!(
            output["explicit_resolutions"],
            serde_json::json!(["Report.docx"])
        );
        let commit: Address = serde_json::from_value(output["commit"].clone()).unwrap();
        assert_eq!(
            f.repo.commit(&commit).await.unwrap().commit.files["Report.docx"],
            left_file
        );
        f.finish().await;
    }

    #[test]
    fn merge_arguments_parse_and_the_flag_is_optional() {
        let command = Action::augment_subcommands(Command::new("repository").no_binary_name(true));
        let parse = |args: &[&str]| {
            let matches = command.clone().try_get_matches_from(args).unwrap();
            Action::from_arg_matches(&matches).unwrap()
        };
        let base = [
            "merge",
            "L",
            "R",
            "--key",
            "k.der",
            "--message",
            "m",
            "--authored-at-ms",
            "1",
        ];
        assert!(matches!(
            parse(&base),
            Action::Merge {
                same_file_merge: None,
                resolutions: None,
                ..
            }
        ));
        let mut with_flag = base.to_vec();
        with_flag.extend(["--same-file-merge", "clean", "--resolutions", "r.json"]);
        assert!(matches!(
            parse(&with_flag),
            Action::Merge {
                same_file_merge: Some(SameFileMerge::Clean),
                resolutions: Some(_),
                ..
            }
        ));
        let mut bad = base.to_vec();
        bad.extend(["--same-file-merge", "always"]);
        assert!(command.clone().try_get_matches_from(bad).is_err());
        assert!(command
            .clone()
            .try_get_matches_from(["merge", "L", "R"])
            .is_err());
        assert!(matches!(
            parse(&["verify-merge", "abc"]),
            Action::VerifyMerge { .. }
        ));
    }

    #[test]
    fn horizon_plan_arguments_require_a_policy_and_parse_options() {
        let command = Action::augment_subcommands(Command::new("repository").no_binary_name(true));
        let parse = |args: &[&str]| {
            let matches = command.clone().try_get_matches_from(args).unwrap();
            Action::from_arg_matches(&matches).unwrap()
        };
        assert!(command
            .clone()
            .try_get_matches_from(["horizon-plan"])
            .is_err());
        assert!(matches!(
            parse(&["horizon-plan", "--policy", "p.json"]),
            Action::HorizonPlan {
                list: 10,
                as_of_ms: None,
                out: None,
                format: PlanFormat::Json,
                ..
            }
        ));
        assert!(matches!(
            parse(&[
                "horizon-plan",
                "--policy",
                "p.json",
                "--list",
                "0",
                "--as-of-ms",
                "5",
                "--out",
                "plan.json",
                "--format",
                "text",
            ]),
            Action::HorizonPlan {
                list: 0,
                as_of_ms: Some(5),
                out: Some(_),
                format: PlanFormat::Text,
                ..
            }
        ));
        assert!(command
            .clone()
            .try_get_matches_from(["horizon-plan", "--policy", "p", "--format", "xml"])
            .is_err());
    }

    #[tokio::test]
    async fn horizon_plan_is_read_only_lists_every_policy_problem_and_never_overwrites() {
        let f = Fixture::new().await;
        let document = DocumentId(uuid::Uuid::new_v4());
        let first = f.file(document, &[]).await;
        let mut head = f.commit(&[], vec![("a", first)], "c0").await;
        for i in 1..5 {
            let file = f.file(document, &[(0, &format!("edit {i}"))]).await;
            head = f
                .commit(&[&head], vec![("a", file)], &format!("c{i}"))
                .await;
        }
        f.repo.compare_and_swap("main", None, &head).await.unwrap();
        let (path, project) = (f.path.clone(), f.project);
        let dir =
            std::env::temp_dir().join(format!("dynodoc-horizon-cli-{}", uuid::Uuid::new_v4()));
        std::fs::create_dir_all(&dir).unwrap();
        let policy = dir.join("policy.json");
        std::fs::write(
            &policy,
            r#"{"version":1,"id":"cli","tier":"history","window":{"keep_last_commits":2},"protect":{"branch_heads":true}}"#,
        )
        .unwrap();
        let wal = PathBuf::from(format!("{}-wal", path.display()));
        let snapshot = || {
            (
                std::fs::read(&path).unwrap(),
                std::fs::read(&wal).unwrap_or_default(),
            )
        };
        let before = snapshot();
        let plan_args = |out: Option<PathBuf>, format| Action::HorizonPlan {
            policy: policy.clone(),
            list: 2,
            as_of_ms: None,
            out,
            format,
        };
        run(path.clone(), project, plan_args(None, PlanFormat::Text))
            .await
            .unwrap();
        let out = dir.join("plan.json");
        run(
            path.clone(),
            project,
            plan_args(Some(out.clone()), PlanFormat::Json),
        )
        .await
        .unwrap();
        assert!(snapshot() == before, "planning wrote to the database");
        // The file is the library's canonical plan for the same inputs.
        let expected = f
            .repo
            .horizon_plan(
                &Policy::load(&std::fs::read(&policy).unwrap(), LoadOptions::default()).unwrap(),
                &PlanOptions {
                    as_of_ms: None,
                    list: 2,
                },
            )
            .await
            .unwrap();
        let written = std::fs::read(&out).unwrap();
        assert_eq!(written, expected.canonical_bytes().unwrap());
        let value: serde_json::Value = serde_json::from_slice(&written).unwrap();
        assert_eq!(value["commits"]["released"], 3);
        assert_eq!(value["read_only"], true);
        // A second run to the same file refuses and leaves it untouched.
        assert!(run(
            path.clone(),
            project,
            plan_args(Some(out.clone()), PlanFormat::Json)
        )
        .await
        .is_err());
        assert_eq!(std::fs::read(&out).unwrap(), written);
        // An invalid policy lists every problem; a missing one is an error too.
        std::fs::write(
            &policy,
            r#"{"version":1,"id":"cli","tier":"history","window":{},"protect":{"branch_heads":false},"x":1}"#,
        )
        .unwrap();
        let error = run(path.clone(), project, plan_args(None, PlanFormat::Json))
            .await
            .unwrap_err()
            .to_string();
        for code in [
            "policy.window_empty",
            "policy.heads_unprotected",
            "policy.unknown_field",
        ] {
            assert!(error.contains(code), "{error}");
        }
        std::fs::remove_file(&policy).unwrap();
        assert!(
            run(path.clone(), project, plan_args(None, PlanFormat::Json))
                .await
                .is_err()
        );
        // A missing database is an error and is never created.
        let missing = path.with_extension("absent");
        assert!(
            run(missing.clone(), project, plan_args(None, PlanFormat::Json))
                .await
                .is_err()
        );
        assert!(!missing.exists());
        // A time rule takes the instant from the flag, not from a clock.
        std::fs::write(
            &policy,
            r#"{"version":1,"id":"days","tier":"content","window":{"keep_within_days":1}}"#,
        )
        .unwrap();
        let timed_out = dir.join("timed.json");
        run(
            path.clone(),
            project,
            Action::HorizonPlan {
                policy: policy.clone(),
                list: 0,
                as_of_ms: Some(1_700_000_000_000),
                out: Some(timed_out.clone()),
                format: PlanFormat::Text,
            },
        )
        .await
        .unwrap();
        let timed: serde_json::Value =
            serde_json::from_slice(&std::fs::read(&timed_out).unwrap()).unwrap();
        assert_eq!(timed["as_of_ms"], 1_700_000_000_000i64);
        std::fs::remove_dir_all(&dir).unwrap();
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
