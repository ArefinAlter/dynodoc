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
use engine_core::materializer::{apply_payload, ApplyError, DocumentState, MaterializedNode};
use engine_shared::{DocumentId, Event, EventPayload, IdentityId, NodeId};
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
    apply_checked_op(state, document_id, actor, op, None).await
}

/// Serialize validation and append against the same document state. A stale field
/// write is refused; edits to other fields remain independent.
pub async fn apply_checked_op(
    state: &AppState,
    document_id: DocumentId,
    actor: IdentityId,
    op: EventPayload,
    base_seq: Option<i64>,
) -> Result<Event, ApiError> {
    let (mut tx, role, current) = begin_write(state, document_id, actor).await?;
    governance::authorize(role, &op, &current).map_err(ApiError::from)?;
    if crate::access::changes_content(&op) {
        crate::access::ensure_team_edit(&mut tx, document_id.0, actor.0).await?;
    }
    if let (
        Some(base),
        EventPayload::FieldEdited {
            node_id,
            field,
            value,
        },
    ) = (base_seq, &op)
    {
        let changed: bool = sqlx::query_scalar(
            "select exists(select 1 from event where document_id=$1 and seq>$2
             and target_node_id=$3 and ((type='FieldEdited' and payload->>'field'=$4)
             or (type='RichTextPatched' and $4 in ('content','label'))))",
        )
        .bind(document_id.0)
        .bind(base)
        .bind(&node_id.0)
        .bind(field)
        .fetch_one(&mut *tx)
        .await?;
        let canonical = current
            .nodes
            .get(node_id)
            .and_then(|n| n.current_fields.get(field));
        if changed && canonical != Some(value) {
            return Err(ApiError::Conflict { reason: "Another researcher changed this field. Review the latest version before saving your change.".into() });
        }
    }
    let op = engine_core::richtext::compact_event(&current, &op);
    materialize_node_change(&mut tx, document_id, &op).await?;
    let inserted = log::append_in_tx(&mut tx, document_id, &op, actor)
        .await
        .map_err(ApiError::from)?;
    if matches!(op, EventPayload::Deployed { .. }) {
        checkpoint_in_tx(
            &mut tx,
            document_id,
            &inserted,
            state.named_snapshot_storage,
        )
        .await?;
    }
    tx.commit().await?;

    if !matches!(op, EventPayload::Deployed { .. }) {
        log::post_commit_snapshot(&state.pool, document_id, std::slice::from_ref(&op))
            .await
            .map_err(ApiError::from)?;
    }
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
    let (mut tx, role, current) = begin_write(state, document_id, actor).await?;

    // The wrapped op an accept must append, re-validated against current state.
    let wrapped = governance::plan_acceptance(role, &current, suggestion_id)?;
    // Accepting a suggestion changes the team version, like merging.
    crate::access::ensure_team_edit(&mut tx, document_id.0, actor.0).await?;
    let wrapped = engine_core::richtext::compact_event(&current, &wrapped);
    let accepted = EventPayload::SuggestionAccepted {
        suggestion_id: suggestion_id.to_string(),
    };

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
/// so the `event.target_node_id → node(id)` FK holds when the event is inserted. Rows
/// follow the fold: the op is applied with [`apply_payload`] to the stored rows it
/// touches, and those rows are written back; a removed choice stays `deleted` in the
/// table. Suggestion/comment/deploy ops touch no node row. The caller must hold the
/// document writer lock (`begin_write*`); the row lookup does not lock the rows itself.
pub(crate) async fn materialize_node_change(
    tx: &mut Transaction<'_, Postgres>,
    document_id: DocumentId,
    op: &EventPayload,
) -> Result<(), ApiError> {
    let ids = match op {
        EventPayload::NodeCreated { node_id, .. }
        | EventPayload::FieldEdited { node_id, .. }
        | EventPayload::RichTextPatched { node_id, .. }
        | EventPayload::NodeMoved { node_id, .. }
        | EventPayload::NodeRestored { node_id }
        | EventPayload::NodeDeleted { node_id } => vec![node_id.0.clone()],
        EventPayload::ChoiceAdded { choice_id, .. }
        | EventPayload::ChoiceRemoved { choice_id, .. } => vec![choice_id.0.clone()],
        EventPayload::CommentReplied { thread_id, .. }
        | EventPayload::CommentResolved { thread_id, .. } => {
            let exists: bool=sqlx::query_scalar("select exists(select 1 from event where id=$1 and document_id=$2 and type='CommentAdded')")
                .bind(thread_id).bind(document_id.0).fetch_one(&mut **tx).await?;
            if !exists {
                return Err(ApiError::BadRequest {
                    reason: "Discussion not found in this document".into(),
                });
            }
            return Ok(());
        }
        EventPayload::CommentAdded { .. }
        | EventPayload::SuggestionProposed { .. }
        | EventPayload::SuggestionAccepted { .. }
        | EventPayload::SuggestionRejected { .. }
        | EventPayload::Deployed { .. } => return Ok(()),
    };
    type Row = (
        String,
        Option<String>,
        String,
        String,
        Value,
        Option<String>,
        bool,
    );
    let created = matches!(
        op,
        EventPayload::NodeCreated { .. } | EventPayload::ChoiceAdded { .. }
    );
    // Moves, deletes and restores leave the (possibly large) fields untouched, so
    // they are neither read nor written.
    let fields = matches!(
        op,
        EventPayload::FieldEdited { .. } | EventPayload::RichTextPatched { .. }
    );
    // A create has no stored row to fold onto; only field edits read `current_fields`.
    let rows: Vec<Row> = if created {
        Vec::new()
    } else {
        sqlx::query_as(
            "select id, parent_id, type, pos,
                    case when $3 then current_fields else '{}'::jsonb end,
                    var_name, deleted
             from node where document_id = $1 and id = any($2)",
        )
        .bind(document_id.0)
        .bind(&ids)
        .bind(fields)
        .fetch_all(&mut **tx)
        .await?
    };
    let mut state = DocumentState::default();
    for (id, parent_id, node_type, pos, current_fields, var_name, deleted) in rows {
        state.nodes.insert(
            NodeId(id.clone()),
            MaterializedNode {
                id: NodeId(id),
                parent_id: parent_id.map(NodeId),
                node_type,
                pos,
                current_fields,
                var_name,
                deleted,
            },
        );
    }
    apply_payload(&mut state, op).map_err(|e| match e {
        // A patch that does not apply to the stored content is the caller's to fix;
        // anything else means the table broke its own invariants.
        ApplyError::RichText(_) => ApiError::BadRequest {
            reason: e.to_string(),
        },
        _ => ApiError::Internal(e.to_string()),
    })?;
    for id in &ids {
        let Some(node) = state.nodes.get(&NodeId(id.clone())) else {
            return Err(ApiError::Internal(format!(
                "node {id} has no row in document {}",
                document_id.0
            )));
        };
        if created {
            insert_node(
                tx,
                document_id,
                &node.id,
                &node.node_type,
                node.parent_id.as_ref(),
                &node.pos,
                &node.current_fields,
                node.var_name.as_deref(),
            )
            .await?;
            continue;
        }
        let sql = if fields {
            "update node set parent_id = $3, pos = $4, var_name = $5, deleted = $6,
                 current_fields = $7, updated_at = now()
             where id = $1 and document_id = $2"
        } else {
            "update node set parent_id = $3, pos = $4, var_name = $5, deleted = $6,
                 updated_at = now()
             where id = $1 and document_id = $2"
        };
        let mut query = sqlx::query(sql)
            .bind(&node.id.0)
            .bind(document_id.0)
            .bind(node.parent_id.as_ref().map(|p| p.0.as_str()))
            .bind(&node.pos)
            .bind(node.var_name.as_deref())
            .bind(node.deleted || state.removed_choices.contains(&node.id));
        if fields {
            query = query.bind(&node.current_fields);
        }
        query.execute(&mut **tx).await?;
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

/// Every write path takes this lock before reading state or validating a proposal.
/// Match the core append lock without upgrading it: writers still exclude each
/// other and SHARE/UPDATE permission locks, while checkpoints retain KEY SHARE
/// protection against erasure without stopping content edits.
pub(crate) async fn begin_write(
    state: &AppState,
    document_id: DocumentId,
    actor: IdentityId,
) -> Result<(Transaction<'_, Postgres>, Role, DocumentState), ApiError> {
    let (mut tx, role) = begin_write_locked(state, document_id, actor).await?;
    let (current, _) = engine_core::snapshot::read_current_state(&mut tx, document_id).await?;
    Ok((tx, role, current))
}

pub(crate) async fn begin_write_locked(
    state: &AppState,
    document_id: DocumentId,
    actor: IdentityId,
) -> Result<(Transaction<'_, Postgres>, Role), ApiError> {
    let mut tx = state.pool.begin().await?;
    let exists: Option<uuid::Uuid> =
        sqlx::query_scalar("select id from document where id=$1 for no key update")
            .bind(document_id.0)
            .fetch_optional(&mut *tx)
            .await?;
    if exists.is_none() {
        return Err(ApiError::NotFound);
    }
    crate::projects::lock_document_project(&mut tx, document_id.0).await?;
    let role: Option<String> = sqlx::query_scalar("select effective_document_role($1,$2)")
        .bind(document_id.0)
        .bind(actor.0)
        .fetch_one(&mut *tx)
        .await?;
    let role = role
        .and_then(|r| r.parse().ok())
        .ok_or(ApiError::Forbidden)?;
    Ok((tx, role))
}

async fn checkpoint_in_tx(
    tx: &mut Transaction<'_, Postgres>,
    document_id: DocumentId,
    event: &Event,
    storage: engine_core::snapshot::NamedStorage,
) -> Result<(), ApiError> {
    let id = engine_core::snapshot_verification::take_with_storage_in_connection(
        tx,
        document_id,
        event.seq,
        storage,
    )
    .await?
    .id
    .0;
    sqlx::query("update document set status='deployed', deployed_snapshot_id=$2, updated_at=now() where id=$1")
        .bind(document_id.0).bind(id).execute(&mut **tx).await?;
    Ok(())
}
