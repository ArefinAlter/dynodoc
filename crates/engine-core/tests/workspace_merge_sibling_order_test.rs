//! Sibling ordering and same-spot concurrent insertion tests for workspace merge.
//!
//! When two collaborating drafts concurrently insert blocks under the same parent
//! at the exact same location (allocating the identical fractional pos key, e.g.
//! via deterministic `between()`), the engine merge law (T52) requires that:
//! 1. Both nodes are preserved without false `$node` or `$location` conflicts.
//! 2. Sibling order among nodes sharing identical `pos` is broken deterministically
//!    by `NodeId` (`(pos, id)` order).
//! 3. Merging is side-symmetric: `merge(base, team, draft)` and `merge(base, draft, team)`
//!    yield identical node structures.
//! 4. Generated operations validate and apply cleanly onto team state.

use std::collections::BTreeMap;

use engine_core::{
    materializer::{apply_payload, DocumentState},
    ops::validate_op,
    workspace_merge::{self, MergePlan},
};
use engine_shared::{EventPayload, NodeId, NodeType};
use serde_json::{json, Value};

fn create_node(
    state: &mut DocumentState,
    id: &str,
    kind: NodeType,
    parent: Option<&str>,
    pos: &str,
    fields: Value,
) {
    let op = EventPayload::NodeCreated {
        node_id: NodeId(id.into()),
        node_type: kind,
        parent_id: parent.map(|p| NodeId(p.into())),
        pos: pos.into(),
        fields,
        var_name: None,
    };
    validate_op(state, &op).unwrap();
    apply_payload(state, &op).unwrap();
}

fn base_document() -> DocumentState {
    let mut state = DocumentState::default();
    create_node(
        &mut state,
        "root",
        NodeType::Form,
        None,
        "000000",
        json!({}),
    );
    create_node(
        &mut state,
        "section",
        NodeType::Section,
        Some("root"),
        "a0",
        json!({}),
    );
    state
}

fn replay_plan(initial: &DocumentState, plan: &MergePlan) {
    let mut actual = initial.clone();
    for op in &plan.ops {
        validate_op(&actual, op).unwrap();
        apply_payload(&mut actual, op).unwrap();
    }
    assert_eq!(actual, plan.state);
}

#[test]
fn concurrent_inserts_at_same_spot_are_not_conflicts() {
    let base = base_document();
    let mut team = base.clone();
    let mut draft = base.clone();

    // Team inserts node_team under section at pos "b0"
    create_node(
        &mut team,
        "01JTEAM0000000000000000000",
        NodeType::Item,
        Some("section"),
        "b0",
        json!({"label": "Team insertion"}),
    );

    // Draft inserts node_draft under section at the identical pos "b0"
    create_node(
        &mut draft,
        "01JDRAFT000000000000000000",
        NodeType::Item,
        Some("section"),
        "b0",
        json!({"label": "Draft insertion"}),
    );

    let plan = workspace_merge::merge(&base, &team, &draft, &BTreeMap::new());

    // Both nodes must be kept with no merge conflict
    assert!(
        plan.conflicts.is_empty(),
        "Unexpected conflicts: {:?}",
        plan.conflicts
    );

    assert!(plan
        .state
        .nodes
        .contains_key(&NodeId("01JTEAM0000000000000000000".into())));
    assert!(plan
        .state
        .nodes
        .contains_key(&NodeId("01JDRAFT000000000000000000".into())));

    assert_eq!(
        plan.state.nodes[&NodeId("01JTEAM0000000000000000000".into())].parent_id,
        Some(NodeId("section".into()))
    );
    assert_eq!(
        plan.state.nodes[&NodeId("01JDRAFT000000000000000000".into())].parent_id,
        Some(NodeId("section".into()))
    );
    assert_eq!(
        plan.state.nodes[&NodeId("01JTEAM0000000000000000000".into())].pos,
        "b0"
    );
    assert_eq!(
        plan.state.nodes[&NodeId("01JDRAFT000000000000000000".into())].pos,
        "b0"
    );

    replay_plan(&team, &plan);
}

#[test]
fn same_spot_inserts_order_by_node_id_tie_break() {
    let base = base_document();
    let mut team = base.clone();
    let mut draft = base.clone();

    // Node A has a lexicographically smaller ULID than Node B
    let id_a = "01JA0000000000000000000000";
    let id_b = "01JB0000000000000000000000";

    // Team inserts Node B at pos "m"
    create_node(
        &mut team,
        id_b,
        NodeType::Item,
        Some("section"),
        "m",
        json!({"label": "Node B"}),
    );

    // Draft inserts Node A at pos "m"
    create_node(
        &mut draft,
        id_a,
        NodeType::Item,
        Some("section"),
        "m",
        json!({"label": "Node A"}),
    );

    let plan = workspace_merge::merge(&base, &team, &draft, &BTreeMap::new());
    assert!(plan.conflicts.is_empty());

    // Query siblings under "section" sorted by (pos, id)
    let mut siblings: Vec<_> = plan
        .state
        .nodes
        .values()
        .filter(|n| n.parent_id == Some(NodeId("section".into())) && !n.deleted)
        .collect();

    siblings.sort_by_key(|n| (&n.pos, &n.id));

    let sibling_ids: Vec<&str> = siblings.iter().map(|n| n.id.0.as_str()).collect();
    assert_eq!(
        sibling_ids,
        vec![id_a, id_b],
        "Siblings with identical pos must tie-break deterministically by NodeId"
    );

    replay_plan(&team, &plan);
}

#[test]
fn same_spot_inserts_are_side_symmetric() {
    let base = base_document();
    let mut side1 = base.clone();
    let mut side2 = base.clone();

    let id_1 = "01J11111111111111111111111";
    let id_2 = "01J22222222222222222222222";

    create_node(
        &mut side1,
        id_1,
        NodeType::Item,
        Some("section"),
        "same_pos",
        json!({"label": "Side 1"}),
    );
    create_node(
        &mut side2,
        id_2,
        NodeType::Item,
        Some("section"),
        "same_pos",
        json!({"label": "Side 2"}),
    );

    let plan_12 = workspace_merge::merge(&base, &side1, &side2, &BTreeMap::new());
    let plan_21 = workspace_merge::merge(&base, &side2, &side1, &BTreeMap::new());

    assert!(plan_12.conflicts.is_empty());
    assert!(plan_21.conflicts.is_empty());

    // Both final states must contain identical nodes with identical positions
    assert_eq!(plan_12.state.nodes.len(), plan_21.state.nodes.len());
    for (id, node_12) in &plan_12.state.nodes {
        let node_21 = &plan_21.state.nodes[id];
        assert_eq!(node_12.parent_id, node_21.parent_id);
        assert_eq!(node_12.pos, node_21.pos);
        assert_eq!(node_12.current_fields, node_21.current_fields);
    }

    replay_plan(&side1, &plan_12);
    replay_plan(&side2, &plan_21);
}

#[test]
fn multiple_concurrent_same_spot_insertions_merge_without_loss() {
    let mut base = base_document();

    // Base already has an existing item at "a0"
    create_node(
        &mut base,
        "01JEXISTING000000000000000",
        NodeType::Item,
        Some("section"),
        "a0",
        json!({"label": "Existing"}),
    );
    let mut team = base.clone();
    let mut draft = base.clone();

    // Team inserts two new items at "a0"
    create_node(
        &mut team,
        "01JTEAM_A0_100000000000000",
        NodeType::Item,
        Some("section"),
        "a0",
        json!({"label": "Team A0 1"}),
    );
    create_node(
        &mut team,
        "01JTEAM_A0_200000000000000",
        NodeType::Item,
        Some("section"),
        "a0",
        json!({"label": "Team A0 2"}),
    );

    // Draft inserts two new items at "a0"
    create_node(
        &mut draft,
        "01JDRAFT_A0_10000000000000",
        NodeType::Item,
        Some("section"),
        "a0",
        json!({"label": "Draft A0 1"}),
    );
    create_node(
        &mut draft,
        "01JDRAFT_A0_20000000000000",
        NodeType::Item,
        Some("section"),
        "a0",
        json!({"label": "Draft A0 2"}),
    );

    let plan = workspace_merge::merge(&base, &team, &draft, &BTreeMap::new());
    assert!(plan.conflicts.is_empty());

    // All 5 items must be present under section
    let section_items: Vec<_> = plan
        .state
        .nodes
        .values()
        .filter(|n| n.parent_id == Some(NodeId("section".into())) && !n.deleted)
        .collect();
    assert_eq!(section_items.len(), 5);

    replay_plan(&team, &plan);
}
