//! Pure three-way semantic merge (docs/21 item 3).
//!
//! Given a common ancestor state `O` and two branches `A` and `B` — each a sequence of
//! [`EventPayload`]s applied since `O` — produce a [`MergeResult`]: the events that can
//! be auto-merged, the same-field collisions that need a human four-card decision, and
//! the integrity warnings the *combined* state would carry.
//!
//! The unit of merge is per-node, per-field, keyed on stable node identity (docs/04
//! §D.1) — not text lines. Two authors editing different fields of the same item, or
//! the same field to the same value, merge silently; only a genuine same-field
//! divergence surfaces as a [`FourCardConflict`]. This is the §C.3.6 defense against
//! both spurious conflicts and silent breakage: after auto-merging the easy cases we
//! run [`validate_integrity`] on the *proposed merged state*, so a pair of individually
//! clean edits that jointly break a reference (A renames a var, B references the old
//! name) is caught and the merge is blocked.
//!
//! Everything here is pure — no DB, no I/O — and deterministic: conflicts and warnings
//! are sorted by `(node_id, field)` so two runs over equal inputs compare equal.

use std::collections::{BTreeMap, BTreeSet};

use engine_shared::{EventPayload, NodeId};
use serde::{Deserialize, Serialize};
use serde_json::Value;

use crate::integrity::{validate_integrity, IntegrityViolation};
use crate::materializer::{apply_payload, DocumentState};

/// How a single `(node, field)` changed across the two branches relative to the
/// ancestor. The merge auto-applies every class except [`ChangeClass::BothDifferent`],
/// which becomes a [`FourCardConflict`].
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum ChangeClass {
    /// Neither branch touched the field (never enqueued; present for completeness).
    BothAbsent,
    /// Only branch A changed the field.
    OnlyInA,
    /// Only branch B changed the field.
    OnlyInB,
    /// Both branches changed the field to the *same* value.
    BothSame,
    /// Both branches changed the field to *different* values — a conflict.
    BothDifferent,
}

/// A same-field divergence requiring a human decision. The four "cards" are the field
/// identity plus the three values: the common ancestor, branch A's (left), branch B's
/// (right).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct FourCardConflict {
    pub node_id: NodeId,
    pub field: String,
    /// The ancestor value, or `None` if the field did not exist in the ancestor.
    pub ancestor: Option<Value>,
    /// Branch A's value.
    pub left: Value,
    /// Branch B's value.
    pub right: Value,
}

/// The outcome of a three-way merge. The caller (the stage-21 API follow-up) decides
/// what to do, but [`MergeResult::is_blocked`] encodes the doc's rule: a merge is
/// blocked while any conflict is unresolved or any integrity warning stands.
#[derive(Debug, Clone, PartialEq, Default, Serialize, Deserialize)]
pub struct MergeResult {
    /// Events that merge cleanly and can be committed as-is.
    pub auto_merged_events: Vec<EventPayload>,
    /// Same-field collisions awaiting a four-card decision.
    pub pending_conflicts: Vec<FourCardConflict>,
    /// Integrity violations the proposed merged state would carry (computed over
    /// ancestor + `auto_merged_events`).
    pub integrity_warnings: Vec<IntegrityViolation>,
}

impl MergeResult {
    /// A merge is blocked while there is any unresolved conflict or integrity warning.
    pub fn is_blocked(&self) -> bool {
        !self.pending_conflicts.is_empty() || !self.integrity_warnings.is_empty()
    }
}

/// Compute the three-way merge of `branch_a` and `branch_b` over the common `ancestor`.
///
/// Structural limitation (PoC): field-level edits ([`EventPayload::FieldEdited`]) and
/// whole-node additions ([`EventPayload::NodeCreated`]) present in only one branch are
/// merged precisely. Other structural ops (moves, deletes, choice add/remove) are
/// carried through additively from each branch — concurrent *structural* divergence on
/// the same node is not split into a four-card decision in this PoC. No change is ever
/// silently dropped; field-level conflicts are surfaced and the post-merge integrity
/// pass catches semantic breakage regardless of op type.
pub fn three_way_merge(
    ancestor: &DocumentState,
    branch_a: &[EventPayload],
    branch_b: &[EventPayload],
) -> MergeResult {
    let a = BranchDelta::collect(ancestor, branch_a);
    let b = BranchDelta::collect(ancestor, branch_b);

    let mut auto_merged_events: Vec<EventPayload> = Vec::new();
    let mut pending_conflicts: Vec<FourCardConflict> = Vec::new();

    // --- Field edits: classify each touched (node, field) across both branches. ---
    let touched: BTreeSet<&(NodeId, String)> =
        a.field_edits.keys().chain(b.field_edits.keys()).collect();
    for key in touched {
        let in_a = a.field_edits.get(key);
        let in_b = b.field_edits.get(key);
        match (in_a, in_b) {
            (Some(va), None) => {
                auto_merged_events.push(field_edited(key, va.clone()));
            }
            (None, Some(vb)) => {
                auto_merged_events.push(field_edited(key, vb.clone()));
            }
            (Some(va), Some(vb)) if va == vb => {
                // BothSame: identical edit, apply once.
                auto_merged_events.push(field_edited(key, va.clone()));
            }
            (Some(va), Some(vb)) => {
                // BothDifferent: a four-card conflict.
                let ancestor_value = ancestor
                    .nodes
                    .get(&key.0)
                    .and_then(|n| n.current_fields.get(&key.1).cloned());
                pending_conflicts.push(FourCardConflict {
                    node_id: key.0.clone(),
                    field: key.1.clone(),
                    ancestor: ancestor_value,
                    left: va.clone(),
                    right: vb.clone(),
                });
            }
            (None, None) => {}
        }
    }

    // --- Whole-node creations: a node added in only one branch auto-merges. A node
    //     created in both branches keeps A's create (B's is a benign duplicate of the
    //     same stable id); its field edits are still classified above. ---
    let mut creates: BTreeMap<&NodeId, &EventPayload> = BTreeMap::new();
    for (id, payload) in a.created.iter().chain(b.created.iter()) {
        creates.entry(id).or_insert(payload);
    }
    for payload in creates.values() {
        auto_merged_events.push((*payload).clone());
    }

    // --- Other structural ops (move / delete / choice / comment / suggestion): carry
    //     each branch's through additively in branch order, A before B. ---
    for payload in a.structural.iter().chain(b.structural.iter()) {
        auto_merged_events.push(payload.clone());
    }

    // Deterministic ordering: creates first (so later edits land on live nodes), then
    // field edits, then structural ops. Within each, stable by node/field.
    sort_events(&mut auto_merged_events);
    pending_conflicts.sort_by(|x, y| (&x.node_id, &x.field).cmp(&(&y.node_id, &y.field)));

    // --- Phase 4: run integrity over the proposed merged state. ---
    let integrity_warnings = integrity_of_merged(ancestor, &auto_merged_events);

    MergeResult {
        auto_merged_events,
        pending_conflicts,
        integrity_warnings,
    }
}

/// The per-`(node, field)` and structural changes a branch makes relative to ancestor.
struct BranchDelta {
    /// Last-write value for each `(node, field)` whose value differs from ancestor.
    field_edits: BTreeMap<(NodeId, String), Value>,
    /// Nodes created in this branch (not present in ancestor), keyed by id.
    created: BTreeMap<NodeId, EventPayload>,
    /// Structural ops carried through verbatim (moves, deletes, choices, comments,
    /// suggestions, deploys).
    structural: Vec<EventPayload>,
}

impl BranchDelta {
    fn collect(ancestor: &DocumentState, branch: &[EventPayload]) -> Self {
        let mut field_edits: BTreeMap<(NodeId, String), Value> = BTreeMap::new();
        let mut created: BTreeMap<NodeId, EventPayload> = BTreeMap::new();
        let mut structural: Vec<EventPayload> = Vec::new();
        let mut current = ancestor.clone();

        for payload in branch {
            // Compact canonical events carry the same field semantics as legacy
            // full values. Expand against this branch's own sequential base.
            let expanded;
            let payload = if let EventPayload::RichTextPatched { node_id, .. } = payload {
                match apply_payload(&mut current, payload) {
                    Ok(()) => {
                        expanded = EventPayload::FieldEdited {
                            node_id: node_id.clone(),
                            field: "content".into(),
                            value: current.nodes[node_id].current_fields["content"].clone(),
                        };
                        &expanded
                    }
                    Err(_) => {
                        structural.push(payload.clone());
                        continue;
                    }
                }
            } else {
                let _ = apply_payload(&mut current, payload);
                payload
            };
            match payload {
                EventPayload::FieldEdited {
                    node_id,
                    field,
                    value,
                } => {
                    let key = (node_id.clone(), field.clone());
                    let ancestor_value = ancestor
                        .nodes
                        .get(node_id)
                        .and_then(|n| n.current_fields.get(field));
                    // Only record an actual change vs ancestor; a no-op edit (writing
                    // back the ancestor value) is not a change to classify.
                    if ancestor_value != Some(value) {
                        field_edits.insert(key, value.clone());
                    } else {
                        field_edits.remove(&key);
                    }
                }
                EventPayload::NodeCreated { node_id, .. } => {
                    if !ancestor.nodes.contains_key(node_id) {
                        created.insert(node_id.clone(), payload.clone());
                    } else {
                        structural.push(payload.clone());
                    }
                }
                other => structural.push(other.clone()),
            }
        }

        BranchDelta {
            field_edits,
            created,
            structural,
        }
    }
}

fn field_edited(key: &(NodeId, String), value: Value) -> EventPayload {
    EventPayload::FieldEdited {
        node_id: key.0.clone(),
        field: key.1.clone(),
        value,
    }
}

/// Order auto-merged events so they apply cleanly: `NodeCreated` first, then
/// `FieldEdited` (by node then field), then everything else, each group deterministic.
fn sort_events(events: &mut [EventPayload]) {
    fn key(p: &EventPayload) -> (u8, String, String) {
        match p {
            EventPayload::NodeCreated { node_id, .. } => (0, node_id.0.clone(), String::new()),
            EventPayload::FieldEdited { node_id, field, .. } => {
                (1, node_id.0.clone(), field.clone())
            }
            other => (
                2,
                other
                    .target_node_id()
                    .map(|n| n.0.clone())
                    .unwrap_or_default(),
                String::new(),
            ),
        }
    }
    events.sort_by_key(key);
}

/// Build the proposed merged state (ancestor + auto-merged events) and run the
/// referential-integrity pass on it. Events that fail to apply (e.g. an edit to a node
/// neither branch created and ancestor lacks) are skipped rather than panicking; they
/// cannot contribute to a valid merged state.
fn integrity_of_merged(
    ancestor: &DocumentState,
    auto_merged_events: &[EventPayload],
) -> Vec<IntegrityViolation> {
    let mut merged = ancestor.clone();
    let mut failures = Vec::new();
    for payload in auto_merged_events {
        if let Err(error) = apply_payload(&mut merged, payload) {
            failures.push(IntegrityViolation::InvalidOperation {
                reason: error.to_string(),
            });
        }
    }
    failures.extend(validate_integrity(&merged));
    failures
}

#[cfg(test)]
mod tests {
    use std::collections::{BTreeMap, HashSet};

    use engine_shared::NodeType;
    use serde_json::json;

    use super::*;
    use crate::materializer::MaterializedNode;

    fn item(id: &str, var: Option<&str>, fields: Value) -> MaterializedNode {
        MaterializedNode {
            id: NodeId(id.into()),
            parent_id: None,
            node_type: "item".into(),
            pos: "a0".into(),
            current_fields: fields,
            var_name: var.map(str::to_string),
            deleted: false,
        }
    }

    fn ancestor_of(nodes: Vec<MaterializedNode>) -> DocumentState {
        let mut map = BTreeMap::new();
        for n in nodes {
            map.insert(n.id.clone(), n);
        }
        DocumentState {
            nodes: map,
            comments: Vec::new(),
            suggestions: BTreeMap::new(),
            removed_choices: HashSet::new(),
        }
    }

    fn edit(node: &str, field: &str, value: Value) -> EventPayload {
        EventPayload::FieldEdited {
            node_id: NodeId(node.into()),
            field: field.into(),
            value,
        }
    }

    /// A var rename is modeled as a redefining `NodeCreated` on the same stable id with
    /// a new `var_name` (the materializer's `var_name` lives on the node, not in
    /// `current_fields`, so it is not a `FieldEdited`).
    fn rename_var(id: &str, fields: Value, new_var: &str) -> EventPayload {
        EventPayload::NodeCreated {
            node_id: NodeId(id.into()),
            node_type: NodeType::Item,
            parent_id: None,
            pos: "a0".into(),
            fields,
            var_name: Some(new_var.into()),
        }
    }

    #[test]
    fn different_fields_same_node_both_auto_merge() {
        // Doc test (d): A edits prompt of item 7, B edits constraint of item 7.
        let ancestor = ancestor_of(vec![item(
            "07",
            Some("q7"),
            json!({ "prompt": "old", "constraint": ". > 0" }),
        )]);
        let a = vec![edit("07", "prompt", json!("new prompt"))];
        let b = vec![edit("07", "constraint", json!(". > 5"))];
        let result = three_way_merge(&ancestor, &a, &b);
        assert!(
            result.pending_conflicts.is_empty(),
            "different fields don't conflict"
        );
        assert_eq!(result.auto_merged_events.len(), 2);
        assert!(!result.is_blocked());
    }

    #[test]
    fn same_field_divergence_is_a_four_card_conflict() {
        // Doc test (e): A edits prompt -> "Foo", B edits prompt -> "Bar".
        let ancestor = ancestor_of(vec![item("07", Some("q7"), json!({ "prompt": "old" }))]);
        let a = vec![edit("07", "prompt", json!("Foo"))];
        let b = vec![edit("07", "prompt", json!("Bar"))];
        let result = three_way_merge(&ancestor, &a, &b);
        assert_eq!(result.pending_conflicts.len(), 1);
        let c = &result.pending_conflicts[0];
        assert_eq!(c.node_id, NodeId("07".into()));
        assert_eq!(c.field, "prompt");
        assert_eq!(c.ancestor, Some(json!("old")));
        assert_eq!(c.left, json!("Foo"));
        assert_eq!(c.right, json!("Bar"));
        assert!(result.is_blocked());
        // The conflicting field is NOT auto-applied.
        assert!(result.auto_merged_events.is_empty());
    }

    #[test]
    fn both_same_edit_merges_once_without_conflict() {
        let ancestor = ancestor_of(vec![item("07", Some("q7"), json!({ "prompt": "old" }))]);
        let a = vec![edit("07", "prompt", json!("same"))];
        let b = vec![edit("07", "prompt", json!("same"))];
        let result = three_way_merge(&ancestor, &a, &b);
        assert!(result.pending_conflicts.is_empty());
        assert_eq!(result.auto_merged_events.len(), 1);
        assert!(!result.is_blocked());
    }

    #[test]
    fn only_in_one_branch_auto_merges() {
        let ancestor = ancestor_of(vec![item("07", Some("q7"), json!({ "prompt": "old" }))]);
        let a = vec![edit("07", "prompt", json!("A only"))];
        let b: Vec<EventPayload> = Vec::new();
        let result = three_way_merge(&ancestor, &a, &b);
        assert!(result.pending_conflicts.is_empty());
        assert_eq!(
            result.auto_merged_events,
            vec![edit("07", "prompt", json!("A only"))]
        );
        assert!(!result.is_blocked());
    }

    #[test]
    fn new_node_in_only_one_branch_auto_merges() {
        let ancestor = ancestor_of(vec![item("01", Some("a"), json!({ "label": "A" }))]);
        let create = EventPayload::NodeCreated {
            node_id: NodeId("02".into()),
            node_type: NodeType::Item,
            parent_id: None,
            pos: "a1".into(),
            fields: json!({ "label": "B" }),
            var_name: Some("b".into()),
        };
        let a = vec![create.clone()];
        let b: Vec<EventPayload> = Vec::new();
        let result = three_way_merge(&ancestor, &a, &b);
        assert!(result.pending_conflicts.is_empty());
        assert_eq!(result.auto_merged_events, vec![create]);
        assert!(!result.is_blocked());
    }

    #[test]
    fn rename_breaking_a_reference_is_caught_by_integrity() {
        // Doc integrity test: rename a referenced varName -> dangling ref. A renames
        // item-01's var from "x" to "y" while item-02 references ${x}.
        let ancestor = ancestor_of(vec![
            item("01", Some("x"), json!({ "label": "X" })),
            item("02", Some("q2"), json!({ "relevance": "${x} > 5" })),
        ]);
        let a = vec![rename_var("01", json!({ "label": "X" }), "y")];
        let b: Vec<EventPayload> = Vec::new();
        let result = three_way_merge(&ancestor, &a, &b);
        assert!(
            result
                .integrity_warnings
                .iter()
                .any(|v| matches!(v, IntegrityViolation::DanglingReference { var_name, .. } if var_name == "x")),
            "rename leaves ${{x}} dangling"
        );
        assert!(result.is_blocked());
    }

    #[test]
    fn cycle_in_merged_state_is_caught() {
        // Doc integrity test: a -> b -> c -> a cycle in relevance. B introduces the
        // edge that closes the cycle; the post-merge integrity pass reports it.
        let ancestor = ancestor_of(vec![
            item("0A", Some("a"), json!({ "relevance": "${b} = 1" })),
            item("0B", Some("b"), json!({ "relevance": "${c} = 1" })),
            item("0C", Some("c"), json!({ "label": "C" })),
        ]);
        let a: Vec<EventPayload> = Vec::new();
        // B closes the cycle: c's relevance now depends on a.
        let b = vec![edit("0C", "relevance", json!("${a} = 1"))];
        let result = three_way_merge(&ancestor, &a, &b);
        assert!(
            result
                .integrity_warnings
                .iter()
                .any(|v| matches!(v, IntegrityViolation::RelevanceCycle { .. })),
            "a->b->c->a cycle detected on merged state"
        );
        assert!(result.is_blocked());
    }

    #[test]
    fn added_relevance_plus_rename_auto_merges_but_blocks_on_integrity() {
        // Doc merge test (c): A adds an item with relevance="${x} > 5"; B renames var
        // x -> y. Naive per-field merge auto-merges both, but the combined state has a
        // dangling ref, so the merge is blocked.
        let ancestor = ancestor_of(vec![item(
            "01",
            Some("x"),
            json!({ "type": "integer", "label": "X" }),
        )]);
        let a = vec![EventPayload::NodeCreated {
            node_id: NodeId("02".into()),
            node_type: NodeType::Item,
            parent_id: None,
            pos: "a1".into(),
            fields: json!({ "relevance": "${x} > 5" }),
            var_name: Some("q2".into()),
        }];
        let b = vec![rename_var(
            "01",
            json!({ "type": "integer", "label": "X" }),
            "y",
        )];
        let result = three_way_merge(&ancestor, &a, &b);
        // Both branches' changes auto-merged (no same-field conflict)...
        assert!(result.pending_conflicts.is_empty());
        // ...but the merged state dangles on ${x}.
        assert!(result
            .integrity_warnings
            .iter()
            .any(|v| matches!(v, IntegrityViolation::DanglingReference { var_name, .. } if var_name == "x")));
        assert!(result.is_blocked());
    }
}
