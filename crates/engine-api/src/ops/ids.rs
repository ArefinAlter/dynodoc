//! Wire-write checks only: historical/server-generated operations retain their IDs.
use crate::error::ApiError;
use engine_shared::{ids::is_canonical_ulid, EventPayload, NodeId};
fn invalid() -> ApiError {
    ApiError::BadRequest {
        reason: "New node IDs must be canonical 26-character uppercase ULIDs".into(),
    }
}
pub(crate) fn new_identities(op: &EventPayload) -> Result<(), ApiError> {
    match op {
        EventPayload::NodeCreated { node_id, .. } => {
            if !is_canonical_ulid(&node_id.0) {
                return Err(invalid());
            }
        }
        EventPayload::ChoiceAdded { choice_id, .. } => {
            if !is_canonical_ulid(&choice_id.0) {
                return Err(invalid());
            }
        }
        EventPayload::SuggestionProposed { detail, .. } => {
            if let Ok(wrapped) = engine_core::ops::decode_wrapped_op(detail) {
                new_identities(&wrapped)?;
            }
        }
        _ => {}
    }
    Ok(())
}
pub(crate) fn operations(ops: &[EventPayload]) -> Result<(), ApiError> {
    for op in ops {
        new_identities(op)?;
    }
    Ok(())
}
/// Selections name nodes that already exist, so they keep every spelling a stored node can
/// have (a historical node with a non-canonical ID can still be shared or merged). Only the
/// size is bounded, with the same per-ID limit history recovery has always applied.
pub(crate) fn selection(ids: &[NodeId]) -> Result<(), ApiError> {
    if ids.len() > 20_000 {
        return Err(ApiError::BadRequest {
            reason: "Select at most 20,000 blocks at a time".into(),
        });
    }
    if ids.iter().any(|id| id.0.is_empty() || id.0.len() > 128) {
        return Err(ApiError::BadRequest {
            reason: "Selected node IDs must be 1 to 128 bytes".into(),
        });
    }
    Ok(())
}
