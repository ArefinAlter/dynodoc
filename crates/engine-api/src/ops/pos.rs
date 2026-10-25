//! Validation of position keys at the wire write boundary only.
//! Historical/server-generated operations and database reads remain permissive.

use crate::error::ApiError;
use engine_shared::{ids::is_valid_pos_key, EventPayload};

fn invalid_pos() -> ApiError {
    ApiError::BadRequest {
        reason: format!(
            "Position key must be non-empty printable ASCII without trailing spaces (max {} bytes)",
            engine_shared::ids::MAX_POS_KEY_BYTES
        ),
    }
}

pub(crate) fn validate_operation_pos(op: &EventPayload) -> Result<(), ApiError> {
    match op {
        EventPayload::NodeCreated { pos, .. } => {
            if !is_valid_pos_key(pos) {
                return Err(invalid_pos());
            }
        }
        EventPayload::NodeMoved { new_pos, .. } => {
            if !is_valid_pos_key(new_pos) {
                return Err(invalid_pos());
            }
        }
        EventPayload::SuggestionProposed { detail, .. } => {
            if let Ok(wrapped) = engine_core::ops::decode_wrapped_op(detail) {
                validate_operation_pos(&wrapped)?;
            }
        }
        _ => {}
    }
    Ok(())
}

pub(crate) fn validate_operations_pos(ops: &[EventPayload]) -> Result<(), ApiError> {
    for op in ops {
        validate_operation_pos(op)?;
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use engine_shared::{NodeId, NodeType};
    use serde_json::json;

    #[test]
    fn validate_operation_pos_checks_node_created_and_moved() {
        let node_id = NodeId("01ARZ3NDEKTSV4RRFFQ69G5FAV".into());
        let valid_create = EventPayload::NodeCreated {
            node_id: node_id.clone(),
            node_type: NodeType::Item,
            parent_id: None,
            pos: "a0".into(),
            fields: json!({}),
            var_name: None,
        };
        assert!(validate_operation_pos(&valid_create).is_ok());

        let invalid_create = EventPayload::NodeCreated {
            node_id: node_id.clone(),
            node_type: NodeType::Item,
            parent_id: None,
            pos: "".into(),
            fields: json!({}),
            var_name: None,
        };
        assert!(validate_operation_pos(&invalid_create).is_err());

        let valid_move = EventPayload::NodeMoved {
            node_id: node_id.clone(),
            new_parent_id: None,
            new_pos: "a0V".into(),
        };
        assert!(validate_operation_pos(&valid_move).is_ok());

        let invalid_move = EventPayload::NodeMoved {
            node_id: node_id.clone(),
            new_parent_id: None,
            new_pos: "m ".into(),
        };
        assert!(validate_operation_pos(&invalid_move).is_err());
    }

    #[test]
    fn validate_operation_pos_checks_wrapped_suggestions() {
        let node_id = NodeId("01ARZ3NDEKTSV4RRFFQ69G5FAV".into());
        let bad_create = EventPayload::NodeCreated {
            node_id: node_id.clone(),
            node_type: NodeType::Item,
            parent_id: None,
            pos: "\n".into(),
            fields: json!({}),
            var_name: None,
        };
        let suggestion = EventPayload::SuggestionProposed {
            suggestion_id: "s1".into(),
            target_node_id: None,
            detail: serde_json::to_value(&bad_create).unwrap(),
        };
        assert!(validate_operation_pos(&suggestion).is_err());

        let good_create = EventPayload::NodeCreated {
            node_id: node_id.clone(),
            node_type: NodeType::Item,
            parent_id: None,
            pos: "pending".into(),
            fields: json!({}),
            var_name: None,
        };
        let good_suggestion = EventPayload::SuggestionProposed {
            suggestion_id: "s2".into(),
            target_node_id: None,
            detail: serde_json::to_value(&good_create).unwrap(),
        };
        assert!(validate_operation_pos(&good_suggestion).is_ok());
    }
}
