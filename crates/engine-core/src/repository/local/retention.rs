//! Read-only retention planning for one local project repository.
//!
//! The plan reports which retained commits and objects no branch head or recovery
//! provenance reference can reach. It never deletes, rewrites, signs or publishes
//! anything: every query runs in one read snapshot on a connection that SQLite
//! itself opens with `query_only`. Deleting what the plan lists is a separate
//! policy decision that is deliberately not implemented (docs/LOCAL-RETENTION-PLAN.md).
//!
//! Complexity, with C retained commits, O retained objects and F file entries
//! summed over all commits: one pass over the commit table (C signature checks),
//! O(C + parent/source edges) graph work, one point read per *distinct* reachable
//! object (each hashed once), and at most F set lookups for file roots. Memory is
//! O(C + O) fixed-size keys (about 100 bytes per commit and 64 per object); no file
//! state is reconstructed and only one encoded object (at most 1 MiB) and one
//! page of commit bodies are resident at a time.

use super::{decode_commit, storage, LocalRepository, StorageUsage};
use crate::{
    repository::{hex, unhex, Error, SignedCommit},
    shared_checkpoint::{self, Address},
};
use serde::Serialize;
use sqlx::{Connection, SqliteConnection};
use std::collections::{HashSet, VecDeque};

pub const FORMAT: &str = "dynodoc.local-retention-plan.v1";
/// Retained objects the planner will track in memory; larger repositories need a
/// streaming implementation with an external index and are refused explicitly.
pub const MAX_TRACKED_OBJECTS: i64 = 4_000_000;
pub const MAX_TRACKED_COMMITS: i64 = 1_000_000;
pub const MAX_TOP_REASONS: usize = 100;
pub const MAX_LISTED: usize = 1000;
const COMMIT_PAGE: i64 = 16;
const OBJECT_PAGE: i64 = 256;
const NONE: u32 = u32::MAX;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct GcPlanOptions {
    /// Most pinned reasons (largest first) included in the result.
    pub top_reasons: usize,
    /// Most unreachable commits and staged objects listed as examples.
    pub list: usize,
}

impl Default for GcPlanOptions {
    fn default() -> Self {
        Self {
            top_reasons: 10,
            list: 10,
        }
    }
}

impl GcPlanOptions {
    fn validate(self) -> Result<(), Error> {
        if self.top_reasons == 0 || self.top_reasons > MAX_TOP_REASONS {
            return Err(Error::Limit("retention plan reason count"));
        }
        if self.list > MAX_LISTED {
            return Err(Error::Limit("retention plan example count"));
        }
        Ok(())
    }
}

/// Encoded payload totals. `bytes` is `object_bytes + commit_bytes`, the same
/// quantity the persistent capacity accounting charges.
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize)]
pub struct Counts {
    pub objects: u64,
    pub object_bytes: u64,
    pub commits: u64,
    pub commit_bytes: u64,
    pub bytes: u64,
}

impl Counts {
    fn object(&mut self, bytes: usize) {
        self.objects += 1;
        self.object_bytes += bytes as u64;
        self.bytes += bytes as u64;
    }
    fn commit(&mut self, bytes: usize) {
        self.commits += 1;
        self.commit_bytes += bytes as u64;
        self.bytes += bytes as u64;
    }
    fn add(&mut self, other: &Counts) {
        self.objects += other.objects;
        self.object_bytes += other.object_bytes;
        self.commits += other.commits;
        self.commit_bytes += other.commit_bytes;
        self.bytes += other.bytes;
    }
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
pub struct BranchHead {
    pub name: String,
    pub commit: Address,
    pub generation: i64,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
pub struct Category {
    pub category: &'static str,
    #[serde(flatten)]
    pub counts: Counts,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
pub struct Reason {
    pub reason: String,
    #[serde(flatten)]
    pub counts: Counts,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
pub struct Unreachable {
    #[serde(flatten)]
    pub counts: Counts,
    pub categories: Vec<Category>,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
pub struct Pinned {
    pub reasons: Vec<Reason>,
    pub omitted_reasons: usize,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
pub struct CommitExample {
    pub commit: Address,
    pub category: &'static str,
    pub bytes: u64,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
pub struct ObjectExample {
    pub document: uuid::Uuid,
    pub address: Address,
    pub bytes: u64,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
pub struct Examples {
    pub commits: Vec<CommitExample>,
    pub staged_objects: Vec<ObjectExample>,
}

/// Cross-check against persistent capacity counters (schema 2 only).
#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
pub struct Accounting {
    pub recorded: Option<StorageUsage>,
    pub consistent: Option<bool>,
}

/// Deterministic, side-effect-free description of what retention could affect.
#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
pub struct GcPlan {
    pub format: &'static str,
    pub project: uuid::Uuid,
    /// Always true: this value describes data; no operation was performed.
    pub read_only: bool,
    pub local_schema: i64,
    pub branches: Vec<BranchHead>,
    pub retained: Counts,
    pub reachable: Counts,
    pub unreachable: Unreachable,
    /// Encoded payload bytes only (unreachable objects + commits). Physical file
    /// size, WAL, indexes and free pages are outside this estimate.
    pub reclaimable_bytes_estimate: u64,
    pub pinned: Pinned,
    pub examples: Examples,
    pub accounting: Accounting,
}

impl GcPlan {
    /// Stable plain-text rendering of the same facts, for terminals and logs.
    pub fn summary(&self) -> String {
        let line = |name: &str, c: &Counts| {
            format!(
                "  {name:<12} objects {:>9} ({} bytes)  commits {:>7} ({} bytes)\n",
                c.objects, c.object_bytes, c.commits, c.commit_bytes
            )
        };
        let mut text = format!(
            "Local retention plan (read-only: nothing was deleted, written or signed)\nproject {}\nlocal schema {}\n",
            self.project, self.local_schema
        );
        text.push_str(&format!("branches {}\n", self.branches.len()));
        for branch in &self.branches {
            text.push_str(&format!(
                "  {} -> {} (generation {})\n",
                branch.name,
                branch.commit.as_str(),
                branch.generation
            ));
        }
        text.push_str("retained\n");
        text.push_str(&line("total", &self.retained));
        text.push_str(&line("reachable", &self.reachable));
        text.push_str(&line("unreachable", &self.unreachable.counts));
        for category in &self.unreachable.categories {
            text.push_str(&line(category.category, &category.counts));
        }
        text.push_str(&format!(
            "reclaimable encoded bytes (estimate, not physical file size): {}\n",
            self.reclaimable_bytes_estimate
        ));
        text.push_str("pinned by (largest first)\n");
        for reason in &self.pinned.reasons {
            text.push_str(&line(&reason.reason, &reason.counts));
        }
        if self.pinned.omitted_reasons > 0 {
            text.push_str(&format!(
                "  ... {} smaller reasons omitted\n",
                self.pinned.omitted_reasons
            ));
        }
        if !self.examples.commits.is_empty() {
            text.push_str("unreachable commits (examples)\n");
            for example in &self.examples.commits {
                text.push_str(&format!(
                    "  {} {} {} bytes\n",
                    example.commit.as_str(),
                    example.category,
                    example.bytes
                ));
            }
        }
        if !self.examples.staged_objects.is_empty() {
            text.push_str("staged objects (examples)\n");
            for example in &self.examples.staged_objects {
                text.push_str(&format!(
                    "  {} {} {} bytes\n",
                    example.document,
                    example.address.as_str(),
                    example.bytes
                ));
            }
        }
        match &self.accounting.consistent {
            Some(true) => text.push_str("capacity accounting: consistent with scanned rows\n"),
            Some(false) => text.push_str("capacity accounting: INCONSISTENT with scanned rows\n"),
            None => text.push_str("capacity accounting: not recorded (schema 1)\n"),
        }
        text
    }
}

type ObjectKey = (u128, [u8; 32]);
/// Raw `(parents, recovery sources)` of one commit, before index resolution.
type RawEdges = (Vec<[u8; 32]>, Vec<[u8; 32]>);

struct Node {
    address: [u8; 32],
    bytes: usize,
    parents: Vec<u32>,
    sources: Vec<u32>,
}

impl Node {
    fn dependencies(&self) -> impl Iterator<Item = u32> + '_ {
        self.parents.iter().chain(&self.sources).copied()
    }
}

struct Bucket {
    label: String,
    counts: Counts,
}

fn raw(address: &Address) -> Result<[u8; 32], Error> {
    unhex(address.as_str(), 32)?
        .try_into()
        .map_err(|_| Error::Invalid("object address"))
}

fn address_of(raw: &[u8; 32]) -> Result<Address, Error> {
    Ok(Address::try_from(hex(raw))?)
}

/// Commits are streamed in address order, so `nodes` is sorted by raw address.
fn index_of(nodes: &[Node], address: &[u8; 32]) -> Result<u32, Error> {
    match nodes.binary_search_by(|node| node.address.cmp(address)) {
        Ok(index) => Ok(index as u32),
        Err(_) => Err(Error::Missing(address_of(address)?)),
    }
}

impl LocalRepository {
    /// Describe unreachable commits and objects without changing anything.
    ///
    /// Roots are every branch head. A commit keeps its ancestry parents and any
    /// recovery source/mainline reachable. An object is reachable when a reachable
    /// commit's file state graph references it; the same bytes under another
    /// document are a different object. See `docs/LOCAL-RETENTION-PLAN.md`.
    pub async fn gc_plan(&self, options: GcPlanOptions) -> Result<GcPlan, Error> {
        options.validate()?;
        // A private connection that SQLite itself refuses to write through.
        let mut connection = SqliteConnection::connect_with(
            &self
                .options
                .clone()
                .create_if_missing(false)
                .pragma("query_only", "ON"),
        )
        .await?;
        let result = plan(&mut connection, self.project, options).await;
        let _ = connection.close().await;
        result
    }
}

async fn plan(
    connection: &mut SqliteConnection,
    project: uuid::Uuid,
    options: GcPlanOptions,
) -> Result<GcPlan, Error> {
    let mut tx = connection.begin().await?;
    let (schema, bound): (i64, String) =
        sqlx::query_as("select version,project from local_meta where singleton=1")
            .fetch_one(&mut *tx)
            .await?;
    if bound != project.to_string() || !matches!(schema, 1 | storage::SCHEMA_VERSION) {
        return Err(Error::Invalid("local schema/project"));
    }
    let (object_rows, object_bytes): (i64, i64) =
        sqlx::query_as("select count(*),coalesce(sum(length(bytes)),0) from objects")
            .fetch_one(&mut *tx)
            .await?;
    let commit_rows: i64 = sqlx::query_scalar("select count(*) from commits")
        .fetch_one(&mut *tx)
        .await?;
    if object_rows > MAX_TRACKED_OBJECTS {
        return Err(Error::Limit("retention plan tracked objects"));
    }
    if commit_rows > MAX_TRACKED_COMMITS {
        return Err(Error::Limit("retention plan tracked commits"));
    }
    let heads: Vec<(String, String, i64)> =
        sqlx::query_as("select name,head,generation from refs order by name")
            .fetch_all(&mut *tx)
            .await?;
    let branches = heads
        .into_iter()
        .map(|(name, head, generation)| {
            Ok(BranchHead {
                name,
                commit: Address::try_from(head)?,
                generation,
            })
        })
        .collect::<Result<Vec<_>, Error>>()?;

    // Phase 1: verified commit graph, streamed in address order.
    let mut nodes: Vec<Node> = Vec::new();
    let mut raw_edges: Vec<RawEdges> = Vec::new();
    let mut after = String::new();
    loop {
        let rows: Vec<(String, Vec<u8>)> = sqlx::query_as(
            "select address,body from commits where address>? order by address limit ?",
        )
        .bind(&after)
        .bind(COMMIT_PAGE)
        .fetch_all(&mut *tx)
        .await?;
        let Some(last) = rows.last() else { break };
        after = last.0.clone();
        for (address, body) in rows {
            let address = Address::try_from(address)?;
            let signed = decode_commit(&body, &address, project)?;
            let dependencies = signed.commit.dependencies();
            let (parents, sources) = dependencies.split_at(signed.commit.parents.len());
            nodes.push(Node {
                address: raw(&address)?,
                bytes: body.len(),
                parents: Vec::new(),
                sources: Vec::new(),
            });
            raw_edges.push((
                parents.iter().map(raw).collect::<Result<_, _>>()?,
                sources.iter().map(raw).collect::<Result<_, _>>()?,
            ));
        }
    }
    for (i, (parents, sources)) in raw_edges.iter().enumerate() {
        let resolve = |list: &[[u8; 32]]| -> Result<Vec<u32>, Error> {
            list.iter().map(|a| index_of(&nodes, a)).collect()
        };
        let (parents, sources) = (resolve(parents)?, resolve(sources)?);
        nodes[i].parents = parents;
        nodes[i].sources = sources;
    }
    drop(raw_edges);

    // Phase 2: reachability and attribution. Buckets 2i and 2i+1 are branch i's
    // head and history; the last three are recovery, superseded and orphaned.
    let count = nodes.len();
    let mut buckets: Vec<Bucket> = Vec::new();
    for branch in &branches {
        for kind in ["branch-head", "branch-history"] {
            buckets.push(Bucket {
                label: format!("{kind}:{}", branch.name),
                counts: Counts::default(),
            });
        }
    }
    let recovery = buckets.len() as u32;
    for label in ["recovery-source", "superseded", "orphaned"] {
        buckets.push(Bucket {
            label: label.into(),
            counts: Counts::default(),
        });
    }
    let (superseded, orphaned) = (recovery + 1, recovery + 2);
    let mut class = vec![NONE; count];
    let mut order: Vec<u32> = Vec::new();
    let mut head_index = Vec::with_capacity(branches.len());
    for (i, branch) in branches.iter().enumerate() {
        let head = index_of(&nodes, &raw(&branch.commit)?)?;
        head_index.push(head);
        if class[head as usize] == NONE {
            class[head as usize] = 2 * i as u32;
            order.push(head);
        }
    }
    let mut seen = vec![false; count];
    for (i, head) in head_index.iter().enumerate() {
        if std::mem::replace(&mut seen[*head as usize], true) {
            continue;
        }
        let mut queue = VecDeque::from([*head]);
        while let Some(current) = queue.pop_front() {
            for &parent in &nodes[current as usize].parents {
                if !std::mem::replace(&mut seen[parent as usize], true) {
                    if class[parent as usize] == NONE {
                        class[parent as usize] = 2 * i as u32 + 1;
                        order.push(parent);
                    }
                    queue.push_back(parent);
                }
            }
        }
    }
    // Provenance: sources of any retained commit, then everything they depend on.
    let retained_by_ancestry = order.clone();
    let mut stack: Vec<u32> = Vec::new();
    for &commit in &retained_by_ancestry {
        for &source in &nodes[commit as usize].sources {
            if class[source as usize] == NONE {
                class[source as usize] = recovery;
                order.push(source);
                stack.push(source);
            }
        }
    }
    while let Some(current) = stack.pop() {
        for dependency in nodes[current as usize].dependencies() {
            if class[dependency as usize] == NONE {
                class[dependency as usize] = recovery;
                order.push(dependency);
                stack.push(dependency);
            }
        }
    }
    let reachable_commits = order.len();
    // An unreachable commit that depends (transitively) on retained history is a
    // superseded alternative line; one with no such link is orphaned.
    let mut state = vec![0u8; count]; // 0 new, 1 active, 2 disconnected, 3 connected
    for start in 0..count {
        if class[start] != NONE || state[start] != 0 {
            continue;
        }
        state[start] = 1;
        let mut frames: Vec<(usize, usize, bool)> = vec![(start, 0, false)];
        while let Some(frame) = frames.last_mut() {
            let (node, next, hit) = *frame;
            let total = nodes[node].parents.len() + nodes[node].sources.len();
            if hit || next == total {
                state[node] = if hit { 3 } else { 2 };
                frames.pop();
                if let Some(parent) = frames.last_mut() {
                    parent.2 |= hit;
                }
                continue;
            }
            frame.1 += 1;
            let parents = &nodes[node].parents;
            let dependency = match parents.get(next) {
                Some(parent) => *parent,
                None => nodes[node].sources[next - parents.len()],
            } as usize;
            if class[dependency] != NONE || state[dependency] == 3 {
                frame.2 = true;
            } else if state[dependency] == 0 {
                state[dependency] = 1;
                frames.push((dependency, 0, false));
            }
        }
    }
    let mut unreachable_commits = Vec::new();
    for index in 0..count {
        if class[index] == NONE {
            class[index] = if state[index] == 3 {
                superseded
            } else {
                orphaned
            };
            unreachable_commits.push(index as u32);
        }
    }
    let mut visit_order = order;
    // Strongest reachable reasons first, then superseded lines, then orphans.
    visit_order.extend(
        unreachable_commits
            .iter()
            .copied()
            .filter(|i| class[*i as usize] == superseded),
    );
    visit_order.extend(
        unreachable_commits
            .iter()
            .copied()
            .filter(|i| class[*i as usize] == orphaned),
    );

    // Phase 3: each commit's file roots, then every object not yet attributed.
    let mut visited: HashSet<ObjectKey> = HashSet::new();
    for &commit in &visit_order {
        let node = &nodes[commit as usize];
        let bucket = &mut buckets[class[commit as usize] as usize];
        bucket.counts.commit(node.bytes);
        let body: Vec<u8> = sqlx::query_scalar("select body from commits where address=?")
            .bind(hex(&node.address))
            .fetch_one(&mut *tx)
            .await?;
        // Phase 1 verified these exact rows in this same snapshot.
        let signed: SignedCommit = serde_json::from_slice(&body)?;
        for file in signed.commit.files.values() {
            let root = (file.document.0.as_u128(), raw(&file.state)?);
            walk(&mut tx, &mut visited, &mut bucket.counts, root).await?;
        }
    }

    // Phase 4: staged = stored but referenced by no stored commit.
    let mut retained = Counts::default();
    retained.objects = object_rows as u64;
    retained.object_bytes = object_bytes as u64;
    retained.commits = count as u64;
    retained.commit_bytes = nodes.iter().map(|n| n.bytes as u64).sum();
    retained.bytes = retained.object_bytes + retained.commit_bytes;
    let mut categories = [
        Category {
            category: "staged",
            counts: Counts::default(),
        },
        Category {
            category: "orphaned",
            counts: buckets[orphaned as usize].counts.clone(),
        },
        Category {
            category: "superseded",
            counts: buckets[superseded as usize].counts.clone(),
        },
    ];
    let attributed: u64 = buckets.iter().map(|b| b.counts.object_bytes).sum();
    let attributed_objects: u64 = buckets.iter().map(|b| b.counts.objects).sum();
    if attributed_objects != visited.len() as u64 || attributed > retained.object_bytes {
        return Err(Error::Invalid("retention plan accounting"));
    }
    categories[0].counts.objects = retained.objects - attributed_objects;
    categories[0].counts.object_bytes = retained.object_bytes - attributed;
    categories[0].counts.bytes = categories[0].counts.object_bytes;
    let mut unreachable_counts = Counts::default();
    for category in &categories {
        unreachable_counts.add(&category.counts);
    }
    let mut reachable = Counts::default();
    let mut reasons: Vec<Reason> = Vec::new();
    for bucket in &buckets[..recovery as usize + 1] {
        reachable.add(&bucket.counts);
        if bucket.counts.objects + bucket.counts.commits > 0 {
            reasons.push(Reason {
                reason: bucket.label.clone(),
                counts: bucket.counts.clone(),
            });
        }
    }
    reasons.sort_by(|a, b| {
        b.counts
            .bytes
            .cmp(&a.counts.bytes)
            .then_with(|| a.reason.cmp(&b.reason))
    });
    let omitted_reasons = reasons.len().saturating_sub(options.top_reasons);
    reasons.truncate(options.top_reasons);
    debug_assert_eq!(reachable.commits as usize, reachable_commits);

    // Phase 5: bounded examples in key order.
    let mut examples = Examples {
        commits: Vec::new(),
        staged_objects: Vec::new(),
    };
    for &index in unreachable_commits.iter().take(options.list) {
        let node = &nodes[index as usize];
        examples.commits.push(CommitExample {
            commit: address_of(&node.address)?,
            category: if class[index as usize] == superseded {
                "superseded"
            } else {
                "orphaned"
            },
            bytes: node.bytes as u64,
        });
    }
    if options.list > 0 && categories[0].counts.objects > 0 {
        let (mut document, mut address) = (String::new(), String::new());
        'scan: loop {
            let rows: Vec<(String, String, i64)> = sqlx::query_as(
                "select document,address,length(bytes) from objects where (document,address)>(?,?) order by document,address limit ?",
            )
            .bind(&document)
            .bind(&address)
            .bind(OBJECT_PAGE)
            .fetch_all(&mut *tx)
            .await?;
            let Some(last) = rows.last() else { break };
            (document, address) = (last.0.clone(), last.1.clone());
            for (doc, addr, bytes) in rows {
                let parsed = uuid::Uuid::parse_str(&doc)
                    .map_err(|_| Error::Invalid("stored document id"))?;
                let address = Address::try_from(addr)?;
                if !visited.contains(&(parsed.as_u128(), raw(&address)?)) {
                    examples.staged_objects.push(ObjectExample {
                        document: parsed,
                        address,
                        bytes: bytes as u64,
                    });
                    if examples.staged_objects.len() == options.list {
                        break 'scan;
                    }
                }
            }
        }
    }
    let accounting = if schema == storage::SCHEMA_VERSION {
        let usage = storage::usage(&mut tx).await?;
        let consistent = usage.bytes_used as u64 == retained.bytes
            && usage.objects_used as u64 == retained.objects
            && usage.commits_used as u64 == retained.commits
            && usage.branches_used as usize == branches.len();
        Accounting {
            recorded: Some(usage),
            consistent: Some(consistent),
        }
    } else {
        Accounting {
            recorded: None,
            consistent: None,
        }
    };
    tx.rollback().await?;
    let [staged, orphaned_category, superseded_category] = categories;
    Ok(GcPlan {
        format: FORMAT,
        project,
        read_only: true,
        local_schema: schema,
        branches,
        retained,
        reachable,
        reclaimable_bytes_estimate: unreachable_counts.bytes,
        unreachable: Unreachable {
            counts: unreachable_counts,
            categories: vec![staged, orphaned_category, superseded_category],
        },
        pinned: Pinned {
            reasons,
            omitted_reasons,
        },
        examples,
        accounting,
    })
}

/// Visit every not-yet-attributed object below `root`, charging `counts`. One
/// encoded object is resident at a time; shared subtrees are read once.
async fn walk(
    connection: &mut SqliteConnection,
    visited: &mut HashSet<ObjectKey>,
    counts: &mut Counts,
    root: ObjectKey,
) -> Result<(), Error> {
    if visited.contains(&root) {
        return Ok(());
    }
    let mut stack = vec![root];
    while let Some(key) = stack.pop() {
        if !visited.insert(key) {
            continue;
        }
        let address = address_of(&key.1)?;
        let bytes: Vec<u8> = sqlx::query_scalar(
            "select bytes from objects where document=? and address=? and length(bytes)<=1048576",
        )
        .bind(uuid::Uuid::from_u128(key.0).to_string())
        .bind(address.as_str())
        .fetch_optional(&mut *connection)
        .await?
        .ok_or_else(|| shared_checkpoint::Error::Missing(address.as_str().into()))?;
        counts.object(bytes.len());
        for child in shared_checkpoint::references(&address, &bytes)? {
            let child = (key.0, raw(&child)?);
            if !visited.contains(&child) {
                stack.push(child);
            }
        }
    }
    Ok(())
}
