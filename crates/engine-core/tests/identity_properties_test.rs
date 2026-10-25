//! Engine-level identity properties. Split/join here use delete + fresh create;
//! this does not claim nested editor IDs or future native split/join operations.
#[path = "support/gen.rs"]
mod gen;
use engine_core::{
    integrity::check_invariants,
    materializer::{apply_payload, DocumentState, Materializer},
    ops::validate_op,
    workspace_merge,
};
use engine_shared::{EventPayload, NodeId, NodeType};
use proptest::{prelude::*, test_runner::RngSeed};
use serde_json::json;
use std::collections::{BTreeMap, BTreeSet};

fn canonical(mut state: DocumentState) -> DocumentState {
    for id in &state.removed_choices {
        if let Some(node) = state.nodes.get_mut(id) {
            node.deleted = true;
        }
    }
    state
}
fn side(base: &DocumentState, seed: u64, count: usize) -> DocumentState {
    let mut generator = gen::Gen::new(seed);
    let mut state = base.clone();
    for _ in 0..count {
        if let Some(op) = generator.op(&state, &[]) {
            apply_payload(&mut state, &op).unwrap();
        }
    }
    state
}
fn apply(state: &mut DocumentState, op: EventPayload) {
    validate_op(state, &op).unwrap();
    apply_payload(state, &op).unwrap();
    assert!(check_invariants(state).is_empty());
}
proptest! {
 #![proptest_config(ProptestConfig {cases:1000,rng_seed:RngSeed::Fixed(0x4944454e54495459),..ProptestConfig::default()})]
 #[test]
 fn concurrent_generated_drafts_keep_stable_node_identities(seed in 1u64..1_000_000_000,count in 1usize..45,left_count in 1usize..25,right_count in 1usize..25) {
    let base=Materializer::fold(&gen::history(seed,count)).unwrap();
    let left=side(&base,seed+10_000,left_count);
    let right=side(&base,seed+20_000,right_count);
    prop_assert!(check_invariants(&left).is_empty());prop_assert!(check_invariants(&right).is_empty());
    let all:BTreeSet<_>=left.nodes.keys().chain(right.nodes.keys()).cloned().collect();
    for (team,draft) in [(&left,&right),(&right,&left)] {
        let plan=workspace_merge::merge(&base,team,draft,&BTreeMap::new());
        for (key,node) in &plan.state.nodes {
            prop_assert!(all.contains(key),"merge invented {}",key.0);
            prop_assert_eq!(key,&node.id);
        }
        for (key,node) in &base.nodes {
            prop_assert!(plan.state.nodes.contains_key(key),"stable identity {} vanished",key.0);
            if left.nodes.get(key)==Some(node) && right.nodes.get(key)==Some(node) {
                prop_assert_eq!(&plan.state.nodes[key].current_fields,&node.current_fields);
            }
        }
        if plan.conflicts.is_empty() {
            prop_assert!(check_invariants(&plan.state).is_empty());
            let mut replay=canonical(team.clone());
            for op in &plan.ops {apply_payload(&mut replay,op).unwrap();}
            let shared: BTreeSet<_> = plan.state.nodes.iter()
                .filter(|(id,node)| !node.deleted || team.nodes.contains_key(*id))
                .map(|(id,_)| id.clone()).collect();
            // Private creates deleted before first share intentionally emit no
            // canonical event. Every live or previously canonical ID must replay.
            prop_assert_eq!(replay.nodes.keys().cloned().collect::<BTreeSet<_>>(),shared);
            for (id,node) in &plan.state.nodes {
                if !replay.nodes.contains_key(id) {
                    prop_assert!(node.deleted && !team.nodes.contains_key(id));
                }
            }
            for (id,node) in &replay.nodes {prop_assert_eq!(id,&node.id);}
        }
    }
 }
 #[test]
 fn split_and_join_model_replace_only_the_target_identity(seed in 1u64..1_000_000_000) {
    let mut generator=gen::Gen::new(seed);
    let ids:Vec<_>=(0..7).map(|_|NodeId(generator.id())).collect();
    let mut state=DocumentState::default();
    apply(&mut state,EventPayload::NodeCreated {node_id:ids[0].clone(),node_type:NodeType::Clause,parent_id:None,pos:"a0".into(),fields:json!({}),var_name:None});
    for (i,id) in ids[1..4].iter().enumerate() {
        apply(&mut state,EventPayload::NodeCreated {node_id:id.clone(),node_type:NodeType::Paragraph,parent_id:Some(ids[0].clone()),pos:format!("a{i}"),fields:json!({"label":format!("বাংলা {seed} stable {i}")}),var_name:None});
    }
    let before=state.clone();let original=state.nodes[&ids[2]].current_fields["label"].as_str().unwrap().to_owned();
    let (first,last)=original.split_once(' ').unwrap();
    apply(&mut state,EventPayload::NodeDeleted {node_id:ids[2].clone()});
    for (id,text,pos) in [(&ids[4],first,"a1"),(&ids[5],last,"a1V")] {
        apply(&mut state,EventPayload::NodeCreated {node_id:id.clone(),node_type:NodeType::Paragraph,parent_id:Some(ids[0].clone()),pos:pos.into(),fields:json!({"label":text}),var_name:None});
    }
    for id in [&ids[4],&ids[5]] {apply(&mut state,EventPayload::NodeDeleted {node_id:id.clone()});}
    apply(&mut state,EventPayload::NodeCreated {node_id:ids[6].clone(),node_type:NodeType::Paragraph,parent_id:Some(ids[0].clone()),pos:"a1".into(),fields:json!({"label":format!("{first} {last}")}),var_name:None});
    for index in [0,1,3] {prop_assert_eq!(&state.nodes[&ids[index]],&before.nodes[&ids[index]]);}
    for index in [2,4,5] {prop_assert!(state.nodes[&ids[index]].deleted);prop_assert_eq!(&state.nodes[&ids[index]].id,&ids[index]);}
    prop_assert_eq!(&state.nodes[&ids[6]].current_fields["label"],&json!(original));
    prop_assert_eq!(state.nodes.len(),7);
 }
}

#[test]
fn an_unshared_private_create_delete_needs_no_canonical_identity() {
    let base = Materializer::fold(&gen::history(77, 1)).unwrap();
    let root = base.nodes.keys().next().unwrap().clone();
    let mut draft = base.clone();
    let id = NodeId(gen::Gen::new(88).id());
    apply(
        &mut draft,
        EventPayload::NodeCreated {
            node_id: id.clone(),
            node_type: NodeType::Item,
            parent_id: Some(root),
            pos: "a0".into(),
            fields: json!({"type":"text"}),
            var_name: None,
        },
    );
    apply(
        &mut draft,
        EventPayload::NodeDeleted {
            node_id: id.clone(),
        },
    );
    let plan = workspace_merge::merge(&base, &base, &draft, &BTreeMap::new());
    assert!(plan.conflicts.is_empty());
    assert!(plan.ops.is_empty());
    assert!(plan.state.nodes[&id].deleted);
    assert!(!base.nodes.contains_key(&id));
}
