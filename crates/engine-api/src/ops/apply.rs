//! The write path: governance gate → materialize → append → snapshot → publish.
//!
//! This is the single funnel every content/governance write goes through (docs/16
//! "The stage-18 API surface drives them: it reads current state, calls these gates,
//! and appends"). The steps, in order:
//!
//! 1. **Resolve role.** The actor's per-document [`Role`] from the access list; absent
//!    role ⇒ `Forbidden` (a non-participant cannot write).
//! 2. **Read state.** Current materialized [`DocumentState`] via the snapshot engine.
//! 3. **Authorize.** [`governance::authorize`] composes capability → validity → (for
//!    `Deployed`) the integrity gate. A failure short-circuits before any append.
//! 4. **Materialize + append (one tx).** The `event.target_node_id → node(id)` FK
//!    requires a targeted node's row to exist *before* the event references it, and no
//!    stage before 18 wires the `node` table. So inside one transaction we apply the
//!    op's effect to the `node` table, then [`log::append_in_tx`]. An accept appends
//!    **two** events (the now-canonical wrapped op + `SuggestionAccepted`) atomically.
//! 5. **Commit, snapshot, publish.** After commit, run the post-commit snapshot step
//!    and fan the new event(s) out to SSE subscribers.

use engine_core::governance::{self, Role};
use engine_core::log;
use engine_core::snapshot::SnapshotEngine;
use engine_shared::{DocumentId, Event, EventPayload, IdentityId, NodeId, NodeType};
use serde_json::Value;
use sqlx::{PgPool, Postgres, Transaction};

use crate::auth::store as auth_store;
use crate::error::ApiError;
use crate::AppState;

/// Resolve the actor's role on a document, or `Forbidden` if they have none.
pub async fn require_role(
    pool: &PgPool,
    document_id: DocumentId,
    identity_id: IdentityId,
) -> Result<Role, ApiError> {
    auth_store::role_for(pool, document_id, identity_id.0)
        .await?
        .ok_or(ApiError::Forbidden)
}

/// Apply a single content/governance op end to end and return the canonical event that
/// folded into state (for an accept, the wrapped event). The document must exist.
pub async fn apply_op(
    state: &AppState,
    document_id: DocumentId,
    actor: IdentityId,
    op: EventPayload,
) -> Result<Event, ApiError> {
    let role = require_role(&state.pool, document_id, actor).await?;
    let current = SnapshotEngine::new(state.pool.clone())
        .read_current_state(document_id)
        .await?;

    // Gate: capability → validity → (Deployed) integrity. Pure decision, no I/O.
    governance::authorize(role, &op, &current).map_err(ApiError::from)?;

    let mut tx = state.pool.begin().await?;
    materialize_node_change(&mut tx, document_id, &op).await?;
    let inserted = log::append_in_tx(&mut tx, document_id, &op, actor)
        .await
        .map_err(ApiError::from)?;
    tx.commit().await?;

    log::post_commit_snapshot(&state.pool, document_id, std::slice::from_ref(&op))
        .await
        .map_err(ApiError::from)?;
    state.subscriptions.publish(document_id, inserted.clone());

    Ok(inserted)
}

/// Accept a pending proposal: append the now-canonical wrapped op **and** a
/// `SuggestionAccepted` in one transaction (FR-19 "atomically"), then snapshot/publish.
/// Returns the wrapped (canonical) event.
pub async fn accept_proposal(
    state: &AppState,
    document_id: DocumentId,
    actor: IdentityId,
    suggestion_id: &str,
) -> Result<Event, ApiError> {
    let role = require_role(&state.pool, document_id, actor).await?;
    let current = SnapshotEngine::new(state.pool.clone())
        .read_current_state(document_id)
        .await?;

    // The wrapped op an accept must append, re-validated against current state.
    let wrapped = governance::plan_acceptance(role, &current, suggestion_id)?;
    let accepted = EventPayload::SuggestionAccepted {
        suggestion_id: suggestion_id.to_string(),
    };

    let mut tx = state.pool.begin().await?;
    materialize_node_change(&mut tx, document_id, &wrapped).await?;
    let wrapped_event = log::append_in_tx(&mut tx, document_id, &wrapped, actor)
        .await
        .map_err(ApiError::from)?;
    let accepted_event = log::append_in_tx(&mut tx, document_id, &accepted, actor)
        .await
        .map_err(ApiError::from)?;
    tx.commit().await?;

    log::post_commit_snapshot(&state.pool, document_id, &[wrapped.clone(), accepted])
        .await
        .map_err(ApiError::from)?;
    state
        .subscriptions
        .publish(document_id, wrapped_event.clone());
    state.subscriptions.publish(document_id, accepted_event);

    Ok(wrapped_event)
}

/// Reject a pending proposal: append a `SuggestionRejected` (recorded with its reason,
/// never applied), then publish.
pub async fn reject_proposal(
    state: &AppState,
    document_id: DocumentId,
    actor: IdentityId,
    suggestion_id: &str,
    reason: Option<String>,
) -> Result<Event, ApiError> {
    let op = EventPayload::SuggestionRejected {
        suggestion_id: suggestion_id.to_string(),
        reason,
    };
    apply_op(state, document_id, actor, op).await
}

/// Apply an op's effect to the materialized `node` table inside the append transaction,
/// so the `event.target_node_id → node(id)` FK holds when the event is inserted, and so
/// later reads from a fresh snapshot match the log. Suggestion/comment/deploy ops touch
/// no node row (`CommentAdded` may anchor to a node that already exists; nothing to do).
pub(crate) async fn materialize_node_change(
    tx: &mut Transaction<'_, Postgres>,
    document_id: DocumentId,
    op: &EventPayload,
) -> Result<(), ApiError> {
    match op {
        EventPayload::NodeCreated {
            node_id,
            node_type,
            parent_id,
            pos,
            fields,
            var_name,
        } => {
            insert_node(
                tx,
                document_id,
                node_id,
                node_type.as_str(),
                parent_id.as_ref(),
                pos,
                &object_or_empty(fields),
                var_name.as_deref(),
            )
            .await?;
        }
        EventPayload::ChoiceAdded {
            node_id,
            choice_id,
            fields,
        } => {
            // A choice is a node row parented to the item, keyed by its own id/pos.
            insert_node(
                tx,
                document_id,
                choice_id,
                NodeType::Choice.as_str(),
                Some(node_id),
                &choice_id.0,
                &object_or_empty(fields),
                None,
            )
            .await?;
        }
        EventPayload::FieldEdited {
            node_id,
            field,
            value,
        } => {
            sqlx::query(
                "update node
                 set current_fields = jsonb_set(coalesce(current_fields, '{}'::jsonb), array[$2], $3, true),
                     updated_at = now()
                 where id = $1",
            )
            .bind(&node_id.0)
            .bind(field)
            .bind(value)
            .execute(&mut **tx)
            .await?;
        }
        EventPayload::NodeMoved {
            node_id,
            new_parent_id,
            new_pos,
        } => {
            sqlx::query(
                "update node set parent_id = $2, pos = $3, updated_at = now() where id = $1",
            )
            .bind(&node_id.0)
            .bind(new_parent_id.as_ref().map(|n| n.0.clone()))
            .bind(new_pos)
            .execute(&mut **tx)
            .await?;
        }
        EventPayload::NodeDeleted { node_id } => {
            sqlx::query("update node set deleted = true, updated_at = now() where id = $1")
                .bind(&node_id.0)
                .execute(&mut **tx)
                .await?;
        }
        EventPayload::ChoiceRemoved { choice_id, .. } => {
            sqlx::query("update node set deleted = true, updated_at = now() where id = $1")
                .bind(&choice_id.0)
                .execute(&mut **tx)
                .await?;
        }
        // Comment/suggestion/deploy events carry no node-table mutation. (A comment may
        // target an existing node; its row already exists.)
        EventPayload::CommentAdded { .. }
        | EventPayload::SuggestionProposed { .. }
        | EventPayload::SuggestionAccepted { .. }
        | EventPayload::SuggestionRejected { .. }
        | EventPayload::Deployed { .. } => {}
    }
    Ok(())
}

#[allow(clippy::too_many_arguments)]
async fn insert_node(
    tx: &mut Transaction<'_, Postgres>,
    document_id: DocumentId,
    node_id: &NodeId,
    node_type: &str,
    parent_id: Option<&NodeId>,
    pos: &str,
    fields: &Value,
    var_name: Option<&str>,
) -> Result<(), ApiError> {
    sqlx::query(
        "insert into node (id, document_id, parent_id, type, pos, current_fields, var_name)
         values ($1, $2, $3, $4, $5, $6, $7)",
    )
    .bind(&node_id.0)
    .bind(document_id.0)
    .bind(parent_id.map(|n| n.0.clone()))
    .bind(node_type)
    .bind(pos)
    .bind(fields)
    .bind(var_name)
    .execute(&mut **tx)
    .await?;
    Ok(())
}

/// The materializer stores non-object `fields` under a `"value"` key; mirror that so a
/// fresh snapshot fold matches the node row. Objects pass through unchanged.
fn object_or_empty(fields: &Value) -> Value {
    match fields {
        Value::Object(_) => fields.clone(),
        Value::Null => serde_json::json!({}),
        other => serde_json::json!({ "value": other }),
    }
}
