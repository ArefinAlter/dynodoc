//! Property tests of the read-only horizon planner over random commit DAGs: linear,
//! branchy, criss-cross and histories heavy in recovery (v2) and same-file merge
//! (v3) commits, under random policies of both tiers.
//!
//! The graphs are in memory (the SQLite loader is tested against a real repository
//! in `repository/local/horizon_plan_tests.rs`). Commits are real signed `Commit`
//! values; their states and objects are synthetic but shared, so the object
//! accounting has something to get wrong. Every check below recomputes its answer
//! from the generated history with a naive method, not with the planner's code.
//! A fixed seed makes a red run reproducible; `PROPTEST_RNG_SEED` and
//! `PROPTEST_CASES` still override it.
use engine_core::repository::{
    convergence::{Resolution, STRATEGY},
    horizon::{
        check_state, heads_digest,
        plan::{plan, HistoryView, HorizonPlan, ObjectGraph, PlanOptions},
        policy::{LoadOptions, Policy},
        HeadRef, Hold, HorizonRows, Retained, StateKey,
    },
    recovery::{Kind, Source},
    Commit, FileEntry, SignedCommit,
};
use engine_core::shared_checkpoint::Address;
use engine_shared::DocumentId;
use proptest::{prelude::*, test_runner::RngSeed};
use ring::{digest, signature::Ed25519KeyPair};
use std::collections::{BTreeMap, BTreeSet};
use uuid::Uuid;

const DAY_MS: i64 = 86_400_000;

struct Rng(u64);
impl Rng {
    fn below(&mut self, n: usize) -> usize {
        self.0 ^= self.0 << 13;
        self.0 ^= self.0 >> 7;
        self.0 ^= self.0 << 17;
        (self.0 % n as u64) as usize
    }
    fn chance(&mut self, percent: usize) -> bool {
        self.below(100) < percent
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Shape {
    Linear,
    Branchy,
    CrissCross,
    Heavy,
}

const SHAPES: [Shape; 4] = [
    Shape::Linear,
    Shape::Branchy,
    Shape::CrissCross,
    Shape::Heavy,
];

fn address(parts: &[u64]) -> Address {
    let bytes: Vec<u8> = parts.iter().flat_map(|p| p.to_be_bytes()).collect();
    let hash = digest::digest(&digest::SHA256, &bytes);
    Address::try_from(
        hash.as_ref()
            .iter()
            .map(|b| format!("{b:02x}"))
            .collect::<String>(),
    )
    .unwrap()
}

struct Node {
    signed: SignedCommit,
    address: Address,
    parents: Vec<usize>,
    version: u8,
    height: u64,
    authored_at_ms: i64,
    states: Vec<StateKey>,
    /// Indices of the recovery source and mainline commits, for a v2 commit.
    recovery: Vec<usize>,
}

struct History {
    seed: u64,
    shape: Shape,
    project: Uuid,
    nodes: Vec<Node>,
    heads: Vec<HeadRef>,
    holds: Vec<Hold>,
    objects: ObjectGraph,
    state_objects: Vec<(StateKey, u32)>,
}

/// Best common ancestors by the textbook definition, on any index graph.
fn naive_bases(parents: &[Vec<usize>], a: usize, b: usize) -> Vec<usize> {
    let ancestors = |start: usize| -> BTreeSet<usize> {
        let mut seen = BTreeSet::from([start]);
        let mut stack = vec![start];
        while let Some(node) = stack.pop() {
            for &p in &parents[node] {
                if seen.insert(p) {
                    stack.push(p);
                }
            }
        }
        seen
    };
    let (left, right) = (ancestors(a), ancestors(b));
    let common: BTreeSet<usize> = left.intersection(&right).copied().collect();
    let dominated: BTreeSet<usize> = common
        .iter()
        .flat_map(|c| parents[*c].iter().copied())
        .collect();
    common.difference(&dominated).copied().collect()
}

fn generate(seed: u64, shape: Shape) -> History {
    let mut rng = Rng(seed | 1);
    let key = Ed25519KeyPair::from_seed_unchecked(&[7; 32]).unwrap();
    let project = Uuid::from_u128(0x5000 + u128::from(seed % 997));
    let docs = [Uuid::from_u128(0xa1), Uuid::from_u128(0xa2)];
    let count = 6 + rng.below(28);
    // Parent lists by index, decided first so v3 can ask for a unique base.
    let mut parents: Vec<Vec<usize>> = Vec::new();
    let mut versions: Vec<u8> = Vec::new();
    for i in 0..count {
        let (list, version) = match shape {
            Shape::Linear => (if i == 0 { vec![] } else { vec![i - 1] }, 1),
            Shape::Branchy => (branchy_parents(&mut rng, i), 1),
            Shape::CrissCross => (criss_cross_parents(i), 1),
            Shape::Heavy => {
                let mut list = branchy_parents(&mut rng, i);
                let mut version = 1;
                if i >= 2 && rng.chance(30) {
                    // A same-file merge needs two parents with exactly one best base.
                    for _ in 0..8 {
                        let (x, y) = (rng.below(i), rng.below(i));
                        if x != y && naive_bases(&parents, x, y).len() == 1 {
                            list = vec![x, y];
                            version = 3;
                            break;
                        }
                    }
                }
                if version == 1 && i >= 1 && rng.chance(25) {
                    list = vec![rng.below(i)];
                    version = 2;
                }
                (list, version)
            }
        };
        parents.push(list);
        versions.push(version);
    }
    let mut heights: Vec<u64> = Vec::new();
    for list in &parents {
        heights.push(list.iter().map(|p| heights[*p] + 1).max().unwrap_or(0));
    }
    let mut states_a: Vec<Address> = Vec::new();
    let mut states_b: Vec<Address> = Vec::new();
    let mut nodes: Vec<Node> = Vec::new();
    for i in 0..count {
        let a = if i > 0 && rng.chance(25) {
            states_a[rng.below(i)].clone()
        } else {
            address(&[seed, 1, i as u64])
        };
        let b = if i > 0 && rng.chance(70) {
            states_b[i - 1].clone()
        } else {
            address(&[seed, 2, i as u64])
        };
        states_a.push(a.clone());
        states_b.push(b.clone());
        let mut files = BTreeMap::from([
            (
                "a".to_owned(),
                FileEntry {
                    document: DocumentId(docs[0]),
                    state: a.clone(),
                },
            ),
            (
                "b".to_owned(),
                FileEntry {
                    document: DocumentId(docs[1]),
                    state: b.clone(),
                },
            ),
        ]);
        if rng.chance(15) {
            files.remove("b");
        }
        let (mut recovery, mut resolutions, mut sources) = (None, Vec::new(), Vec::new());
        match versions[i] {
            2 => {
                let source = rng.below(i);
                let mainline = rng.chance(60).then(|| rng.below(i));
                sources.push(source);
                sources.extend(mainline);
                recovery = Some(Source {
                    kind: if rng.chance(50) {
                        Kind::Revert
                    } else {
                        Kind::CherryPick
                    },
                    commit: nodes[source].address.clone(),
                    mainline: mainline.map(|m| nodes[m].address.clone()),
                });
            }
            3 => {
                // Base, left and right: existing file states when there are enough.
                let mut pool: Vec<Address> = states_a[..i].to_vec();
                pool.sort();
                pool.dedup();
                let mut pick = |salt: u64, rng: &mut Rng| -> Address {
                    if pool.len() >= 3 && rng.chance(70) {
                        pool.remove(rng.below(pool.len()))
                    } else {
                        address(&[seed, 3, i as u64, salt])
                    }
                };
                let (base, left, right) = (pick(0, &mut rng), pick(1, &mut rng), pick(2, &mut rng));
                resolutions.push(Resolution {
                    path: "a".into(),
                    document: DocumentId(docs[0]),
                    base,
                    left,
                    right,
                    merged: a.clone(),
                    strategy: STRATEGY.into(),
                });
            }
            _ => {}
        }
        let commit = Commit {
            version: versions[i],
            project,
            parents: parents[i]
                .iter()
                .map(|p| nodes[*p].address.clone())
                .collect(),
            files,
            author_key: String::new(),
            // Whole days, so a window cut lands exactly on some commit's time.
            authored_at_ms: (10 + i as i64) * DAY_MS,
            message: format!("commit {i}"),
            recovery,
            same_file_resolutions: resolutions,
        };
        let authored_at_ms = commit.authored_at_ms;
        let signed = SignedCommit::sign(commit, &key).unwrap();
        let address = signed.verify().unwrap();
        let states = signed
            .commit
            .files
            .values()
            .map(|f| StateKey {
                document: f.document.0,
                state_root: f.state.clone(),
            })
            .collect();
        nodes.push(Node {
            signed,
            address,
            parents: parents[i].clone(),
            version: versions[i],
            height: heights[i],
            authored_at_ms,
            states,
            recovery: sources,
        });
    }

    // Branch heads: main at the last commit; the others anywhere.
    let mut heads = vec![HeadRef {
        branch: "main".into(),
        commit: nodes[count - 1].address.clone(),
        generation: 1 + rng.below(9) as i64,
    }];
    match shape {
        Shape::CrissCross => heads.push(HeadRef {
            branch: "dev".into(),
            commit: nodes[count.saturating_sub(2)].address.clone(),
            generation: 1,
        }),
        Shape::Linear => {}
        _ => {
            for name in ["dev", "review/x", "review/y"] {
                if rng.chance(55) {
                    heads.push(HeadRef {
                        branch: name.into(),
                        commit: nodes[rng.below(count)].address.clone(),
                        generation: 1 + rng.below(5) as i64,
                    });
                }
            }
        }
    }
    let mut holds = Vec::new();
    while rng.chance(35) && holds.len() < 3 {
        holds.push(Hold {
            commit: nodes[rng.below(count)].address.clone(),
            reason: format!("hold {}", holds.len()),
        });
    }
    if rng.chance(10) {
        holds.push(Hold {
            commit: address(&[seed, 9]),
            reason: "ghost".into(),
        });
    }

    // Objects: every distinct state has a root object over one to three shared
    // chunks and sometimes a private leaf.
    let mut objects = ObjectGraph::default();
    let add = |objects: &mut ObjectGraph, size: u64, children: Vec<u32>| {
        objects.sizes.push(size);
        objects.children.push(children);
        objects.sizes.len() as u32 - 1
    };
    let chunks: Vec<u32> = (0..8)
        .map(|c| add(&mut objects, 20 + 37 * c, vec![]))
        .collect();
    let distinct: BTreeSet<StateKey> = nodes.iter().flat_map(|n| n.states.clone()).collect();
    let mut state_objects = Vec::new();
    for state in distinct {
        let mut children: Vec<u32> = (0..1 + rng.below(3))
            .map(|_| chunks[rng.below(chunks.len())])
            .collect();
        if rng.chance(40) {
            children.push(add(&mut objects, 5 + rng.below(50) as u64, vec![]));
        }
        children.sort_unstable();
        children.dedup();
        let root = add(&mut objects, 60 + rng.below(40) as u64, children);
        state_objects.push((state, root));
    }
    History {
        seed,
        shape,
        project,
        nodes,
        heads,
        holds,
        objects,
        state_objects,
    }
}

fn branchy_parents(rng: &mut Rng, i: usize) -> Vec<usize> {
    if i == 0 {
        return vec![];
    }
    match rng.below(10) {
        0..=5 => vec![i - 1],
        6..=7 => vec![rng.below(i)],
        _ if i >= 2 => {
            let first = rng.below(i);
            let mut second = rng.below(i);
            if second == first {
                second = (first + 1) % i;
            }
            vec![first, second]
        }
        _ => vec![i - 1],
    }
}

/// Two lines that merge into each other, rung after rung; then a short tail.
/// Rung `r` holds nodes `2r - 1` and `2r`; the first merges (left, right) of the
/// rung below, the second (right, left), so every rung pair is a criss-cross.
fn criss_cross_parents(i: usize) -> Vec<usize> {
    match i {
        0 => vec![],
        1 | 2 => vec![0],
        _ if i <= 12 => {
            let rung = i.div_ceil(2);
            let (left, right) = (2 * (rung - 1) - 1, 2 * (rung - 1));
            if i % 2 == 1 {
                vec![left, right]
            } else {
                vec![right, left]
            }
        }
        _ => vec![i - 1],
    }
}

// ---- building views and policies ------------------------------------------------

fn view_of(h: &History, order: &[usize], with_objects: bool) -> HistoryView {
    let mut view = HistoryView::new(h.project);
    for head in &h.heads {
        view.add_branch(head.clone()).unwrap();
    }
    for &i in order {
        let node = &h.nodes[i];
        let bytes = serde_json::to_vec(&node.signed).unwrap().len() as u64;
        view.add_commit(&node.address, &node.signed.commit, bytes, Some(node.height))
            .unwrap();
    }
    for hold in &h.holds {
        view.add_hold(hold.clone()).unwrap();
    }
    if with_objects {
        view.objects = h.objects.clone();
        for (state, root) in &h.state_objects {
            view.set_state_object(state.document, &state.state_root, *root);
        }
    }
    view
}

fn shuffled(n: usize, salt: u64, seed: u64) -> Vec<usize> {
    let mut rng = Rng((seed ^ salt.wrapping_mul(0x9e37_79b9_7f4a_7c15)) | 1);
    let mut order: Vec<usize> = (0..n).collect();
    for i in (1..n).rev() {
        order.swap(i, rng.below(i + 1));
    }
    order
}

struct Setup {
    policy: Policy,
    options: PlanOptions,
    json: String,
}

fn policy_for(h: &History, rng: &mut Rng) -> Setup {
    let history = rng.chance(70);
    let mut window = Vec::new();
    if rng.chance(55) {
        window.push(format!("\"keep_last_commits\":{}", 1 + rng.below(6)));
    }
    if rng.chance(30) {
        window.push(format!("\"min_commits_kept\":{}", 1 + rng.below(3)));
    }
    if rng.chance(30) {
        window.push(format!("\"keep_within_days\":{}", 1 + rng.below(8)));
    }
    if window.is_empty() {
        window.push("\"min_commits_kept\":1".to_owned());
    }
    let mut protect = vec!["\"branch_heads\":true".to_owned()];
    if rng.chance(65) {
        protect.push("\"merge_bases\":{\"against\":\"main\",\"branches\":[\"*\"]}".into());
    }
    protect.push(format!(
        "\"merge_inputs\":\"{}\"",
        ["none", "bodies", "states"][rng.below(3)]
    ));
    if rng.chance(30) {
        let pins: Vec<String> = (0..1 + rng.below(2))
            .map(|_| format!("\"{}\"", h.nodes[rng.below(h.nodes.len())].address.as_str()))
            .collect();
        protect.push(format!("\"commits\":[{}]", pins.join(",")));
    }
    let drop = history && rng.chance(12);
    let json = format!(
        "{{\"version\":1,\"id\":\"p-{}\",\"tier\":\"{}\",\"window\":{{{}}},\"protect\":{{{}}}{}}}",
        h.seed % 1000,
        if history { "history" } else { "content" },
        window.join(","),
        protect.join(","),
        if drop {
            ",\"topology\":\"drop\",\"acknowledge_unmergeable_history\":true"
        } else {
            ""
        }
    );
    let policy = Policy::load(json.as_bytes(), LoadOptions::default())
        .unwrap_or_else(|e| panic!("generated policy {json} is invalid: {e}"));
    let options = PlanOptions {
        // Ten days before the first commit up to the last: a different cut each seed.
        as_of_ms: Some((10 + h.nodes.len() as i64) * DAY_MS),
        list: 5,
    };
    Setup {
        policy,
        options,
        json,
    }
}

// ---- the properties -------------------------------------------------------------

fn parents_of(h: &History) -> Vec<Vec<usize>> {
    h.nodes.iter().map(|n| n.parents.clone()).collect()
}

fn index_of(h: &History, address: &Address) -> usize {
    h.nodes.iter().position(|n| &n.address == address).unwrap()
}

/// The graph a replica holds after the horizon: stored bodies keep the parents
/// their commit names, skeleton entries the parents the plan recorded. Built from
/// the plan's outputs alone.
fn retained_graph(h: &History, plan: &HorizonPlan) -> Vec<Vec<usize>> {
    let mut graph: Vec<Option<Vec<usize>>> = vec![None; h.nodes.len()];
    for address in &plan.sets.retained {
        let i = index_of(h, address);
        graph[i] = Some(h.nodes[i].parents.clone());
    }
    for entry in &plan.sets.skeleton {
        let i = index_of(h, &entry.address);
        assert!(graph[i].is_none(), "released and retained: {}", h.seed);
        graph[i] = Some(entry.parents.iter().map(|p| index_of(h, p)).collect());
    }
    graph
        .into_iter()
        .enumerate()
        .map(|(i, parents)| {
            parents.unwrap_or_else(|| {
                panic!(
                    "commit {i} is neither retained nor released (seed {})",
                    h.seed
                )
            })
        })
        .collect()
}

fn check(h: &History, setup: &Setup) -> HorizonPlan {
    let ctx = format!("seed {} shape {:?} policy {}", h.seed, h.shape, setup.json);
    let count = h.nodes.len();
    let order: Vec<usize> = (0..count).collect();
    let p = plan(&view_of(h, &order, true), &setup.policy, &setup.options)
        .unwrap_or_else(|e| panic!("{ctx}: {e}"));
    let history = setup.policy.tier == engine_core::repository::horizon::Tier::History;
    let keep: BTreeSet<&Address> = p.sets.retained.iter().collect();
    let released: BTreeSet<&Address> = p.sets.released.iter().collect();
    let all: BTreeSet<&Address> = h.nodes.iter().map(|n| &n.address).collect();

    // The plan is a partition of the stored commits, and its numbers agree.
    assert!(keep.is_disjoint(&released), "{ctx}");
    assert_eq!(
        keep.union(&released).copied().collect::<BTreeSet<_>>(),
        all,
        "{ctx}"
    );
    assert_eq!(p.commits.total as usize, count, "{ctx}");
    assert_eq!(p.commits.retained as usize, keep.len(), "{ctx}");
    assert_eq!(p.commits.released as usize, released.len(), "{ctx}");
    assert!(p.read_only && p.hash_is_valid(), "{ctx}");
    if !history {
        assert!(
            released.is_empty(),
            "a content horizon releases no commit: {ctx}"
        );
    }
    let bytes_released: u64 = p
        .sets
        .released
        .iter()
        .map(|a| {
            serde_json::to_vec(&h.nodes[index_of(h, a)].signed)
                .unwrap()
                .len() as u64
        })
        .sum();
    assert_eq!(p.counts.commit_bytes_released, bytes_released, "{ctx}");

    // I5: heads unchanged, retained, and bound by the digest.
    assert_eq!(
        p.heads,
        {
            let mut heads = h.heads.clone();
            heads.sort_by(|a, b| a.branch.cmp(&b.branch));
            heads
        },
        "{ctx}"
    );
    assert_eq!(p.heads_digest, heads_digest(&h.heads).unwrap(), "{ctx}");
    for head in &h.heads {
        assert!(keep.contains(&head.commit), "head released: {ctx}");
    }

    // Holds and policy pins are retained; held ghosts only warn.
    for hold in &h.holds {
        if all.contains(&hold.commit) {
            assert!(keep.contains(&hold.commit), "hold released: {ctx}");
        } else {
            assert!(
                p.warnings.iter().any(|w| w.contains(hold.commit.as_str())),
                "ghost hold unreported: {ctx}"
            );
        }
    }
    for pin in &setup.policy.protect.commits {
        assert!(keep.contains(pin), "pinned commit released: {ctx}");
    }

    // The window rules.
    let parents = parents_of(h);
    if let Some(days) = setup.policy.window.keep_within_days {
        let cutoff = setup.options.as_of_ms.unwrap() - i64::from(days) * DAY_MS;
        for node in h.nodes.iter().filter(|n| n.authored_at_ms >= cutoff) {
            assert!(
                keep.contains(&node.address),
                "recent commit released: {ctx}"
            );
        }
    }
    let width = [
        setup.policy.window.keep_last_commits,
        setup.policy.window.min_commits_kept,
    ]
    .into_iter()
    .flatten()
    .max();
    if let Some(width) = width {
        for head in &h.heads {
            let mut current = Some(index_of(h, &head.commit));
            for _ in 0..width {
                let Some(i) = current else { break };
                assert!(
                    keep.contains(&h.nodes[i].address),
                    "window commit released: {ctx}"
                );
                current = parents[i].first().copied();
            }
        }
    }

    // Merge-base pins: every best base of (main, branch) is retained, criss-cross included.
    if setup.policy.protect.merge_bases.is_some() {
        let main = index_of(h, &h.heads[0].commit);
        for head in h.heads.iter().filter(|x| x.branch != "main") {
            for base in naive_bases(&parents, main, index_of(h, &head.commit)) {
                assert!(
                    keep.contains(&h.nodes[base].address),
                    "merge base released: {ctx}"
                );
            }
        }
    }

    // Closure: a commit kept for its own sake brings its provenance along.
    for (kept, reason) in &p.sets.kept {
        let i = index_of(h, kept);
        assert!(keep.contains(kept), "{ctx}");
        if reason.starts_with("evidence") {
            continue;
        }
        let node = &h.nodes[i];
        match node.version {
            2 => {
                for &s in &node.recovery {
                    assert!(
                        keep.contains(&h.nodes[s].address),
                        "recovery source released: {ctx}"
                    );
                }
                assert!(keep.contains(&h.nodes[node.parents[0]].address), "{ctx}");
            }
            3 => {
                for &q in &node.parents {
                    assert!(
                        keep.contains(&h.nodes[q].address),
                        "merge parent released: {ctx}"
                    );
                }
                let bases = naive_bases(&parents, node.parents[0], node.parents[1]);
                assert_eq!(bases.len(), 1, "{ctx}");
                assert!(
                    keep.contains(&h.nodes[bases[0]].address),
                    "merge base released: {ctx}"
                );
            }
            _ => {}
        }
    }

    if history && setup.policy.topology == engine_core::repository::horizon::policy::Topology::Keep
    {
        // I2: skeleton closure, heights, frontier and every H3..H8 over the plan's own rows.
        let retained: Vec<Retained> = p
            .sets
            .retained
            .iter()
            .map(|a| {
                let n = &h.nodes[index_of(h, a)];
                Retained {
                    address: n.address.clone(),
                    parents: n.signed.commit.parents.clone(),
                    height: n.height,
                    states: n.states.clone(),
                }
            })
            .collect();
        let released_states: BTreeSet<&StateKey> = p.sets.released_states.iter().collect();
        let present: BTreeSet<StateKey> = retained
            .iter()
            .flat_map(|r| r.states.clone())
            .filter(|s| !released_states.contains(s))
            .collect();
        let record = p.draft_record(1, None, 1_790_000_000_000).unwrap();
        let rows = [HorizonRows {
            record: &record,
            skeleton: &p.sets.skeleton,
            frontier: &p.sets.frontier,
            released_states: &p.sets.released_states,
        }];
        check_state(&retained, &rows, &h.heads, &present).unwrap_or_else(|e| panic!("{e}: {ctx}"));
        for entry in &p.sets.skeleton {
            let node = &h.nodes[index_of(h, &entry.address)];
            assert_eq!(entry.height, node.height, "{ctx}");
            // The first parent stays first.
            assert_eq!(entry.parents, node.signed.commit.parents, "{ctx}");
        }

        // I3: merge bases on the retained graph equal the full graph's, for every pair.
        let graph = retained_graph(h, &p);
        for x in 0..count {
            for y in x..count {
                assert_eq!(
                    naive_bases(&graph, x, y),
                    naive_bases(&parents, x, y),
                    "merge bases of {x} and {y}: {ctx}"
                );
            }
        }

        // States: exactly the states no stored commit still uses are released.
        let kept_states: BTreeSet<StateKey> =
            retained.iter().flat_map(|r| r.states.clone()).collect();
        let all_states: BTreeSet<StateKey> =
            h.nodes.iter().flat_map(|n| n.states.clone()).collect();
        let want: Vec<StateKey> = all_states.difference(&kept_states).cloned().collect();
        assert_eq!(p.sets.released_states, want, "{ctx}");
    } else if history {
        // Dropped topology: nothing but the retained bodies is left to check by graph.
        assert!(p.sets.skeleton.is_empty(), "{ctx}");
        assert_eq!(
            p.roots.skeleton,
            plan(
                &view_of(h, &(0..count).collect::<Vec<_>>(), true),
                &setup.policy,
                &setup.options,
            )
            .unwrap()
            .roots
            .skeleton,
            "{ctx}"
        );
    }

    // Object accounting against an independent computation: per-state reach sets.
    let object_graph = &h.objects;
    let root_of: BTreeMap<&StateKey, u32> = h.state_objects.iter().map(|(s, o)| (s, *o)).collect();
    let reach = |state: &StateKey| -> BTreeSet<u32> {
        let mut seen = BTreeSet::new();
        let mut stack = vec![root_of[state]];
        while let Some(o) = stack.pop() {
            if seen.insert(o) {
                stack.extend(object_graph.children[o as usize].iter().copied());
            }
        }
        seen
    };
    let released_set: BTreeSet<&StateKey> = p.sets.released_states.iter().collect();
    let all_states: BTreeSet<&StateKey> = root_of.keys().copied().collect();
    let mut freed: BTreeSet<u32> = BTreeSet::new();
    let mut live: BTreeSet<u32> = BTreeSet::new();
    for state in &all_states {
        if released_set.contains(state) {
            freed.extend(reach(state));
        } else {
            live.extend(reach(state));
        }
    }
    let freed: BTreeSet<u32> = freed.difference(&live).copied().collect();
    assert_eq!(p.counts.objects_released as usize, freed.len(), "{ctx}");
    assert_eq!(
        p.counts.object_bytes_released,
        freed
            .iter()
            .map(|o| object_graph.sizes[*o as usize])
            .sum::<u64>(),
        "{ctx}"
    );
    assert_eq!(
        p.counts.states_released as usize,
        p.sets.released_states.len(),
        "{ctx}"
    );
    assert!(p.objects_measured, "{ctx}");
    // A head's states are never released (H6), in either tier.
    for head in &h.heads {
        for state in &h.nodes[index_of(h, &head.commit)].states {
            assert!(!released_set.contains(state), "head state released: {ctx}");
        }
    }
    p
}

fn config() -> ProptestConfig {
    let cases = std::env::var("PROPTEST_CASES")
        .ok()
        .and_then(|v| v.parse().ok())
        .unwrap_or(300);
    let mut config = ProptestConfig::with_cases(cases);
    if std::env::var_os("PROPTEST_RNG_SEED").is_none() {
        config.rng_seed = RngSeed::Fixed(0x686f_7269_7a6f);
    }
    config.failure_persistence = None;
    config
}

proptest! {
    #![proptest_config(config())]

    #[test]
    fn plans_keep_every_invariant_on_every_shape(seed in 1..u64::MAX, shape in 0usize..4) {
        let history = generate(seed, SHAPES[shape]);
        let mut rng = Rng(seed.rotate_left(17) | 1);
        let setup = policy_for(&history, &mut rng);
        check(&history, &setup);
    }

    #[test]
    fn plan_and_hash_do_not_depend_on_insertion_order_or_runs(seed in 1..u64::MAX, shape in 0usize..4) {
        let h = generate(seed, SHAPES[shape]);
        let mut rng = Rng(seed.rotate_left(29) | 1);
        let setup = policy_for(&h, &mut rng);
        let n = h.nodes.len();
        let natural: Vec<usize> = (0..n).collect();
        let first = plan(&view_of(&h, &natural, true), &setup.policy, &setup.options).unwrap();
        let bytes = first.canonical_bytes().unwrap();
        for salt in 1..=4 {
            let order = shuffled(n, salt, seed);
            let again = plan(&view_of(&h, &order, true), &setup.policy, &setup.options).unwrap();
            prop_assert_eq!(&again.plan_hash, &first.plan_hash);
            prop_assert_eq!(again.canonical_bytes().unwrap(), bytes.clone());
            prop_assert_eq!(&again.sets, &first.sets);
        }
        // Branches and holds in another order, and a different example count, change nothing
        // that the hash binds.
        let mut other = generate(seed, SHAPES[shape]);
        other.heads.reverse();
        other.holds.reverse();
        let reordered = plan(
            &view_of(&other, &shuffled(n, 9, seed), true),
            &setup.policy,
            &PlanOptions { list: 0, ..setup.options },
        ).unwrap();
        prop_assert_eq!(&reordered.plan_hash, &first.plan_hash);
        prop_assert!(reordered.examples.released.is_empty());
    }

    #[test]
    fn planning_the_retained_set_again_releases_nothing(seed in 1..u64::MAX, shape in 0usize..4) {
        let h = generate(seed, SHAPES[shape]);
        let mut rng = Rng(seed.rotate_left(41) | 1);
        let setup = policy_for(&h, &mut rng);
        let first = plan(&view_of(&h, &(0..h.nodes.len()).collect::<Vec<_>>(), true), &setup.policy, &setup.options).unwrap();
        if first.tier == engine_core::repository::horizon::Tier::History
            && setup.policy.topology == engine_core::repository::horizon::policy::Topology::Keep
        {
            // What a replica holds after the horizon: the retained bodies and the skeleton.
            let retained: BTreeSet<&Address> = first.sets.retained.iter().collect();
            let order: Vec<usize> = (0..h.nodes.len())
                .filter(|i| retained.contains(&h.nodes[*i].address))
                .collect();
            let mut view = view_of(&h, &order, true);
            for entry in &first.sets.skeleton {
                view.add_prior_skeleton(entry.clone());
            }
            let second = plan(&view, &setup.policy, &setup.options).unwrap();
            prop_assert!(second.sets.released.is_empty(), "seed {} {}", seed, setup.json);
            prop_assert_eq!(&second.sets.retained, &first.sets.retained);
            prop_assert_eq!(second.counts.commits_released, 0);
            prop_assert_eq!(second.counts.states_released, 0);
            prop_assert_eq!(second.counts.objects_released, 0);
            // A third plan is the second plan: the fixed point is stable.
            let third = plan(&view, &setup.policy, &setup.options).unwrap();
            prop_assert_eq!(second.plan_hash, third.plan_hash);
        }
    }
}

/// The laws mean little if the generator never reaches the cases they guard.
#[test]
fn the_generator_reaches_the_cases_the_laws_guard() {
    let (mut released, mut multi_base, mut v2_evidence, mut v3_evidence) = (0, 0, 0, 0);
    let (mut content, mut dropped, mut holds, mut warned, mut shared_states) = (0, 0, 0, 0, 0);
    for seed in 1..=400u64 {
        let h = generate(seed, SHAPES[(seed % 4) as usize]);
        let mut rng = Rng(seed.rotate_left(17) | 1);
        let setup = policy_for(&h, &mut rng);
        let p = check(&h, &setup);
        released += (!p.sets.released.is_empty()) as u32;
        let parents = parents_of(&h);
        multi_base += (0..h.nodes.len())
            .any(|x| (x..h.nodes.len()).any(|y| naive_bases(&parents, x, y).len() > 1))
            as u32;
        v2_evidence += p.sets.kept.iter().any(|(_, r)| *r == "evidence-recovery") as u32;
        v3_evidence += p.sets.kept.iter().any(|(_, r)| *r == "evidence-merge") as u32;
        content += (!matches!(p.tier, engine_core::repository::horizon::Tier::History)) as u32;
        dropped += (p.topology == "drop") as u32;
        holds += p.sets.kept.iter().any(|(_, r)| *r == "hold") as u32;
        warned += (!p.warnings.is_empty()) as u32;
        let states: BTreeSet<_> = h.nodes.iter().flat_map(|n| n.states.clone()).collect();
        shared_states +=
            (states.len() < h.nodes.iter().map(|n| n.states.len()).sum::<usize>()) as u32;
    }
    eprintln!(
        "released={released} multi_base={multi_base} v2_evidence={v2_evidence} v3_evidence={v3_evidence} content={content} dropped={dropped} holds={holds} warned={warned} shared_states={shared_states}"
    );
    // Floors at about half of what seeds 1..=400 produced (272, 138, 47, 67, 118, 20, 125, 46, 400).
    assert!(released >= 130, "released {released}");
    assert!(multi_base >= 70, "criss-cross {multi_base}");
    assert!(v2_evidence >= 25, "v2 evidence {v2_evidence}");
    assert!(v3_evidence >= 30, "v3 evidence {v3_evidence}");
    assert!(content >= 60, "content tier {content}");
    assert!(dropped >= 10, "dropped topology {dropped}");
    assert!(holds >= 60, "holds {holds}");
    assert!(warned >= 20, "warnings {warned}");
    assert!(shared_states >= 200, "shared states {shared_states}");
}

// ---- refusals, edge cases and one counter-example ------------------------------------

fn key() -> Ed25519KeyPair {
    Ed25519KeyPair::from_seed_unchecked(&[7; 32]).unwrap()
}

fn project() -> Uuid {
    Uuid::from_u128(0x77)
}

/// A v1 commit without files; `tag` makes it distinct.
fn plain(parents: &[&Address], tag: i64) -> (Address, Commit) {
    let commit = Commit {
        version: 1,
        project: project(),
        parents: parents.iter().map(|a| (*a).clone()).collect(),
        files: BTreeMap::new(),
        author_key: String::new(),
        authored_at_ms: tag,
        message: format!("c{tag}"),
        recovery: None,
        same_file_resolutions: Vec::new(),
    };
    let signed = SignedCommit::sign(commit, &key()).unwrap();
    (signed.verify().unwrap(), signed.commit)
}

fn load(json: &str) -> Policy {
    Policy::load(json.as_bytes(), LoadOptions::default()).unwrap()
}

const FLOOR: &str = r#"{"version":1,"id":"floor","tier":"history","window":{"min_commits_kept":1},"protect":{"branch_heads":true}}"#;

fn view_with(commits: &[&(Address, Commit)], heads: &[(&str, &Address)]) -> HistoryView {
    let mut view = HistoryView::new(project());
    for (branch, commit) in heads {
        view.add_branch(HeadRef {
            branch: (*branch).into(),
            commit: (*commit).clone(),
            generation: 1,
        })
        .unwrap();
    }
    for (address, commit) in commits {
        view.add_commit(address, commit, 100, None).unwrap();
    }
    view
}

#[test]
fn a_head_only_policy_on_a_chain_keeps_exactly_the_head() {
    let a = plain(&[], 1);
    let b = plain(&[&a.0], 2);
    let c = plain(&[&b.0], 3);
    let view = view_with(&[&c, &a, &b], &[("main", &c.0)]);
    let p = plan(&view, &load(FLOOR), &PlanOptions::default()).unwrap();
    assert_eq!(p.sets.retained, vec![c.0.clone()]);
    let mut released = vec![a.0.clone(), b.0.clone()];
    released.sort();
    assert_eq!(p.sets.released, released);
    // The frontier is the one edge from the head into the skeleton, at b's height.
    let edge = &p.sets.frontier[0];
    assert_eq!(p.sets.frontier.len(), 1);
    assert_eq!(
        (&edge.child, &edge.parent, edge.parent_height),
        (&c.0, &b.0, 1)
    );
    let skeleton: BTreeMap<&Address, (&Vec<Address>, u64)> = p
        .sets
        .skeleton
        .iter()
        .map(|e| (&e.address, (&e.parents, e.height)))
        .collect();
    assert_eq!(skeleton[&a.0], (&vec![], 0));
    assert_eq!(skeleton[&b.0], (&vec![a.0.clone()], 1));
    // No branch at all: nothing is a head, so everything is released.
    let none = view_with(&[&a], &[]);
    let p = plan(&none, &load(FLOOR), &PlanOptions::default()).unwrap();
    assert!(p.sets.retained.is_empty() && p.sets.released == vec![a.0.clone()]);
    assert!(p.heads.is_empty());
}

#[test]
fn the_planner_refuses_inputs_it_cannot_plan_and_says_why() {
    use engine_core::repository::horizon::{Error, Invariant, SkeletonEntry};
    let a = plain(&[], 1);
    let b = plain(&[&a.0], 2);
    let ghost = address(&[99]);
    let timed = load(
        r#"{"version":1,"id":"t","tier":"history","window":{"keep_within_days":3},"protect":{"branch_heads":true}}"#,
    );
    let floor = load(FLOOR);
    let defaults = PlanOptions::default();
    let good = view_with(&[&a, &b], &[("main", &b.0)]);
    // A time rule without an instant is refused; with one it plans.
    assert!(matches!(
        plan(&good, &timed, &defaults),
        Err(Error::Plan(_))
    ));
    let at_zero = PlanOptions {
        as_of_ms: Some(0),
        list: 0,
    };
    plan(&good, &timed, &at_zero).unwrap();
    let too_many = PlanOptions {
        as_of_ms: None,
        list: 1001,
    };
    assert!(matches!(
        plan(&good, &floor, &too_many),
        Err(Error::Limit(_))
    ));
    // A head, a parent or a recovery source that is nowhere.
    let lost_head = view_with(&[&a, &b], &[("main", &ghost)]);
    assert!(matches!(
        plan(&lost_head, &floor, &defaults),
        Err(Error::Missing(_))
    ));
    let orphan = plain(&[&ghost], 3);
    let lost_parent = view_with(&[&a, &orphan], &[("main", &orphan.0)]);
    assert!(matches!(
        plan(&lost_parent, &floor, &defaults),
        Err(Error::Missing(_))
    ));
    let (address, mut v2) = plain(&[&a.0], 4);
    v2.version = 2;
    v2.recovery = Some(Source {
        kind: Kind::Revert,
        commit: ghost.clone(),
        mainline: None,
    });
    let mut lost_source = HistoryView::new(project());
    lost_source.add_commit(&a.0, &a.1, 1, None).unwrap();
    lost_source.add_commit(&address, &v2, 1, None).unwrap();
    assert!(matches!(
        plan(&lost_source, &floor, &defaults),
        Err(Error::Missing(_))
    ));
    // A head that an earlier horizon released, and a commit both stored and released.
    let row = |c: &(Address, Commit), height| SkeletonEntry {
        address: c.0.clone(),
        parents: c.1.parents.clone(),
        height,
    };
    let mut released_head = view_with(&[&a], &[("main", &b.0)]);
    released_head.add_prior_skeleton(row(&b, 1));
    assert!(matches!(
        plan(&released_head, &floor, &defaults),
        Err(Error::Violated(Invariant::H6, _))
    ));
    let mut both = view_with(&[&a, &b], &[("main", &b.0)]);
    both.add_prior_skeleton(row(&a, 0));
    assert!(matches!(
        plan(&both, &floor, &defaults),
        Err(Error::Violated(Invariant::H7, _))
    ));
    // A stored height that is not parent + 1, and an inconsistent object graph.
    let mut wrong_height = HistoryView::new(project());
    wrong_height.add_commit(&a.0, &a.1, 1, Some(0)).unwrap();
    wrong_height.add_commit(&b.0, &b.1, 1, Some(5)).unwrap();
    assert!(matches!(
        plan(&wrong_height, &floor, &defaults),
        Err(Error::Format(_))
    ));
    let mut bad_objects = view_with(&[&a, &b], &[("main", &b.0)]);
    bad_objects.objects.sizes = vec![1, 2];
    bad_objects.objects.children = vec![vec![7], vec![]];
    assert!(matches!(
        plan(&bad_objects, &floor, &defaults),
        Err(Error::Format(_))
    ));
    let mut short_objects = view_with(&[&a, &b], &[("main", &b.0)]);
    short_objects.objects.sizes = vec![1, 2];
    assert!(matches!(
        plan(&short_objects, &floor, &defaults),
        Err(Error::Format(_))
    ));
    // Building a view is checked too.
    let mut view = HistoryView::new(project());
    let foreign = Commit {
        project: Uuid::from_u128(1),
        ..a.1.clone()
    };
    assert!(view.add_commit(&a.0, &foreign, 1, None).is_err());
    let head = |name: &str, generation| HeadRef {
        branch: name.into(),
        commit: a.0.clone(),
        generation,
    };
    view.add_branch(head("main", 1)).unwrap();
    assert!(
        view.add_branch(head("main", 2)).is_err(),
        "duplicate branch"
    );
    assert!(view.add_branch(head("a b", 1)).is_err(), "invalid name");
    assert!(view.add_branch(head("dev", 0)).is_err(), "generation");
    let hold = |reason: &str| Hold {
        commit: a.0.clone(),
        reason: reason.into(),
    };
    assert!(view.add_hold(hold("")).is_err());
    assert!(view.add_hold(hold("tab\there")).is_err());
    view.add_hold(hold("fine")).unwrap();
}

#[test]
fn a_merge_commit_without_a_unique_base_cannot_be_kept_and_is_refused() {
    use engine_core::repository::horizon::Error;
    // a; b and c on a; d = (b, c) and e = (c, b): the bases of d and e are b and c.
    let a = plain(&[], 1);
    let b = plain(&[&a.0], 2);
    let c = plain(&[&a.0], 3);
    let d = plain(&[&b.0, &c.0], 4);
    let e = plain(&[&c.0, &b.0], 5);
    let document = DocumentId(Uuid::from_u128(5));
    let state = address(&[1]);
    let merge = Commit {
        version: 3,
        parents: vec![d.0.clone(), e.0.clone()],
        files: BTreeMap::from([(
            "a".to_owned(),
            FileEntry {
                document,
                state: state.clone(),
            },
        )]),
        message: "merge".into(),
        authored_at_ms: 6,
        same_file_resolutions: vec![Resolution {
            path: "a".into(),
            document,
            base: address(&[2]),
            left: address(&[3]),
            right: address(&[4]),
            merged: state,
            strategy: STRATEGY.into(),
        }],
        ..plain(&[], 6).1
    };
    let signed = SignedCommit::sign(merge, &key()).unwrap();
    let m = (signed.verify().unwrap(), signed.commit);
    let all = [&a, &b, &c, &d, &e, &m];
    let kept = view_with(&all, &[("main", &m.0)]);
    assert!(matches!(
        plan(&kept, &load(FLOOR), &PlanOptions::default()),
        Err(Error::Plan(_))
    ));
    // Not kept for its own sake (the head is d), the same commit is just history.
    let other = view_with(&all, &[("main", &d.0)]);
    let p = plan(&other, &load(FLOOR), &PlanOptions::default()).unwrap();
    assert!(p.sets.released.contains(&m.0));
}

/// The design's I3 asks for the same merge bases "on the retained graph". Bodies
/// alone cannot give them even when the base is retained: with `k` released, `a`
/// still names `k` as a parent but nothing says that `k` leads on to `x`, the base
/// of `a` and `b`. The skeleton row of `k` supplies that edge.
#[test]
fn merge_bases_need_the_skeleton_not_the_bodies_alone() {
    let x = plain(&[], 1);
    let k = plain(&[&x.0], 2);
    let a = plain(&[&k.0], 3);
    let b = plain(&[&x.0], 4);
    let view = view_with(&[&x, &k, &a, &b], &[("main", &a.0), ("dev", &b.0)]);
    let pin = load(
        r#"{"version":1,"id":"pin","tier":"history","window":{"min_commits_kept":1},"protect":{"branch_heads":true,"merge_bases":{"against":"main","branches":["*"]}}}"#,
    );
    let p = plan(&view, &pin, &PlanOptions::default()).unwrap();
    let kept: BTreeSet<&Address> = p.sets.retained.iter().collect();
    assert!(kept.contains(&x.0), "the base of the two heads is pinned");
    assert!(!kept.contains(&k.0), "k is released");
    let commits = [&x, &k, &a, &b];
    let index: BTreeMap<&Address, usize> =
        commits.iter().enumerate().map(|(i, c)| (&c.0, i)).collect();
    let parents =
        |c: &(Address, Commit)| -> Vec<usize> { c.1.parents.iter().map(|p| index[p]).collect() };
    let full: Vec<Vec<usize>> = commits.iter().map(|c| parents(c)).collect();
    assert_eq!(
        naive_bases(&full, 2, 3),
        vec![0],
        "x is the base in the full graph"
    );
    // Bodies only: a released commit is a node with no known parents.
    let bodies_only: Vec<Vec<usize>> = commits
        .iter()
        .map(|c| {
            if kept.contains(&c.0) {
                parents(c)
            } else {
                vec![]
            }
        })
        .collect();
    assert!(naive_bases(&bodies_only, 2, 3).is_empty());
    // Bodies plus the plan's skeleton agree with the full graph.
    let mut with_skeleton = bodies_only.clone();
    for entry in &p.sets.skeleton {
        with_skeleton[index[&entry.address]] = entry.parents.iter().map(|p| index[p]).collect();
    }
    assert_eq!(naive_bases(&with_skeleton, 2, 3), vec![0]);
}

/// Timing, not a correctness check: `cargo test --test horizon_plan_props_test --
/// --ignored --nocapture`. Plan time at the design's local limit of 100,000 commits,
/// with a fork every 50 commits and a merge every 10, in a debug build.
#[test]
#[ignore]
fn planning_a_hundred_thousand_commits_takes_seconds() {
    let mut commits: Vec<(Address, Commit)> = Vec::new();
    let mut rng = Rng(0x1234_5678_9abc_def1);
    let build = std::time::Instant::now();
    for i in 0..100_000usize {
        let parents: Vec<Address> = match i {
            0 => vec![],
            _ if i % 10 == 0 && i >= 20 => {
                let other = i - 2 - rng.below(18);
                vec![commits[i - 1].0.clone(), commits[other].0.clone()]
            }
            _ if i % 50 == 0 => vec![commits[rng.below(i)].0.clone()],
            _ => vec![commits[i - 1].0.clone()],
        };
        let refs: Vec<&Address> = parents.iter().collect();
        commits.push(plain(&refs, i as i64 + 1));
    }
    eprintln!("built {} commits in {:?}", commits.len(), build.elapsed());
    let mut view = HistoryView::new(project());
    for (branch, at) in [("main", 99_999), ("dev", 99_000)] {
        view.add_branch(HeadRef {
            branch: branch.into(),
            commit: commits[at].0.clone(),
            generation: 1,
        })
        .unwrap();
    }
    for (address, commit) in &commits {
        view.add_commit(address, commit, 600, None).unwrap();
    }
    let policy = load(
        r#"{"version":1,"id":"big","tier":"history","window":{"keep_last_commits":500},"protect":{"branch_heads":true,"merge_bases":{"against":"main","branches":["*"]}}}"#,
    );
    let started = std::time::Instant::now();
    let p = plan(&view, &policy, &PlanOptions::default()).unwrap();
    eprintln!(
        "planned {} commits in {:?}: retained {}, released {}",
        p.commits.total,
        started.elapsed(),
        p.commits.retained,
        p.commits.released
    );
    assert_eq!(p.commits.total, 100_000);
}
