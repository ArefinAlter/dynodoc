//! Unit tests of the horizon record, its roots and its invariants. Every invariant
//! H1 to H10 has at least one negative test that fails for that rule alone.

use super::*;
use ring::signature::{Ed25519KeyPair, KeyPair};

fn a(n: u8) -> Address {
    Address::try_from(format!("{n:02x}").repeat(32)).unwrap()
}

fn doc() -> uuid::Uuid {
    uuid::Uuid::from_u128(0xd0c)
}

fn state(n: u8) -> StateKey {
    StateKey {
        document: doc(),
        state_root: a(0xa0 + n),
    }
}

fn pair(n: u8) -> Ed25519KeyPair {
    Ed25519KeyPair::from_seed_unchecked(&[n; 32]).unwrap()
}

fn policy_ref() -> PolicyRef {
    PolicyRef {
        id: "team-default".into(),
        hash: a(0x70),
    }
}

/// r0 <- r1 <- r2 <- r3 with `main` at r3. The first horizon released r0 and r1
/// (and their states); r2 and r3 stay. Everything below is consistent.
struct World {
    retained: Vec<Retained>,
    record: HorizonRecord,
    skeleton: Vec<SkeletonEntry>,
    frontier: Vec<FrontierEdge>,
    released: Vec<StateKey>,
    heads: Vec<HeadRef>,
    present: BTreeSet<StateKey>,
}

fn world() -> World {
    let skeleton = vec![
        SkeletonEntry {
            address: a(1),
            parents: vec![],
            height: 0,
        },
        SkeletonEntry {
            address: a(2),
            parents: vec![a(1)],
            height: 1,
        },
    ];
    let frontier = vec![FrontierEdge {
        child: a(3),
        parent: a(2),
        parent_height: 1,
    }];
    let released = vec![state(1), state(2)];
    let heads = vec![HeadRef {
        branch: "main".into(),
        commit: a(4),
        generation: 3,
    }];
    let retained = vec![
        Retained {
            address: a(3),
            parents: vec![a(2)],
            height: 2,
            states: vec![state(3)],
        },
        Retained {
            address: a(4),
            parents: vec![a(3)],
            height: 3,
            states: vec![state(4)],
        },
    ];
    let record = HorizonRecord {
        version: 1,
        kind: RECORD_KIND.into(),
        project: uuid::Uuid::from_u128(7),
        sequence: 1,
        previous: None,
        created_at_ms: 1_791_000_000_000,
        tier: Tier::History,
        policy: policy_ref(),
        heads_digest: heads_digest(&heads).unwrap(),
        counts: Counts {
            commits_released: 2,
            commit_bytes_released: 600,
            states_released: 2,
            objects_released: 3,
            object_bytes_released: 300,
        },
        roots: Roots {
            skeleton: skeleton_root(&skeleton).unwrap(),
            frontier: frontier_root(&frontier).unwrap(),
            released_states: released_states_root(&released).unwrap(),
        },
        archive: None,
        holds: vec![],
        evidence: vec!["recovery_sources".into()],
    };
    World {
        present: [state(3), state(4)].into_iter().collect(),
        retained,
        record,
        skeleton,
        frontier,
        released,
        heads,
    }
}

impl World {
    fn check(&self) -> Result<(), Error> {
        let rows = [HorizonRows {
            record: &self.record,
            skeleton: &self.skeleton,
            frontier: &self.frontier,
            released_states: &self.released,
        }];
        check_state(&self.retained, &rows, &self.heads, &self.present)
    }
}

fn is(result: Result<(), Error>, invariant: Invariant) -> bool {
    matches!(result, Err(Error::Violated(found, _)) if found == invariant)
}

fn envelope(record: &HorizonRecord, signers: &[(Role, u8)]) -> Envelope {
    let message = signing_input(&record.address().unwrap());
    let mut signatures: Vec<Signature> = signers
        .iter()
        .map(|(role, n)| {
            let key = pair(*n);
            Signature {
                role: *role,
                key: hex(key.public_key().as_ref()),
                signature: hex(key.sign(&message).as_ref()),
            }
        })
        .collect();
    signatures.sort_by(|x, y| x.key.cmp(&y.key));
    Envelope {
        horizon: record.clone(),
        signatures,
    }
}

#[test]
fn a_consistent_world_passes_every_state_check() {
    world().check().unwrap();
    world().record.validate().unwrap();
}

// ---- H1 ----------------------------------------------------------------------

#[test]
fn h1_sequence_and_previous_must_agree() {
    let mut record = world().record;
    record.sequence = 0;
    assert!(is(record.validate(), Invariant::H1));
    let mut record = world().record;
    record.sequence = MAX_JSON_INT + 1;
    assert!(is(record.validate(), Invariant::H1));
    let mut record = world().record;
    record.previous = Some(a(9));
    assert!(
        is(record.validate(), Invariant::H1),
        "sequence 1 has no previous"
    );
    let mut record = world().record;
    record.sequence = 2;
    assert!(
        is(record.validate(), Invariant::H1),
        "later sequences need previous"
    );
}

#[test]
fn h1_chain_must_be_contiguous_linked_and_in_one_project() {
    let first = world().record;
    let mut second = first.clone();
    second.sequence = 2;
    second.previous = Some(first.address().unwrap());
    let signers = [(Role::Owner, 1), (Role::Authority, 2)];
    let chain = [envelope(&first, &signers), envelope(&second, &signers)];
    let need = |_: &PolicyRef| Some(2);
    validate_chain(&chain, Scope::Shared, need).unwrap();

    // Starts at 2, skips 2, repeats 1: each breaks contiguity.
    assert!(is(
        validate_chain(&chain[1..], Scope::Shared, need),
        Invariant::H1
    ));
    let mut gap = second.clone();
    gap.sequence = 3;
    let gapped = [envelope(&first, &signers), envelope(&gap, &signers)];
    assert!(is(
        validate_chain(&gapped, Scope::Shared, need),
        Invariant::H1
    ));
    let repeated = [chain[0].clone(), chain[0].clone()];
    assert!(is(
        validate_chain(&repeated, Scope::Shared, need),
        Invariant::H1
    ));
    // previous names the wrong record.
    let mut forked = second.clone();
    forked.previous = Some(a(0x99));
    let forked = [envelope(&first, &signers), envelope(&forked, &signers)];
    assert!(is(
        validate_chain(&forked, Scope::Shared, need),
        Invariant::H1
    ));
    // Another project in the middle of a chain.
    let mut foreign = second;
    foreign.project = uuid::Uuid::from_u128(8);
    let foreign = [envelope(&first, &signers), envelope(&foreign, &signers)];
    assert!(is(
        validate_chain(&foreign, Scope::Shared, need),
        Invariant::H1
    ));
}

// ---- H2 ----------------------------------------------------------------------

#[test]
fn h2_signatures_must_verify_be_distinct_legal_and_enough() {
    let record = world().record;
    let good = envelope(&record, &[(Role::Owner, 1), (Role::Authority, 2)]);
    assert_eq!(
        good.verify(Scope::Shared, 2).unwrap(),
        record.address().unwrap()
    );
    assert!(
        is(good.verify(Scope::Shared, 3).map(|_| ()), Invariant::H2),
        "too few"
    );
    assert!(
        is(good.verify(Scope::Shared, 0).map(|_| ()), Invariant::H2),
        "zero required"
    );
    assert!(
        is(good.verify(Scope::Local, 2).map(|_| ()), Invariant::H2),
        "authority is not local"
    );
    let device = envelope(&record, &[(Role::Device, 1)]);
    device.verify(Scope::Local, 1).unwrap();
    assert!(is(
        device.verify(Scope::Shared, 1).map(|_| ()),
        Invariant::H2
    ));

    // A flipped signature bit, and a signature of another record.
    let mut bad = good.clone();
    bad.signatures[0].signature = format!("{:02x}{}", 0xffu8, &bad.signatures[0].signature[2..]);
    assert!(is(bad.verify(Scope::Shared, 2).map(|_| ()), Invariant::H2));
    let mut other = record.clone();
    other.created_at_ms += 1;
    let mut swapped = good.clone();
    swapped.signatures = envelope(&other, &[(Role::Owner, 1), (Role::Authority, 2)]).signatures;
    assert!(is(
        swapped.verify(Scope::Shared, 2).map(|_| ()),
        Invariant::H2
    ));
    // The same key twice, and the wrong order, are refused before counting.
    let mut twice = good.clone();
    twice.signatures = vec![good.signatures[0].clone(), good.signatures[0].clone()];
    assert!(is(
        twice.verify(Scope::Shared, 2).map(|_| ()),
        Invariant::H2
    ));
    let mut reversed = good.clone();
    reversed.signatures.reverse();
    assert!(is(
        reversed.verify(Scope::Shared, 2).map(|_| ()),
        Invariant::H2
    ));
    // Malformed key or signature text.
    let mut garbled = good.clone();
    garbled.signatures[0].key = "zz".into();
    assert!(is(
        garbled.verify(Scope::Shared, 1).map(|_| ()),
        Invariant::H2
    ));
    // More signatures than an envelope may carry.
    let mut many = good.clone();
    many.signatures = (0..9).map(|_| good.signatures[0].clone()).collect();
    assert!(matches!(
        many.verify(Scope::Shared, 1),
        Err(Error::Limit(_))
    ));
}

#[test]
fn h2_a_commit_signature_cannot_be_replayed_as_a_horizon_signature() {
    let record = world().record;
    let address = record.address().unwrap();
    let key = pair(1);
    // Same key, same 32 address bytes, but the commit signature domain.
    let mut message = b"dynodoc.project-commit-signature\0v1\0".to_vec();
    message.extend(raw(&address));
    let replay = Envelope {
        horizon: record.clone(),
        signatures: vec![Signature {
            role: Role::Owner,
            key: hex(key.public_key().as_ref()),
            signature: hex(key.sign(&message).as_ref()),
        }],
    };
    assert!(is(
        replay.verify(Scope::Shared, 1).map(|_| ()),
        Invariant::H2
    ));
    // And a signature over the bare address bytes, with no domain at all.
    let bare = Envelope {
        horizon: record,
        signatures: vec![Signature {
            role: Role::Owner,
            key: hex(key.public_key().as_ref()),
            signature: hex(key.sign(&raw(&address)).as_ref()),
        }],
    };
    assert!(is(bare.verify(Scope::Shared, 1).map(|_| ()), Invariant::H2));
}

#[test]
fn h2_a_chain_needs_its_policy_to_be_known() {
    let record = world().record;
    let chain = [envelope(&record, &[(Role::Owner, 1)])];
    assert!(is(
        validate_chain(&chain, Scope::Shared, |_| None),
        Invariant::H2
    ));
    validate_chain(&chain, Scope::Shared, |_| Some(1)).unwrap();
}

// ---- H3 and H4 ---------------------------------------------------------------

#[test]
fn h3_a_retained_commit_cannot_have_an_unaccounted_parent() {
    let mut w = world();
    w.retained[0].parents = vec![a(0x55)];
    assert!(is(w.check(), Invariant::H3));
    // A second parent that is nowhere is just as bad.
    let mut w = world();
    w.retained[1].parents.push(a(0x56));
    assert!(is(w.check(), Invariant::H3));
}

#[test]
fn h4_skeleton_parents_and_heights_must_close() {
    let mut w = world();
    w.skeleton[1].parents = vec![a(0x77)];
    w.record.roots.skeleton = skeleton_root(&w.skeleton).unwrap();
    assert!(is(w.check(), Invariant::H4), "parent nowhere");
    let mut w = world();
    w.skeleton[1].height = 5;
    w.record.roots.skeleton = skeleton_root(&w.skeleton).unwrap();
    w.frontier[0].parent_height = 5;
    w.record.roots.frontier = frontier_root(&w.frontier).unwrap();
    assert!(is(w.check(), Invariant::H4), "height is not parent + 1");
    let mut w = world();
    w.skeleton[0].height = 1;
    w.record.roots.skeleton = skeleton_root(&w.skeleton).unwrap();
    assert!(is(w.check(), Invariant::H4), "a root has height 0");
    let mut w = world();
    w.skeleton[1].parents = vec![a(1), a(1), a(1)];
    w.record.roots.skeleton = skeleton_root(&w.skeleton).unwrap_or_else(|_| a(0));
    assert!(w.check().is_err(), "three parents");
}

#[test]
fn h4_frontier_edges_must_match_a_body_and_a_skeleton_entry() {
    let mut w = world();
    w.frontier[0].parent_height = 9;
    w.record.roots.frontier = frontier_root(&w.frontier).unwrap();
    assert!(is(w.check(), Invariant::H4), "wrong parent height");
    let mut w = world();
    w.frontier[0].parent = a(1);
    w.record.roots.frontier = frontier_root(&w.frontier).unwrap();
    assert!(is(w.check(), Invariant::H4), "child 3 has no parent 1");
    let mut w = world();
    w.frontier[0].child = a(0x44);
    w.record.roots.frontier = frontier_root(&w.frontier).unwrap();
    assert!(is(w.check(), Invariant::H4), "child is not retained");
}

// ---- H5 ----------------------------------------------------------------------

#[test]
fn h5_rows_must_reproduce_the_committed_roots_and_counts() {
    let mut w = world();
    w.skeleton[0].height = 0;
    w.skeleton.push(SkeletonEntry {
        address: a(0x10),
        parents: vec![],
        height: 0,
    });
    assert!(is(w.check(), Invariant::H5), "extra skeleton row, old root");
    let mut w = world();
    w.frontier.clear();
    assert!(is(w.check(), Invariant::H5), "frontier row removed");
    let mut w = world();
    w.released.pop();
    assert!(is(w.check(), Invariant::H5), "released state removed");
    let mut w = world();
    w.record.counts.commits_released = 3;
    assert!(
        is(w.check(), Invariant::H5),
        "row count differs from commits_released"
    );
    let mut w = world();
    w.record.counts.states_released = 3;
    assert!(
        is(w.check(), Invariant::H5),
        "row count differs from states_released"
    );
    // Dropped topology: the commits were released but no skeleton rows exist.
    let mut w = world();
    w.skeleton.clear();
    w.record.roots.skeleton = skeleton_root(&[]).unwrap();
    w.record.evidence = vec!["topology:drop".into()];
    w.retained[0].parents.clear();
    w.frontier.clear();
    w.record.roots.frontier = frontier_root(&[]).unwrap();
    w.check().unwrap();
}

// ---- H6 ----------------------------------------------------------------------

#[test]
fn h6_a_current_head_is_never_released() {
    let mut w = world();
    w.heads[0].commit = a(2);
    assert!(is(w.check(), Invariant::H6), "head is a skeleton commit");
    let mut w = world();
    w.heads[0].commit = a(0x33);
    assert!(is(w.check(), Invariant::H6), "head is unknown");
    let mut w = world();
    // The head's own state is listed as released.
    w.released.push(state(4));
    w.released.sort();
    w.present.remove(&state(4));
    w.record.counts.states_released = 3;
    w.record.roots.released_states = released_states_root(&w.released).unwrap();
    assert!(is(w.check(), Invariant::H6));
}

// ---- H7 ----------------------------------------------------------------------

#[test]
fn h7_retained_and_released_are_disjoint_and_released_once() {
    let mut w = world();
    w.retained.push(Retained {
        address: a(1),
        parents: vec![],
        height: 0,
        states: vec![],
    });
    assert!(is(w.check(), Invariant::H7), "released commit also stored");
    let mut w = world();
    w.retained.push(w.retained[0].clone());
    assert!(is(w.check(), Invariant::H7), "stored twice");
    // The same address released by two horizons.
    let w = world();
    let mut second = w.record.clone();
    second.sequence = 2;
    second.previous = Some(w.record.address().unwrap());
    let rows = [
        HorizonRows {
            record: &w.record,
            skeleton: &w.skeleton,
            frontier: &w.frontier,
            released_states: &w.released,
        },
        HorizonRows {
            record: &second,
            skeleton: &w.skeleton,
            frontier: &w.frontier,
            released_states: &w.released,
        },
    ];
    assert!(is(
        check_state(&w.retained, &rows, &w.heads, &w.present),
        Invariant::H7
    ));
}

// ---- H8 ----------------------------------------------------------------------

#[test]
fn h8_released_states_are_absent_and_present_ones_are_not_silently_missing() {
    let mut w = world();
    w.present.insert(state(1));
    assert!(
        is(w.check(), Invariant::H8),
        "released state still has objects"
    );
    let mut w = world();
    w.present.remove(&state(3));
    assert!(
        is(w.check(), Invariant::H8),
        "state of a retained commit is gone and unlisted"
    );
}

// ---- H9 ----------------------------------------------------------------------

#[test]
fn h9_recorded_counters_must_equal_a_recount() {
    let counters = Usage {
        commits: 2,
        objects: 5,
        bytes: 900,
    };
    check_counters(counters, counters).unwrap();
    for off in [
        Usage {
            commits: 3,
            ..counters
        },
        Usage {
            objects: 4,
            ..counters
        },
        Usage {
            bytes: 901,
            ..counters
        },
    ] {
        assert!(is(check_counters(off, counters), Invariant::H9));
    }
}

// ---- H10 and record form -----------------------------------------------------

#[test]
fn h10_a_record_carries_no_path_message_key_or_name() {
    let record = world().record;
    let mut value = serde_json::to_value(&record).unwrap();
    for field in ["path", "message", "author_key", "document_name", "files"] {
        value[field] = serde_json::json!("x");
        let bytes = serde_json::to_vec(&value).unwrap();
        assert!(
            is(
                HorizonRecord::from_bytes(&bytes).map(|_| ()),
                Invariant::H10
            ),
            "{field}"
        );
        value.as_object_mut().unwrap().remove(field);
    }
    // Nested objects are closed too.
    let mut value = serde_json::to_value(&record).unwrap();
    value["policy"]["path"] = serde_json::json!("x");
    let bytes = serde_json::to_vec(&value).unwrap();
    assert!(is(
        HorizonRecord::from_bytes(&bytes).map(|_| ()),
        Invariant::H10
    ));

    // Free text is only a hold reason: bounded, printable.
    let hold = |reason: &str| {
        let mut record = world().record;
        record.holds = vec![Hold {
            commit: a(1),
            reason: reason.into(),
        }];
        record.evidence = vec!["holds".into()];
        record.validate()
    };
    hold("legal-hold:LH-17").unwrap();
    hold(&"é".repeat(100)).unwrap();
    assert!(is(hold(""), Invariant::H10));
    assert!(is(hold(&"x".repeat(201)), Invariant::H10));
    assert!(is(hold("two\nlines"), Invariant::H10));
    assert!(is(hold("nul\0"), Invariant::H10));
    assert!(is(hold(&"é".repeat(101)), Invariant::H10), "202 bytes");
    // Evidence is a closed vocabulary; the policy id a plain identifier.
    let mut record = world().record;
    record.evidence = vec!["secret:path/to/file".into()];
    assert!(is(record.validate(), Invariant::H10));
    let mut record = world().record;
    record.policy.id = "Team Default".into();
    assert!(is(record.validate(), Invariant::H10));
}

#[test]
fn record_form_is_strict_canonical_and_bounded() {
    let record = world().record;
    let bytes = record.bytes().unwrap();
    assert_eq!(HorizonRecord::from_bytes(&bytes).unwrap(), record);
    // Whitespace, key order and null/absent differences are not accepted.
    let pretty = serde_json::to_vec_pretty(&record).unwrap();
    assert!(HorizonRecord::from_bytes(&pretty).is_err());
    let mut value: serde_json::Value = serde_json::from_slice(&bytes).unwrap();
    value.as_object_mut().unwrap().remove("previous");
    assert!(HorizonRecord::from_bytes(&serde_json::to_vec(&value).unwrap()).is_err());
    value["previous"] = serde_json::Value::Null;
    value["archive"] = serde_json::Value::Null;
    assert!(HorizonRecord::from_bytes(&canonical_bytes(&value).unwrap()).is_err());
    assert!(HorizonRecord::from_bytes(b"{").is_err());
    assert!(matches!(
        HorizonRecord::from_bytes(&vec![b' '; MAX_RECORD_BYTES + 1]),
        Err(Error::Limit(_))
    ));
    // Uppercase hex is not an address.
    let upper = String::from_utf8(bytes.clone())
        .unwrap()
        .replace(a(0x70).as_str(), &"7A".repeat(32));
    assert!(HorizonRecord::from_bytes(upper.as_bytes()).is_err());

    // Numbers stay within what every JSON implementation represents.
    let mut big = record.clone();
    big.created_at_ms = MAX_JSON_INT + 1;
    assert!(matches!(big.validate(), Err(Error::Format(_))));
    big.created_at_ms = MAX_JSON_INT;
    big.validate().unwrap();
    let mut big = record.clone();
    big.counts.commit_bytes_released = MAX_JSON_INT + 1;
    assert!(matches!(big.validate(), Err(Error::Format(_))));

    // Version, kind and project.
    for change in [
        |r: &mut HorizonRecord| r.version = 2,
        |r: &mut HorizonRecord| r.kind = "dynodoc.other".into(),
        |r: &mut HorizonRecord| r.project = uuid::Uuid::nil(),
    ] {
        let mut changed = record.clone();
        change(&mut changed);
        assert!(matches!(changed.validate(), Err(Error::Format(_))));
    }
    // The address commits to every field, and differs from the policy-hash domain.
    let mut changed = record.clone();
    changed.counts.states_released += 1;
    assert_ne!(changed.address().unwrap(), record.address().unwrap());
}

#[test]
fn record_consistency_rules_catch_impossible_counts_and_archives() {
    let mut r = world().record;
    r.tier = Tier::Content;
    assert!(
        matches!(r.validate(), Err(Error::Format(_))),
        "content releases no commit"
    );
    let mut r = world().record;
    r.counts.commits_released = 0;
    assert!(
        matches!(r.validate(), Err(Error::Format(_))),
        "bytes without commits"
    );
    let mut r = world().record;
    r.counts.objects_released = 0;
    assert!(
        matches!(r.validate(), Err(Error::Format(_))),
        "object bytes without objects"
    );
    let mut r = world().record;
    r.counts.states_released = 0;
    assert!(
        matches!(r.validate(), Err(Error::Format(_))),
        "objects without states"
    );
    let mut r = world().record;
    r.archive = Some(Archive {
        sha256: a(5),
        bytes: 10,
        commits: 2,
        objects: 3,
    });
    r.validate().unwrap();
    r.archive.as_mut().unwrap().commits = 1;
    assert!(
        matches!(r.validate(), Err(Error::Format(_))),
        "archive commits differ"
    );
    r.archive.as_mut().unwrap().commits = 2;
    r.archive.as_mut().unwrap().objects = 4;
    assert!(
        matches!(r.validate(), Err(Error::Format(_))),
        "archive objects differ"
    );
    let mut r = world().record;
    r.evidence = vec!["recovery_sources".into(), "holds".into()];
    assert!(
        matches!(r.validate(), Err(Error::Format(_))),
        "evidence must be sorted"
    );
    let mut r = world().record;
    r.holds = vec![
        Hold {
            commit: a(2),
            reason: "b".into(),
        },
        Hold {
            commit: a(1),
            reason: "a".into(),
        },
    ];
    assert!(
        matches!(r.validate(), Err(Error::Format(_))),
        "holds must be sorted"
    );
    let mut r = world().record;
    r.holds = (0..=MAX_HOLDS as u32)
        .map(|i| Hold {
            commit: a(1),
            reason: format!("hold {i:04}"),
        })
        .collect();
    assert!(matches!(r.validate(), Err(Error::Limit(_))));
}

#[test]
fn a_record_of_maximum_holds_fits_the_size_limit() {
    let mut r = world().record;
    r.holds = (0..MAX_HOLDS)
        .map(|i| Hold {
            commit: a(1),
            reason: format!("{i:04}{}", "é".repeat(98)),
        })
        .collect();
    r.holds.sort();
    r.evidence = vec!["holds".into()];
    assert!(r.bytes().unwrap().len() <= MAX_RECORD_BYTES);
}

#[test]
fn heads_digest_is_order_free_and_binds_name_commit_and_generation() {
    let heads = world().heads;
    let one = heads_digest(&heads).unwrap();
    let mut two = heads.clone();
    two.push(HeadRef {
        branch: "dev".into(),
        commit: a(9),
        generation: 1,
    });
    let forward = heads_digest(&two).unwrap();
    two.reverse();
    assert_eq!(heads_digest(&two).unwrap(), forward);
    assert_ne!(forward, one);
    for change in [
        |h: &mut HeadRef| h.branch = "main2".into(),
        |h: &mut HeadRef| h.commit = a(8),
        |h: &mut HeadRef| h.generation += 1,
    ] {
        let mut changed = heads.clone();
        change(&mut changed[0]);
        assert_ne!(heads_digest(&changed).unwrap(), one);
    }
    // An empty set is a valid, distinct digest; duplicates and bad names are not.
    assert_ne!(heads_digest(&[]).unwrap(), one);
    let duplicate = vec![heads[0].clone(), heads[0].clone()];
    assert!(heads_digest(&duplicate).is_err());
    let mut bad = heads.clone();
    bad[0].branch = "a b".into();
    assert!(heads_digest(&bad).is_err());
    let mut bad = heads;
    bad[0].generation = 0;
    assert!(heads_digest(&bad).is_err());
}

#[test]
fn merkle_roots_follow_rfc_6962_shape() {
    // The eight-leaf and three-leaf trees recomputed by hand from the definitions.
    let leaves = |count: u8| -> Vec<Address> { (1..=count).map(a).collect() };
    let lh = |address: &Address| leaf_hash(&[RETAINED_LEAF_DOMAIN, &raw(address)]);
    let node = |l: [u8; 32], r: [u8; 32]| sha256(&[&[0x01], &l, &r]);
    let three = leaves(3);
    let expected = node(node(lh(&three[0]), lh(&three[1])), lh(&three[2]));
    assert_eq!(retained_root(&three).unwrap(), address_of(&expected));
    let five = leaves(5);
    let first_four = node(
        node(lh(&five[0]), lh(&five[1])),
        node(lh(&five[2]), lh(&five[3])),
    );
    assert_eq!(
        retained_root(&five).unwrap(),
        address_of(&node(first_four, lh(&five[4])))
    );
    // One leaf is its own leaf hash; none is the hash of the empty string.
    assert_eq!(
        retained_root(&leaves(1)).unwrap(),
        address_of(&lh(&leaves(1)[0]))
    );
    assert_eq!(retained_root(&[]).unwrap(), address_of(&sha256(&[])));
    // Leaf kinds are domain separated: the same bytes under another kind differ.
    let edge = FrontierEdge {
        child: a(1),
        parent: a(2),
        parent_height: 0,
    };
    let skeleton = SkeletonEntry {
        address: a(1),
        parents: vec![a(2)],
        height: 0,
    };
    assert_ne!(
        frontier_root(&[edge]).unwrap(),
        skeleton_root(&[skeleton]).unwrap()
    );
}

#[test]
fn skeleton_parent_order_and_count_are_part_of_the_leaf() {
    let make = |parents: Vec<Address>| SkeletonEntry {
        address: a(9),
        parents,
        height: 4,
    };
    let ab = skeleton_root(&[make(vec![a(1), a(2)])]).unwrap();
    let ba = skeleton_root(&[make(vec![a(2), a(1)])]).unwrap();
    let one = skeleton_root(&[make(vec![a(1)])]).unwrap();
    assert_ne!(ab, ba, "the first parent is significant");
    assert_ne!(ab, one);
    assert!(skeleton_root(&[make(vec![a(1), a(2), a(3)])]).is_err());
    let mut entry = make(vec![]);
    entry.height = MAX_JSON_INT + 1;
    assert!(skeleton_root(&[entry]).is_err());
}
