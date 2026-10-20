//! Read-only horizon planner (design section 6.1, phase P1).
//!
//! [`plan`] turns an in-memory [`HistoryView`] and a [`Policy`] into a
//! [`HorizonPlan`]: which commits stay as bodies, which would be released into a
//! skeleton, which file states and objects would be released, and the three Merkle
//! roots a horizon record would commit to. It reads nothing and writes nothing;
//! the SQLite loader that builds a view from a local repository is
//! `LocalRepository::horizon_plan` in `local/horizon_plan.rs`.
//!
//! Rules, in the order they run (design sections 4.2 and 6.1):
//!
//! 1. Every branch head stays, as does every held commit and every commit the
//!    policy names.
//! 2. `merge_bases`: for each branch matching the policy, every best common
//!    ancestor of its head and the `against` head stays (criss-cross gives several).
//! 3. The window: the first `max(keep_last_commits, min_commits_kept)` commits
//!    along first parents from each head, and every commit authored within
//!    `keep_within_days` of `as_of_ms`.
//! 4. Evidence closure, one level and not recursive: for a kept recovery (v2)
//!    commit its recovery source, mainline and first parent; for a kept same-file
//!    merge (v3) commit both parents and the unique merge base.
//! 5. Tier `history`: every body not kept becomes a skeleton entry (address, ordered
//!    parents, height). Tier `content`: every body stays; only file states shrink.
//!
//! Cost with `C` stored commits, `E` parent edges, `V` v3 commits and `O` objects:
//! `O(C + E)` graph work, one bounded ancestry walk per v3 commit and per matching
//! branch (worst case `O(V * C)`; each walk is capped at [`MAX_ANCESTORS`] like
//! the importer's), and `O(O)` for object accounting. Memory is about 150 bytes
//! per commit plus 4 bytes per file entry and 40 bytes per object.

use super::policy::{MergeInputs, Policy, Topology};
use super::{
    address_of, canonical_bytes, frontier_root, heads_digest, raw, released_states_root,
    retained_root, sha256, skeleton_root, Counts, Error, FrontierEdge, HeadRef, Hold,
    HorizonRecord, Invariant, PolicyRef, Roots, SkeletonEntry, StateKey, Tier, MAX_HOLDS,
    MAX_HOLD_REASON_BYTES, MAX_JSON_INT, RECORD_KIND, RECORD_VERSION,
};
use crate::{
    repository::{Commit, MAX_ANCESTORS},
    shared_checkpoint::Address,
};
use serde::Serialize;
use std::collections::{BTreeMap, BTreeSet, HashMap, HashSet, VecDeque};

pub const FORMAT: &str = "dynodoc.horizon-plan.v1";
/// Domain of the plan hash an applier is handed back (design section 6.2).
const HASH_DOMAIN: &[u8] = b"dynodoc.horizon-plan-hash\0v1\0";
pub const MAX_LISTED: usize = 1000;
const DAY_MS: i64 = 86_400_000;

type Raw = [u8; 32];

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct PlanOptions {
    /// The instant `keep_within_days` counts back from. Required exactly when the
    /// policy has that rule; it is echoed in the plan, never read from a clock here.
    pub as_of_ms: Option<i64>,
    /// Released and pinned commits listed as examples (0 to [`MAX_LISTED`]).
    pub list: usize,
}

impl Default for PlanOptions {
    fn default() -> Self {
        Self {
            as_of_ms: None,
            list: 10,
        }
    }
}

/// Per-object sizes and references of the typed state graphs, by dense index.
#[derive(Clone, Debug, Default)]
pub struct ObjectGraph {
    /// Logical (canonical encoded) bytes.
    pub sizes: Vec<u64>,
    pub children: Vec<Vec<u32>>,
}

struct ViewCommit {
    address: Raw,
    version: u8,
    parents: Vec<Raw>,
    /// Recovery source and mainline, when the commit is a v2 commit.
    recovery: Vec<Raw>,
    authored_at_ms: i64,
    bytes: u64,
    stored_height: Option<u64>,
    /// Interned states of the file entries, sorted and unique.
    files: Vec<u32>,
    /// v3 records: interned `[base, left, right]` states.
    inputs: Vec<[u32; 3]>,
}

/// Everything the planner reads, held in memory. Build it with the `add_*`
/// methods in any order: the plan depends on the set, not on insertion order.
pub struct HistoryView {
    project: uuid::Uuid,
    branches: Vec<HeadRef>,
    commits: Vec<ViewCommit>,
    seen: HashSet<Raw>,
    skeleton: Vec<SkeletonEntry>,
    holds: Vec<Hold>,
    states: Vec<(uuid::Uuid, Raw)>,
    state_index: HashMap<(uuid::Uuid, Raw), u32>,
    /// Object index of each interned state's root, when measured.
    state_objects: HashMap<u32, u32>,
    pub objects: ObjectGraph,
}

impl HistoryView {
    pub fn new(project: uuid::Uuid) -> Self {
        Self {
            project,
            branches: Vec::new(),
            commits: Vec::new(),
            seen: HashSet::new(),
            skeleton: Vec::new(),
            holds: Vec::new(),
            states: Vec::new(),
            state_index: HashMap::new(),
            state_objects: HashMap::new(),
            objects: ObjectGraph::default(),
        }
    }

    pub fn project(&self) -> uuid::Uuid {
        self.project
    }

    fn intern(&mut self, document: uuid::Uuid, root: Raw) -> u32 {
        let next = self.states.len() as u32;
        *self.state_index.entry((document, root)).or_insert_with(|| {
            self.states.push((document, root));
            next
        })
    }

    pub fn add_branch(&mut self, head: HeadRef) -> Result<(), Error> {
        if !crate::repository::valid_branch(&head.branch)
            || head.generation <= 0
            || self.branches.iter().any(|b| b.branch == head.branch)
        {
            return Err(Error::Format("branch head"));
        }
        self.branches.push(head);
        Ok(())
    }

    /// Add a stored commit the caller has already verified (signature, address,
    /// project). `bytes` is the stored envelope length; `stored_height` the stored
    /// height column, which the planner recomputes and compares.
    pub fn add_commit(
        &mut self,
        address: &Address,
        commit: &Commit,
        bytes: u64,
        stored_height: Option<u64>,
    ) -> Result<(), Error> {
        if commit.project != self.project {
            return Err(Error::Format("commit project"));
        }
        let address = raw(address);
        if !self.seen.insert(address) {
            return Ok(());
        }
        let mut files = Vec::with_capacity(commit.files.len());
        for entry in commit.files.values() {
            files.push(self.intern(entry.document.0, raw(&entry.state)));
        }
        files.sort_unstable();
        files.dedup();
        let mut inputs = Vec::with_capacity(commit.same_file_resolutions.len());
        for record in &commit.same_file_resolutions {
            let document = record.document.0;
            inputs.push([
                self.intern(document, raw(&record.base)),
                self.intern(document, raw(&record.left)),
                self.intern(document, raw(&record.right)),
            ]);
        }
        self.commits.push(ViewCommit {
            address,
            version: commit.version,
            parents: commit.parents.iter().map(raw).collect(),
            recovery: commit
                .recovery
                .iter()
                .flat_map(|source| std::iter::once(&source.commit).chain(source.mainline.iter()))
                .map(raw)
                .collect(),
            authored_at_ms: commit.authored_at_ms,
            bytes,
            stored_height,
            files,
            inputs,
        });
        Ok(())
    }

    /// A commit already released by an earlier horizon (design: the skeleton).
    pub fn add_prior_skeleton(&mut self, entry: SkeletonEntry) {
        self.skeleton.push(entry);
    }

    pub fn add_hold(&mut self, hold: Hold) -> Result<(), Error> {
        if hold.reason.is_empty()
            || hold.reason.len() > MAX_HOLD_REASON_BYTES
            || hold.reason.chars().any(char::is_control)
        {
            return Err(Error::Format("hold reason"));
        }
        self.holds.push(hold);
        Ok(())
    }

    /// Every distinct state some stored commit's file entry names, sorted.
    pub fn state_keys(&self) -> Vec<(uuid::Uuid, Address)> {
        let mut keys: Vec<_> = self
            .commits
            .iter()
            .flat_map(|c| c.files.iter())
            .map(|i| self.states[*i as usize])
            .collect::<BTreeSet<_>>()
            .into_iter()
            .collect();
        keys.sort();
        keys.into_iter()
            .map(|(document, root)| (document, address_of(&root)))
            .collect()
    }

    /// Declare which object of [`Self::objects`] is the root of a state.
    pub fn set_state_object(&mut self, document: uuid::Uuid, root: &Address, object: u32) {
        if let Some(state) = self.state_index.get(&(document, raw(root))) {
            self.state_objects.insert(*state, object);
        }
    }
}

// ---- the plan ----------------------------------------------------------------

#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
enum Reason {
    BranchHead,
    Hold,
    PolicyCommit,
    MergeBase,
    RecentCount,
    RecentTime,
    EvidenceRecovery,
    EvidenceMerge,
}

impl Reason {
    fn label(self) -> &'static str {
        match self {
            Self::BranchHead => "branch-head",
            Self::Hold => "hold",
            Self::PolicyCommit => "policy-commit",
            Self::MergeBase => "merge-base",
            Self::RecentCount => "recent-count",
            Self::RecentTime => "recent-time",
            Self::EvidenceRecovery => "evidence-recovery",
            Self::EvidenceMerge => "evidence-merge",
        }
    }
    /// Kept for its own sake, not as evidence for another kept commit.
    fn primary(self) -> bool {
        self <= Self::RecentTime
    }
    /// Listed among the pinned examples (neither a head nor window membership).
    fn pinned(self) -> bool {
        !matches!(
            self,
            Self::BranchHead | Self::RecentCount | Self::RecentTime
        )
    }
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
pub struct PlanCommits {
    pub total: u64,
    pub retained: u64,
    pub released: u64,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
pub struct PlanRoots {
    /// Over the retained commit addresses; not a record field.
    pub retained: Address,
    pub skeleton: Address,
    pub frontier: Address,
    pub released_states: Address,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
pub struct ReasonCount {
    pub reason: &'static str,
    pub commits: u64,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
pub struct ReleasedExample {
    pub commit: Address,
    pub bytes: u64,
    pub height: u64,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
pub struct PinnedExample {
    pub commit: Address,
    pub reason: &'static str,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
pub struct Examples {
    pub released: Vec<ReleasedExample>,
    pub pinned: Vec<PinnedExample>,
}

/// The full sets behind the roots. Not part of the plan JSON (the roots commit to
/// them); an applier or a test recomputes them or compares against the roots.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct PlanSets {
    /// Commits whose bodies stay, sorted. Every stored commit under tier `content`.
    pub retained: Vec<Address>,
    /// Commits released by this plan, sorted. Empty under tier `content`.
    pub released: Vec<Address>,
    /// Skeleton rows; empty under `topology: drop` even when commits are released.
    pub skeleton: Vec<SkeletonEntry>,
    pub frontier: Vec<FrontierEdge>,
    pub released_states: Vec<StateKey>,
    /// Why each kept body stays (the first reason that applies), sorted by address.
    pub kept: Vec<(Address, &'static str)>,
}

/// Deterministic, side-effect-free description of a horizon. Its canonical JSON
/// has no timestamp or host value; `plan_hash` binds everything except `examples`.
#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
pub struct HorizonPlan {
    pub format: &'static str,
    pub project: uuid::Uuid,
    /// Always true: this value describes data; no operation was performed.
    pub read_only: bool,
    pub plan_hash: String,
    pub tier: Tier,
    pub topology: &'static str,
    /// Present exactly when the policy has a `keep_within_days` rule.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub as_of_ms: Option<i64>,
    pub policy: PolicyRef,
    pub heads_digest: Address,
    pub heads: Vec<HeadRef>,
    pub commits: PlanCommits,
    pub counts: Counts,
    /// Whether every released state's objects were measured; false understates
    /// `objects_released` and `object_bytes_released`.
    pub objects_measured: bool,
    pub roots: PlanRoots,
    /// Why retained commits stay, by the first reason that applies; sums to the
    /// number of commits kept by a rule (all of `retained` under tier `history`).
    pub reasons: Vec<ReasonCount>,
    pub evidence: Vec<String>,
    pub holds: Vec<Hold>,
    pub warnings: Vec<String>,
    pub examples: Examples,
    #[serde(skip)]
    pub sets: PlanSets,
}

impl HorizonPlan {
    /// Canonical JSON bytes of the whole plan (sorted keys, compact).
    pub fn canonical_bytes(&self) -> Result<Vec<u8>, Error> {
        canonical_bytes(self)
    }

    fn compute_hash(&self) -> Result<String, Error> {
        let mut value = serde_json::to_value(self)?;
        let object = value.as_object_mut().expect("a plan is an object");
        object.remove("plan_hash");
        object.remove("examples");
        let bytes = canonical_bytes(&value)?;
        Ok(super::hex(&sha256(&[HASH_DOMAIN, &bytes])))
    }

    /// True when `plan_hash` is the hash of the content it claims to bind.
    pub fn hash_is_valid(&self) -> bool {
        self.compute_hash().is_ok_and(|hash| hash == self.plan_hash)
    }

    /// The record this plan would become, once an applier supplies the sequence,
    /// the previous address and the time. Nothing is signed or stored.
    pub fn draft_record(
        &self,
        sequence: u64,
        previous: Option<Address>,
        created_at_ms: u64,
    ) -> Result<HorizonRecord, Error> {
        let record = HorizonRecord {
            version: RECORD_VERSION,
            kind: RECORD_KIND.to_owned(),
            project: self.project,
            sequence,
            previous,
            created_at_ms,
            tier: self.tier,
            policy: self.policy.clone(),
            heads_digest: self.heads_digest.clone(),
            counts: self.counts.clone(),
            roots: Roots {
                skeleton: self.roots.skeleton.clone(),
                frontier: self.roots.frontier.clone(),
                released_states: self.roots.released_states.clone(),
            },
            archive: None,
            holds: self.holds.clone(),
            evidence: self.evidence.clone(),
        };
        record.validate()?;
        Ok(record)
    }

    /// Stable plain-text rendering of the same facts, for terminals and logs.
    pub fn summary(&self) -> String {
        let mut text = format!(
            "Horizon plan (read-only: nothing was deleted, written or signed)\nproject {}\npolicy {} ({})\ntier {}, topology {}\nplan hash {}\n",
            self.project,
            self.policy.id,
            self.policy.hash.as_str(),
            match self.tier {
                Tier::Content => "content",
                Tier::History => "history",
            },
            self.topology,
            self.plan_hash
        );
        if let Some(as_of) = self.as_of_ms {
            text.push_str(&format!("as of {as_of} ms\n"));
        }
        text.push_str(&format!("branches {}\n", self.heads.len()));
        for head in &self.heads {
            text.push_str(&format!(
                "  {} -> {} (generation {})\n",
                head.branch,
                head.commit.as_str(),
                head.generation
            ));
        }
        text.push_str(&format!(
            "commits {}: retain {}, release {}\n",
            self.commits.total, self.commits.retained, self.commits.released
        ));
        let counts = &self.counts;
        text.push_str(&format!(
            "release frees {} commit bytes, {} states, {} objects ({} bytes{})\n",
            counts.commit_bytes_released,
            counts.states_released,
            counts.objects_released,
            counts.object_bytes_released,
            if self.objects_measured {
                ""
            } else {
                ", not measured"
            }
        ));
        text.push_str("kept because (first reason that applies)\n");
        for reason in &self.reasons {
            text.push_str(&format!("  {:<18} {:>9}\n", reason.reason, reason.commits));
        }
        for warning in &self.warnings {
            text.push_str(&format!("warning: {warning}\n"));
        }
        if !self.examples.released.is_empty() {
            text.push_str("released commits (examples)\n");
            for example in &self.examples.released {
                text.push_str(&format!(
                    "  {} height {} {} bytes\n",
                    example.commit.as_str(),
                    example.height,
                    example.bytes
                ));
            }
        }
        text
    }
}

/// The merged commit graph: bodies first (sorted by address), then the skeleton.
struct Graph<'a> {
    view: &'a HistoryView,
    /// `order[i]` is the view index of body node `i`.
    order: Vec<usize>,
    bodies: usize,
    addr: Vec<Raw>,
    parents: Vec<Vec<u32>>,
    height: Vec<u64>,
}

struct Scratch {
    tick: u32,
    left: Vec<u32>,
    right: Vec<u32>,
    dominated: Vec<u32>,
}

impl<'a> Graph<'a> {
    fn build(view: &'a HistoryView) -> Result<Self, Error> {
        let mut order: Vec<usize> = (0..view.commits.len()).collect();
        order.sort_by_key(|i| view.commits[*i].address);
        let mut skeleton: Vec<&SkeletonEntry> = view.skeleton.iter().collect();
        skeleton.sort_by(|a, b| a.address.cmp(&b.address));
        let bodies = order.len();
        let mut addr: Vec<Raw> = order.iter().map(|i| view.commits[*i].address).collect();
        addr.extend(skeleton.iter().map(|e| raw(&e.address)));
        let total = addr.len();
        let find = |a: &Raw| -> Option<u32> {
            addr[..bodies]
                .binary_search(a)
                .ok()
                .or_else(|| addr[bodies..].binary_search(a).ok().map(|i| i + bodies))
                .map(|i| i as u32)
        };
        // Both halves are sorted; an address in both, or twice in the skeleton, is H7.
        if addr[bodies..].windows(2).any(|pair| pair[0] == pair[1])
            || addr[bodies..]
                .iter()
                .any(|a| addr[..bodies].binary_search(a).is_ok())
        {
            return Err(Error::Violated(
                Invariant::H7,
                "a commit is both stored and released, or released twice".into(),
            ));
        }
        let mut parents: Vec<Vec<u32>> = Vec::with_capacity(total);
        for i in &order {
            let mut list = Vec::new();
            for parent in &view.commits[*i].parents {
                list.push(find(parent).ok_or_else(|| Error::Missing(address_of(parent)))?);
            }
            parents.push(list);
        }
        for entry in &skeleton {
            let mut list = Vec::new();
            for parent in &entry.parents {
                let parent = raw(parent);
                list.push(find(&parent).ok_or_else(|| Error::Missing(address_of(&parent)))?);
            }
            parents.push(list);
        }
        // A commit's recovery references must exist too (import requires them).
        for i in &order {
            for source in &view.commits[*i].recovery {
                find(source).ok_or_else(|| Error::Missing(address_of(source)))?;
            }
        }
        // Heights by iterative post-order; a cycle cannot occur among hash-linked
        // commits, so one is reported as corruption.
        let mut height = vec![0u64; total];
        let mut state = vec![0u8; total];
        for start in 0..total {
            if state[start] == 2 {
                continue;
            }
            let mut stack: Vec<(usize, usize)> = vec![(start, 0)];
            state[start] = 1;
            while let Some(&(node, next)) = stack.last() {
                if let Some(&parent) = parents[node].get(next) {
                    stack.last_mut().expect("non-empty").1 += 1;
                    match state[parent as usize] {
                        0 => {
                            state[parent as usize] = 1;
                            stack.push((parent as usize, 0));
                        }
                        1 => return Err(Error::Format("commit graph has a cycle")),
                        _ => {}
                    }
                } else {
                    height[node] = if node < bodies {
                        parents[node]
                            .iter()
                            .map(|p| height[*p as usize] + 1)
                            .max()
                            .unwrap_or(0)
                    } else {
                        skeleton[node - bodies].height
                    };
                    state[node] = 2;
                    stack.pop();
                }
            }
        }
        for (node, i) in order.iter().enumerate() {
            if view.commits[*i]
                .stored_height
                .is_some_and(|stored| stored != height[node])
            {
                return Err(Error::Format("stored commit height"));
            }
        }
        Ok(Self {
            view,
            order,
            bodies,
            addr,
            parents,
            height,
        })
    }

    fn find(&self, a: &Raw) -> Option<u32> {
        let bodies = self.bodies;
        self.addr[..bodies]
            .binary_search(a)
            .ok()
            .or_else(|| {
                self.addr[bodies..]
                    .binary_search(a)
                    .ok()
                    .map(|i| i + bodies)
            })
            .map(|i| i as u32)
    }

    fn commit(&self, node: usize) -> &ViewCommit {
        &self.view.commits[self.order[node]]
    }

    /// `start` and all its ancestors, marking `stamp` with `tick`.
    fn ancestors(&self, start: u32, stamp: &mut [u32], tick: u32) -> Result<Vec<u32>, Error> {
        let mut found = Vec::new();
        let mut queue = VecDeque::from([start]);
        stamp[start as usize] = tick;
        while let Some(node) = queue.pop_front() {
            if found.len() == MAX_ANCESTORS {
                return Err(Error::Limit("ancestor count"));
            }
            found.push(node);
            for &parent in &self.parents[node as usize] {
                if stamp[parent as usize] != tick {
                    stamp[parent as usize] = tick;
                    queue.push_back(parent);
                }
            }
        }
        Ok(found)
    }

    /// All best common ancestors (common ancestors that are not the parent of
    /// another), exactly as `LocalRepository::merge_bases` computes them.
    fn merge_bases(&self, a: u32, b: u32, scratch: &mut Scratch) -> Result<Vec<u32>, Error> {
        scratch.tick += 1;
        let tick = scratch.tick;
        let left = self.ancestors(a, &mut scratch.left, tick)?;
        self.ancestors(b, &mut scratch.right, tick)?;
        let common: Vec<u32> = left
            .into_iter()
            .filter(|v| scratch.right[*v as usize] == tick)
            .collect();
        for &node in &common {
            for &parent in &self.parents[node as usize] {
                scratch.dominated[parent as usize] = tick;
            }
        }
        let mut bases: Vec<u32> = common
            .into_iter()
            .filter(|v| scratch.dominated[*v as usize] != tick)
            .collect();
        bases.sort_unstable();
        Ok(bases)
    }
}

fn mark(keep: &mut [Option<Reason>], node: u32, reason: Reason) {
    keep[node as usize].get_or_insert(reason);
}

pub fn plan(
    view: &HistoryView,
    policy: &Policy,
    options: &PlanOptions,
) -> Result<HorizonPlan, Error> {
    if options.list > MAX_LISTED {
        return Err(Error::Limit("horizon plan example count"));
    }
    if view.holds.len() > MAX_HOLDS {
        return Err(Error::Limit("horizon holds"));
    }
    let objects = &view.objects;
    if objects.sizes.len() != objects.children.len()
        || objects
            .children
            .iter()
            .flatten()
            .any(|child| *child as usize >= objects.sizes.len())
        || view
            .state_objects
            .values()
            .any(|object| *object as usize >= objects.sizes.len())
    {
        return Err(Error::Format("object graph"));
    }
    let cutoff = match (policy.window.keep_within_days, options.as_of_ms) {
        (Some(days), Some(as_of)) => Some(as_of.saturating_sub(i64::from(days) * DAY_MS)),
        (Some(_), None) => return Err(Error::Plan("keep_within_days needs an explicit as_of_ms")),
        (None, _) => None,
    };
    let graph = Graph::build(view)?;
    let bodies = graph.bodies;
    let total = graph.addr.len();
    let mut warnings: BTreeSet<String> = BTreeSet::new();
    let mut scratch = Scratch {
        tick: 0,
        left: vec![0; total],
        right: vec![0; total],
        dominated: vec![0; total],
    };

    // Heads, by branch name.
    let mut heads = view.branches.clone();
    heads.sort_by(|a, b| a.branch.cmp(&b.branch));
    let mut head_node: BTreeMap<&str, u32> = BTreeMap::new();
    for head in &heads {
        match graph.find(&raw(&head.commit)) {
            Some(node) if (node as usize) < bodies => {
                head_node.insert(&head.branch, node);
            }
            Some(_) => {
                return Err(Error::Violated(
                    Invariant::H6,
                    format!(
                        "the head of {} was released by an earlier horizon",
                        head.branch
                    ),
                ))
            }
            None => return Err(Error::Missing(head.commit.clone())),
        }
    }

    // Rules 1 to 3.
    let mut keep: Vec<Option<Reason>> = vec![None; total];
    for node in head_node.values() {
        mark(&mut keep, *node, Reason::BranchHead);
    }
    let mut pin =
        |keep: &mut Vec<Option<Reason>>, commit: &Address, reason: Reason, what: &str| match graph
            .find(&raw(commit))
        {
            Some(node) if (node as usize) < bodies => mark(keep, node, reason),
            _ => {
                warnings.insert(format!("{what} {} is not a stored commit", commit.as_str()));
            }
        };
    let mut held: Vec<&Hold> = view.holds.iter().collect();
    held.sort();
    held.dedup();
    for hold in &held {
        pin(&mut keep, &hold.commit, Reason::Hold, "hold on");
    }
    for commit in &policy.protect.commits {
        pin(&mut keep, commit, Reason::PolicyCommit, "protected commit");
    }
    if let Some(bases) = &policy.protect.merge_bases {
        match head_node.get(bases.against.as_str()) {
            None => {
                warnings.insert(format!(
                    "merge_bases.against branch {} does not exist; no fork point was pinned",
                    bases.against
                ));
            }
            Some(&against) => {
                for (name, &node) in &head_node {
                    if *name != bases.against && bases.matches(name) {
                        for base in graph.merge_bases(against, node, &mut scratch)? {
                            if (base as usize) < bodies {
                                mark(&mut keep, base, Reason::MergeBase);
                            }
                        }
                    }
                }
            }
        }
    }
    let count = policy
        .window
        .keep_last_commits
        .into_iter()
        .chain(policy.window.min_commits_kept)
        .max();
    if let Some(count) = count {
        for &head in head_node.values() {
            let mut current = Some(head);
            for _ in 0..count {
                let Some(node) = current else { break };
                mark(&mut keep, node, Reason::RecentCount);
                current = graph.parents[node as usize]
                    .first()
                    .copied()
                    .filter(|p| (*p as usize) < bodies);
            }
        }
    }
    if let Some(cutoff) = cutoff {
        for node in 0..bodies {
            if graph.commit(node).authored_at_ms >= cutoff {
                mark(&mut keep, node as u32, Reason::RecentTime);
            }
        }
    }

    // Rule 4: one level of evidence, computed from the commits kept so far.
    let primary: Vec<u32> = (0..bodies as u32)
        .filter(|n| keep[*n as usize].is_some())
        .collect();
    let mut evidence: Vec<(u32, Reason)> = Vec::new();
    for &node in &primary {
        let commit = graph.commit(node as usize);
        let parents = &graph.parents[node as usize];
        match commit.version {
            2 => {
                for source in &commit.recovery {
                    evidence.push((
                        graph.find(source).expect("checked"),
                        Reason::EvidenceRecovery,
                    ));
                }
                if let Some(first) = parents.first() {
                    evidence.push((*first, Reason::EvidenceRecovery));
                }
            }
            3 => {
                for parent in parents {
                    evidence.push((*parent, Reason::EvidenceMerge));
                }
                if let [left, right] = parents.as_slice() {
                    match graph.merge_bases(*left, *right, &mut scratch)?.as_slice() {
                        [base] => evidence.push((*base, Reason::EvidenceMerge)),
                        _ => return Err(Error::Plan("a merge commit has no unique merge base")),
                    }
                }
            }
            _ => {}
        }
    }
    for (node, reason) in evidence {
        if (node as usize) < bodies {
            mark(&mut keep, node, reason);
        } else {
            warnings.insert(format!(
                "evidence commit {} was released by an earlier horizon",
                super::hex(&graph.addr[node as usize])
            ));
        }
    }

    // Retained and released commits by tier.
    let history = policy.tier == Tier::History;
    let retained_nodes: Vec<u32> = (0..bodies as u32)
        .filter(|n| !history || keep[*n as usize].is_some())
        .collect();
    let released_nodes: Vec<u32> = (0..bodies as u32)
        .filter(|n| history && keep[*n as usize].is_none())
        .collect();
    let in_released: HashSet<u32> = released_nodes.iter().copied().collect();
    let skeleton_rows: Vec<SkeletonEntry> = released_nodes
        .iter()
        .map(|n| SkeletonEntry {
            address: address_of(&graph.addr[*n as usize]),
            parents: graph.parents[*n as usize]
                .iter()
                .map(|p| address_of(&graph.addr[*p as usize]))
                .collect(),
            height: graph.height[*n as usize],
        })
        .collect();
    let mut frontier: Vec<FrontierEdge> = Vec::new();
    for &node in &retained_nodes {
        for &parent in &graph.parents[node as usize] {
            if in_released.contains(&parent) {
                frontier.push(FrontierEdge {
                    child: address_of(&graph.addr[node as usize]),
                    parent: address_of(&graph.addr[parent as usize]),
                    parent_height: graph.height[parent as usize],
                });
            }
        }
    }
    frontier.sort_by(|a, b| (&a.child, &a.parent).cmp(&(&b.child, &b.parent)));
    let dropped = policy.topology == Topology::Drop;
    let skeleton_out = if dropped { Vec::new() } else { skeleton_rows };

    // States: retained set S, then everything else is released.
    let state_nodes: Vec<u32> = if history {
        retained_nodes.clone()
    } else {
        (0..bodies as u32)
            .filter(|n| {
                keep[*n as usize].is_some_and(|r| r.primary() || r == Reason::EvidenceRecovery)
            })
            .collect()
    };
    let mut retained_states: HashSet<u32> = HashSet::new();
    for &node in &state_nodes {
        retained_states.extend(graph.commit(node as usize).files.iter().copied());
    }
    if !history && policy.protect.merge_inputs == MergeInputs::States {
        for &node in &primary {
            for inputs in &graph.commit(node as usize).inputs {
                retained_states.extend(inputs.iter().copied());
            }
        }
    }
    let mut all_states: BTreeSet<u32> = BTreeSet::new();
    for node in 0..bodies {
        all_states.extend(graph.commit(node).files.iter().copied());
    }
    let released_ids: Vec<u32> = all_states
        .iter()
        .copied()
        .filter(|s| !retained_states.contains(s))
        .collect();
    let mut released_states: Vec<StateKey> = released_ids
        .iter()
        .map(|s| {
            let (document, root) = view.states[*s as usize];
            StateKey {
                document,
                state_root: address_of(&root),
            }
        })
        .collect();
    released_states.sort();

    // Objects only released states reach.
    let measured = released_ids
        .iter()
        .all(|s| view.state_objects.contains_key(s))
        && retained_states
            .iter()
            .filter(|s| all_states.contains(s))
            .all(|s| view.state_objects.contains_key(s));
    let (objects_released, object_bytes_released) = {
        let graph_objects = &view.objects;
        let mut reached = vec![false; graph_objects.sizes.len()];
        let mut stack: Vec<u32> = Vec::new();
        let mut walk = |roots: &mut dyn Iterator<Item = u32>,
                        reached: &mut Vec<bool>,
                        tally: &mut Option<(u64, u64)>| {
            stack.extend(roots);
            while let Some(object) = stack.pop() {
                if std::mem::replace(&mut reached[object as usize], true) {
                    continue;
                }
                if let Some((count, bytes)) = tally {
                    *count += 1;
                    *bytes += graph_objects.sizes[object as usize];
                }
                stack.extend(
                    graph_objects.children[object as usize]
                        .iter()
                        .filter(|c| !reached[**c as usize]),
                );
            }
        };
        let root_of = |s: &u32| view.state_objects.get(s).copied();
        let mut kept_roots = retained_states.iter().filter_map(root_of);
        walk(&mut kept_roots, &mut reached, &mut None);
        let mut released_roots = released_ids.iter().filter_map(root_of);
        let mut tally = Some((0, 0));
        walk(&mut released_roots, &mut reached, &mut tally);
        tally.unwrap_or_default()
    };

    // Roots, counts, reasons.
    let retained_addresses: Vec<Address> = retained_nodes
        .iter()
        .map(|n| address_of(&graph.addr[*n as usize]))
        .collect();
    let released_addresses: Vec<Address> = released_nodes
        .iter()
        .map(|n| address_of(&graph.addr[*n as usize]))
        .collect();
    let counts = Counts {
        commits_released: released_nodes.len() as u64,
        commit_bytes_released: released_nodes
            .iter()
            .map(|n| graph.commit(*n as usize).bytes)
            .sum(),
        states_released: released_states.len() as u64,
        objects_released,
        object_bytes_released,
    };
    let numbers = [
        counts.commits_released,
        counts.commit_bytes_released,
        counts.states_released,
        counts.objects_released,
        counts.object_bytes_released,
    ];
    if numbers.iter().any(|n| *n > MAX_JSON_INT) {
        return Err(Error::Limit("horizon counts"));
    }
    let kept_reasons: Vec<(Address, &'static str)> = (0..bodies)
        .filter_map(|n| keep[n].map(|r| (address_of(&graph.addr[n]), r.label())))
        .collect();
    let mut by_reason: BTreeMap<Reason, u64> = BTreeMap::new();
    for reason in keep.iter().take(bodies).flatten() {
        *by_reason.entry(*reason).or_default() += 1;
    }
    let mut evidence_words: BTreeSet<String> = BTreeSet::new();
    evidence_words.insert("recovery_sources".into());
    evidence_words.insert(
        match policy.protect.merge_inputs {
            MergeInputs::None => "merge_inputs:none",
            MergeInputs::Bodies => "merge_inputs:bodies",
            MergeInputs::States => "merge_inputs:states",
        }
        .into(),
    );
    if policy.protect.merge_bases.is_some() {
        evidence_words.insert("merge_bases".into());
    }
    if !held.is_empty() {
        evidence_words.insert("holds".into());
    }
    if dropped {
        evidence_words.insert("topology:drop".into());
    }

    let examples = Examples {
        released: released_nodes
            .iter()
            .take(options.list)
            .map(|n| ReleasedExample {
                commit: address_of(&graph.addr[*n as usize]),
                bytes: graph.commit(*n as usize).bytes,
                height: graph.height[*n as usize],
            })
            .collect(),
        pinned: retained_nodes
            .iter()
            .filter_map(|n| keep[*n as usize].filter(|r| r.pinned()).map(|r| (*n, r)))
            .take(options.list)
            .map(|(n, r)| PinnedExample {
                commit: address_of(&graph.addr[n as usize]),
                reason: r.label(),
            })
            .collect(),
    };
    let roots = PlanRoots {
        retained: retained_root(&retained_addresses)?,
        skeleton: skeleton_root(&skeleton_out)?,
        frontier: frontier_root(&frontier)?,
        released_states: released_states_root(&released_states)?,
    };
    let mut result = HorizonPlan {
        format: FORMAT,
        project: view.project,
        read_only: true,
        plan_hash: String::new(),
        tier: policy.tier,
        topology: if dropped { "drop" } else { "keep" },
        as_of_ms: cutoff.and(options.as_of_ms),
        policy: policy.policy_ref(),
        heads_digest: heads_digest(&heads)?,
        heads,
        commits: PlanCommits {
            total: bodies as u64,
            retained: retained_nodes.len() as u64,
            released: released_nodes.len() as u64,
        },
        counts,
        objects_measured: measured,
        roots,
        reasons: by_reason
            .into_iter()
            .map(|(reason, commits)| ReasonCount {
                reason: reason.label(),
                commits,
            })
            .collect(),
        evidence: evidence_words.into_iter().collect(),
        holds: held.into_iter().cloned().collect(),
        warnings: warnings.into_iter().collect(),
        examples,
        sets: PlanSets {
            retained: retained_addresses,
            released: released_addresses,
            skeleton: skeleton_out,
            frontier,
            released_states,
            kept: kept_reasons,
        },
    };
    result.plan_hash = result.compute_hash()?;
    Ok(result)
}
