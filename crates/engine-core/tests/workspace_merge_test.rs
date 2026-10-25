//! Missing cross-node merge scenarios, including replay of each emitted plan.
use std::collections::BTreeMap;

use engine_core::{
    materializer::{apply_payload, DocumentState},
    ops::validate_op,
    workspace_merge::{self, MergePlan},
};
use engine_shared::{EventPayload, NodeId, NodeType};
use serde_json::{json, Value};

fn create(
    state: &mut DocumentState,
    id: &str,
    kind: NodeType,
    parent: Option<&str>,
    fields: Value,
) {
    let op = EventPayload::NodeCreated {
        node_id: NodeId(id.into()),
        node_type: kind,
        parent_id: parent.map(|p| NodeId(p.into())),
        pos: id.into(),
        fields,
        var_name: None,
    };
    validate_op(state, &op).unwrap();
    apply_payload(state, &op).unwrap();
}

fn base() -> DocumentState {
    let mut state = DocumentState::default();
    create(&mut state, "root", NodeType::Form, None, json!({}));
    create(
        &mut state,
        "left",
        NodeType::Section,
        Some("root"),
        json!({}),
    );
    create(
        &mut state,
        "right",
        NodeType::Section,
        Some("root"),
        json!({}),
    );
    create(
        &mut state,
        "paragraph",
        NodeType::Item,
        Some("left"),
        json!({"label":"Original"}),
    );
    state
}

fn edit(state: &mut DocumentState, field: &str, value: Value) {
    let op = EventPayload::FieldEdited {
        node_id: NodeId("paragraph".into()),
        field: field.into(),
        value,
    };
    validate_op(state, &op).unwrap();
    apply_payload(state, &op).unwrap();
}

fn replay(team: &DocumentState, plan: &MergePlan) {
    let mut actual = team.clone();
    for op in &plan.ops {
        validate_op(&actual, op).unwrap();
        apply_payload(&mut actual, op).unwrap();
    }
    assert_eq!(actual, plan.state);
}

#[test]
fn image_rule_and_page_break_insertions_merge_with_another_blocks_edit() {
    for content in [
        json!({"type":"image","attrs":{"src":"synthetic.png","alt":"Synthetic"}}),
        json!({"type":"horizontalRule"}),
        json!({"type":"pageBreak"}),
    ] {
        let base = base();
        let mut team = base.clone();
        edit(&mut team, "label", json!("Team edit"));
        let mut draft = base.clone();
        create(
            &mut draft,
            "inserted",
            NodeType::Item,
            Some("right"),
            json!({"content":content}),
        );
        for (team, draft) in [(&team, &draft), (&draft, &team)] {
            let plan = workspace_merge::merge(&base, team, draft, &BTreeMap::new());
            assert!(plan.conflicts.is_empty(), "{:?}", plan.conflicts);
            assert_eq!(
                plan.state.nodes[&NodeId("inserted".into())].current_fields["content"],
                content
            );
            assert_eq!(
                plan.state.nodes[&NodeId("paragraph".into())].current_fields["label"],
                "Team edit"
            );
            replay(team, &plan);
        }
    }
}

#[test]
fn move_and_edit_of_one_block_merge() {
    let base = base();
    let mut team = base.clone();
    let movement = EventPayload::NodeMoved {
        node_id: NodeId("paragraph".into()),
        new_parent_id: Some(NodeId("right".into())),
        new_pos: "z".into(),
    };
    validate_op(&team, &movement).unwrap();
    apply_payload(&mut team, &movement).unwrap();
    let mut draft = base.clone();
    edit(&mut draft, "label", json!("Edited while moved"));
    for (team, draft) in [(&team, &draft), (&draft, &team)] {
        let plan = workspace_merge::merge(&base, team, draft, &BTreeMap::new());
        assert!(plan.conflicts.is_empty());
        let node = &plan.state.nodes[&NodeId("paragraph".into())];
        assert_eq!(node.parent_id, Some(NodeId("right".into())));
        assert_eq!(node.pos, "z");
        assert_eq!(node.current_fields["label"], "Edited while moved");
        replay(team, &plan);
    }
}

#[test]
fn page_setup_first_set_by_both_sides_merges_independent_keys() {
    let base = base();
    let mut team = base.clone();
    let mut draft = base.clone();
    edit(&mut team, "page_setup", json!({"marginTop":20}));
    edit(&mut draft, "page_setup", json!({"landscape":true}));
    for (team, draft) in [(&team, &draft), (&draft, &team)] {
        let plan = workspace_merge::merge(&base, team, draft, &BTreeMap::new());
        assert!(plan.conflicts.is_empty());
        assert_eq!(
            plan.state.nodes[&NodeId("paragraph".into())].current_fields["page_setup"],
            json!({"marginTop":20,"landscape":true})
        );
        replay(team, &plan);
    }
}

#[test]
fn same_parent_different_pos_is_a_location_conflict() {
    let base = base();
    let moved = |pos: &str| {
        let mut state = base.clone();
        apply_payload(
            &mut state,
            &EventPayload::NodeMoved {
                node_id: NodeId("paragraph".into()),
                new_parent_id: Some(NodeId("left".into())),
                new_pos: pos.into(),
            },
        )
        .unwrap();
        state
    };
    let team = moved("a");
    let draft = moved("z");
    let unresolved = workspace_merge::merge(&base, &team, &draft, &BTreeMap::new());
    assert_eq!(unresolved.conflicts.len(), 1);
    assert_eq!(unresolved.conflicts[0].key, "paragraph:$location");
    replay(&team, &unresolved);
    for (choice, expected) in [("team", "a"), ("draft", "z")] {
        let plan = workspace_merge::merge(
            &base,
            &team,
            &draft,
            &BTreeMap::from([("paragraph:$location".into(), choice.into())]),
        );
        assert!(plan.conflicts.is_empty());
        assert_eq!(plan.state.nodes[&NodeId("paragraph".into())].pos, expected);
        replay(&team, &plan);
    }
}

#[test]
fn equal_edits_on_both_sides_emit_no_duplicate_operations() {
    let base = base();
    let mut team = base.clone();
    edit(&mut team, "label", json!("Same edit"));
    let draft = team.clone();
    let plan = workspace_merge::merge(&base, &team, &draft, &BTreeMap::new());
    assert!(plan.conflicts.is_empty());
    assert!(plan.ops.is_empty());
    assert_eq!(plan.state, team);
    replay(&team, &plan);
}
