//! Semantic operation handlers (docs/16 part 1).
//!
//! Each [`EventPayload`] variant is a typed semantic op on the stable-identity node
//! tree. Before an op may be appended to the log it must be *valid against the current
//! materialized state* — the node it targets exists, the move would not create a
//! cycle, a new `var_name` is unique, and so on. [`validate_op`] is that gate: a pure
//! function of `(&DocumentState, &EventPayload)` returning `Ok(())` or a typed
//! [`OpError`]. It performs **no I/O** and never mutates state, so it is exhaustively
//! unit-testable without a database.
//!
//! Validation runs *before* the append (docs/16 "Don't let validation run after the
//! append"): once an event is in the log it is immutable, so an invalid op must be
//! refused, never compensated. The capability check and the `Deployed` integrity gate
//! wrap this in [`crate::governance`].

use engine_shared::{EventPayload, NodeId, NodeType};

use crate::materializer::{DocumentState, MaterializedNode};

/// Why a semantic operation is not valid against the current state.
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum OpError {
    /// The op targets a node that does not exist in the current state.
    #[error("node {0} does not exist")]
    NodeNotFound(String),
    /// The op targets a tombstoned node (deleted nodes are immutable).
    #[error("node {0} is deleted")]
    NodeDeleted(String),
    #[error("move or remove the children of node {0} before removing it")]
    NodeHasChildren(String),
    /// A `NodeCreated`/`ChoiceAdded` reused an id already present in the tree.
    #[error("node {0} already exists")]
    NodeExists(String),
    /// The named parent does not exist.
    #[error("parent {0} does not exist")]
    ParentNotFound(String),
    /// A `parent_type` may not contain a `child_type` (e.g. a choice cannot hold a section).
    #[error("a {parent} node cannot contain a {child} node")]
    IllegalContainment { parent: String, child: String },
    /// A non-root node had no parent, or a root node had the wrong type.
    #[error("invalid root: a {0} node cannot be a document root")]
    InvalidRoot(String),
    /// The `var_name` is already used by another live node in this document.
    #[error("var_name {0:?} is already used by another node")]
    DuplicateVarName(String),
    /// A `NodeMoved` would place a node inside its own subtree (a cycle).
    #[error("moving node {node} under {new_parent} would create a cycle")]
    MoveCreatesCycle { node: String, new_parent: String },
    /// `ChoiceAdded`/`ChoiceRemoved` targeted a node that cannot hold choices.
    #[error("node {0} is not a choice-bearing item")]
    NotChoiceBearing(String),
    /// `ChoiceRemoved` named a choice that is not present under the item.
    #[error("choice {0} does not exist")]
    ChoiceNotFound(String),
    /// A suggestion id collided with an existing proposal.
    #[error("suggestion {0} already exists")]
    SuggestionExists(String),
    /// `SuggestionAccepted`/`SuggestionRejected` named a proposal that does not exist.
    #[error("suggestion {0} does not exist")]
    SuggestionNotFound(String),
    /// The named suggestion is already resolved (accepted or rejected).
    #[error("suggestion {0} is not pending")]
    SuggestionNotPending(String),
    /// A `SuggestionProposed.detail` did not decode to a valid wrapped content op.
    #[error("suggestion wraps an invalid operation: {0}")]
    InvalidWrappedOp(String),
    #[error("invalid discussion: {0}")]
    InvalidDiscussion(String),
}

/// Validate one operation against the current materialized state.
///
/// `Ok(())` means the op's preconditions hold and it is safe to hash and append.
/// Validation is total over [`EventPayload`]: every variant is handled.
pub fn validate_op(state: &DocumentState, op: &EventPayload) -> Result<(), OpError> {
    match op {
        EventPayload::RichTextPatched { node_id, patch } => {
            let node = require_live(state, node_id)?;
            crate::richtext::apply_patch(&node.current_fields["content"], patch)
                .map(|_| ())
                .map_err(|e| OpError::InvalidWrappedOp(e.to_string()))
        }
        EventPayload::NodeCreated {
            node_id,
            node_type,
            parent_id,
            var_name,
            ..
        } => validate_node_created(
            state,
            node_id,
            *node_type,
            parent_id.as_ref(),
            var_name.as_deref(),
        ),

        EventPayload::FieldEdited {
            node_id,
            field,
            value,
        } => {
            require_live(state, node_id)?;
            if field == "var_name" || field == "varName" {
                let name = value.as_str().ok_or_else(|| {
                    OpError::InvalidWrappedOp("Variable name must be text".into())
                })?;
                if var_name_taken(state, name, Some(node_id)) {
                    return Err(OpError::DuplicateVarName(name.into()));
                }
            }
            Ok(())
        }

        EventPayload::NodeMoved {
            node_id,
            new_parent_id,
            ..
        } => validate_node_moved(state, node_id, new_parent_id.as_ref()),

        EventPayload::NodeRestored { node_id } => {
            let node = require_present(state, node_id)?;
            if let Some(parent) = &node.parent_id {
                require_live(state, parent)?;
            }
            if let Some(name) = node.var_name.as_deref().filter(|n| !n.is_empty()) {
                if var_name_taken(state, name, Some(node_id)) {
                    return Err(OpError::DuplicateVarName(name.into()));
                }
            }
            Ok(())
        }
        EventPayload::NodeDeleted { node_id } => {
            if state.nodes.values().any(|n| {
                n.parent_id.as_ref() == Some(node_id)
                    && !n.deleted
                    && !state.removed_choices.contains(&n.id)
            }) {
                return Err(OpError::NodeHasChildren(node_id.0.clone()));
            }
            // Deleting an already-deleted node is a no-op error; require it live.
            require_live(state, node_id)?;
            Ok(())
        }

        EventPayload::ChoiceAdded {
            node_id, choice_id, ..
        } => {
            let parent = require_live(state, node_id)?;
            require_choice_bearing(parent)?;
            if state.nodes.contains_key(choice_id) {
                return Err(OpError::NodeExists(choice_id.0.clone()));
            }
            Ok(())
        }

        EventPayload::ChoiceRemoved { node_id, choice_id } => {
            require_live(state, node_id)?;
            let choice = state
                .nodes
                .get(choice_id)
                .ok_or_else(|| OpError::ChoiceNotFound(choice_id.0.clone()))?;
            if choice.parent_id.as_ref() != Some(node_id) {
                return Err(OpError::ChoiceNotFound(choice_id.0.clone()));
            }
            Ok(())
        }

        EventPayload::CommentReplied { thread_id, body } => {
            if thread_id.len() != 26 || body.trim().is_empty() || body.len() > 10_000 {
                return Err(OpError::InvalidDiscussion(
                    "Reply must contain 1–10,000 bytes and a valid thread ID".into(),
                ));
            }
            Ok(())
        }
        EventPayload::CommentResolved { thread_id, .. } => {
            if thread_id.len() != 26 {
                return Err(OpError::InvalidDiscussion("Invalid thread ID".into()));
            }
            Ok(())
        }
        EventPayload::CommentAdded { node_id, body } => {
            if body.trim().is_empty() || body.len() > 10_000 {
                return Err(OpError::InvalidDiscussion(
                    "Comment must contain 1–10,000 bytes".into(),
                ));
            }
            if let Some(node_id) = node_id {
                require_present(state, node_id)?;
            }
            Ok(())
        }

        EventPayload::SuggestionProposed {
            suggestion_id,
            detail,
            ..
        } => {
            if state.suggestions.contains_key(suggestion_id) {
                return Err(OpError::SuggestionExists(suggestion_id.clone()));
            }
            // The wrapped op must itself be a valid content op against current state.
            let wrapped = decode_wrapped_op(detail)?;
            validate_op(state, &wrapped)
        }

        EventPayload::SuggestionAccepted { suggestion_id }
        | EventPayload::SuggestionRejected { suggestion_id, .. } => {
            require_pending(state, suggestion_id)
        }

        // `Deployed` has no structural precondition here; the referential-integrity
        // gate (docs/16 part 4) is applied in `governance::authorize`.
        EventPayload::Deployed { .. } => Ok(()),
    }
}

/// Decode a `SuggestionProposed.detail` into the content op it wraps. A proposal's
/// `detail` is the internally-tagged JSON of the wrapped [`EventPayload`] (docs/16
/// "proposal lifecycle"). A proposal may not wrap another proposal, an accept/reject,
/// or a deploy — only a content change.
pub fn decode_wrapped_op(detail: &serde_json::Value) -> Result<EventPayload, OpError> {
    let op: EventPayload = serde_json::from_value(detail.clone())
        .map_err(|err| OpError::InvalidWrappedOp(err.to_string()))?;
    if !is_content_op(&op) {
        return Err(OpError::InvalidWrappedOp(format!(
            "{} cannot be proposed as a suggestion",
            op.event_type().as_str()
        )));
    }
    Ok(op)
}

/// Whether `op` changes document content (and so may be wrapped in a suggestion).
/// Suggestion-lifecycle and deploy events are governance acts, not content.
fn is_content_op(op: &EventPayload) -> bool {
    matches!(
        op,
        EventPayload::NodeCreated { .. }
            | EventPayload::NodeMoved { .. }
            | EventPayload::NodeDeleted { .. }
            | EventPayload::NodeRestored { .. }
            | EventPayload::FieldEdited { .. }
            | EventPayload::ChoiceAdded { .. }
            | EventPayload::ChoiceRemoved { .. }
            | EventPayload::CommentAdded { .. }
    )
}

fn validate_node_created(
    state: &DocumentState,
    node_id: &NodeId,
    node_type: NodeType,
    parent_id: Option<&NodeId>,
    var_name: Option<&str>,
) -> Result<(), OpError> {
    if state.nodes.contains_key(node_id) {
        return Err(OpError::NodeExists(node_id.0.clone()));
    }
    match parent_id {
        Some(parent_id) => {
            let parent = state
                .nodes
                .get(parent_id)
                .ok_or_else(|| OpError::ParentNotFound(parent_id.0.clone()))?;
            if parent.deleted {
                return Err(OpError::NodeDeleted(parent_id.0.clone()));
            }
            let parent_type = parse_node_type(&parent.node_type);
            if let Some(parent_type) = parent_type {
                if !can_contain(parent_type, node_type) {
                    return Err(OpError::IllegalContainment {
                        parent: parent_type.as_str().to_string(),
                        child: node_type.as_str().to_string(),
                    });
                }
            }
        }
        // A parentless node is a document root: only Form/Clause may be roots.
        None if !is_root_type(node_type) => {
            return Err(OpError::InvalidRoot(node_type.as_str().to_string()));
        }
        None => {}
    }
    if let Some(var_name) = var_name.filter(|v| !v.is_empty()) {
        if var_name_taken(state, var_name, None) {
            return Err(OpError::DuplicateVarName(var_name.to_string()));
        }
    }
    Ok(())
}

fn validate_node_moved(
    state: &DocumentState,
    node_id: &NodeId,
    new_parent_id: Option<&NodeId>,
) -> Result<(), OpError> {
    let node = require_live(state, node_id)?;
    let Some(new_parent_id) = new_parent_id else {
        // Moving to the root: legal only for a root-eligible type.
        let node_type = parse_node_type(&node.node_type);
        if node_type.is_some_and(|t| !is_root_type(t)) {
            return Err(OpError::InvalidRoot(node.node_type.clone()));
        }
        return Ok(());
    };
    let new_parent = require_live(state, new_parent_id)?;

    // Containment: the new parent must be able to hold this node's type.
    if let (Some(parent_type), Some(child_type)) = (
        parse_node_type(&new_parent.node_type),
        parse_node_type(&node.node_type),
    ) {
        if !can_contain(parent_type, child_type) {
            return Err(OpError::IllegalContainment {
                parent: parent_type.as_str().to_string(),
                child: child_type.as_str().to_string(),
            });
        }
    }

    // Cycle check (docs/16, Vol II §C.2.2): the new parent must not be the node
    // itself or any of its descendants. Walk up from new_parent to the root; if we
    // meet `node_id`, the move would form a cycle.
    if new_parent_id == node_id || is_descendant_of(state, new_parent_id, node_id) {
        return Err(OpError::MoveCreatesCycle {
            node: node_id.0.clone(),
            new_parent: new_parent_id.0.clone(),
        });
    }
    Ok(())
}

/// Is `candidate` inside the subtree rooted at `ancestor`? Walks parent links upward
/// from `candidate`; bounded by the node count so a pre-existing cycle cannot loop.
fn is_descendant_of(state: &DocumentState, candidate: &NodeId, ancestor: &NodeId) -> bool {
    let mut cursor = Some(candidate);
    let mut steps = 0;
    while let Some(current) = cursor {
        if steps > state.nodes.len() {
            return false; // defensive: never spin on a malformed tree
        }
        steps += 1;
        let Some(node) = state.nodes.get(current) else {
            return false;
        };
        match node.parent_id.as_ref() {
            Some(parent) if parent == ancestor => return true,
            Some(parent) => cursor = Some(parent),
            None => return false,
        }
    }
    false
}

// --- small precondition helpers -------------------------------------------------

fn require_present<'a>(
    state: &'a DocumentState,
    node_id: &NodeId,
) -> Result<&'a MaterializedNode, OpError> {
    state
        .nodes
        .get(node_id)
        .ok_or_else(|| OpError::NodeNotFound(node_id.0.clone()))
}

fn require_live<'a>(
    state: &'a DocumentState,
    node_id: &NodeId,
) -> Result<&'a MaterializedNode, OpError> {
    let node = require_present(state, node_id)?;
    if node.deleted {
        return Err(OpError::NodeDeleted(node_id.0.clone()));
    }
    Ok(node)
}

fn require_choice_bearing(node: &MaterializedNode) -> Result<(), OpError> {
    match parse_node_type(&node.node_type) {
        Some(NodeType::Item) => Ok(()),
        _ => Err(OpError::NotChoiceBearing(node.id.0.clone())),
    }
}

fn require_pending(state: &DocumentState, suggestion_id: &str) -> Result<(), OpError> {
    let suggestion = state
        .suggestions
        .get(suggestion_id)
        .ok_or_else(|| OpError::SuggestionNotFound(suggestion_id.to_string()))?;
    // Resolved iff accepted, or rejected (a recorded reason). Pending otherwise.
    if suggestion.accepted || suggestion.rejected_reason.is_some() {
        return Err(OpError::SuggestionNotPending(suggestion_id.to_string()));
    }
    Ok(())
}

/// Is `var_name` already used by a live node other than `exclude`?
fn var_name_taken(state: &DocumentState, var_name: &str, exclude: Option<&NodeId>) -> bool {
    state.nodes.values().any(|node| {
        !node.deleted && Some(&node.id) != exclude && node.var_name.as_deref() == Some(var_name)
    })
}

fn parse_node_type(raw: &str) -> Option<NodeType> {
    raw.parse().ok()
}

/// Containment rules for the instrument tree (docs/04 §D.1). Form/Section organize;
/// Items hold Choices; Clause/Paragraph are the policy-document kinds the engine
/// stays compatible with. A Choice and a Paragraph are leaves — they contain nothing.
fn can_contain(parent: NodeType, child: NodeType) -> bool {
    use NodeType::*;
    matches!(
        (parent, child),
        (Form, Section)
            | (Form, Item)
            | (Section, Section)
            | (Section, Item)
            | (Item, Choice)
            | (Clause, Clause)
            | (Clause, Paragraph)
    )
}

/// Document-root-eligible node types (a parentless node must be one of these).
fn is_root_type(node_type: NodeType) -> bool {
    matches!(node_type, NodeType::Form | NodeType::Clause)
}

#[cfg(test)]
mod tests {
    use std::collections::{BTreeMap, HashSet};

    use engine_shared::NodeType;
    use serde_json::json;

    use super::*;
    use crate::materializer::{DocumentState, SuggestionState};

    fn n(id: &str, ty: NodeType, parent: Option<&str>, var: Option<&str>) -> MaterializedNode {
        MaterializedNode {
            id: NodeId(id.into()),
            parent_id: parent.map(|p| NodeId(p.into())),
            node_type: ty.as_str().into(),
            pos: "a0".into(),
            current_fields: json!({}),
            var_name: var.map(str::to_string),
            deleted: false,
        }
    }

    fn state(nodes: Vec<MaterializedNode>) -> DocumentState {
        let mut map = BTreeMap::new();
        for node in nodes {
            map.insert(node.id.clone(), node);
        }
        DocumentState {
            nodes: map,
            comments: Vec::new(),
            suggestions: BTreeMap::new(),
            removed_choices: HashSet::new(),
        }
    }

    fn created(id: &str, ty: NodeType, parent: Option<&str>, var: Option<&str>) -> EventPayload {
        EventPayload::NodeCreated {
            node_id: NodeId(id.into()),
            node_type: ty,
            parent_id: parent.map(|p| NodeId(p.into())),
            pos: "a0".into(),
            fields: json!({}),
            var_name: var.map(str::to_string),
        }
    }

    #[test]
    fn node_created_under_valid_parent_ok() {
        let s = state(vec![n("01F", NodeType::Form, None, None)]);
        assert!(validate_op(&s, &created("01S", NodeType::Section, Some("01F"), None)).is_ok());
    }

    #[test]
    fn node_created_rejects_illegal_containment() {
        // A choice cannot contain a section (the docs/16 example).
        let s = state(vec![
            n("01F", NodeType::Form, None, None),
            n("01I", NodeType::Item, Some("01F"), None),
            n("01C", NodeType::Choice, Some("01I"), None),
        ]);
        let err =
            validate_op(&s, &created("01S", NodeType::Section, Some("01C"), None)).unwrap_err();
        assert!(matches!(err, OpError::IllegalContainment { .. }));
    }

    #[test]
    fn node_created_rejects_missing_parent_and_dup_var() {
        let s = state(vec![
            n("01F", NodeType::Form, None, None),
            n("01I", NodeType::Item, Some("01F"), Some("age")),
        ]);
        assert!(matches!(
            validate_op(&s, &created("01X", NodeType::Item, Some("nope"), None)).unwrap_err(),
            OpError::ParentNotFound(_)
        ));
        assert!(matches!(
            validate_op(
                &s,
                &created("01Y", NodeType::Item, Some("01F"), Some("age"))
            )
            .unwrap_err(),
            OpError::DuplicateVarName(_)
        ));
    }

    #[test]
    fn parentless_non_root_rejected() {
        let s = state(vec![]);
        assert!(matches!(
            validate_op(&s, &created("01I", NodeType::Item, None, None)).unwrap_err(),
            OpError::InvalidRoot(_)
        ));
        assert!(validate_op(&s, &created("01F", NodeType::Form, None, None)).is_ok());
    }

    #[test]
    fn field_edit_requires_live_node() {
        let mut deleted = n("01I", NodeType::Item, Some("01F"), None);
        deleted.deleted = true;
        let s = state(vec![n("01F", NodeType::Form, None, None), deleted]);
        let edit = EventPayload::FieldEdited {
            node_id: NodeId("01I".into()),
            field: "label".into(),
            value: json!("x"),
        };
        assert!(matches!(
            validate_op(&s, &edit).unwrap_err(),
            OpError::NodeDeleted(_)
        ));
    }

    #[test]
    fn move_into_own_subtree_is_a_cycle() {
        // 01F > 01S > 01I. Moving 01S under 01I would create a cycle.
        let s = state(vec![
            n("01F", NodeType::Form, None, None),
            n("01S", NodeType::Section, Some("01F"), None),
            n("01I", NodeType::Item, Some("01S"), None),
        ]);
        let mv = EventPayload::NodeMoved {
            node_id: NodeId("01S".into()),
            new_parent_id: Some(NodeId("01I".into())),
            new_pos: "a0".into(),
        };
        assert!(matches!(
            validate_op(&s, &mv).unwrap_err(),
            OpError::IllegalContainment { .. } | OpError::MoveCreatesCycle { .. }
        ));
    }

    #[test]
    fn move_section_under_section_ok() {
        let s = state(vec![
            n("01F", NodeType::Form, None, None),
            n("01S1", NodeType::Section, Some("01F"), None),
            n("01S2", NodeType::Section, Some("01F"), None),
        ]);
        let mv = EventPayload::NodeMoved {
            node_id: NodeId("01S2".into()),
            new_parent_id: Some(NodeId("01S1".into())),
            new_pos: "a1".into(),
        };
        assert!(validate_op(&s, &mv).is_ok());
    }

    #[test]
    fn choice_ops_require_item_parent_and_existing_choice() {
        let s = state(vec![
            n("01F", NodeType::Form, None, None),
            n("01I", NodeType::Item, Some("01F"), None),
            n("01C", NodeType::Choice, Some("01I"), None),
        ]);
        // Adding a choice under the form (not an item) is rejected.
        let add_bad = EventPayload::ChoiceAdded {
            node_id: NodeId("01F".into()),
            choice_id: NodeId("01C2".into()),
            fields: json!({}),
        };
        assert!(matches!(
            validate_op(&s, &add_bad).unwrap_err(),
            OpError::NotChoiceBearing(_)
        ));
        // Removing a choice that is not under the item is rejected.
        let rm_bad = EventPayload::ChoiceRemoved {
            node_id: NodeId("01I".into()),
            choice_id: NodeId("01NOPE".into()),
        };
        assert!(matches!(
            validate_op(&s, &rm_bad).unwrap_err(),
            OpError::ChoiceNotFound(_)
        ));
        // Removing the real choice is fine.
        let rm_ok = EventPayload::ChoiceRemoved {
            node_id: NodeId("01I".into()),
            choice_id: NodeId("01C".into()),
        };
        assert!(validate_op(&s, &rm_ok).is_ok());
    }

    #[test]
    fn suggestion_proposed_validates_wrapped_op() {
        let s = state(vec![
            n("01F", NodeType::Form, None, None),
            n("01I", NodeType::Item, Some("01F"), None),
        ]);
        let wrapped = EventPayload::FieldEdited {
            node_id: NodeId("01I".into()),
            field: "label".into(),
            value: json!("New label"),
        };
        let good = EventPayload::SuggestionProposed {
            suggestion_id: "s1".into(),
            target_node_id: Some(NodeId("01I".into())),
            detail: serde_json::to_value(&wrapped).unwrap(),
        };
        assert!(validate_op(&s, &good).is_ok());

        // A suggestion wrapping an edit to a missing node is rejected at propose time.
        let bad_wrapped = EventPayload::FieldEdited {
            node_id: NodeId("missing".into()),
            field: "label".into(),
            value: json!("x"),
        };
        let bad = EventPayload::SuggestionProposed {
            suggestion_id: "s2".into(),
            target_node_id: None,
            detail: serde_json::to_value(&bad_wrapped).unwrap(),
        };
        assert!(matches!(
            validate_op(&s, &bad).unwrap_err(),
            OpError::NodeNotFound(_)
        ));

        // A suggestion may not wrap a Deployed (not a content op).
        let bad_kind = EventPayload::SuggestionProposed {
            suggestion_id: "s3".into(),
            target_node_id: None,
            detail: serde_json::to_value(EventPayload::Deployed { snapshot_id: None }).unwrap(),
        };
        assert!(matches!(
            validate_op(&s, &bad_kind).unwrap_err(),
            OpError::InvalidWrappedOp(_)
        ));
    }

    #[test]
    fn accept_reject_require_pending_suggestion() {
        let mut s = state(vec![]);
        s.suggestions.insert(
            "pending".into(),
            SuggestionState {
                target_node_id: None,
                detail: json!({}),
                accepted: false,
                rejected_reason: None,
            },
        );
        s.suggestions.insert(
            "done".into(),
            SuggestionState {
                target_node_id: None,
                detail: json!({}),
                accepted: true,
                rejected_reason: None,
            },
        );
        assert!(validate_op(
            &s,
            &EventPayload::SuggestionAccepted {
                suggestion_id: "pending".into()
            }
        )
        .is_ok());
        assert!(matches!(
            validate_op(
                &s,
                &EventPayload::SuggestionAccepted {
                    suggestion_id: "done".into()
                }
            )
            .unwrap_err(),
            OpError::SuggestionNotPending(_)
        ));
        assert!(matches!(
            validate_op(
                &s,
                &EventPayload::SuggestionRejected {
                    suggestion_id: "ghost".into(),
                    reason: None
                }
            )
            .unwrap_err(),
            OpError::SuggestionNotFound(_)
        ));
    }
}
