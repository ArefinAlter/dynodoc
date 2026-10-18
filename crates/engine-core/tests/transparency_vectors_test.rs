//! Reproduces `fixtures/transparency-v1.json` byte for byte. The fixture is written
//! by `fixtures/gen_transparency_vectors.py`, an independent pure-Python
//! implementation; never regenerate it to make one of these checks pass.
use engine_core::transparency::merkle::{
    consistency_proof, inclusion_proof, tree_root, verify_consistency, verify_inclusion,
};
use engine_core::transparency::tiles::tile_index_path;
use engine_core::transparency::{
    assess, Error, Hash, InclusionBundle, KeyKind, KnownCheckpoints, Leaf, Level, Limit, Log,
    MemoryTileStore, MemoryTree, NoProofs, Note, Origin, Policy, Signer, TagKey, TileLog,
    VerifierKey,
};
use ring::digest::{digest, SHA256};
use serde_json::Value;
use uuid::Uuid;

fn fixture() -> Value {
    serde_json::from_str(include_str!("fixtures/transparency-v1.json")).unwrap()
}

fn hex(bytes: &[u8]) -> String {
    bytes.iter().map(|b| format!("{b:02x}")).collect()
}

fn unhex(text: &str) -> Vec<u8> {
    assert!(text.len() % 2 == 0, "odd hex {text}");
    (0..text.len())
        .step_by(2)
        .map(|i| u8::from_str_radix(&text[i..i + 2], 16).unwrap())
        .collect()
}

fn h32(text: &str) -> Hash {
    unhex(text).try_into().unwrap()
}

fn s(v: &Value) -> &str {
    v.as_str().unwrap()
}

fn n(v: &Value) -> u64 {
    v.as_u64().unwrap()
}

fn path_of(v: &Value) -> Vec<Hash> {
    v.as_array().unwrap().iter().map(|h| h32(s(h))).collect()
}

fn sha256_hex(bytes: &[u8]) -> String {
    hex(digest(&SHA256, bytes).as_ref())
}

fn keys(f: &Value) -> (TagKey, TagKey) {
    (
        TagKey::new(h32(s(&f["keys"]["k_project"]))),
        TagKey::new(h32(s(&f["keys"]["k_installation"]))),
    )
}

fn leaf_from(inputs: &Value, k_project: &TagKey, k_installation: &TagKey) -> Leaf {
    match s(&inputs["type"]) {
        "ref" => Leaf::RefReceipt {
            tag: k_project.ref_tag(s(&inputs["branch"]), n(&inputs["generation"])),
            receipt: h32(s(&inputs["receipt"])),
        },
        "rot" => Leaf::AuthorityRotation {
            tag: k_installation.rotation_tag(),
            rotation: h32(s(&inputs["rotation"])),
        },
        "hzn" => Leaf::HistoryHorizon {
            tag: k_project.horizon_tag(n(&inputs["seq"])),
            horizon: h32(s(&inputs["horizon"])),
            seq: n(&inputs["seq"]),
        },
        "evh" => Leaf::EventHeads {
            root: h32(s(&inputs["root"])),
            count: n(&inputs["count"]),
            interval: n(&inputs["interval"]),
        },
        "ver" => Leaf::NamedVersion {
            tag: k_project.version_tag(&Uuid::parse_str(s(&inputs["document"])).unwrap()),
            version: h32(s(&inputs["version"])),
        },
        "suc" => Leaf::LogSuccessor {
            prev_origin: Origin::new(s(&inputs["prev_origin"])).unwrap(),
            size: n(&inputs["size"]),
            root: h32(s(&inputs["root"])),
        },
        other => panic!("unknown leaf type {other}"),
    }
}

fn tree_leaves(f: &Value) -> Vec<Leaf> {
    let (k_project, k_installation) = keys(f);
    f["leaves"]
        .as_array()
        .unwrap()
        .iter()
        .map(|record| leaf_from(record, &k_project, &k_installation))
        .collect()
}

fn tree_of(leaves: &[Leaf]) -> MemoryTree {
    let mut tree = MemoryTree::new();
    for leaf in leaves {
        tree.append_entry(&leaf.entry().unwrap());
    }
    tree
}

fn log_signer(f: &Value) -> Signer {
    let k = &f["keys"]["log"];
    Signer::from_seed(s(&k["name"]), KeyKind::Log, &h32(s(&k["seed"]))).unwrap()
}

fn witness_signers(f: &Value) -> Vec<Signer> {
    f["keys"]["witnesses"]
        .as_array()
        .unwrap()
        .iter()
        .map(|k| Signer::from_seed(s(&k["name"]), KeyKind::Cosigner, &h32(s(&k["seed"]))).unwrap())
        .collect()
}

#[test]
fn transparency_vectors_keys_and_key_ids() {
    let f = fixture();
    let log = log_signer(&f).verifier_key();
    let k = &f["keys"]["log"];
    assert_eq!(hex(log.public()), s(&k["public"]));
    assert_eq!(hex(&log.key_id()), s(&k["key_id"]));
    assert_eq!(log.to_string(), s(&k["vkey"]));
    assert_eq!(VerifierKey::parse(s(&k["vkey"])).unwrap(), log);
    // The same seed under another name and role is a different key id.
    let rogue = &f["keys"]["rogue_log"];
    let rogue_key = Signer::from_seed(s(&rogue["name"]), KeyKind::Log, &h32(s(&rogue["seed"])))
        .unwrap()
        .verifier_key();
    assert_eq!(hex(rogue_key.public()), s(&rogue["public"]));
    assert_eq!(hex(&rogue_key.key_id()), s(&rogue["key_id"]));
    for (signer, k) in witness_signers(&f)
        .iter()
        .zip(f["keys"]["witnesses"].as_array().unwrap())
    {
        let key = signer.verifier_key();
        assert_eq!(hex(key.public()), s(&k["public"]));
        assert_eq!(hex(&key.key_id()), s(&k["key_id"]));
        assert_eq!(key.to_string(), s(&k["vkey"]));
        assert_eq!(VerifierKey::parse(s(&k["vkey"])).unwrap(), key);
    }
}

#[test]
fn transparency_vectors_tags() {
    let f = fixture();
    let (k_project, k_installation) = keys(&f);
    for t in f["tags"].as_array().unwrap() {
        let got = match s(&t["kind"]) {
            "ref" => k_project.ref_tag(s(&t["branch"]), n(&t["generation"])),
            "rot" => k_installation.rotation_tag(),
            "hzn" => k_project.horizon_tag(n(&t["seq"])),
            "ver" => k_project.version_tag(&Uuid::parse_str(s(&t["document"])).unwrap()),
            other => panic!("{other}"),
        };
        assert_eq!(hex(&got), s(&t["tag"]), "{t}");
    }
}

#[test]
fn transparency_vectors_leaves_and_leaf_hashes() {
    let f = fixture();
    let leaves = tree_leaves(&f);
    let records = f["leaves"].as_array().unwrap();
    assert_eq!(leaves.len(), 64);
    for (leaf, record) in leaves.iter().zip(records) {
        assert_eq!(
            String::from_utf8(leaf.data().unwrap()).unwrap(),
            s(&record["data"])
        );
        assert_eq!(hex(&leaf.hash().unwrap()), s(&record["leaf_hash"]));
        assert_eq!(Leaf::parse(s(&record["data"]).as_bytes()).unwrap(), *leaf);
        // Tags in the data are the independently computed tags.
        if let Some(tag) = record["tag"].as_str() {
            assert!(s(&record["data"]).contains(tag));
        }
    }
}

#[test]
fn transparency_vectors_roots_inclusion_and_consistency() {
    let f = fixture();
    let leaves = tree_leaves(&f);
    let tree = tree_of(&leaves);
    let roots = f["roots"].as_array().unwrap();
    assert_eq!(roots.len(), 64);
    for (i, root) in roots.iter().enumerate() {
        assert_eq!(
            hex(&tree_root(&tree, i as u64 + 1).unwrap()),
            s(root),
            "size {}",
            i + 1
        );
    }
    let leaf_hash = |i: u64| leaves[i as usize].hash().unwrap();
    let inclusion = f["inclusion"].as_array().unwrap();
    assert!(inclusion.len() > 100);
    for case in inclusion {
        let (size, index) = (n(&case["size"]), n(&case["index"]));
        let expected = path_of(&case["path"]);
        let got = inclusion_proof(&tree, index, size).unwrap();
        assert_eq!(got, expected, "inclusion {index} of {size}");
        let root = tree_root(&tree, size).unwrap();
        verify_inclusion(&leaf_hash(index), index, size, &expected, &root).unwrap();
    }
    let consistency = f["consistency"].as_array().unwrap();
    assert!(consistency.len() > 100);
    for case in consistency {
        let (first, second) = (n(&case["first"]), n(&case["second"]));
        let expected = path_of(&case["proof"]);
        let got = consistency_proof(&tree, first, second).unwrap();
        assert_eq!(got, expected, "consistency {first} to {second}");
        let (r1, r2) = (
            tree_root(&tree, first).unwrap(),
            tree_root(&tree, second).unwrap(),
        );
        verify_consistency(first, second, &r1, &r2, &expected).unwrap();
    }
}

fn tile_entries(f: &Value) -> Vec<Vec<u8>> {
    let (k_project, _) = keys(f);
    (0..n(&f["tiles"]["leaf_count"]))
        .map(|i| {
            let mut input = b"dynodoc.test-receipt".to_vec();
            input.extend_from_slice(&i.to_be_bytes());
            Leaf::RefReceipt {
                tag: k_project.ref_tag("main", i + 1),
                receipt: digest(&SHA256, &input).as_ref().try_into().unwrap(),
            }
            .entry()
            .unwrap()
        })
        .collect()
}

#[test]
fn transparency_vectors_tiles() {
    let f = fixture();
    let t = &f["tiles"];
    for (index, expected) in t["index_paths"].as_object().unwrap() {
        assert_eq!(tile_index_path(index.parse().unwrap()), s(expected));
    }
    let entries = tile_entries(&f);
    let mut log = TileLog::open(MemoryTileStore::new()).unwrap();
    assert_eq!(log.append_batch(&entries).unwrap(), 0);
    assert_eq!(hex(&log.root()), s(&t["root"]));
    // Exactly the expected files, with the expected bytes.
    let expected = t["files"].as_object().unwrap();
    let stored = log.store().files();
    assert_eq!(
        stored.keys().map(String::as_str).collect::<Vec<_>>(),
        expected.keys().map(String::as_str).collect::<Vec<_>>()
    );
    for (path, bytes) in stored {
        assert_eq!(sha256_hex(bytes), s(&expected[path]["sha256"]), "{path}");
        assert_eq!(bytes.len() as u64, n(&expected[path]["bytes"]), "{path}");
    }
    for (path, bytes) in t["small_files"].as_object().unwrap() {
        assert_eq!(hex(&stored[path]), s(bytes), "{path}");
    }
    // Proofs generated from the tiles equal the independent ones.
    let leaf_hash = |i: u64| engine_core::transparency::merkle::leaf_hash(&entries[i as usize]);
    for case in t["inclusion"].as_array().unwrap() {
        let (size, index) = (n(&case["size"]), n(&case["index"]));
        let expected = path_of(&case["path"]);
        assert_eq!(
            inclusion_proof(&log, index, size).unwrap(),
            expected,
            "{size}/{index}"
        );
        verify_inclusion(
            &leaf_hash(index),
            index,
            size,
            &expected,
            &tree_root(&log, size).unwrap(),
        )
        .unwrap();
    }
    for case in t["consistency"].as_array().unwrap() {
        let (first, second) = (n(&case["first"]), n(&case["second"]));
        let expected = path_of(&case["proof"]);
        assert_eq!(consistency_proof(&log, first, second).unwrap(), expected);
        verify_consistency(
            first,
            second,
            &tree_root(&log, first).unwrap(),
            &tree_root(&log, second).unwrap(),
            &expected,
        )
        .unwrap();
    }
    // Entries come back out of the bundles.
    for index in [0usize, 255, 256, 511, 512, 599] {
        assert_eq!(log.entry(index as u64).unwrap(), entries[index]);
    }
}

fn log_of(f: &Value) -> Log<MemoryTileStore> {
    let origin = Origin::new(s(&f["checkpoint"]["size64"]["origin"])).unwrap();
    let mut log = Log::open(origin, MemoryTileStore::new()).unwrap();
    log.append_batch(&tree_leaves(f)).unwrap();
    log
}

/// A note for the first `size` leaves with the log signature and the cosignatures of
/// the witnesses in `which`, at the given times.
fn note_of(
    f: &Value,
    log: &Log<MemoryTileStore>,
    size: u64,
    which: &[usize],
    times: &[u64],
) -> Note {
    let text = engine_core::transparency::Checkpoint {
        origin: log.origin().as_str().to_string(),
        size,
        root: tree_root(log, size).unwrap(),
    }
    .text();
    let mut note = Note::new(&text)
        .unwrap()
        .with_signature(log_signer(f).sign_checkpoint(&text).unwrap())
        .unwrap();
    let witnesses = witness_signers(f);
    for &i in which {
        note = note
            .with_signature(witnesses[i].cosign_checkpoint(&text, times[i]).unwrap())
            .unwrap();
    }
    note
}

fn times(f: &Value, name: &str) -> Vec<u64> {
    f["checkpoint"][name]
        .as_array()
        .unwrap()
        .iter()
        .map(n)
        .collect()
}

#[test]
fn transparency_vectors_checkpoint_text_and_signed_notes() {
    let f = fixture();
    let log = log_of(&f);
    let cp = &f["checkpoint"];
    assert_eq!(log.checkpoint().text(), s(&cp["size64"]["text"]));
    assert_eq!(hex(&log.root()), s(&cp["size64"]["root"]));
    assert_eq!(hex(&tree_root(&log, 40).unwrap()), s(&cp["size40"]["root"]));
    let (t, old) = (times(&f, "witness_times"), times(&f, "old_witness_times"));
    let notes = &f["notes"];
    for (name, size, which, at) in [
        ("size64_log_only", 64, &[][..], &t),
        ("size64_one", 64, &[0][..], &t),
        ("size64_quorum", 64, &[0, 1][..], &t),
        ("size64_full", 64, &[0, 1, 2][..], &t),
        ("size40_quorum", 40, &[0, 1][..], &old),
    ] {
        let note = note_of(&f, &log, size, which, at);
        // Signing is deterministic (Ed25519), so these are the independent bytes.
        assert_eq!(note.encode(), s(&notes[name]), "{name}");
        assert_eq!(
            Note::parse(s(&notes[name]).as_bytes()).unwrap(),
            note,
            "{name}"
        );
    }
    let text = log.checkpoint().text();
    let sig = log_signer(&f).sign_checkpoint(&text).unwrap();
    let only = Note::new(&text)
        .unwrap()
        .with_signature(sig)
        .unwrap()
        .encode();
    assert!(only.ends_with(s(&cp["log_signature_line_size64"])));
    let w1 = witness_signers(&f)[0]
        .cosign_checkpoint(&text, t[0])
        .unwrap();
    let w1_note = Note::new(&text)
        .unwrap()
        .with_signature(w1)
        .unwrap()
        .encode();
    assert!(w1_note.ends_with(s(&cp["cosignature_line_w1_size64"])));
}

#[test]
fn transparency_vectors_policy_accepts_the_notes() {
    let f = fixture();
    let policy = Policy::from_json(f["policy"].to_string().as_bytes()).unwrap();
    let verified = |name: &str| {
        policy
            .verify_checkpoint(&Note::parse(s(&f["notes"][name]).as_bytes()).unwrap())
            .unwrap()
    };
    assert!(verified("size64_log_only").cosigners().is_empty());
    assert_eq!(verified("size64_one").cosigners().len(), 1);
    let quorum = verified("size64_quorum");
    assert_eq!(quorum.cosigners().len(), 2);
    assert_eq!(quorum.cosigners()[0].time, times(&f, "witness_times")[0]);
    assert_eq!(quorum.cosigners()[0].operator, "verne");
    assert_eq!(verified("size64_full").cosigners().len(), 3);
    assert_eq!(quorum.checkpoint().size, 64);
}

#[test]
fn transparency_vectors_bundles() {
    let f = fixture();
    let log = log_of(&f);
    let index = n(&f["bundle_index"]);
    for (name, wire) in f["bundles"].as_object().unwrap() {
        let bundle = InclusionBundle::from_json(wire.to_string().as_bytes()).unwrap();
        assert_eq!(bundle.index, index);
        // The log builds the same bundle from its own state and the same note.
        let note = Note::parse(bundle.checkpoint.as_bytes()).unwrap();
        assert_eq!(log.bundle(index, &note).unwrap(), bundle, "{name}");
    }
    let quorum =
        InclusionBundle::from_json(f["bundles"]["size64_quorum"].to_string().as_bytes()).unwrap();
    assert_eq!(
        String::from_utf8(quorum.to_json()).unwrap(),
        s(&f["bundle_canonical_json"])
    );
    quorum.verify_path(&log.root()).unwrap();
}

fn limit_label(limit: &Limit) -> &'static str {
    match limit {
        Limit::NoPriorCheckpoint => "no_prior_checkpoint",
        Limit::QuorumNotMet { .. } => "quorum_not_met",
        Limit::ConsistencyUnavailable { .. } => "consistency_unavailable",
    }
}

fn reason(error: &Error) -> &'static str {
    match error {
        Error::MalformedNote(_) | Error::Base64(_) => "malformed_note",
        Error::DuplicateSignature(_) => "duplicate_signature",
        Error::MissingLogSignature => "missing_log_signature",
        Error::BadSignature(_) => "bad_signature",
        Error::WrongOrigin { .. } => "wrong_origin",
        Error::CheckpointExtension => "checkpoint_extension",
        Error::MalformedCheckpoint(_) => "malformed_checkpoint",
        Error::BadInclusionProof => "bad_inclusion_proof",
        Error::IndexOutOfRange { .. } => "index_out_of_range",
        Error::BundleSizeMismatch { .. } => "bundle_size_mismatch",
        other => panic!("unmapped error {other:?}"),
    }
}

struct FixedProof {
    first: u64,
    second: u64,
    proof: Vec<Hash>,
}

impl engine_core::transparency::ConsistencySource for FixedProof {
    fn consistency_proof(&self, first: u64, second: u64) -> Option<Vec<Hash>> {
        (first == self.first && second == self.second).then(|| self.proof.clone())
    }
}

#[test]
fn transparency_vectors_assurance_levels() {
    let f = fixture();
    let policies = [
        (
            "main",
            Policy::from_json(f["policy"].to_string().as_bytes()).unwrap(),
        ),
        (
            "shared_operator",
            Policy::from_json(f["policy_shared_operator"].to_string().as_bytes()).unwrap(),
        ),
    ];
    let proof = FixedProof {
        first: 40,
        second: 64,
        proof: path_of(&f["proof_40_to_64"]),
    };
    for case in f["assessments"].as_array().unwrap() {
        let name = s(&case["name"]);
        let policy = &policies
            .iter()
            .find(|(p, _)| *p == s(&case["policy"]))
            .unwrap()
            .1;
        let bundle =
            InclusionBundle::from_json(f["bundles"][s(&case["bundle"])].to_string().as_bytes())
                .unwrap();
        let mut known = KnownCheckpoints::new(policy.origin());
        if let Some(note) = case["known"].as_str() {
            let verified = policy
                .verify_checkpoint(&Note::parse(s(&f["notes"][note]).as_bytes()).unwrap())
                .unwrap();
            known.record(&verified).unwrap();
        }
        let proofs: &dyn engine_core::transparency::ConsistencySource =
            if case["proofs"].as_bool() == Some(true) {
                &proof
            } else {
                &NoProofs
            };
        let leaf = bundle.leaf.clone();
        let got = assess(
            policy,
            &bundle,
            Some(&leaf),
            &known,
            proofs,
            n(&case["now"]),
        );
        assert_eq!(got.rejection, None, "{name}");
        assert_eq!(got.level.to_string(), s(&case["level"]), "{name}");
        let limits: Vec<&str> = got.limits.iter().map(limit_label).collect();
        let want: Vec<&str> = case["limits"].as_array().unwrap().iter().map(s).collect();
        assert_eq!(limits, want, "{name}");
    }
}

#[test]
fn transparency_vectors_rejected_notes() {
    let f = fixture();
    let policy = Policy::from_json(f["policy"].to_string().as_bytes()).unwrap();
    let cases = f["rejected_notes"].as_array().unwrap();
    assert!(cases.len() >= 12);
    let mut reasons = std::collections::BTreeSet::new();
    for case in cases {
        let name = s(&case["name"]);
        let error = Note::parse(s(&case["note"]).as_bytes())
            .and_then(|note| policy.verify_checkpoint(&note))
            .expect_err(name);
        assert_eq!(reason(&error), s(&case["reason"]), "{name}: {error:?}");
        reasons.insert(s(&case["reason"]));
    }
    // Every rule has at least one case of its own.
    assert_eq!(reasons.len(), 7, "{reasons:?}");
}

#[test]
fn transparency_vectors_rejected_bundles() {
    let f = fixture();
    let policy = Policy::from_json(f["policy"].to_string().as_bytes()).unwrap();
    let known = KnownCheckpoints::new(policy.origin());
    let now = n(&f["now"]);
    for case in f["rejected_bundles"].as_array().unwrap() {
        let name = s(&case["name"]);
        let bundle = InclusionBundle::from_json(case["bundle"].to_string().as_bytes()).unwrap();
        let got = assess(&policy, &bundle, None, &known, &NoProofs, now);
        assert_eq!(got.level, Level::L0, "{name}");
        let rejection = got.rejection.expect(name);
        assert_eq!(
            reason(&rejection),
            s(&case["reason"]),
            "{name}: {rejection:?}"
        );
    }
}
