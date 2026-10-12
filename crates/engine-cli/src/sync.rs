//! Selected commit transfer. The separate receipt journal is remote tracking;
//! fetch/push never move local working branches or write a native document.
use anyhow::{anyhow, ensure, Context};
use base64::{engine::general_purpose::STANDARD, Engine};
use clap::{Args, Subcommand};
use engine_core::{
    repository::{
        attestation::{Pin, Scope, SignedReceipt},
        local::{LocalRepository, ObjectKey, ObjectPacket},
        receipt_journal::ReceiptJournal,
        Head, SignedCommit, MAX_COMMIT_BYTES,
    },
    shared_checkpoint::{self, typed, Address, Limits, ObjectStore},
};
use engine_shared::DocumentId;
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use std::{
    collections::{BTreeMap, BTreeSet},
    io::Read,
    path::{Path, PathBuf},
};
use uuid::Uuid;
mod http;
use http::{Client, Remote};
#[cfg(test)]
mod tests;
mod transfer;
const MAX_COMMITS: usize = 4096;
const MAX_PLAN_BYTES: usize = 64 * 1024 * 1024;
const GRAPH_OBJECTS: usize = 65536;
const GRAPH_BYTES: usize = 64 * 1024 * 1024;

#[derive(Args)]
pub struct Options {
    /// Local SQLite repository; fetch adds verified history but never changes its branches.
    repository: PathBuf,
    project: Uuid,
    /// Independently prepared JSON with version, url, authority and public_key.
    #[arg(long)]
    remote: PathBuf,
    /// Account access token in a private file; never pass the token as an argument.
    #[arg(long)]
    token_file: PathBuf,
    /// Separate durable client receipt journal (must not be the repository database).
    #[arg(long)]
    journal: PathBuf,
    #[arg(long)]
    branch: String,
    /// Synthetic local tests only. HTTPS is required for other addresses.
    #[arg(long)]
    allow_loopback_http: bool,
    #[command(subcommand)]
    action: Action,
}
#[derive(Subcommand)]
pub enum Action {
    /// Preview every newly visible commit (including ancestry/recovery sources).
    Plan {
        candidate: String,
        /// Write a new file; refuses to overwrite an existing review plan.
        #[arg(long)]
        out: PathBuf,
        /// Publish for review without advancing the selected remote branch.
        #[arg(long)]
        publish_only: bool,
    },
    /// Execute an explicitly reviewed plan; never publishes unlisted dependencies.
    Push { plan: PathBuf },
    /// Retrieve the selected branch's signed history and advance its separate pin.
    Fetch,
    /// Fetch and fast-forward an explicitly selected local branch using exact local CAS.
    /// Divergent history needs a separate comparison/merge. Never writes a native file.
    Pull {
        local_branch: String,
        #[arg(long, requires = "expected_generation")]
        expected: Option<String>,
        #[arg(long, requires = "expected")]
        expected_generation: Option<i64>,
    },
    /// Read the last verified remote head offline, without contacting the server.
    Tracking,
}
#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct Plan {
    version: u8,
    remote: Remote,
    project: Uuid,
    branch: String,
    candidate: Address,
    expected: Option<Head>,
    update_branch: bool,
    /// Topological signed manifests: explicit disclosure of messages, paths and sources.
    publish: Vec<SignedCommit>,
}
fn read(path: &Path, cap: usize) -> anyhow::Result<Vec<u8>> {
    let mut bytes = Vec::new();
    std::fs::File::open(path)?
        .take(cap as u64 + 1)
        .read_to_end(&mut bytes)?;
    ensure!(bytes.len() <= cap, "input exceeds byte limit");
    Ok(bytes)
}
pub async fn run(options: Options) -> anyhow::Result<()> {
    ensure!(
        engine_core::repository::valid_branch(&options.branch),
        "invalid branch"
    );
    let remote: Remote = serde_json::from_slice(&read(&options.remote, 8192)?)?;
    let scope = Scope {
        authority: remote.authority,
        project: options.project,
        branch: options.branch,
    };
    // Open the repository first so the journal refuses aliases to it, even on first use.
    let repo = LocalRepository::open(&options.repository, options.project).await?;
    let journal = ReceiptJournal::open(&options.journal).await?;
    let result = async {
        if matches!(options.action, Action::Tracking) { return Ok(serde_json::to_value(journal.pin(&scope).await?)?); }
        let token = String::from_utf8(read(&options.token_file, 16384)?)?;
        let client = Client::new(remote, options.project, token.trim(), options.allow_loopback_http)?;
        client.authority().await?;
        match options.action {
            Action::Plan { candidate, out, publish_only } => {
                let plan = plan(&repo, &client, &journal, &scope, Address::try_from(candidate)?, !publish_only).await?;
                let bytes = serde_json::to_vec_pretty(&plan)?;
                ensure!(bytes.len() <= MAX_PLAN_BYTES, "review plan byte limit");
                use std::io::Write;
                let mut file = std::fs::OpenOptions::new().write(true).create_new(true).open(out)?;
                file.write_all(&bytes)?;
                file.sync_all()?;
                Ok(json!({"commits_to_publish":plan.publish.len(),"candidate":plan.candidate,"update_branch":plan.update_branch,"expected":plan.expected,"instruction":"Review the complete plan, including parent and recovery commits, before sync push."}))
            }
            Action::Push { plan: input } => {
                let approved: Plan = serde_json::from_slice(&read(&input, MAX_PLAN_BYTES)?)?;
                push(&repo, &client, &journal, &scope, approved).await
            }
            Action::Fetch => {
                let receipt = client.head(&scope).await?.ok_or_else(|| anyhow!("remote branch is absent; local tracking is retained"))?;
                proof(&client, &journal, &scope, &receipt).await?;
                let count = transfer::fetch(&repo, &client, &receipt.receipt.head.commit).await?;
                track(&client, &journal, &scope, receipt.clone()).await?;
                Ok(json!({"head":receipt.receipt.head,"new_commits":count,"local_branches_changed":false}))
            }
            Action::Pull { local_branch, expected, expected_generation } => {
                let expected = match (expected,expected_generation) {
                    (Some(address),Some(generation)) => Some(Head { commit:Address::try_from(address)?,generation }),
                    (None,None) => None,
                    _ => anyhow::bail!("expected local address and generation must be supplied together"),
                };
                pull(&repo,&client,&journal,&scope,&local_branch,expected.as_ref()).await
            }
            Action::Tracking => unreachable!(),
        }
    }.await;
    journal.close().await;
    repo.close().await;
    println!("{}", serde_json::to_string_pretty(&result?)?);
    Ok(())
}

async fn pull(
    repo: &LocalRepository,
    client: &Client,
    journal: &ReceiptJournal,
    scope: &Scope,
    branch: &str,
    expected: Option<&Head>,
) -> anyhow::Result<Value> {
    ensure!(
        engine_core::repository::valid_branch(branch),
        "invalid local branch"
    );
    ensure!(
        repo.head(branch).await?.as_ref() == expected,
        "local branch changed; inspect its current address and generation"
    );
    let receipt = client
        .head(scope)
        .await?
        .ok_or_else(|| anyhow!("remote branch is absent"))?;
    proof(client, journal, scope, &receipt).await?;
    transfer::fetch(repo, client, &receipt.receipt.head.commit).await?;
    track(client, journal, scope, receipt.clone()).await?;
    if let Some(head) = expected {
        if head.commit == receipt.receipt.head.commit {
            ensure!(
                repo.head(branch).await?.as_ref() == expected,
                "local branch changed during pull"
            );
            return Ok(
                json!({"local_head":head,"remote_head":receipt.receipt.head,"local_branch_changed":false,"native_file_changed":false}),
            );
        }
        ensure!(
            repo.merge_bases(&head.commit, &receipt.receipt.head.commit)
                .await?
                .contains(&head.commit),
            "local history diverged; fetched history is retained, compare and resolve explicitly"
        );
    }
    let head = repo
        .compare_and_swap(branch, expected, &receipt.receipt.head.commit)
        .await?;
    Ok(
        json!({"local_head":head,"remote_head":receipt.receipt.head,"local_branch_changed":true,"native_file_changed":false}),
    )
}

/// Verify a bounded contiguous proof in memory before retaining its pages. An
/// initial checkpoint trusts the independently supplied authority, not old history.
async fn proof(
    client: &Client,
    journal: &ReceiptJournal,
    scope: &Scope,
    target: &SignedReceipt,
) -> anyhow::Result<Vec<SignedReceipt>> {
    Pin::trust_checkpoint(scope, &client.remote.public_key, target.clone())?;
    let Some(mut pin) = journal.pin(scope).await? else {
        return Ok(vec![]);
    };
    ensure!(!pin.revoked(), "remote authority is locally revoked");
    ensure!(
        pin.receipt().receipt.issuer_key == client.remote.public_key,
        "configured key differs from journal; use explicit authority rotation"
    );
    ensure!(
        pin.receipt().receipt.head.generation <= target.receipt.head.generation,
        "remote rollback below pinned generation"
    );
    let mut proof = Vec::new();
    while pin.receipt().receipt.head.generation < target.receipt.head.generation {
        #[derive(Deserialize)]
        struct Page {
            items: Vec<SignedReceipt>,
        }
        let remaining = target.receipt.head.generation - pin.receipt().receipt.head.generation;
        let page: Page = client
            .get(
                "history/receipts",
                &[
                    ("branch", scope.branch.clone()),
                    (
                        "after_generation",
                        pin.receipt().receipt.head.generation.to_string(),
                    ),
                    ("limit", remaining.min(256).to_string()),
                ],
                2 * 1024 * 1024,
            )
            .await?;
        ensure!(
            !page.items.is_empty()
                && page.items.len() <= remaining.min(256) as usize
                && proof.len() + page.items.len() <= MAX_COMMITS,
            "receipt catch-up limit or missing proof; fetch in smaller trusted checkpoints"
        );
        let next = pin.advance(&page.items)?;
        ensure!(
            next.receipt().receipt.head.generation > pin.receipt().receipt.head.generation,
            "receipt proof made no progress"
        );
        proof.extend(page.items);
        pin = next;
    }
    ensure!(pin.receipt() == target, "remote fork at pinned generation");
    Ok(proof)
}
async fn track(
    client: &Client,
    journal: &ReceiptJournal,
    scope: &Scope,
    receipt: SignedReceipt,
) -> anyhow::Result<()> {
    let checked = proof(client, journal, scope, &receipt).await?;
    if journal.pin(scope).await?.is_none() {
        journal
            .trust_checkpoint(scope, &client.remote.public_key, receipt)
            .await?;
    } else if checked.is_empty() {
        // Recheck inside a transaction: another process may have advanced/revoked.
        journal.advance(scope, &[receipt]).await?;
    } else {
        for page in checked.chunks(256) {
            journal.advance(scope, page).await?;
        }
    }
    Ok(())
}

async fn plan(
    repo: &LocalRepository,
    client: &Client,
    journal: &ReceiptJournal,
    scope: &Scope,
    candidate: Address,
    update_branch: bool,
) -> anyhow::Result<Plan> {
    let head = client.head(scope).await?;
    if let Some(receipt) = &head {
        proof(client, journal, scope, receipt).await?;
    } else {
        ensure!(
            journal.pin(scope).await?.is_none(),
            "remote branch disappeared; tracking is retained"
        );
    }
    let mut stack = vec![(candidate.clone(), false)];
    let mut visited = BTreeSet::new();
    let mut waiting = BTreeMap::new();
    let mut publish = Vec::new();
    let mut bytes = 0;
    while let Some((address, exit)) = stack.pop() {
        if exit {
            if let Some(signed) = waiting.remove(&address) {
                publish.push(signed);
            }
            continue;
        }
        if !visited.insert(address.clone()) {
            continue;
        }
        ensure!(
            visited.len() <= MAX_COMMITS,
            "selected history exceeds transfer commit limit"
        );
        let signed = repo.commit(&address).await?;
        if client.published(&address).await?.is_some() {
            continue;
        }
        bytes += serde_json::to_vec(&signed)?.len();
        ensure!(bytes <= MAX_PLAN_BYTES / 2, "selected manifest byte limit");
        stack.push((address.clone(), true));
        for dependency in signed.commit.dependencies().into_iter().rev() {
            stack.push((dependency, false));
        }
        waiting.insert(address, signed);
    }
    Ok(Plan {
        version: 1,
        remote: client.remote.clone(),
        project: repo.project(),
        branch: scope.branch.clone(),
        candidate,
        expected: head.map(|r| r.receipt.head),
        update_branch,
        publish,
    })
}

async fn push(
    repo: &LocalRepository,
    client: &Client,
    journal: &ReceiptJournal,
    scope: &Scope,
    approved: Plan,
) -> anyhow::Result<Value> {
    ensure!(
        approved.version == 1
            && approved.remote == client.remote
            && approved.project == repo.project()
            && approved.branch == scope.branch,
        "plan does not match explicit remote/project/branch"
    );
    ensure!(approved.publish.len() <= MAX_COMMITS, "plan commit limit");
    let fresh = plan(
        repo,
        client,
        journal,
        scope,
        approved.candidate.clone(),
        approved.update_branch,
    )
    .await?;
    if approved.update_branch && fresh.expected != approved.expected {
        // Lost successful CAS response: verify the exact next generation, then fetch
        // receipts. Never repeat a stale mutation or overwrite someone else's work.
        let generation = approved
            .expected
            .as_ref()
            .map_or(1, |h| h.generation.saturating_add(1));
        ensure!(
            fresh.expected
                == Some(Head {
                    commit: approved.candidate.clone(),
                    generation
                }),
            "remote branch changed; fetch, compare and make a new plan"
        );
        let receipt = client
            .head(scope)
            .await?
            .ok_or_else(|| anyhow!("remote branch disappeared"))?;
        ensure!(
            Some(&receipt.receipt.head) == fresh.expected.as_ref(),
            "remote branch changed during retry"
        );
        track(client, journal, scope, receipt).await?;
        return Ok(
            json!({"head":fresh.expected,"already_applied":true,"local_branches_changed":false}),
        );
    }
    let mut permitted = BTreeMap::new();
    for signed in &approved.publish {
        let address = signed.verify()?;
        ensure!(
            signed.commit.project == repo.project() && permitted.insert(address, signed).is_none(),
            "invalid/duplicate approved commit"
        );
    }
    for signed in &fresh.publish {
        ensure!(
            permitted
                .get(&signed.verify()?)
                .is_some_and(|p| p.commit == signed.commit && p.signature == signed.signature),
            "new private dependency is absent from reviewed plan"
        );
    }
    for signed in &fresh.publish {
        transfer::upload(repo, client, signed).await?;
        client.post("staging/commits", json!(signed)).await?;
        client
            .post("history/commits", json!({"address":signed.verify()?}))
            .await?;
    }
    if !approved.update_branch {
        return Ok(json!({"candidate":approved.candidate,"published":true,"branch_changed":false}));
    }
    if approved
        .expected
        .as_ref()
        .is_some_and(|h| h.commit == approved.candidate)
    {
        let receipt = client
            .head(scope)
            .await?
            .ok_or_else(|| anyhow!("remote branch disappeared"))?;
        ensure!(
            Some(&receipt.receipt.head) == approved.expected.as_ref(),
            "remote branch changed during up-to-date check; fetch before tracking it"
        );
        track(client, journal, scope, receipt).await?;
        return Ok(
            json!({"head":approved.expected,"already_applied":true,"local_branches_changed":false}),
        );
    }
    let result = client.post("history/refs", json!({"branch":scope.branch,"candidate":approved.candidate,"expected":approved.expected})).await
        .context("candidate publication may have succeeded; retained candidates can be reviewed; fetch after any uncertain ref result")?;
    let receipt: SignedReceipt = serde_json::from_value(
        result
            .get("signed_receipt")
            .cloned()
            .ok_or_else(|| anyhow!("missing ref receipt"))?,
    )?;
    ensure!(
        receipt.receipt.head.commit == approved.candidate
            && receipt.receipt.head.generation
                == approved
                    .expected
                    .as_ref()
                    .map_or(1, |h| h.generation.saturating_add(1)),
        "unexpected accepted ref"
    );
    track(client, journal, scope, receipt.clone()).await?;
    Ok(json!({"head":receipt.receipt.head,"published":true,"local_branches_changed":false}))
}
