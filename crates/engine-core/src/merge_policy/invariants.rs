//! The general invariants INV-G1 to INV-G8 of design 5.1, checked on a merged state.
//!
//! A check never repairs anything. It lists the violations the merged state has; a
//! violation that an input already had is not the merge's doing and is only counted.
//! In `flag` mode the introduced ones are evidence; in `conflict` mode the plan also
//! gets a conflict for each, unless a conflict on the same participants already
//! reports it (the tree checks of `workspace_merge` raise those for G1, G3 and G4).
//!
//! The state-level part of INV-G8 is checked here: the protection flag is a
//! boolean on a worksheet section. The edit guards themselves act on operations
//! (`governance::authorize`), not on a state, and are enforced when the plan's
//! operations are applied.
//!
//! INV-G5 reuses `integrity::validate_integrity` (questionnaire references, relevance
//! cycles, type use), minus the duplicate-name case that INV-G4 reports.

use super::{
    evidence::{InvariantReport, MergeEvidence, StructuralConflict},
    policy::InvariantMode,
};
use crate::{
    integrity::{validate_integrity, IntegrityViolation},
    materializer::DocumentState,
    workspace_merge::{find_parent_cycle, Conflict, MergePlan},
};
use engine_shared::NodeId;
use serde::Serialize;
use serde_json::{json, Value};
use std::collections::{BTreeMap, BTreeSet};

pub const IDS: [&str; 8] = [
    "INV-G1", "INV-G2", "INV-G3", "INV-G4", "INV-G5", "INV-G6", "INV-G7", "INV-G8",
];

/// One broken invariant. Two violations are the same when their invariant, subject
/// and detail are, which is how "introduced by the merge" is told from "already
/// there".
#[derive(Clone, Debug, PartialEq, Eq, PartialOrd, Ord, Serialize)]
pub struct Violation {
    pub invariant: &'static str,
    pub subject: String,
    pub detail: String,
    /// The nodes the violation is about; used to match existing conflicts.
    #[serde(skip)]
    pub participants: Vec<NodeId>,
}

fn violation(
    invariant: &'static str,
    subject: &str,
    detail: String,
    nodes: Vec<NodeId>,
) -> Violation {
    Violation {
        invariant,
        subject: subject.into(),
        detail,
        participants: nodes,
    }
}

/// The conflict fields that already report a violation of this invariant.
fn reported_by(invariant: &str) -> &'static [&'static str] {
    match invariant {
        "INV-G1" => &["$location"],
        "INV-G3" => &["$node"],
        "INV-G4" => &["var_name"],
        _ => &[],
    }
}

/// Every violation in `state`, sorted and without repeats.
pub fn check(state: &DocumentState) -> Vec<Violation> {
    let mut found = BTreeSet::new();

    // INV-G1: the parent graph is acyclic. The cycle is written from its smallest id.
    if let Some(mut cycle) = find_parent_cycle(state) {
        if let Some(at) = (0..cycle.len()).min_by_key(|i| &cycle[*i]) {
            cycle.rotate_left(at);
        }
        let names: Vec<&str> = cycle.iter().map(|n| n.0.as_str()).collect();
        found.insert(violation(
            "INV-G1",
            &cycle[0].0,
            format!("parent cycle {}", names.join(" > ")),
            cycle.clone(),
        ));
    }

    let mut by_name: BTreeMap<&str, Vec<&NodeId>> = BTreeMap::new();
    for node in state.nodes.values().filter(|n| !n.deleted) {
        // INV-G2: containment is legal and a root is a root type.
        match &node.parent_id {
            None => {
                if let Some(t) = crate::ops::parse_node_type(&node.node_type) {
                    if !crate::ops::is_root_type(t) {
                        found.insert(violation(
                            "INV-G2",
                            &node.id.0,
                            format!("`{}` cannot be a document root", node.node_type),
                            vec![node.id.clone()],
                        ));
                    }
                }
            }
            Some(parent) => match state.nodes.get(parent) {
                // INV-G3: a live node's parent exists and is live.
                None => {
                    found.insert(violation(
                        "INV-G3",
                        &node.id.0,
                        format!("parent {} does not exist", parent.0),
                        vec![parent.clone()],
                    ));
                }
                Some(p) if p.deleted => {
                    found.insert(violation(
                        "INV-G3",
                        &node.id.0,
                        format!("parent {} is deleted", parent.0),
                        vec![parent.clone()],
                    ));
                }
                Some(p) => {
                    let types = (
                        crate::ops::parse_node_type(&p.node_type),
                        crate::ops::parse_node_type(&node.node_type),
                    );
                    if let (Some(pt), Some(ct)) = types {
                        if !crate::ops::can_contain(pt, ct) {
                            found.insert(violation(
                                "INV-G2",
                                &node.id.0,
                                format!("`{}` cannot contain `{}`", p.node_type, node.node_type),
                                vec![node.id.clone()],
                            ));
                        }
                    }
                }
            },
        }
        if let Some(name) = node.var_name.as_deref().filter(|v| !v.is_empty()) {
            by_name.entry(name).or_default().push(&node.id);
        }
        // INV-G6: the label of a rich-text node is the plain text of its content.
        let content = &node.current_fields["content"];
        if content.is_object()
            && node.current_fields["label"] != json!(crate::richtext::plain_text(content))
        {
            found.insert(violation(
                "INV-G6",
                &node.id.0,
                "label differs from the plain text of content".into(),
                vec![node.id.clone()],
            ));
        }
        // INV-G8 (state part): the protection flag is a boolean on a worksheet.
        let flag = &node.current_fields["sheet_protected"];
        let allowed = flag.is_null()
            || (flag.is_boolean()
                && node.node_type == "section"
                && node.current_fields["kind"] == "sheet");
        if !allowed {
            found.insert(violation(
                "INV-G8",
                &node.id.0,
                "sheet_protected must be a boolean on a worksheet section".into(),
                vec![node.id.clone()],
            ));
        }
    }
    // INV-G4: live variable names are unique.
    for (name, nodes) in by_name.iter().filter(|(_, nodes)| nodes.len() > 1) {
        let ids: Vec<&str> = nodes.iter().map(|n| n.0.as_str()).collect();
        found.insert(violation(
            "INV-G4",
            name,
            format!("variable name `{name}` is held by {}", ids.join(", ")),
            nodes.iter().map(|n| (*n).clone()).collect(),
        ));
    }

    // INV-G5: references resolve, no relevance cycles, types fit.
    for v in validate_integrity(state) {
        let nodes: Vec<NodeId> = match &v {
            IntegrityViolation::DanglingReference { node_id, .. }
            | IntegrityViolation::InvalidExpression { node_id, .. }
            | IntegrityViolation::TypeMismatch { node_id, .. }
            | IntegrityViolation::UndefinedChoice { node_id, .. } => vec![node_id.clone()],
            IntegrityViolation::RelevanceCycle { cycle } => cycle.clone(),
            IntegrityViolation::DuplicateVarName { .. }
            | IntegrityViolation::InvalidOperation { .. } => continue,
        };
        let subject = nodes.first().map_or(String::new(), |n| n.0.clone());
        found.insert(violation(
            "INV-G5",
            &subject,
            serde_json::to_string(&v).unwrap_or_default(),
            nodes,
        ));
    }

    // INV-G7: comments and suggestions target nodes that exist.
    let preview = |text: &str| text.chars().take(40).collect::<String>();
    for comment in &state.comments {
        if let Some(target) = comment
            .node_id
            .as_ref()
            .filter(|t| !state.nodes.contains_key(*t))
        {
            found.insert(violation(
                "INV-G7",
                &target.0,
                format!(
                    "comment \"{}\" targets a node that does not exist",
                    preview(&comment.body)
                ),
                vec![target.clone()],
            ));
        }
    }
    for (id, suggestion) in &state.suggestions {
        if let Some(target) = suggestion
            .target_node_id
            .as_ref()
            .filter(|t| !state.nodes.contains_key(*t))
        {
            found.insert(violation(
                "INV-G7",
                &target.0,
                format!("suggestion {id} targets a node that does not exist"),
                vec![target.clone()],
            ));
        }
    }
    found.into_iter().collect()
}

/// The violations of `merged` that neither input has, and how many it shares with
/// an input.
pub fn introduced(
    merged: &DocumentState,
    team: &DocumentState,
    draft: &DocumentState,
) -> (Vec<Violation>, usize) {
    let known: BTreeSet<Violation> = check(team).into_iter().chain(check(draft)).collect();
    let (new, old): (Vec<_>, Vec<_>) = check(merged).into_iter().partition(|v| !known.contains(v));
    (new, old.len())
}

/// A `$invariant` conflict for each violation no existing conflict reports.
pub fn conflicts(introduced: &[Violation], existing: &[Conflict]) -> Vec<Conflict> {
    let mut out: Vec<Conflict> = Vec::new();
    for v in introduced {
        let fields = reported_by(v.invariant);
        let covered = existing
            .iter()
            .any(|c| fields.contains(&c.field.as_str()) && v.participants.contains(&c.node_id));
        if covered {
            continue;
        }
        let node_id = v
            .participants
            .first()
            .cloned()
            .unwrap_or_else(|| NodeId(v.subject.clone()));
        let key = format!("{}:$invariant:{}", node_id.0, v.invariant);
        if out.iter().chain(existing).any(|c| c.key == key) {
            continue;
        }
        out.push(Conflict {
            key,
            node_id,
            field: "$invariant".into(),
            ancestor: Value::Null,
            team: Value::Null,
            draft: json!({"invariant": v.invariant, "subject": v.subject, "detail": v.detail}),
        });
    }
    out
}

/// The post-merge step of `merge_with_policy`: name the conflicts the tree checks
/// raised, list the introduced violations and, in `conflict` mode, add a conflict
/// for each. The merged state is never touched.
pub(crate) fn apply(
    plan: &mut MergePlan,
    team: &DocumentState,
    draft: &DocumentState,
    mode: InvariantMode,
    evidence: &mut MergeEvidence,
) {
    let decided: BTreeSet<&str> = evidence
        .decisions
        .iter()
        .filter_map(|d| d.conflict_key.as_deref())
        .collect();
    evidence.structural = plan
        .conflicts
        .iter()
        .filter(|c| !decided.contains(c.key.as_str()))
        .map(|c| StructuralConflict {
            invariant: match c.field.as_str() {
                "$location" => "INV-G1",
                "$node" => "INV-G3",
                "var_name" => "INV-G4",
                _ => "tree-check",
            },
            key: c.key.clone(),
            node: c.node_id.clone(),
            field: c.field.clone(),
        })
        .collect();
    let (introduced, preexisting) = introduced(&plan.state, team, draft);
    let added = match mode {
        InvariantMode::Flag => Vec::new(),
        InvariantMode::Conflict => conflicts(&introduced, &plan.conflicts),
    };
    evidence.invariants = InvariantReport {
        mode,
        introduced,
        preexisting,
        conflicts: added.iter().map(|c| c.key.clone()).collect(),
    };
    plan.conflicts.extend(added);
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::materializer::{CommentState, MaterializedNode, SuggestionState};

    fn node(id: &str, kind: &str, parent: Option<&str>, fields: Value) -> MaterializedNode {
        MaterializedNode {
            id: NodeId(id.into()),
            parent_id: parent.map(|p| NodeId(p.into())),
            node_type: kind.into(),
            pos: "a".into(),
            current_fields: fields,
            var_name: None,
            deleted: false,
        }
    }

    fn state(nodes: Vec<MaterializedNode>) -> DocumentState {
        DocumentState {
            nodes: nodes.into_iter().map(|n| (n.id.clone(), n)).collect(),
            ..Default::default()
        }
    }

    fn valid() -> DocumentState {
        state(vec![
            node("root", "form", None, json!({})),
            node(
                "sec",
                "section",
                Some("root"),
                json!({"kind": "sheet", "sheet_protected": true}),
            ),
            node("item", "item", Some("sec"), json!({"label": "x"})),
        ])
    }

    fn ids(state: &DocumentState) -> Vec<(&'static str, String)> {
        check(state)
            .into_iter()
            .map(|v| (v.invariant, v.subject))
            .collect()
    }

    #[test]
    fn a_valid_state_has_no_violations() {
        assert_eq!(check(&valid()), []);
    }

    #[test]
    fn g1_a_parent_cycle_is_named_from_its_smallest_id() {
        let mut s = valid();
        s.nodes.get_mut(&NodeId("sec".into())).unwrap().parent_id = Some(NodeId("item".into()));
        let found = check(&s);
        let g1: Vec<_> = found.iter().filter(|v| v.invariant == "INV-G1").collect();
        assert_eq!(g1.len(), 1);
        assert_eq!(g1[0].detail, "parent cycle item > sec");
        assert_eq!(g1[0].participants.len(), 2);
    }

    #[test]
    fn g2_roots_and_containment() {
        let mut s = valid();
        s.nodes.insert(
            NodeId("para".into()),
            node("para", "paragraph", None, json!({})),
        );
        s.nodes.insert(
            NodeId("bad".into()),
            node("bad", "section", Some("item"), json!({})),
        );
        // A type the engine does not know is not judged.
        s.nodes
            .insert(NodeId("odd".into()), node("odd", "table", None, json!({})));
        assert_eq!(
            ids(&s),
            [
                ("INV-G2", "bad".to_string()),
                ("INV-G2", "para".to_string())
            ]
        );
    }

    #[test]
    fn g3_a_live_node_needs_a_live_parent() {
        let mut s = valid();
        s.nodes.get_mut(&NodeId("sec".into())).unwrap().deleted = true;
        s.nodes.insert(
            NodeId("lost".into()),
            node("lost", "item", Some("gone"), json!({})),
        );
        let found = check(&s);
        let details: Vec<_> = found
            .iter()
            .map(|v| (v.invariant, v.detail.as_str()))
            .collect();
        assert_eq!(
            details,
            [
                ("INV-G3", "parent sec is deleted"),
                ("INV-G3", "parent gone does not exist")
            ]
        );
    }

    #[test]
    fn g4_live_variable_names_are_unique() {
        let mut s = valid();
        for (id, name) in [("a", "q"), ("b", "q"), ("c", "r")] {
            let mut n = node(id, "item", Some("sec"), json!({}));
            n.var_name = Some(name.into());
            s.nodes.insert(n.id.clone(), n);
        }
        s.nodes.get_mut(&NodeId("c".into())).unwrap().var_name = Some("q".into());
        s.nodes.get_mut(&NodeId("c".into())).unwrap().deleted = true;
        let found = check(&s);
        assert_eq!(found.len(), 1);
        assert_eq!(found[0].detail, "variable name `q` is held by a, b");
    }

    #[test]
    fn g5_integrity_violations_other_than_duplicate_names() {
        let mut s = valid();
        s.nodes.insert(
            NodeId("q".into()),
            node(
                "q",
                "item",
                Some("sec"),
                json!({"relevance": "${missing} = 1"}),
            ),
        );
        let found = check(&s);
        assert_eq!(found.len(), 1);
        assert_eq!(found[0].invariant, "INV-G5");
        assert!(found[0].detail.contains("DanglingReference"));
    }

    #[test]
    fn g6_the_label_is_the_plain_text_of_the_content() {
        let content =
            |text: &str| json!({"type":"paragraph","content":[{"type":"text","text":text}]});
        let mut s = valid();
        s.nodes.insert(
            NodeId("p".into()),
            node(
                "p",
                "item",
                Some("sec"),
                json!({"label": "Hello", "content": content("Hello")}),
            ),
        );
        assert_eq!(check(&s), []);
        s.nodes.get_mut(&NodeId("p".into())).unwrap().current_fields["label"] = json!("Other");
        assert_eq!(ids(&s), [("INV-G6", "p".to_string())]);
        s.nodes.get_mut(&NodeId("p".into())).unwrap().current_fields["label"] = Value::Null;
        assert_eq!(ids(&s), [("INV-G6", "p".to_string())]);
    }

    #[test]
    fn g7_comments_and_suggestions_target_existing_nodes() {
        let mut s = valid();
        s.comments.push(CommentState {
            node_id: Some(NodeId("item".into())),
            body: "ok".into(),
        });
        s.comments.push(CommentState {
            node_id: None,
            body: "document".into(),
        });
        s.comments.push(CommentState {
            node_id: Some(NodeId("gone".into())),
            body: "lost".into(),
        });
        s.suggestions.insert(
            "s".into(),
            SuggestionState {
                target_node_id: Some(NodeId("gone2".into())),
                detail: json!({}),
                accepted: false,
                rejected_reason: None,
            },
        );
        assert_eq!(
            ids(&s),
            [
                ("INV-G7", "gone".to_string()),
                ("INV-G7", "gone2".to_string())
            ]
        );
    }

    #[test]
    fn g8_the_protection_flag_is_a_boolean_on_a_worksheet() {
        let mut s = valid();
        s.nodes
            .get_mut(&NodeId("item".into()))
            .unwrap()
            .current_fields["sheet_protected"] = json!(true);
        s.nodes
            .get_mut(&NodeId("sec".into()))
            .unwrap()
            .current_fields["sheet_protected"] = json!("yes");
        assert_eq!(
            ids(&s),
            [
                ("INV-G8", "item".to_string()),
                ("INV-G8", "sec".to_string())
            ]
        );
    }

    #[test]
    fn only_violations_neither_input_has_are_introduced() {
        let team = valid();
        let mut draft = valid();
        draft
            .nodes
            .get_mut(&NodeId("sec".into()))
            .unwrap()
            .current_fields["sheet_protected"] = json!(1);
        // The merged state keeps the draft's flag and adds a missing comment target.
        let mut merged = draft.clone();
        merged.comments.push(CommentState {
            node_id: Some(NodeId("gone".into())),
            body: "x".into(),
        });
        let (new, old) = introduced(&merged, &team, &draft);
        assert_eq!((new.len(), old), (1, 1));
        assert_eq!(new[0].invariant, "INV-G7");
    }

    #[test]
    fn conflicts_skip_what_an_existing_conflict_already_reports() {
        let mut s = valid();
        s.nodes.get_mut(&NodeId("sec".into())).unwrap().parent_id = Some(NodeId("item".into()));
        s.comments.push(CommentState {
            node_id: Some(NodeId("gone".into())),
            body: "x".into(),
        });
        let found = check(&s);
        let existing = vec![Conflict {
            key: "item:$location".into(),
            node_id: NodeId("item".into()),
            field: "$location".into(),
            ancestor: Value::Null,
            team: Value::Null,
            draft: Value::Null,
        }];
        let added = conflicts(&found, &existing);
        let keys: Vec<_> = added.iter().map(|c| c.key.as_str()).collect();
        // G1 is covered by the `$location` conflict on a participant; G2 and G7
        // have no such conflict.
        assert_eq!(keys, ["sec:$invariant:INV-G2", "gone:$invariant:INV-G7"]);
        assert!(added.iter().all(|c| c.field == "$invariant"));
        assert_eq!(added[1].draft["invariant"], "INV-G7");
        // Without the existing conflict the cycle is reported as well.
        assert_eq!(conflicts(&found, &[]).len(), 3);
    }
}
