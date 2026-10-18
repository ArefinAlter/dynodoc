//! Property, tamper, restart, rollback and fork tests for the transparency module.
//! Every random choice comes from a fixed-seed generator, so a failure reproduces.
//!
//! The 10^6-leaf case is `#[ignore]`d; run it with
//! `cargo test -p engine-core --test transparency_props_test -- --ignored one_million`
//! (about 3 s in a debug build).
use std::collections::BTreeSet;

use engine_core::transparency::merkle::{
    consistency_proof, inclusion_proof, leaf_hash, tree_root, verify_consistency, verify_inclusion,
};
use engine_core::transparency::{
    assess, Assessment, Checkpoint, ConsistencySource, Decision, Error, FsTileStore, HashSource,
    InclusionBundle, KeyKind, KnownCheckpoints, Leaf, Level, Limit, Log, MemoryTileStore,
    MemoryTree, NoProofs, Note, OnFailure, Origin, Policy, Signer, TileLog,
};
use ring::digest::{digest, SHA256};
use serde_json::json;

/// SplitMix64: tiny, fixed-seed, good enough to pick sizes and indices.
struct Rng(u64);

impl Rng {
    fn next(&mut self) -> u64 {
        self.0 = self.0.wrapping_add(0x9E37_79B9_7F4A_7C15);
        let mut z = self.0;
        z = (z ^ (z >> 30)).wrapping_mul(0xBF58_476D_1CE4_E5B9);
        z = (z ^ (z >> 27)).wrapping_mul(0x94D0_49BB_1331_11EB);
        z ^ (z >> 31)
    }

    /// Uniform in `0..n` (`n > 0`).
    fn below(&mut self, n: u64) -> u64 {
        self.next() % n
    }
}

fn entry(i: u64) -> Vec<u8> {
    format!("leaf entry {i}").into_bytes()
}

fn memory_tree(n: u64) -> MemoryTree {
    let mut tree = MemoryTree::new();
    for i in 0..n {
        tree.append_entry(&entry(i));
    }
    tree
}

/// Sizes worth testing for a tree of up to `max` leaves: edges, powers of two and
/// their neighbours, and random ones.
fn sizes(rng: &mut Rng, max: u64, random: usize) -> Vec<u64> {
    let mut sizes = BTreeSet::from([1, 2, 3, max]);
    let mut p = 1;
    while p <= max {
        for s in [p - 1, p, p + 1] {
            if (1..=max).contains(&s) {
                sizes.insert(s);
            }
        }
        p *= 2;
    }
    for _ in 0..random {
        sizes.insert(1 + rng.below(max));
    }
    sizes.into_iter().collect()
}

fn check_random_proofs<S: HashSource>(
    source: &S,
    max: u64,
    seed: u64,
    per_size: usize,
    pairs: usize,
) {
    let mut rng = Rng(seed);
    for size in sizes(&mut rng, max, 25) {
        let root = tree_root(source, size).unwrap();
        let mut indices = vec![0, size - 1];
        indices.extend((0..per_size).map(|_| rng.below(size)));
        for index in indices {
            let leaf = source.node(0, index).unwrap();
            let proof = inclusion_proof(source, index, size).unwrap();
            verify_inclusion(&leaf, index, size, &proof, &root)
                .unwrap_or_else(|e| panic!("inclusion {index} of {size}: {e}"));
        }
        for _ in 0..pairs {
            let first = rng.below(size + 1);
            let proof = consistency_proof(source, first, size).unwrap();
            verify_consistency(
                first,
                size,
                &tree_root(source, first).unwrap(),
                &root,
                &proof,
            )
            .unwrap_or_else(|e| panic!("consistency {first} to {size}: {e}"));
        }
    }
}

#[test]
fn transparency_props_inclusion_and_consistency_for_random_sizes_up_to_1e5() {
    let tree = memory_tree(100_000);
    check_random_proofs(&tree, 100_000, 0x5eed_0001, 40, 40);
}

#[test]
fn transparency_props_every_index_and_pair_for_small_random_sizes() {
    let mut rng = Rng(0x5eed_0002);
    let tree = memory_tree(2000);
    for size in sizes(&mut rng, 2000, 6).into_iter().filter(|s| *s <= 600) {
        let root = tree_root(&tree, size).unwrap();
        for index in 0..size {
            let proof = inclusion_proof(&tree, index, size).unwrap();
            verify_inclusion(&tree.node(0, index).unwrap(), index, size, &proof, &root).unwrap();
        }
    }
    for _ in 0..20 {
        let second = 1 + rng.below(2000);
        let root = tree_root(&tree, second).unwrap();
        for first in (0..=second).step_by(1 + (second / 150) as usize) {
            let proof = consistency_proof(&tree, first, second).unwrap();
            verify_consistency(
                first,
                second,
                &tree_root(&tree, first).unwrap(),
                &root,
                &proof,
            )
            .unwrap();
        }
    }
}

#[test]
#[ignore = "10^6 leaves; see the module docs for the command"]
fn transparency_props_one_million_leaves() {
    let tree = memory_tree(1_000_000);
    check_random_proofs(&tree, 1_000_000, 0x5eed_0003, 25, 25);
}

#[test]
fn transparency_props_tile_store_equals_incremental_computation() {
    // 70 000 crosses the first level-2 tile boundary (65 536 = 256^2).
    let max = 70_000u64;
    let memory = memory_tree(max);
    let entries: Vec<Vec<u8>> = (0..max).map(entry).collect();
    let mut tiles = TileLog::open(MemoryTileStore::new()).unwrap();
    let mut rng = Rng(0x5eed_0004);
    let mut start = 0usize;
    while start < entries.len() {
        let len = 1 + rng.below(3000) as usize;
        let end = (start + len).min(entries.len());
        tiles.append_batch(&entries[start..end]).unwrap();
        assert_eq!(
            tiles.root(),
            tree_root(&memory, end as u64).unwrap(),
            "after {end}"
        );
        start = end;
    }
    assert_eq!(tiles.size(), max);
    for size in sizes(&mut rng, max, 10) {
        assert_eq!(
            tree_root(&tiles, size).unwrap(),
            tree_root(&memory, size).unwrap()
        );
    }
    for _ in 0..60 {
        let size = 1 + rng.below(max);
        let index = rng.below(size);
        assert_eq!(
            inclusion_proof(&tiles, index, size).unwrap(),
            inclusion_proof(&memory, index, size).unwrap()
        );
        let first = rng.below(size + 1);
        assert_eq!(
            consistency_proof(&tiles, first, size).unwrap(),
            consistency_proof(&memory, first, size).unwrap()
        );
        assert_eq!(tiles.entry(index).unwrap(), entry(index));
    }
    check_random_proofs(&tiles, max, 0x5eed_0005, 3, 3);
}

#[test]
fn transparency_props_restart_safety_of_the_directory_store() {
    let dir = std::env::temp_dir().join(format!("dynodoc-restart-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    let total = 3000u64;
    let memory = memory_tree(total);
    let entries: Vec<Vec<u8>> = (0..total).map(entry).collect();
    let mut rng = Rng(0x5eed_0006);
    let mut start = 0usize;
    let mut reopens = 0;
    while start < entries.len() {
        // Drop the log (a restart) before every batch.
        let mut log = TileLog::open(FsTileStore::open(&dir).unwrap()).unwrap();
        assert_eq!(log.size(), start as u64);
        assert_eq!(log.root(), tree_root(&memory, start as u64).unwrap());
        let end = (start + 1 + rng.below(400) as usize).min(entries.len());
        log.append_batch(&entries[start..end]).unwrap();
        start = end;
        reopens += 1;
    }
    assert!(reopens > 5);
    let log = TileLog::open(FsTileStore::open(&dir).unwrap()).unwrap();
    assert_eq!(log.root(), memory.root());
    let proof = inclusion_proof(&log, 1234, total).unwrap();
    verify_inclusion(
        &leaf_hash(&entry(1234)),
        1234,
        total,
        &proof,
        &memory.root(),
    )
    .unwrap();
    assert_eq!(log.entry(2999).unwrap(), entry(2999));
    std::fs::remove_dir_all(&dir).unwrap();
}

// ---- tamper ---------------------------------------------------------------

#[test]
fn transparency_tamper_any_byte_of_a_proof_or_root_or_leaf_fails() {
    let tree = memory_tree(70);
    for (index, size) in [(0u64, 70u64), (33, 70), (69, 70), (5, 13), (0, 1), (6, 8)] {
        let root = tree_root(&tree, size).unwrap();
        let leaf = tree.node(0, index).unwrap();
        let proof = inclusion_proof(&tree, index, size).unwrap();
        verify_inclusion(&leaf, index, size, &proof, &root).unwrap();
        for element in 0..proof.len() {
            for byte in 0..32 {
                let mut bad = proof.clone();
                bad[element][byte] ^= 0x01;
                assert!(verify_inclusion(&leaf, index, size, &bad, &root).is_err());
            }
        }
        for byte in 0..32 {
            let (mut bad_leaf, mut bad_root) = (leaf, root);
            bad_leaf[byte] ^= 0x80;
            bad_root[byte] ^= 0x80;
            assert!(verify_inclusion(&bad_leaf, index, size, &proof, &root).is_err());
            assert!(verify_inclusion(&leaf, index, size, &proof, &bad_root).is_err());
        }
    }
    // A path belongs to one index: no other index (or one past the end) verifies with it.
    for size in [13u64, 70] {
        let root = tree_root(&tree, size).unwrap();
        for index in 0..size {
            let leaf = tree.node(0, index).unwrap();
            let proof = inclusion_proof(&tree, index, size).unwrap();
            for other in (0..size + 3).filter(|o| *o != index) {
                assert!(
                    verify_inclusion(&leaf, other, size, &proof, &root).is_err(),
                    "path of {index} accepted for {other} of {size}"
                );
            }
        }
    }
    for (first, second) in [(1u64, 70u64), (6, 21), (32, 70), (33, 70), (69, 70)] {
        let (r1, r2) = (
            tree_root(&tree, first).unwrap(),
            tree_root(&tree, second).unwrap(),
        );
        let proof = consistency_proof(&tree, first, second).unwrap();
        verify_consistency(first, second, &r1, &r2, &proof).unwrap();
        for element in 0..proof.len() {
            for byte in 0..32 {
                let mut bad = proof.clone();
                bad[element][byte] ^= 0x01;
                assert!(verify_consistency(first, second, &r1, &r2, &bad).is_err());
            }
        }
        for byte in 0..32 {
            let (mut b1, mut b2) = (r1, r2);
            b1[byte] ^= 0x80;
            b2[byte] ^= 0x80;
            assert!(verify_consistency(first, second, &b1, &r2, &proof).is_err());
            assert!(verify_consistency(first, second, &r1, &b2, &proof).is_err());
        }
    }
}

const ORIGIN: &str = "log.test.example/dynodoc/v1/transparency";
const T0: u64 = 1_760_000_000;

struct World {
    log_signer: Signer,
    witnesses: Vec<Signer>,
    policy: Policy,
}

fn world() -> World {
    let log_signer = Signer::from_seed(ORIGIN, KeyKind::Log, &[0x41; 32]).unwrap();
    let names = [
        "w1.verne.example/w",
        "w2.uni-a.example/w",
        "w3.customer.example/w",
    ];
    let witnesses: Vec<Signer> = names
        .iter()
        .zip(0x42u8..)
        .map(|(name, seed)| Signer::from_seed(name, KeyKind::Cosigner, &[seed; 32]).unwrap())
        .collect();
    let operators = ["verne", "uni-a", "customer"];
    let policy = json!({
        "version": 1,
        "log": {"origin": ORIGIN, "vkey": log_signer.verifier_key().to_string()},
        "witnesses": witnesses.iter().zip(operators).map(|(w, op)| json!({
            "name": w.verifier_key().name(),
            "vkey": w.verifier_key().to_string(),
            "operator": op,
        })).collect::<Vec<_>>(),
        "quorum": {"threshold": 2, "distinct_operators": true},
        "freshness": {"max_age_seconds": 3600},
        "on_failure": "reject",
    });
    World {
        log_signer,
        witnesses,
        policy: Policy::from_json(policy.to_string().as_bytes()).unwrap(),
    }
}

/// A leaf of history `variant` at position `i`; different variants never collide.
fn leaf(variant: u8, i: u64) -> Leaf {
    let mut input = vec![variant];
    input.extend_from_slice(&i.to_be_bytes());
    let h = digest(&SHA256, &input);
    let mut other = input.clone();
    other.push(1);
    Leaf::RefReceipt {
        tag: h.as_ref().try_into().unwrap(),
        receipt: digest(&SHA256, &other).as_ref().try_into().unwrap(),
    }
}

/// A log whose first `shared` leaves are history 0 and the rest `variant`'s, up to `size`.
fn history(variant: u8, shared: u64, size: u64) -> Log<MemoryTileStore> {
    let mut log = Log::open(Origin::new(ORIGIN).unwrap(), MemoryTileStore::new()).unwrap();
    let leaves: Vec<Leaf> = (0..size)
        .map(|i| leaf(if i < shared { 0 } else { variant }, i))
        .collect();
    log.append_batch(&leaves).unwrap();
    log
}

/// The checkpoint of the first `size` leaves, signed by the log and the witnesses in
/// `which` at `time`.
fn signed(w: &World, log: &Log<MemoryTileStore>, size: u64, which: &[usize], time: u64) -> Note {
    let text = Checkpoint {
        origin: ORIGIN.to_string(),
        size,
        root: tree_root(log, size).unwrap(),
    }
    .text();
    let mut note = Note::new(&text)
        .unwrap()
        .with_signature(w.log_signer.sign_checkpoint(&text).unwrap())
        .unwrap();
    for &i in which {
        note = note
            .with_signature(w.witnesses[i].cosign_checkpoint(&text, time).unwrap())
            .unwrap();
    }
    note
}

fn know(w: &World, notes: &[&Note]) -> KnownCheckpoints {
    let mut known = KnownCheckpoints::new(w.policy.origin());
    for note in notes {
        known
            .record(&w.policy.verify_checkpoint(note).unwrap())
            .unwrap();
    }
    known
}

fn run(
    w: &World,
    bundle: &InclusionBundle,
    known: &KnownCheckpoints,
    proofs: &dyn ConsistencySource,
) -> Assessment {
    assess(&w.policy, bundle, None, known, proofs, T0 + 10)
}

#[test]
fn transparency_tamper_any_byte_of_a_checkpoint_or_bundle_lowers_the_outcome() {
    let w = world();
    let log = history(0, 0, 40);
    let note = signed(&w, &log, 40, &[0, 1], T0);
    let bundle = log.bundle(17, &note).unwrap();
    let known = KnownCheckpoints::new(w.policy.origin());
    let baseline = run(&w, &bundle, &known, &NoProofs);
    assert_eq!((baseline.level, &baseline.rejection), (Level::L2, &None));
    // Threshold 2 with exactly two cosignatures: any change that drops or damages
    // one of the three signatures, or the text they cover, must show.
    let outcome_is_lower = |a: &Assessment| a.rejection.is_some() || a.level < Level::L2;
    let encoded = note.encode().into_bytes();
    let mut flips = 0;
    for position in 0..encoded.len() {
        for mask in [0x01u8, 0x20, 0x80] {
            let mut bad = encoded.clone();
            bad[position] ^= mask;
            let Ok(text) = String::from_utf8(bad) else {
                continue;
            };
            let mut tampered = bundle.clone();
            tampered.checkpoint = text;
            assert!(
                outcome_is_lower(&run(&w, &tampered, &known, &NoProofs)),
                "byte {position} mask {mask:#x} of the checkpoint went unnoticed"
            );
            flips += 1;
        }
    }
    assert!(flips > 600);
    // Whole bundle JSON: a mutation either fails to parse or lowers the outcome.
    let json = bundle.to_json();
    for position in 0..json.len() {
        for mask in [0x01u8, 0x20, 0x80] {
            let mut bad = json.clone();
            bad[position] ^= mask;
            if let Ok(parsed) = InclusionBundle::from_json(&bad) {
                assert!(
                    outcome_is_lower(&run(&w, &parsed, &known, &NoProofs)),
                    "byte {position} mask {mask:#x} of the bundle went unnoticed"
                );
            }
        }
    }
    // Leaf data: every byte of the leaf entry is bound to the leaf hash.
    let entry = bundle.leaf.entry().unwrap();
    for position in 0..entry.len() {
        let mut bad = entry.clone();
        bad[position] ^= 0x01;
        let still_a_leaf = Leaf::from_entry(&bad);
        if let Ok(parsed) = still_a_leaf {
            assert_ne!(parsed.hash().unwrap(), bundle.leaf.hash().unwrap());
        }
        assert_ne!(leaf_hash(&bad), bundle.leaf.hash().unwrap());
    }
}

// ---- rollback, split view, forks, quorum ------------------------------------

#[test]
fn transparency_assurance_first_contact_is_at_most_l2() {
    let w = world();
    let log = history(0, 0, 50);
    let note = signed(&w, &log, 50, &[0, 1, 2], T0);
    let bundle = log.bundle(3, &note).unwrap();
    let got = run(&w, &bundle, &KnownCheckpoints::new(w.policy.origin()), &log);
    assert_eq!((got.level, got.rejection.clone()), (Level::L2, None));
    assert_eq!(got.limits, vec![Limit::NoPriorCheckpoint]);
    assert_eq!(got.decision(Level::L3, OnFailure::Reject), Decision::Reject);
    assert_eq!(got.decision(Level::L2, OnFailure::Reject), Decision::Accept);
    assert!(got.trusted_checkpoint().is_some());
}

#[test]
fn transparency_assurance_l3_follows_the_log_forward_and_back() {
    let w = world();
    let log = history(0, 0, 150);
    let at100 = signed(&w, &log, 100, &[0, 1], T0);
    let known = know(&w, &[&at100]);
    // Forward: a later checkpoint with a proof from the log.
    let at150 = signed(&w, &log, 150, &[1, 2], T0);
    let got = run(&w, &log.bundle(120, &at150).unwrap(), &known, &log);
    assert_eq!(
        (got.level, got.rejection, got.limits),
        (Level::L3, None, vec![])
    );
    // Backward: an honest older checkpoint is a prefix of the known one.
    let at60 = signed(&w, &log, 60, &[0, 2], T0);
    let got = run(&w, &log.bundle(10, &at60).unwrap(), &known, &log);
    assert_eq!((got.level, got.rejection.clone()), (Level::L3, None));
    // The known checkpoint itself.
    let got = run(&w, &log.bundle(99, &at100).unwrap(), &known, &NoProofs);
    assert_eq!((got.level, got.rejection.clone()), (Level::L3, None));
    // Without a proof the level stays L2 and says why.
    let got = run(&w, &log.bundle(120, &at150).unwrap(), &known, &NoProofs);
    assert_eq!(got.level, Level::L2);
    assert_eq!(
        got.limits,
        vec![Limit::ConsistencyUnavailable { known_size: 100 }]
    );
    assert_eq!(got.decision(Level::L3, OnFailure::Reject), Decision::Reject);
}

#[test]
fn transparency_assurance_rollback_to_a_rewritten_history_is_detected() {
    let w = world();
    let honest = history(0, 0, 100);
    let known = know(&w, &[&signed(&w, &honest, 100, &[0, 1], T0)]);
    // The log rewrites everything from leaf 60 on and presents a *shorter* tree,
    // cosigned by a colluding quorum.
    let rewritten = history(1, 60, 90);
    let note = signed(&w, &rewritten, 90, &[0, 1], T0);
    let bundle = rewritten.bundle(80, &note).unwrap();
    // Whatever proof it offers: its own (for its own root), or the honest log's.
    for source in [&rewritten as &dyn ConsistencySource, &honest, &NoProofs] {
        let got = run(&w, &bundle, &known, source);
        assert_ne!(got.level, Level::L3);
        assert_eq!(got.decision(Level::L3, OnFailure::Reject), Decision::Reject);
    }
    assert_eq!(
        run(&w, &bundle, &known, &honest).rejection,
        Some(Error::Inconsistent {
            known: 100,
            new: 90
        })
    );
    // A leaf from the shared prefix gets no help from the shared prefix.
    let early = rewritten.bundle(10, &note).unwrap();
    assert_eq!(
        run(&w, &early, &known, &honest).rejection,
        Some(Error::Inconsistent {
            known: 100,
            new: 90
        })
    );
    // An honest truncation is a prefix: consistent (staleness is the freshness window's job).
    let prefix = signed(&w, &honest, 90, &[0, 1], T0);
    let got = run(&w, &honest.bundle(80, &prefix).unwrap(), &known, &honest);
    assert_eq!((got.level, got.rejection.clone()), (Level::L3, None));
}

#[test]
fn transparency_assurance_forked_history_is_detected() {
    let w = world();
    let honest = history(0, 0, 100);
    let known = know(&w, &[&signed(&w, &honest, 100, &[0, 1], T0)]);
    // Two logs agree on the first 50 leaves, then diverge; the fork is larger.
    let fork = history(2, 50, 120);
    let note = signed(&w, &fork, 120, &[0, 1], T0);
    let bundle = fork.bundle(110, &note).unwrap();
    // The fork's own proof from 100 is for the fork's size-100 root, not the known one.
    let got = run(&w, &bundle, &known, &fork);
    assert_eq!(got.level, Level::L0);
    assert_eq!(
        got.rejection,
        Some(Error::Inconsistent {
            known: 100,
            new: 120
        })
    );
    assert!(got.trusted_checkpoint().is_none());
    // The honest log cannot prove a 120-leaf tree: no proof, no L3.
    let got = run(&w, &bundle, &known, &honest);
    assert_eq!(got.level, Level::L2);
    assert_eq!(
        got.limits,
        vec![Limit::ConsistencyUnavailable { known_size: 100 }]
    );
}

#[test]
fn transparency_assurance_two_roots_at_one_size_is_a_split_view() {
    let w = world();
    let honest = history(0, 0, 100);
    let at100 = signed(&w, &honest, 100, &[0, 1], T0);
    let known = know(&w, &[&at100]);
    let other = history(3, 70, 100);
    let note = signed(&w, &other, 100, &[0, 1, 2], T0);
    let got = run(&w, &other.bundle(5, &note).unwrap(), &known, &NoProofs);
    assert_eq!(
        (got.level, got.rejection.clone()),
        (Level::L0, Some(Error::SplitView { size: 100 }))
    );
    // Recording it as well is refused, and nothing is stored.
    let mut set = know(&w, &[&at100]);
    let verified = w.policy.verify_checkpoint(&note).unwrap();
    assert_eq!(set.record(&verified), Err(Error::SplitView { size: 100 }));
    assert_eq!(set.len(), 1);
    assert_eq!(set.latest().unwrap().0, 100);
}

#[test]
fn transparency_assurance_a_forked_receipt_without_inclusion_is_rejected() {
    let w = world();
    let log = history(0, 0, 30);
    let note = signed(&w, &log, 30, &[0, 1], T0);
    let genuine = log.bundle(7, &note).unwrap();
    let known = KnownCheckpoints::new(w.policy.origin());
    // The fork: same scope and generation (same tag), another receipt address.
    let forked = match genuine.leaf.clone() {
        Leaf::RefReceipt { tag, .. } => Leaf::RefReceipt {
            tag,
            receipt: [0xee; 32],
        },
        other => panic!("{other:?}"),
    };
    // 1. The genuine bundle with the fork's leaf swapped in: the path does not fold.
    let mut swapped = genuine.clone();
    swapped.leaf = forked.clone();
    let got = run(&w, &swapped, &known, &NoProofs);
    assert_eq!(
        (got.level, got.rejection.clone()),
        (Level::L0, Some(Error::BadInclusionProof))
    );
    // 2. The client holds the fork and is handed the genuine bundle: leaf mismatch.
    let got = assess(
        &w.policy,
        &genuine,
        Some(&forked),
        &known,
        &NoProofs,
        T0 + 10,
    );
    assert_eq!(
        (got.level, got.rejection.clone()),
        (Level::L0, Some(Error::LeafMismatch))
    );
    // 3. A log that does include the fork, signed by the log but not witnessed: L1,
    // which the policy (threshold 2) does not accept.
    let mut cheat = Log::open(Origin::new(ORIGIN).unwrap(), MemoryTileStore::new()).unwrap();
    let mut leaves: Vec<Leaf> = (0..30).map(|i| leaf(0, i)).collect();
    leaves[7] = forked.clone();
    cheat.append_batch(&leaves).unwrap();
    let alone = signed(&w, &cheat, 30, &[], T0);
    let got = assess(
        &w.policy,
        &cheat.bundle(7, &alone).unwrap(),
        Some(&forked),
        &known,
        &NoProofs,
        T0 + 10,
    );
    assert_eq!((got.level, got.rejection.clone()), (Level::L1, None));
    assert_eq!(
        got.limits[0],
        Limit::QuorumNotMet {
            counted: 0,
            needed: 2
        }
    );
    assert_eq!(
        got.decision(Level::L2, w.policy.on_failure()),
        Decision::Reject
    );
    assert!(got.trusted_checkpoint().is_none());
}

#[test]
fn transparency_assurance_freshness_unknown_and_corrupt_cosignatures() {
    let w = world();
    let log = history(0, 0, 20);
    let known = KnownCheckpoints::new(w.policy.origin());
    let bundle_at = |note: &Note| log.bundle(4, note).unwrap();
    // Stale (older than 3600 s) and future-dated (more than 3600 s ahead) do not count.
    let stale = signed(&w, &log, 20, &[0, 1], T0 - 4000);
    let got = assess(&w.policy, &bundle_at(&stale), None, &known, &NoProofs, T0);
    assert_eq!(got.level, Level::L1);
    let future = signed(&w, &log, 20, &[0, 1], T0 + 4000);
    let got = assess(&w.policy, &bundle_at(&future), None, &known, &NoProofs, T0);
    assert_eq!(got.level, Level::L1);
    // Inside the window on both sides counts.
    let edge = signed(&w, &log, 20, &[0, 1], T0 - 3600);
    let got = assess(&w.policy, &bundle_at(&edge), None, &known, &NoProofs, T0);
    assert_eq!(got.level, Level::L2);
    // One fresh and one stale cosignature are one vote.
    let text = Checkpoint {
        origin: ORIGIN.to_string(),
        size: 20,
        root: log.root(),
    }
    .text();
    let mixed = Note::new(&text)
        .unwrap()
        .with_signature(w.log_signer.sign_checkpoint(&text).unwrap())
        .unwrap()
        .with_signature(w.witnesses[0].cosign_checkpoint(&text, T0).unwrap())
        .unwrap()
        .with_signature(w.witnesses[1].cosign_checkpoint(&text, T0 - 9000).unwrap())
        .unwrap();
    let got = assess(&w.policy, &bundle_at(&mixed), None, &known, &NoProofs, T0);
    assert_eq!(
        (got.level, got.limits[0].clone()),
        (
            Level::L1,
            Limit::QuorumNotMet {
                counted: 1,
                needed: 2
            }
        )
    );
    // A cosignature from a key outside the policy is ignored, not counted, not an error.
    let stranger =
        Signer::from_seed("w9.stranger.example/w", KeyKind::Cosigner, &[0x77; 32]).unwrap();
    let with_stranger = signed(&w, &log, 20, &[0], T0)
        .with_signature(stranger.cosign_checkpoint(&text, T0).unwrap())
        .unwrap();
    let got = assess(
        &w.policy,
        &bundle_at(&with_stranger),
        None,
        &known,
        &NoProofs,
        T0,
    );
    assert_eq!((got.level, got.rejection.clone()), (Level::L1, None));
    // A known witness's corrupt cosignature rejects the checkpoint outright.
    let good = signed(&w, &log, 20, &[0, 1], T0).encode();
    let witness_line = good.lines().find(|l| l.contains("w1.verne")).unwrap();
    let mut corrupt = witness_line.as_bytes().to_vec();
    let at = corrupt.len() - 8;
    corrupt[at] = if corrupt[at] == b'A' { b'B' } else { b'A' };
    let corrupt = good.replace(witness_line, &String::from_utf8(corrupt).unwrap());
    let mut bundle = bundle_at(&signed(&w, &log, 20, &[0, 1], T0));
    bundle.checkpoint = corrupt;
    let got = assess(&w.policy, &bundle, None, &known, &NoProofs, T0);
    assert!(
        matches!(got.rejection, Some(Error::BadSignature(_))),
        "{:?}",
        got.rejection
    );
    assert_eq!(got.level, Level::L0);
}

#[test]
fn transparency_assurance_known_checkpoints_belong_to_one_log() {
    let w = world();
    let log = history(0, 0, 10);
    let note = signed(&w, &log, 10, &[0, 1], T0);
    let elsewhere = Origin::new("elsewhere.test.example/dynodoc/v1/transparency").unwrap();
    let wrong = KnownCheckpoints::new(&elsewhere);
    let got = run(&w, &log.bundle(0, &note).unwrap(), &wrong, &NoProofs);
    assert!(matches!(got.rejection, Some(Error::WrongOrigin { .. })));
    let mut wrong = wrong;
    let verified = w.policy.verify_checkpoint(&note).unwrap();
    assert!(matches!(
        wrong.record(&verified),
        Err(Error::WrongOrigin { .. })
    ));
    assert!(wrong.is_empty());
}

#[test]
fn transparency_log_key_and_witness_keys_cannot_stand_in_for_each_other() {
    let w = world();
    let log = history(0, 0, 10);
    let text = log.checkpoint().text();
    // A witness key cannot produce the log's signature, nor the log key a cosignature.
    assert_eq!(
        w.witnesses[0].sign_checkpoint(&text),
        Err(Error::WrongSignerKind)
    );
    assert_eq!(
        w.log_signer.cosign_checkpoint(&text, T0),
        Err(Error::WrongSignerKind)
    );
    // A note signed only by witnesses (no log signature) is refused.
    let mut note = Note::new(&text).unwrap();
    for witness in &w.witnesses {
        note = note
            .with_signature(witness.cosign_checkpoint(&text, T0).unwrap())
            .unwrap();
    }
    assert_eq!(
        w.policy.verify_checkpoint(&note),
        Err(Error::MissingLogSignature)
    );
    // A cosignature does not verify as a log signature even under the log's name:
    // a witness holding a key named like the log is a different key id.
    let imposter = Signer::from_seed(ORIGIN, KeyKind::Cosigner, &[0x41; 32]).unwrap();
    let note = Note::new(&text)
        .unwrap()
        .with_signature(imposter.cosign_checkpoint(&text, T0).unwrap())
        .unwrap();
    assert_eq!(
        w.policy.verify_checkpoint(&note),
        Err(Error::MissingLogSignature)
    );
}
