//! Pure conflict derivation from materialized state (docs/21 item 4, PoC instantiation).
//!
//! The PoC has no CRDT branch source (stage 20 was skipped per
//! `docs/decisions/000-initial.md`). Conflicts are instead **concurrent pending
//! suggestions on the same field**: when two or more *pending* suggestions
//! ([`SuggestionState`] with `!accepted && rejected_reason.is_none()`) each wrap a
//! `FieldEdited` targeting the same `(node_id, field)` but propose *distinct* values,
//! that is a four-card conflict for the author to resolve.
//!
//! This is the PoC instantiation of the merge engine's `BothDifferent` classification
//! (see [`engine_core::merge::FourCardConflict`]), generalized to *N* proposals;
//! `engine_core::merge::three_way_merge` remains the pure 2-branch primitive for the
//! engine. Conflicts are NOT stored — they are recomputed from current state on every
//! `/conflicts` read and every `/resolve` call, so a `conflict_id` always re-derives the
//! same live conflict (or 404s once it is gone).
//!
//! `conflict_id` is a deterministic, opaque URL-safe-base64 encoding of
//! `"{node_id}\u{1f}{field}"`, so the same `(node_id, field)` always yields the same id
//! and `resolve` can round-trip it back to the group it must act on.

use std::collections::BTreeMap;

use base64::engine::general_purpose::URL_SAFE_NO_PAD;
use base64::Engine as _;
use engine_core::materializer::DocumentState;
use serde::Serialize;
use serde_json::Value;
use utoipa::ToSchema;
use uuid::Uuid;

/// A single proposed value in a four-card conflict, attributable to the pending
/// suggestion (and, when the route enriches it, the actor) that proposed it.
#[derive(Debug, Clone, PartialEq, Serialize, ToSchema)]
pub struct ConflictOption {
    /// The pending suggestion this option came from (the id `resolve` accepts).
    pub suggestion_id: String,
    /// The identity that proposed the suggestion, when known. Pure derivation from
    /// state cannot supply this (the materializer does not carry the proposer), so the
    /// derive helper leaves it `None` and the GET route fills it from the event log.
    #[serde(skip_serializing_if = "Option::is_none")]
    #[schema(value_type = Option<String>)]
    pub actor: Option<Uuid>,
    /// The value this suggestion would write to the field.
    pub value: Value,
}

/// One four-card conflict: a `(node_id, field)` whose pending suggestions disagree.
#[derive(Debug, Clone, PartialEq, Serialize, ToSchema)]
pub struct ConflictView {
    /// Opaque, deterministic encoding of `(node_id, field)` (see module docs).
    pub conflict_id: String,
    pub node_id: String,
    pub field: String,
    /// The current canonical field value (the "Original" card), or `None` if the field
    /// has no value yet.
    pub ancestor: Option<Value>,
    /// The competing proposals (≥2, each a distinct value).
    pub options: Vec<ConflictOption>,
}

/// Encode `(node_id, field)` into the opaque `conflict_id`.
pub fn encode_conflict_id(node_id: &str, field: &str) -> String {
    URL_SAFE_NO_PAD.encode(format!("{node_id}\u{1f}{field}"))
}

/// Decode a `conflict_id` back to `(node_id, field)`, if it is well-formed.
pub fn decode_conflict_id(conflict_id: &str) -> Option<(String, String)> {
    let bytes = URL_SAFE_NO_PAD.decode(conflict_id.as_bytes()).ok()?;
    let text = String::from_utf8(bytes).ok()?;
    let (node_id, field) = text.split_once('\u{1f}')?;
    Some((node_id.to_string(), field.to_string()))
}

/// A pending suggestion wrapping a `FieldEdited`, decoded to its target + value.
struct PendingEdit {
    suggestion_id: String,
    node_id: String,
    field: String,
    value: Value,
}

/// Extract every pending suggestion that wraps a `FieldEdited`, in suggestion-id order
/// (the `BTreeMap` iteration order, which is deterministic).
fn pending_field_edits(state: &DocumentState) -> Vec<PendingEdit> {
    let mut out = Vec::new();
    for (suggestion_id, s) in &state.suggestions {
        if s.accepted || s.rejected_reason.is_some() {
            continue;
        }
        let detail = s.detail.as_object();
        let Some(detail) = detail else { continue };
        if detail.get("type").and_then(Value::as_str) != Some("FieldEdited") {
            continue;
        }
        let (Some(node_id), Some(field), Some(value)) = (
            detail.get("node_id").and_then(Value::as_str),
            detail.get("field").and_then(Value::as_str),
            detail.get("value"),
        ) else {
            continue;
        };
        out.push(PendingEdit {
            suggestion_id: suggestion_id.clone(),
            node_id: node_id.to_string(),
            field: field.to_string(),
            value: value.clone(),
        });
    }
    out
}

/// The current canonical value of `(node_id, field)`, if any.
fn ancestor_value(state: &DocumentState, node_id: &str, field: &str) -> Option<Value> {
    state
        .nodes
        .get(&engine_shared::NodeId(node_id.to_string()))
        .and_then(|n| n.current_fields.get(field))
        .cloned()
}

/// Derive all four-card conflicts from `state`: group pending `FieldEdited` suggestions
/// by `(node_id, field)`; a group whose proposals span ≥2 **distinct** values is a
/// conflict. Options carry no actor (the route enriches that). Deterministic order:
/// conflicts by `(node_id, field)`, options by suggestion id.
pub fn derive_conflicts(state: &DocumentState) -> Vec<ConflictView> {
    // Group pending edits by (node_id, field), preserving suggestion-id order.
    let mut groups: BTreeMap<(String, String), Vec<PendingEdit>> = BTreeMap::new();
    for edit in pending_field_edits(state) {
        groups
            .entry((edit.node_id.clone(), edit.field.clone()))
            .or_default()
            .push(edit);
    }

    let mut conflicts = Vec::new();
    for ((node_id, field), edits) in groups {
        // A conflict requires ≥2 *distinct* proposed values.
        let distinct: std::collections::BTreeSet<String> =
            edits.iter().map(|e| e.value.to_string()).collect();
        if distinct.len() < 2 {
            continue;
        }
        let options = edits
            .into_iter()
            .map(|e| ConflictOption {
                suggestion_id: e.suggestion_id,
                actor: None,
                value: e.value,
            })
            .collect();
        conflicts.push(ConflictView {
            conflict_id: encode_conflict_id(&node_id, &field),
            ancestor: ancestor_value(state, &node_id, &field),
            node_id,
            field,
            options,
        });
    }
    conflicts
}

/// Re-derive the single conflict identified by `conflict_id` from current `state`, or
/// `None` if it no longer maps to a live conflict (resolved, or never was one).
pub fn find_conflict(state: &DocumentState, conflict_id: &str) -> Option<ConflictView> {
    derive_conflicts(state)
        .into_iter()
        .find(|c| c.conflict_id == conflict_id)
}

#[cfg(test)]
mod tests {
    use std::collections::{BTreeMap, HashSet};

    use engine_core::materializer::{DocumentState, MaterializedNode, SuggestionState};
    use engine_shared::NodeId;
    use serde_json::json;

    use super::*;

    fn state_with(
        node_field: Option<(&str, &str, Value)>,
        suggestions: Vec<(&str, Value)>,
    ) -> DocumentState {
        let mut nodes = BTreeMap::new();
        if let Some((id, field, value)) = node_field {
            let mut fields = serde_json::Map::new();
            fields.insert(field.to_string(), value);
            nodes.insert(
                NodeId(id.to_string()),
                MaterializedNode {
                    id: NodeId(id.to_string()),
                    parent_id: None,
                    node_type: "item".into(),
                    pos: "a0".into(),
                    current_fields: Value::Object(fields),
                    var_name: None,
                    deleted: false,
                },
            );
        }
        let mut sug = BTreeMap::new();
        for (sid, detail) in suggestions {
            sug.insert(
                sid.to_string(),
                SuggestionState {
                    target_node_id: None,
                    detail,
                    accepted: false,
                    rejected_reason: None,
                },
            );
        }
        DocumentState {
            nodes,
            comments: Vec::new(),
            suggestions: sug,
            removed_choices: HashSet::new(),
        }
    }

    fn edit(node: &str, field: &str, value: Value) -> Value {
        json!({ "type": "FieldEdited", "node_id": node, "field": field, "value": value })
    }

    #[test]
    fn conflict_id_round_trips() {
        let id = encode_conflict_id("01NODE", "prompt");
        assert_eq!(
            decode_conflict_id(&id),
            Some(("01NODE".to_string(), "prompt".to_string()))
        );
    }

    #[test]
    fn two_distinct_pending_edits_on_same_field_are_one_conflict() {
        let state = state_with(
            Some(("01N", "prompt", json!("old"))),
            vec![
                ("s1", edit("01N", "prompt", json!("Foo"))),
                ("s2", edit("01N", "prompt", json!("Bar"))),
            ],
        );
        let conflicts = derive_conflicts(&state);
        assert_eq!(conflicts.len(), 1);
        let c = &conflicts[0];
        assert_eq!(c.node_id, "01N");
        assert_eq!(c.field, "prompt");
        assert_eq!(c.ancestor, Some(json!("old")));
        assert_eq!(c.options.len(), 2);
        let values: Vec<&Value> = c.options.iter().map(|o| &o.value).collect();
        assert!(values.contains(&&json!("Foo")));
        assert!(values.contains(&&json!("Bar")));
        // The id re-derives the same conflict.
        assert_eq!(find_conflict(&state, &c.conflict_id).unwrap(), *c);
    }

    #[test]
    fn single_pending_edit_is_not_a_conflict() {
        let state = state_with(
            Some(("01N", "prompt", json!("old"))),
            vec![("s1", edit("01N", "prompt", json!("Foo")))],
        );
        assert!(derive_conflicts(&state).is_empty());
    }

    #[test]
    fn same_value_from_two_suggestions_is_not_a_conflict() {
        let state = state_with(
            Some(("01N", "prompt", json!("old"))),
            vec![
                ("s1", edit("01N", "prompt", json!("Foo"))),
                ("s2", edit("01N", "prompt", json!("Foo"))),
            ],
        );
        assert!(derive_conflicts(&state).is_empty());
    }

    #[test]
    fn different_fields_do_not_conflict() {
        let state = state_with(
            None,
            vec![
                ("s1", edit("01N", "prompt", json!("Foo"))),
                ("s2", edit("01N", "hint", json!("Bar"))),
            ],
        );
        assert!(derive_conflicts(&state).is_empty());
    }

    #[test]
    fn unknown_conflict_id_finds_nothing() {
        let state = state_with(None, vec![]);
        assert!(find_conflict(&state, "not-a-real-id").is_none());
    }
}
