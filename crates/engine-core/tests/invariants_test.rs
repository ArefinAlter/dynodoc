#[path = "support/gen.rs"]
mod gen;

use engine_core::{
    integrity::{check_invariants, InvariantViolation as V},
    materializer::{apply_payload, DocumentState, MaterializedNode},
};
use engine_shared::{EventPayload, NodeId};
use proptest::{prelude::*, test_runner::RngSeed};
use serde_json::json;

fn id(value: &str) -> NodeId {
    NodeId(value.into())
}
fn graph(links: &[(&str, Option<&str>)]) -> DocumentState {
    let mut state = DocumentState::default();
    for (key, parent) in links {
        state.nodes.insert(
            id(key),
            MaterializedNode {
                id: id(key),
                parent_id: parent.map(id),
                node_type: "section".into(),
                pos: "equal-position".into(),
                current_fields: json!({}),
                var_name: None,
                deleted: false,
            },
        );
    }
    state
}

#[test]
fn reports_identity_links_cycles_and_names_without_mutating_state() {
    let mut state = graph(&[
        ("a", Some("b")),
        ("b", Some("a")),
        ("c", Some("absent")),
        ("d", Some("e")),
        ("e", None),
    ]);
    state.nodes.get_mut(&id("a")).unwrap().id = id("wrong");
    state.nodes.get_mut(&id("e")).unwrap().deleted = true;
    for key in ["b", "c"] {
        state.nodes.get_mut(&id(key)).unwrap().var_name = Some("duplicate".into());
    }
    let original = state.clone();
    let issues = check_invariants(&state);
    assert_eq!(
        issues,
        vec![
            V::NodeIdMismatch {
                key: id("a"),
                node_id: id("wrong")
            },
            V::MissingParent {
                node_id: id("c"),
                parent_id: id("absent")
            },
            V::NonLiveParent {
                node_id: id("d"),
                parent_id: id("e")
            },
            V::ParentCycle {
                node_ids: vec![id("a"), id("b")]
            },
            V::DuplicateVarName {
                var_name: "duplicate".into(),
                node_ids: vec![id("b"), id("c")]
            },
        ]
    );
    assert_eq!(state, original);
    assert_eq!(issues, check_invariants(&state));
}

#[test]
fn tombstones_and_removed_choices_are_not_live_but_their_children_are_checked() {
    let mut state = graph(&[
        ("root", None),
        ("removed", Some("missing")),
        ("child", Some("removed")),
        ("deleted", Some("missing")),
    ]);
    state.removed_choices.insert(id("removed"));
    state.nodes.get_mut(&id("deleted")).unwrap().deleted = true;
    for node in state.nodes.values_mut() {
        node.var_name = Some("shared".into());
    }
    let issues = check_invariants(&state);
    assert_eq!(
        issues,
        vec![
            V::NonLiveParent {
                node_id: id("child"),
                parent_id: id("removed")
            },
            V::DuplicateVarName {
                var_name: "shared".into(),
                node_ids: vec![id("child"), id("root")]
            },
        ]
    );
}

#[test]
fn multiple_roots_equal_positions_and_empty_names_remain_allowed() {
    let mut state = graph(&[("a", None), ("b", None), ("c", Some("a")), ("d", Some("a"))]);
    for node in state.nodes.values_mut() {
        node.var_name = Some(String::new());
    }
    assert!(check_invariants(&state).is_empty());
}

#[test]
fn finds_disconnected_and_self_cycles_once_in_stable_order() {
    let state = graph(&[
        ("a", Some("c")),
        ("b", Some("c")),
        ("c", Some("b")),
        ("d", Some("d")),
    ]);
    assert_eq!(
        check_invariants(&state),
        vec![
            V::ParentCycle {
                node_ids: vec![id("b"), id("c")]
            },
            V::ParentCycle {
                node_ids: vec![id("d")]
            },
        ]
    );
}

#[test]
fn deep_trees_do_not_need_recursion() {
    let mut state = DocumentState::default();
    for i in 0..20_000 {
        let key = format!("{i:026}");
        let parent = (i > 0).then(|| format!("{:026}", i - 1));
        let node = graph(&[(&key, parent.as_deref())])
            .nodes
            .remove(&id(&key))
            .unwrap();
        state.nodes.insert(node.id.clone(), node);
    }
    assert!(check_invariants(&state).is_empty());
}

proptest! {
    #![proptest_config(ProptestConfig { cases: 1_000, rng_seed: RngSeed::Fixed(0x44594e4f494e56), .. ProptestConfig::default() })]
    #[test]
    fn every_valid_event_keeps_structural_invariants(seed in 1..u64::MAX, count in 1usize..80) {
        let mut state = DocumentState::default();
        for event in gen::history(seed, count) {
            let op: EventPayload = serde_json::from_value(event.payload).unwrap();
            apply_payload(&mut state, &op).unwrap();
            prop_assert_eq!(check_invariants(&state), Vec::new(), "seed={}, seq={}", seed, event.seq);
        }
    }
}
