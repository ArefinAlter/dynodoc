//! Snapshotted fold / state materialization (docs/15).
//!
//! The event log is the system of record; reads use a folded `DocumentState`
//! rebuilt from a snapshot plus a short event tail.

use std::collections::{BTreeMap, HashSet};

use engine_shared::{Event, EventPayload, NodeId, Snapshot};
use serde::{Deserialize, Serialize};
use serde_json::{Map, Value};

/// One node in the materialized current state.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct MaterializedNode {
    pub id: NodeId,
    pub parent_id: Option<NodeId>,
    #[serde(rename = "type")]
    pub node_type: String,
    pub pos: String,
    pub current_fields: Value,
    pub var_name: Option<String>,
    pub deleted: bool,
}

/// A reviewer comment carried in the log but not directly changing tree state.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct CommentState {
    pub node_id: Option<NodeId>,
    pub body: String,
}

/// Suggestion workflow state carried alongside the document tree.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct SuggestionState {
    pub target_node_id: Option<NodeId>,
    pub detail: Value,
    pub accepted: bool,
    pub rejected_reason: Option<String>,
}

/// The folded current state used by the read path.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct DocumentState {
    pub nodes: BTreeMap<NodeId, MaterializedNode>,
    #[serde(default)]
    pub comments: Vec<CommentState>,
    #[serde(default)]
    pub suggestions: BTreeMap<String, SuggestionState>,
    #[serde(default)]
    pub removed_choices: HashSet<NodeId>,
}

/// Errors while folding or restoring materialized state.
#[derive(Debug, thiserror::Error)]
pub enum MaterializeError {
    #[error("event seq {seq} has invalid rich text: {reason}")]
    RichText { seq: i64, reason: String },
    #[error("snapshot state could not be deserialized: {0}")]
    Deserialize(#[from] serde_json::Error),
    #[error("event payload for seq {seq} is invalid: {source}")]
    InvalidPayload {
        seq: i64,
        #[source]
        source: serde_json::Error,
    },
    #[error("event seq {seq} referenced missing node {node_id}")]
    MissingNode { seq: i64, node_id: String },
    #[error("node {node_id} has non-object current_fields at seq {seq}")]
    NonObjectFields { seq: i64, node_id: String },
    #[error("event seq {seq} referenced missing suggestion {suggestion_id}")]
    MissingSuggestion { seq: i64, suggestion_id: String },
}

/// Errors while applying a single [`EventPayload`] to a [`DocumentState`] out of the
/// log context (no `seq`). Used by the merge engine (stage 21) when replaying a
/// branch's payloads onto a cloned ancestor state.
#[derive(Debug, thiserror::Error)]
pub enum ApplyError {
    #[error("invalid rich text: {0}")]
    RichText(String),
    #[error("payload referenced missing node {node_id}")]
    MissingNode { node_id: String },
    #[error("node {node_id} has non-object current_fields")]
    NonObjectFields { node_id: String },
    #[error("payload referenced missing suggestion {suggestion_id}")]
    MissingSuggestion { suggestion_id: String },
}

/// Apply a single semantic [`EventPayload`] to `state` in place, mutating the
/// materialized tree exactly as the log fold would. This is the shared core of
/// [`Materializer::fold`] and the stage-21 merge engine, which replays branch payloads
/// onto a cloned ancestor without minting `Event` rows.
pub fn apply_payload(state: &mut DocumentState, payload: &EventPayload) -> Result<(), ApplyError> {
    match payload {
        EventPayload::RichTextPatched { node_id, patch } => {
            let node = state
                .nodes
                .get_mut(node_id)
                .ok_or_else(|| ApplyError::MissingNode {
                    node_id: node_id.0.clone(),
                })?;
            let content = crate::richtext::apply_patch(&node.current_fields["content"], patch)
                .map_err(|e| ApplyError::RichText(e.to_string()))?;
            let fields =
                node.current_fields
                    .as_object_mut()
                    .ok_or_else(|| ApplyError::NonObjectFields {
                        node_id: node_id.0.clone(),
                    })?;
            fields.insert(
                "label".into(),
                Value::String(crate::richtext::plain_text(&content)),
            );
            fields.insert("content".into(), content);
        }
        EventPayload::NodeCreated {
            node_id,
            node_type,
            parent_id,
            pos,
            fields,
            var_name,
        } => {
            state.nodes.insert(
                node_id.clone(),
                MaterializedNode {
                    id: node_id.clone(),
                    parent_id: parent_id.clone(),
                    node_type: node_type.as_str().to_string(),
                    pos: pos.clone(),
                    current_fields: object_or_value(fields.clone()),
                    var_name: var_name.clone(),
                    deleted: false,
                },
            );
        }
        EventPayload::FieldEdited {
            node_id,
            field,
            value,
        } => {
            let node = state
                .nodes
                .get_mut(node_id)
                .ok_or_else(|| ApplyError::MissingNode {
                    node_id: node_id.0.clone(),
                })?;
            let fields =
                node.current_fields
                    .as_object_mut()
                    .ok_or_else(|| ApplyError::NonObjectFields {
                        node_id: node_id.0.clone(),
                    })?;
            fields.insert(field.clone(), value.clone());
            if field == "var_name" || field == "varName" {
                node.var_name = value.as_str().map(str::to_string);
            }
        }
        EventPayload::NodeMoved {
            node_id,
            new_parent_id,
            new_pos,
        } => {
            let node = state
                .nodes
                .get_mut(node_id)
                .ok_or_else(|| ApplyError::MissingNode {
                    node_id: node_id.0.clone(),
                })?;
            node.parent_id = new_parent_id.clone();
            node.pos = new_pos.clone();
        }
        EventPayload::NodeRestored { node_id } => {
            let node = state
                .nodes
                .get_mut(node_id)
                .ok_or_else(|| ApplyError::MissingNode {
                    node_id: node_id.0.clone(),
                })?;
            node.deleted = false;
            state.removed_choices.remove(node_id);
        }
        EventPayload::NodeDeleted { node_id } => {
            let node = state
                .nodes
                .get_mut(node_id)
                .ok_or_else(|| ApplyError::MissingNode {
                    node_id: node_id.0.clone(),
                })?;
            node.deleted = true;
        }
        EventPayload::ChoiceAdded {
            node_id,
            choice_id,
            fields,
        } => {
            state
                .nodes
                .entry(choice_id.clone())
                .or_insert(MaterializedNode {
                    id: choice_id.clone(),
                    parent_id: Some(node_id.clone()),
                    node_type: "choice".to_string(),
                    pos: choice_id.0.clone(),
                    current_fields: object_or_value(fields.clone()),
                    var_name: None,
                    deleted: false,
                });
            state.removed_choices.remove(choice_id);
        }
        EventPayload::ChoiceRemoved { choice_id, .. } => {
            state.removed_choices.insert(choice_id.clone());
        }
        // Thread topology and lifecycle are projected from canonical event IDs.
        // These operations do not change structural state or its Merkle leaves.
        EventPayload::CommentReplied { .. } | EventPayload::CommentResolved { .. } => {}
        EventPayload::CommentAdded { node_id, body } => {
            state.comments.push(CommentState {
                node_id: node_id.clone(),
                body: body.clone(),
            });
        }
        EventPayload::SuggestionProposed {
            suggestion_id,
            target_node_id,
            detail,
        } => {
            state.suggestions.insert(
                suggestion_id.clone(),
                SuggestionState {
                    target_node_id: target_node_id.clone(),
                    detail: detail.clone(),
                    accepted: false,
                    rejected_reason: None,
                },
            );
        }
        EventPayload::SuggestionAccepted { suggestion_id } => {
            let suggestion = state.suggestions.get_mut(suggestion_id).ok_or_else(|| {
                ApplyError::MissingSuggestion {
                    suggestion_id: suggestion_id.clone(),
                }
            })?;
            suggestion.accepted = true;
            suggestion.rejected_reason = None;
        }
        EventPayload::SuggestionRejected {
            suggestion_id,
            reason,
        } => {
            let suggestion = state.suggestions.get_mut(suggestion_id).ok_or_else(|| {
                ApplyError::MissingSuggestion {
                    suggestion_id: suggestion_id.clone(),
                }
            })?;
            suggestion.accepted = false;
            suggestion.rejected_reason = Some(reason.clone().unwrap_or_else(|| "rejected".into()));
        }
        EventPayload::Deployed { .. } => {}
    }
    Ok(())
}

/// Fold event tails into materialized state.
pub struct Materializer;

impl Materializer {
    /// Build current state from genesis by folding the whole event stream.
    pub fn fold(events: &[Event]) -> Result<DocumentState, MaterializeError> {
        let mut state = DocumentState::default();
        Self::fold_into(&mut state, events)?;
        Ok(state)
    }

    /// Restore a prior snapshot and fold the tail events on top.
    pub fn from_snapshot(
        snapshot: &Snapshot,
        tail: &[Event],
    ) -> Result<DocumentState, MaterializeError> {
        let mut state: DocumentState = serde_json::from_value(snapshot.state.clone())?;
        Self::fold_into(&mut state, tail)?;
        Ok(state)
    }

    fn fold_into(state: &mut DocumentState, events: &[Event]) -> Result<(), MaterializeError> {
        for event in events {
            let payload: EventPayload =
                serde_json::from_value(event.payload.clone()).map_err(|source| {
                    MaterializeError::InvalidPayload {
                        seq: event.seq,
                        source,
                    }
                })?;

            apply_payload(state, &payload).map_err(|err| err.with_seq(event.seq))?;
        }

        Ok(())
    }
}

impl ApplyError {
    /// Lift an out-of-context apply error into a [`MaterializeError`] carrying the
    /// log `seq` (used when this is invoked from the snapshot fold path).
    fn with_seq(self, seq: i64) -> MaterializeError {
        match self {
            ApplyError::RichText(reason) => MaterializeError::RichText { seq, reason },
            ApplyError::MissingNode { node_id } => MaterializeError::MissingNode { seq, node_id },
            ApplyError::NonObjectFields { node_id } => {
                MaterializeError::NonObjectFields { seq, node_id }
            }
            ApplyError::MissingSuggestion { suggestion_id } => {
                MaterializeError::MissingSuggestion { seq, suggestion_id }
            }
        }
    }
}

fn object_or_value(value: Value) -> Value {
    match value {
        Value::Object(_) => value,
        other => Value::Object(
            [("value".to_string(), other)]
                .into_iter()
                .collect::<Map<String, Value>>(),
        ),
    }
}

#[cfg(test)]
mod tests {
    use engine_shared::{
        DocumentId, Event, EventId, EventPayload, IdentityId, NodeId, Snapshot, SnapshotId,
    };
    use proptest::prelude::*;
    use serde_json::json;
    use uuid::Uuid;

    use super::*;

    fn event(seq: i64, payload: EventPayload) -> Event {
        Event {
            id: EventId(format!("01TEST{:08}", seq)),
            document_id: DocumentId(Uuid::nil()),
            seq,
            event_type: payload.event_type().as_str().to_string(),
            actor_id: IdentityId(Uuid::nil()),
            target_node_id: payload.target_node_id().cloned(),
            payload: serde_json::to_value(&payload).unwrap(),
            content_hash: vec![0; 32],
            prev_chain_hash: vec![0; 32],
            chain_hash: vec![0; 32],
            created_at: chrono::Utc::now(),
        }
    }

    fn valid_event_sequences() -> impl Strategy<Value = Vec<Event>> {
        (1usize..8).prop_map(|count| {
            let mut out = Vec::new();
            let mut seq = 1i64;
            for idx in 0..count {
                let node = NodeId(format!("01NODE{idx:08}"));
                out.push(event(
                    seq,
                    EventPayload::NodeCreated {
                        node_id: node.clone(),
                        node_type: engine_shared::NodeType::Item,
                        parent_id: None,
                        pos: format!("a{idx:04}"),
                        fields: json!({ "label": format!("Q{idx}") }),
                        var_name: Some(format!("q_{idx}")),
                    },
                ));
                seq += 1;
                out.push(event(
                    seq,
                    EventPayload::FieldEdited {
                        node_id: node.clone(),
                        field: "hint".to_string(),
                        value: json!(format!("hint-{idx}")),
                    },
                ));
                seq += 1;
                if idx % 2 == 0 {
                    out.push(event(
                        seq,
                        EventPayload::NodeMoved {
                            node_id: node.clone(),
                            new_parent_id: None,
                            new_pos: format!("b{idx:04}"),
                        },
                    ));
                    seq += 1;
                }
                if idx % 3 == 0 {
                    out.push(event(seq, EventPayload::NodeDeleted { node_id: node }));
                    seq += 1;
                }
            }
            out
        })
    }

    proptest! {
        #![proptest_config(ProptestConfig::with_cases(128))]

        #[test]
        fn fold_matches_snapshot_plus_tail(events in valid_event_sequences()) {
            let full = Materializer::fold(&events).unwrap();
            for cut in 0..=events.len() {
                let base = Materializer::fold(&events[..cut]).unwrap();
                let snapshot = Snapshot {
                    id: SnapshotId(Uuid::nil()),
                    document_id: DocumentId(Uuid::nil()),
                    through_seq: events.get(cut.saturating_sub(1)).map(|e| e.seq).unwrap_or(0),
                    state: serde_json::to_value(&base).unwrap(),
                    merkle_root: vec![0; 32],
                    event_chain_hash: vec![0; 32],
                    created_at: chrono::Utc::now(),
                };
                let from_snapshot = Materializer::from_snapshot(&snapshot, &events[cut..]).unwrap();
                prop_assert_eq!(from_snapshot, full.clone());
            }
        }
    }

    // Materialization correctness at scale: a 1000-node snapshot + 500-event tail
    // folds to the right state. The wall-clock perf target (<10 ms, release build) is
    // the deferred `criterion` bench — a hard timing assert in a debug `cargo test`
    // flakes on slow/contended CI runners.
    #[test]
    fn materializes_large_document_from_snapshot_plus_tail() {
        let mut nodes = BTreeMap::new();
        for idx in 0..1000 {
            let node_id = NodeId(format!("01BENCH{idx:08}"));
            nodes.insert(
                node_id.clone(),
                MaterializedNode {
                    id: node_id,
                    parent_id: None,
                    node_type: "item".to_string(),
                    pos: format!("a{idx:04}"),
                    current_fields: json!({ "label": format!("Question {idx}") }),
                    var_name: Some(format!("q_{idx}")),
                    deleted: false,
                },
            );
        }

        let snapshot = Snapshot {
            id: SnapshotId(Uuid::nil()),
            document_id: DocumentId(Uuid::nil()),
            through_seq: 1000,
            state: serde_json::to_value(DocumentState {
                nodes,
                comments: Vec::new(),
                suggestions: BTreeMap::new(),
                removed_choices: HashSet::new(),
            })
            .unwrap(),
            merkle_root: vec![0; 32],
            event_chain_hash: vec![0; 32],
            created_at: chrono::Utc::now(),
        };

        let tail: Vec<Event> = (0..500)
            .map(|idx| {
                event(
                    1001 + idx,
                    EventPayload::FieldEdited {
                        node_id: NodeId(format!("01BENCH{:08}", idx % 1000)),
                        field: "label".to_string(),
                        value: json!(format!("Updated {idx}")),
                    },
                )
            })
            .collect();

        let state = Materializer::from_snapshot(&snapshot, &tail).unwrap();

        assert_eq!(state.nodes.len(), 1000);
        // The tail re-edited node 0's "label"; confirm the fold applied it on top of
        // the snapshot's "Question 0".
        assert_eq!(
            state.nodes[&NodeId("01BENCH00000000".to_string())].current_fields["label"],
            json!("Updated 0")
        );
    }
}
