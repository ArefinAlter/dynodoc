//! The semantic event vocabulary as a typed, internally-tagged payload enum.
//!
//! Every change to an instrument is one [`EventPayload`] — a typed semantic op on a
//! stable-identity node (docs/04 §D.1, docs/14). The variant set mirrors the
//! `CHECK` on `event.type` (0002_events.sql) and the [`crate::EventType`] enum: the
//! 11 Research-IDE PoC ops. Consultation-layer events (`VoteCast`,
//! `ModerationDecided`, `ConsultationOpened/Closed`) are out of PoC scope and are not
//! defined here.
//!
//! Serialized with serde's **internally-tagged** representation (`{"type": "...",
//! …fields}`), so the discriminant travels inside the stored JSON payload and the
//! enum round-trips through the `event.payload` JSONB column. The matching
//! `event.type` text column is set from [`EventPayload::event_type`] at append time.
//!
//! These are the on-the-wire shapes only. The operation *handlers* — validation,
//! application to node state, and the governance gate — land in stage 16; keep field
//! sets minimal here so that work is not pre-empted.

use serde::{Deserialize, Serialize};
use serde_json::Value;

use crate::enums::{EventType, NodeType};
use crate::ids::{NodeId, SnapshotId};

/// A typed semantic event payload. The `type` tag is carried inside the JSON.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "type")]
pub enum EventPayload {
    /// A new node was created in the instrument tree.
    NodeCreated {
        node_id: NodeId,
        node_type: NodeType,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        parent_id: Option<NodeId>,
        /// Dense order key (fractional index) among siblings.
        pos: String,
        #[serde(default)]
        fields: Value,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        var_name: Option<String>,
    },
    /// A node was re-parented and/or re-ordered.
    NodeMoved {
        node_id: NodeId,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        new_parent_id: Option<NodeId>,
        new_pos: String,
    },
    /// A node was tombstoned (nodes are never row-deleted).
    NodeDeleted { node_id: NodeId },
    /// Restore a tombstone using its original stable identity.
    NodeRestored { node_id: NodeId },
    /// A single field of a node's `current_fields` was edited.
    FieldEdited {
        node_id: NodeId,
        field: String,
        value: Value,
    },
    /// Compact, base-checked changes to a node's rich-text `content` field.
    RichTextPatched {
        node_id: NodeId,
        patch: crate::richtext::RichTextPatch,
    },
    /// A choice was added under a (select-type) item.
    ChoiceAdded {
        node_id: NodeId,
        choice_id: NodeId,
        #[serde(default)]
        fields: Value,
    },
    /// A choice was removed from an item.
    ChoiceRemoved { node_id: NodeId, choice_id: NodeId },
    /// A comment was added, optionally anchored to a node.
    CommentAdded {
        #[serde(default, skip_serializing_if = "Option::is_none")]
        node_id: Option<NodeId>,
        body: String,
    },
    /// Reply to a canonical CommentAdded event in this document.
    CommentReplied { thread_id: String, body: String },
    /// Resolve or reopen a canonical discussion; previous messages remain intact.
    CommentResolved { thread_id: String, resolved: bool },
    /// A reviewer proposed a change without applying it (the suggesting workflow).
    SuggestionProposed {
        suggestion_id: String,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        target_node_id: Option<NodeId>,
        #[serde(default)]
        detail: Value,
    },
    /// An author accepted a pending suggestion, applying its change.
    SuggestionAccepted { suggestion_id: String },
    /// An author rejected a pending suggestion, with an optional recorded reason.
    SuggestionRejected {
        suggestion_id: String,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        reason: Option<String>,
    },
    /// The draft passed the integrity gate and was deployed, pinning a snapshot.
    Deployed {
        #[serde(default, skip_serializing_if = "Option::is_none")]
        snapshot_id: Option<SnapshotId>,
    },
}

impl EventPayload {
    /// The [`EventType`] discriminant for this payload — the value stored in the
    /// `event.type` column.
    pub const fn event_type(&self) -> EventType {
        match self {
            EventPayload::NodeCreated { .. } => EventType::NodeCreated,
            EventPayload::NodeMoved { .. } => EventType::NodeMoved,
            EventPayload::NodeDeleted { .. } => EventType::NodeDeleted,
            EventPayload::NodeRestored { .. } => EventType::NodeRestored,
            EventPayload::FieldEdited { .. } => EventType::FieldEdited,
            EventPayload::RichTextPatched { .. } => EventType::RichTextPatched,
            EventPayload::ChoiceAdded { .. } => EventType::ChoiceAdded,
            EventPayload::ChoiceRemoved { .. } => EventType::ChoiceRemoved,
            EventPayload::CommentAdded { .. } => EventType::CommentAdded,
            EventPayload::CommentReplied { .. } => EventType::CommentReplied,
            EventPayload::CommentResolved { .. } => EventType::CommentResolved,
            EventPayload::SuggestionProposed { .. } => EventType::SuggestionProposed,
            EventPayload::SuggestionAccepted { .. } => EventType::SuggestionAccepted,
            EventPayload::SuggestionRejected { .. } => EventType::SuggestionRejected,
            EventPayload::Deployed { .. } => EventType::Deployed,
        }
    }

    /// The node this event targets, if any — stored in the `event.target_node_id`
    /// column. Document-level events (`Deployed`, an unanchored `CommentAdded`)
    /// return `None`.
    pub fn target_node_id(&self) -> Option<&NodeId> {
        match self {
            EventPayload::NodeCreated { node_id, .. }
            | EventPayload::NodeMoved { node_id, .. }
            | EventPayload::NodeDeleted { node_id }
            | EventPayload::NodeRestored { node_id }
            | EventPayload::FieldEdited { node_id, .. }
            | EventPayload::RichTextPatched { node_id, .. }
            | EventPayload::ChoiceAdded { node_id, .. }
            | EventPayload::ChoiceRemoved { node_id, .. } => Some(node_id),
            EventPayload::CommentAdded { node_id, .. } => node_id.as_ref(),
            EventPayload::SuggestionProposed { target_node_id, .. } => target_node_id.as_ref(),
            EventPayload::SuggestionAccepted { .. }
            | EventPayload::CommentReplied { .. }
            | EventPayload::CommentResolved { .. }
            | EventPayload::SuggestionRejected { .. }
            | EventPayload::Deployed { .. } => None,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn tag_matches_event_type_and_target() {
        let p = EventPayload::FieldEdited {
            node_id: NodeId("01ABC".into()),
            field: "label".into(),
            value: Value::String("How many?".into()),
        };
        assert_eq!(p.event_type(), EventType::FieldEdited);
        assert_eq!(p.target_node_id(), Some(&NodeId("01ABC".into())));

        // Internally-tagged: the discriminant is inside the JSON object.
        let json = serde_json::to_value(&p).unwrap();
        assert_eq!(json["type"], "FieldEdited");
        assert_eq!(json["field"], "label");

        // Round-trips back to the same payload.
        let back: EventPayload = serde_json::from_value(json).unwrap();
        assert_eq!(back, p);
    }

    #[test]
    fn document_level_events_have_no_target() {
        assert_eq!(
            EventPayload::Deployed { snapshot_id: None }.target_node_id(),
            None
        );
        assert_eq!(
            EventPayload::SuggestionAccepted {
                suggestion_id: "s1".into()
            }
            .event_type(),
            EventType::SuggestionAccepted
        );
    }
}
