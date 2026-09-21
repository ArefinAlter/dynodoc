//! The conflict-resolution write path (docs/21 item 4).
//!
//! Resolving a four-card conflict is a single atomic write that mirrors the
//! [`crate::ops::apply`] funnel: re-derive the conflict from current state, apply the
//! author's decision in one transaction, then snapshot and publish each appended event.
//!
//! Two decision modes (exactly one of the request fields is set):
//!
//! - **Accept a proposal** (`accept_suggestion_id`): apply that suggestion's wrapped
//!   `FieldEdited` as the canonical value (exactly like [`crate::ops::apply::accept_proposal`]:
//!   the wrapped op + a `SuggestionAccepted`), then append a `SuggestionRejected` with
//!   reason `"superseded by conflict resolution"` for every *other* pending suggestion in
//!   the same `(node_id, field)` group.
//! - **Custom value** (`custom_value`): append a direct `FieldEdited` writing the custom
//!   value, then `SuggestionRejected` for *every* pending suggestion in the group.
//!
//! Authorization is the same as accept/reject — **Author only** — enforced by
//! [`apply::require_role`] plus the [`governance::authorize`] capability check the wrapped
//! ops already run through.

use engine_core::governance::{self, Role};
use engine_core::log;
use engine_shared::{DocumentId, Event, EventPayload, IdentityId, NodeId};
use serde_json::Value;

use super::derive::{self, ConflictView};
use crate::error::ApiError;
use crate::ops::apply;
use crate::AppState;

/// The author's decision for `resolve`. Exactly one field must be set.
pub enum Resolution {
    /// Accept one of the conflict's competing proposals.
    Accept { suggestion_id: String },
    /// Override all proposals with a custom value.
    Custom { value: Value },
}

/// Resolve the conflict identified by `conflict_id` against current state. Returns the
/// canonical events appended (the field edit + the suggestion lifecycle events).
///
/// Errors: `Forbidden` (non-Author), `NotFound` (`conflict_id` no longer maps to a live
/// conflict), `BadRequest` (an `accept_suggestion_id` not among the conflict's options).
pub async fn resolve_conflict(
    state: &AppState,
    document_id: DocumentId,
    actor: IdentityId,
    conflict_id: &str,
    resolution: Resolution,
) -> Result<Vec<Event>, ApiError> {
    let (mut tx, role, current) = apply::begin_write(state, document_id, actor).await?;
    if role != Role::Author {
        return Err(ApiError::Forbidden);
    }

    // Re-derive the conflict; a stale id (already resolved) is a 404.
    let conflict = derive::find_conflict(&current, conflict_id).ok_or(ApiError::NotFound)?;

    // Plan the canonical ops this resolution appends. `field_edit` becomes canonical;
    // each `(suggestion_id, accepted)` entry is a lifecycle event.
    let node_id = NodeId(conflict.node_id.clone());
    let (field_edit, lifecycle): (EventPayload, Vec<EventPayload>) = match resolution {
        Resolution::Accept { suggestion_id } => {
            // The accepted suggestion must be one of the conflict's options.
            let chosen = conflict
                .options
                .iter()
                .find(|o| o.suggestion_id == suggestion_id)
                .ok_or_else(|| ApiError::BadRequest {
                    reason: "accept_suggestion_id is not one of this conflict's options".into(),
                })?;
            let edit = EventPayload::FieldEdited {
                node_id: node_id.clone(),
                field: conflict.field.clone(),
                value: chosen.value.clone(),
            };
            // Re-validate the wrapped op against current state before it becomes
            // canonical (mirrors accept_proposal / plan_acceptance).
            governance::authorize(role, &edit, &current).map_err(ApiError::from)?;
            let lifecycle = lifecycle_events(&conflict, Some(&suggestion_id));
            (edit, lifecycle)
        }
        Resolution::Custom { value } => {
            let edit = EventPayload::FieldEdited {
                node_id: node_id.clone(),
                field: conflict.field.clone(),
                value,
            };
            governance::authorize(role, &edit, &current).map_err(ApiError::from)?;
            let lifecycle = lifecycle_events(&conflict, None);
            (edit, lifecycle)
        }
    };

    let field_edit = engine_core::richtext::compact_event(&current, &field_edit);
    // One transaction: materialize the field edit, append it, then append every
    // lifecycle event. All land or none does (mirrors apply::accept_proposal).
    apply::materialize_node_change(&mut tx, document_id, &field_edit).await?;
    let mut appended = Vec::with_capacity(1 + lifecycle.len());
    let edit_event = log::append_in_tx(&mut tx, document_id, &field_edit, actor)
        .await
        .map_err(ApiError::from)?;
    appended.push(edit_event);
    for op in &lifecycle {
        let event = log::append_in_tx(&mut tx, document_id, op, actor)
            .await
            .map_err(ApiError::from)?;
        appended.push(event);
    }
    tx.commit().await?;

    // Snapshot over the full set of payloads, then publish each event to SSE.
    let mut payloads = Vec::with_capacity(1 + lifecycle.len());
    payloads.push(field_edit);
    payloads.extend(lifecycle);
    log::post_commit_snapshot(&state.pool, document_id, &payloads)
        .await
        .map_err(ApiError::from)?;
    for event in &appended {
        state.subscriptions.publish(document_id, event.clone());
    }

    Ok(appended)
}

/// Build the suggestion-lifecycle ops a resolution appends: a `SuggestionAccepted` for the
/// accepted option (if any), and a `SuggestionRejected{reason}` for every *other* pending
/// option in the group (superseded by the resolution).
fn lifecycle_events(conflict: &ConflictView, accepted: Option<&str>) -> Vec<EventPayload> {
    let mut events = Vec::new();
    if let Some(accepted_id) = accepted {
        events.push(EventPayload::SuggestionAccepted {
            suggestion_id: accepted_id.to_string(),
        });
    }
    for option in &conflict.options {
        if Some(option.suggestion_id.as_str()) == accepted {
            continue;
        }
        events.push(EventPayload::SuggestionRejected {
            suggestion_id: option.suggestion_id.clone(),
            reason: Some("superseded by conflict resolution".to_string()),
        });
    }
    events
}
