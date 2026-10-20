//! Reproduces `fixtures/horizon-v1.json` byte for byte. The fixture is written by
//! `fixtures/gen_horizon_vectors.py`, an independent standard-library Python
//! implementation of docs/HORIZON-RECORD-V1.md; never regenerate it to make one of
//! these checks pass.
use engine_core::repository::{
    convergence::{Resolution, STRATEGY},
    horizon::{
        check_state, frontier_root, heads_digest, plan,
        plan::{HistoryView, ObjectGraph, PlanOptions},
        policy::{LoadOptions, Policy},
        released_states_root, retained_root, signing_input, skeleton_root, Envelope, Error,
        FrontierEdge, HeadRef, Hold, HorizonRecord, HorizonRows, Invariant, Retained, Scope,
        SkeletonEntry, StateKey, SIGNATURE_DOMAIN,
    },
    recovery::{Kind, Source},
    Commit, FileEntry,
};
use engine_core::shared_checkpoint::Address;
use engine_shared::DocumentId;
use serde_json::Value;
use std::collections::{BTreeMap, BTreeSet};
use uuid::Uuid;

fn fixture() -> Value {
    serde_json::from_str(include_str!("fixtures/horizon-v1.json")).unwrap()
}

fn s(v: &Value) -> &str {
    v.as_str().unwrap()
}

fn n(v: &Value) -> u64 {
    v.as_u64().unwrap()
}

fn addr(v: &Value) -> Address {
    Address::try_from(s(v).to_owned()).unwrap()
}

fn addrs(v: &Value) -> Vec<Address> {
    v.as_array().unwrap().iter().map(addr).collect()
}

fn state_key(v: &Value) -> StateKey {
    StateKey {
        document: Uuid::parse_str(s(&v["document"])).unwrap(),
        state_root: addr(&v["state_root"]),
    }
}

fn skeleton_rows(v: &Value) -> Vec<SkeletonEntry> {
    v.as_array()
        .unwrap()
        .iter()
        .map(|e| SkeletonEntry {
            address: addr(&e["address"]),
            parents: addrs(&e["parents"]),
            height: n(&e["height"]),
        })
        .collect()
}

fn frontier_rows(v: &Value) -> Vec<FrontierEdge> {
    v.as_array()
        .unwrap()
        .iter()
        .map(|e| FrontierEdge {
            child: addr(&e["child"]),
            parent: addr(&e["parent"]),
            parent_height: n(&e["parent_height"]),
        })
        .collect()
}

fn head_rows(v: &Value) -> Vec<HeadRef> {
    v.as_array()
        .unwrap()
        .iter()
        .map(|h| HeadRef {
            branch: s(&h["branch"]).to_owned(),
            commit: addr(&h["commit"]),
            generation: h["generation"].as_i64().unwrap(),
        })
        .collect()
}

fn nul(text: &str) -> Vec<u8> {
    text.replace("\\0", "\0").into_bytes()
}

#[test]
fn signature_domain_and_domain_strings_match_the_independent_generator() {
    let f = fixture();
    assert_eq!(SIGNATURE_DOMAIN, nul(s(&f["domains"]["signature"])));
    assert_eq!(
        SIGNATURE_DOMAIN,
        b"dynodoc.history-horizon-signature\0v1\0".as_slice()
    );
    // The commit signature domain differs from it, so a commit signature can never
    // be replayed as a horizon signature.
    assert_ne!(
        nul(s(&f["commit_signature_domain"])),
        SIGNATURE_DOMAIN.to_vec()
    );
    assert!(f["rfc8032_vectors_checked"].as_u64().unwrap() >= 3);
}

#[test]
fn merkle_roots_over_every_leaf_kind_and_size_match() {
    let f = fixture();
    let cases = f["merkle"].as_array().unwrap();
    assert_eq!(cases.len(), 4 * 9);
    for case in cases {
        let items = case["items"].as_array().unwrap();
        let root = match s(&case["kind"]) {
            "skeleton" => skeleton_root(&skeleton_rows(&case["items"])).unwrap(),
            "frontier" => frontier_root(&frontier_rows(&case["items"])).unwrap(),
            "states" => {
                let states: Vec<_> = items.iter().map(state_key).collect();
                released_states_root(&states).unwrap()
            }
            "retained" => retained_root(&addrs(&case["items"])).unwrap(),
            other => panic!("unknown kind {other}"),
        };
        assert_eq!(
            root.as_str(),
            s(&case["root"]),
            "{} with {} items",
            s(&case["kind"]),
            items.len()
        );
    }
    // The empty tree hashes the empty string (RFC 6962).
    assert_eq!(
        skeleton_root(&[]).unwrap().as_str(),
        s(&f["empty_root"]),
        "empty root"
    );
}

#[test]
fn leaf_order_is_by_key_and_duplicates_are_refused() {
    let f = fixture();
    let case = f["merkle"]
        .as_array()
        .unwrap()
        .iter()
        .find(|c| s(&c["kind"]) == "skeleton" && c["items"].as_array().unwrap().len() == 7)
        .unwrap();
    let mut rows = skeleton_rows(&case["items"]);
    let root = skeleton_root(&rows).unwrap();
    rows.reverse();
    assert_eq!(skeleton_root(&rows).unwrap(), root);
    rows.push(rows[0].clone());
    assert!(skeleton_root(&rows).is_err());
}

#[test]
fn heads_digests_match_for_every_ordering_and_edge_value() {
    let f = fixture();
    for case in f["heads"].as_array().unwrap() {
        let heads = head_rows(&case["heads"]);
        assert_eq!(heads_digest(&heads).unwrap().as_str(), s(&case["digest"]));
    }
}

#[test]
fn graph_rows_reproduce_the_roots_and_satisfy_every_state_invariant() {
    let f = fixture();
    let g = &f["graph"];
    let skeleton = skeleton_rows(&g["skeleton"]);
    let frontier = frontier_rows(&g["frontier"]);
    let released: Vec<StateKey> = g["released_states"]
        .as_array()
        .unwrap()
        .iter()
        .map(state_key)
        .collect();
    let roots = &g["roots"];
    assert_eq!(
        skeleton_root(&skeleton).unwrap().as_str(),
        s(&roots["skeleton"])
    );
    assert_eq!(
        frontier_root(&frontier).unwrap().as_str(),
        s(&roots["frontier"])
    );
    assert_eq!(
        released_states_root(&released).unwrap().as_str(),
        s(&roots["released_states"])
    );
    assert_eq!(
        retained_root(&addrs(&g["retained"])).unwrap().as_str(),
        s(&roots["retained"])
    );
    let heads = head_rows(&g["heads"]);
    assert_eq!(
        heads_digest(&heads).unwrap().as_str(),
        s(&g["heads_digest"])
    );

    // The record of the first record vector commits to exactly these roots.
    let record =
        HorizonRecord::from_bytes(s(&f["records"]["records"][0]["canonical"]).as_bytes()).unwrap();
    assert_eq!(record.roots.skeleton.as_str(), s(&roots["skeleton"]));
    assert_eq!(record.roots.frontier.as_str(), s(&roots["frontier"]));
    assert_eq!(
        record.roots.released_states.as_str(),
        s(&roots["released_states"])
    );
    assert_eq!(record.heads_digest.as_str(), s(&g["heads_digest"]));
    assert_eq!(record.counts.commits_released, skeleton.len() as u64);
    assert_eq!(record.counts.states_released, released.len() as u64);

    let released_addresses: BTreeSet<Address> = addrs(&g["released"]).into_iter().collect();
    let retained: Vec<Retained> = g["commits"]
        .as_array()
        .unwrap()
        .iter()
        .filter(|c| !released_addresses.contains(&addr(&c["address"])))
        .map(|c| Retained {
            address: addr(&c["address"]),
            parents: addrs(&c["parents"]),
            height: n(&c["height"]),
            states: c["states"]
                .as_array()
                .unwrap()
                .iter()
                .map(state_key)
                .collect(),
        })
        .collect();
    let present: BTreeSet<StateKey> = retained.iter().flat_map(|c| c.states.clone()).collect();
    let rows = [HorizonRows {
        record: &record,
        skeleton: &skeleton,
        frontier: &frontier,
        released_states: &released,
    }];
    check_state(&retained, &rows, &heads, &present).unwrap();
}

#[test]
fn records_envelopes_and_the_chain_reproduce() {
    let f = fixture();
    let r = &f["records"];
    for entry in r["records"].as_array().unwrap() {
        let canonical = s(&entry["canonical"]);
        let record = HorizonRecord::from_bytes(canonical.as_bytes()).unwrap();
        assert_eq!(record.bytes().unwrap(), canonical.as_bytes());
        let address = record.address().unwrap();
        assert_eq!(address.as_str(), s(&entry["address"]));
        let input: String = signing_input(&address)
            .iter()
            .map(|b| format!("{b:02x}"))
            .collect();
        assert_eq!(input, s(&entry["signing_input"]));
        // Parsing the independent generator's own value finds the same record.
        assert_eq!(
            serde_json::to_value(&record).unwrap(),
            entry["record"],
            "record fields"
        );
    }
    // The fixture exercises non-ASCII, quote and backslash escaping in a hold reason.
    let second = HorizonRecord::from_bytes(s(&r["records"][1]["canonical"]).as_bytes()).unwrap();
    assert!(second.holds.iter().any(|h| h.reason.contains('🔒')));
    assert!(second.holds.iter().any(|h| h.reason.contains('\\')));

    for case in r["envelopes"].as_array().unwrap() {
        let bytes = s(&case["canonical"]).as_bytes();
        let scope = match s(&case["scope"]) {
            "local" => Scope::Local,
            _ => Scope::Shared,
        };
        let envelope = Envelope::from_bytes(bytes).unwrap();
        assert_eq!(envelope.bytes().unwrap(), bytes);
        let result = envelope.verify(scope, n(&case["required"]) as u32);
        match s(&case["expect"]) {
            "ok" => {
                result.unwrap_or_else(|e| panic!("{}: {e}", s(&case["name"])));
            }
            "H2" => assert!(
                matches!(result, Err(Error::Violated(Invariant::H2, _))),
                "{} gave {result:?}",
                s(&case["name"])
            ),
            other => panic!("unknown expectation {other}"),
        }
    }

    let chain: Vec<Envelope> = r["chain"]
        .as_array()
        .unwrap()
        .iter()
        .map(|e| Envelope::from_bytes(s(e).as_bytes()).unwrap())
        .collect();
    let hash = r["policy_example_hash"].as_str().unwrap();
    engine_core::repository::horizon::validate_chain(&chain, Scope::Shared, |policy| {
        (policy.hash.as_str() == hash).then_some(2)
    })
    .unwrap();
}

#[test]
fn policies_normalize_and_hash_like_the_generator_and_invalid_ones_report_exactly() {
    let f = fixture();
    for case in f["policies"]["valid"].as_array().unwrap() {
        let options = LoadOptions {
            min_signatures: n(&case["min_signatures"]) as u32,
        };
        let policy = Policy::load(s(&case["input"]).as_bytes(), options)
            .unwrap_or_else(|e| panic!("{}: {e}", s(&case["name"])));
        assert_eq!(
            String::from_utf8(policy.canonical_bytes()).unwrap(),
            s(&case["normalized"]),
            "{}",
            s(&case["name"])
        );
        assert_eq!(
            policy.hash().as_str(),
            s(&case["hash"]),
            "{}",
            s(&case["name"])
        );
        // The normalized form is itself a valid policy with the same hash.
        let again = Policy::load(&policy.canonical_bytes(), options).unwrap();
        assert_eq!(again.hash(), policy.hash());
    }
    for case in f["policies"]["invalid"].as_array().unwrap() {
        let options = LoadOptions {
            min_signatures: n(&case["min_signatures"]) as u32,
        };
        let name = s(&case["name"]);
        let errors = Policy::load(s(&case["input"]).as_bytes(), options).expect_err(name);
        let got: Vec<(String, String)> = errors
            .0
            .iter()
            .map(|i| (i.path.clone(), i.code.to_owned()))
            .collect();
        let mut want: Vec<(String, String)> = case["issues"]
            .as_array()
            .unwrap()
            .iter()
            .map(|i| (s(&i["path"]).to_owned(), s(&i["code"]).to_owned()))
            .collect();
        want.sort();
        assert_eq!(got, want, "{name}: {errors}");
    }
}

fn commit_from(entry: &Value, project: Uuid) -> (Address, Commit) {
    let files: BTreeMap<String, FileEntry> = entry["files"]
        .as_array()
        .unwrap()
        .iter()
        .enumerate()
        .map(|(i, file)| {
            (
                format!("f{i}"),
                FileEntry {
                    document: DocumentId(Uuid::parse_str(s(&file[0])).unwrap()),
                    state: addr(&file[1]),
                },
            )
        })
        .collect();
    let recovery = addrs(&entry["recovery"]);
    let commit = Commit {
        version: n(&entry["version"]) as u8,
        project,
        parents: addrs(&entry["parents"]),
        files,
        author_key: "00".repeat(32),
        authored_at_ms: entry["authored_at_ms"].as_i64().unwrap(),
        message: String::new(),
        recovery: recovery.first().map(|first| Source {
            kind: Kind::CherryPick,
            commit: first.clone(),
            mainline: recovery.get(1).cloned(),
        }),
        same_file_resolutions: entry["inputs"]
            .as_array()
            .unwrap()
            .iter()
            .map(|record| Resolution {
                path: "f0".into(),
                document: DocumentId(Uuid::parse_str(s(&record["document"])).unwrap()),
                base: addr(&record["base"]),
                left: addr(&record["left"]),
                right: addr(&record["right"]),
                merged: addr(&record["right"]),
                strategy: STRATEGY.into(),
            })
            .collect(),
    };
    (addr(&entry["address"]), commit)
}

fn view_from(v: &Value) -> HistoryView {
    let project = Uuid::parse_str(s(&v["project"])).unwrap();
    let mut view = HistoryView::new(project);
    for head in head_rows(&v["branches"]) {
        view.add_branch(head).unwrap();
    }
    for entry in v["commits"].as_array().unwrap() {
        let (address, commit) = commit_from(entry, project);
        view.add_commit(&address, &commit, n(&entry["bytes"]), None)
            .unwrap();
    }
    for entry in skeleton_rows(&v["skeleton"]) {
        view.add_prior_skeleton(entry);
    }
    for hold in v["holds"].as_array().unwrap() {
        view.add_hold(Hold {
            commit: addr(&hold["commit"]),
            reason: s(&hold["reason"]).to_owned(),
        })
        .unwrap();
    }
    let objects = v["objects"].as_array().unwrap();
    view.objects = ObjectGraph {
        sizes: objects.iter().map(|o| n(&o["size"])).collect(),
        children: objects
            .iter()
            .map(|o| {
                o["children"]
                    .as_array()
                    .unwrap()
                    .iter()
                    .map(|c| n(c) as u32)
                    .collect()
            })
            .collect(),
    };
    for state in v["state_objects"].as_array().unwrap() {
        let key = state_key(state);
        view.set_state_object(key.document, &key.state_root, n(&state["object"]) as u32);
    }
    view
}

#[test]
fn planner_scenarios_reproduce_plans_hashes_and_sets_byte_for_byte() {
    let f = fixture();
    let scenarios = f["plans"]["scenarios"].as_array().unwrap();
    assert!(scenarios.len() >= 8);
    for scenario in scenarios {
        let name = s(&scenario["name"]);
        let view = view_from(&f["plans"]["views"][s(&scenario["view"])]);
        let policy = Policy::load(s(&scenario["policy"]).as_bytes(), LoadOptions::default())
            .unwrap_or_else(|e| panic!("{name}: {e}"));
        let options = PlanOptions {
            as_of_ms: scenario["as_of_ms"].as_i64(),
            list: n(&scenario["list"]) as usize,
        };
        let got = plan::plan(&view, &policy, &options).unwrap_or_else(|e| panic!("{name}: {e}"));
        let want = s(&scenario["plan"]);
        assert_eq!(
            String::from_utf8(got.canonical_bytes().unwrap()).unwrap(),
            want,
            "{name}: canonical plan"
        );
        assert_eq!(got.plan_hash, s(&scenario["plan_hash"]), "{name}");
        assert!(got.hash_is_valid(), "{name}");
        assert!(got.read_only);
        assert_eq!(got.sets.retained, addrs(&scenario["retained"]), "{name}");
        assert_eq!(got.sets.released, addrs(&scenario["released"]), "{name}");
        if got.topology == "keep" {
            assert_eq!(
                got.sets.skeleton,
                skeleton_rows(&scenario["skeleton"]),
                "{name}"
            );
        } else {
            assert!(got.sets.skeleton.is_empty(), "{name}");
        }
        // A plan is a record in waiting: the draft validates and commits to its roots.
        let record = got.draft_record(1, None, 1_790_000_000_000).unwrap();
        assert_eq!(record.roots.skeleton, got.roots.skeleton);
        assert_eq!(record.counts, got.counts);
        assert_eq!(
            skeleton_root(&got.sets.skeleton).unwrap(),
            got.roots.skeleton,
            "{name}"
        );
        assert_eq!(
            frontier_root(&got.sets.frontier).unwrap(),
            got.roots.frontier,
            "{name}"
        );
        assert_eq!(
            released_states_root(&got.sets.released_states).unwrap(),
            got.roots.released_states,
            "{name}"
        );
    }
}
