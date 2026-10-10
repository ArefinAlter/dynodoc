//! Permission-checked recovery from retained canonical history into a private draft.
//! Recovery never rewinds the event log or grants permission to merge.
use crate::{auth::AuthContext, error::ApiError, ops::apply, AppState};
use axum::{
    extract::{Path, State},
    routing::post,
    Json, Router,
};
use engine_core::{
    governance::{self, Role},
    materializer::apply_payload,
    workspace_merge,
};
use engine_shared::{DocumentId, IdentityId, NodeId};
use serde::Deserialize;
use serde_json::{json, Value};
use std::collections::BTreeSet;
use uuid::Uuid;

pub fn router() -> Router<AppState> {
    Router::new()
        .route("/documents/:id/history-recovery", post(recover))
        .layer(axum::extract::DefaultBodyLimit::max(256 * 1024))
}

#[derive(Default, Deserialize, PartialEq, utoipa::ToSchema)]
#[serde(rename_all = "snake_case")]
enum Action {
    #[default]
    Preview,
    Create,
}

#[derive(Deserialize, utoipa::ToSchema)]
#[serde(deny_unknown_fields)]
struct RecoveryRequest {
    /// Preview by default. Create makes a private draft, never a canonical edit.
    #[serde(default)]
    action: Action,
    /// Exact retained event revision, including zero for the empty initial state.
    through_seq: i64,
    /// Required on create: the team revision returned by preview.
    base_seq: Option<i64>,
    /// Omit for all content; otherwise select 1-5000 unique logical block IDs.
    node_ids: Option<Vec<String>>,
    /// Required on create. Keep the same UUID and request on a retry.
    request_id: Option<Uuid>,
}

fn bad(reason: &str) -> ApiError {
    ApiError::BadRequest {
        reason: reason.into(),
    }
}

#[utoipa::path(post, path="/documents/{id}/history-recovery", tag="reviews",
    request_body=RecoveryRequest, params(("id"=Uuid,Path)), security(("paseto"=[])),
    responses((status=200,description="Recovery preview or idempotent private draft receipt",body=Value),
    (status=400,description="Invalid revision, selection or structurally invalid recovery"),
    (status=403,description="Current membership required; viewers can only preview"),
    (status=409,description="Team changed since preview or request ID was reused")))]
async fn recover(
    State(state): State<AppState>,
    Path(id): Path<Uuid>,
    auth: AuthContext,
    Json(req): Json<RecoveryRequest>,
) -> Result<Json<Value>, ApiError> {
    let selection = req
        .node_ids
        .map(|ids| {
            if ids.is_empty() || ids.len() > 5000 {
                return Err(bad(
                    "Select 1-5000 blocks, or omit the selection for all content",
                ));
            }
            let unique: BTreeSet<_> = ids.iter().cloned().collect();
            if unique.len() != ids.len() || ids.iter().any(|id| id.is_empty() || id.len() > 128) {
                return Err(bad("Block IDs must be nonempty and unique"));
            }
            Ok(unique.into_iter().collect::<Vec<_>>())
        })
        .transpose()?;
    let (mut tx, role, team) =
        apply::begin_write(&state, DocumentId(id), IdentityId(auth.identity_id)).await?;
    if req.action == Action::Create && role == Role::Auditor {
        return Err(ApiError::Forbidden);
    }
    let head: i64 =
        sqlx::query_scalar("select coalesce(max(seq),0) from event where document_id=$1")
            .bind(id)
            .fetch_one(&mut *tx)
            .await?;
    if req.through_seq < 0 || req.through_seq > head {
        return Err(bad("That revision does not exist in this document"));
    }
    let chain_hash = crate::provenance::anchor(&mut tx, id, req.through_seq).await?;
    let source = json!({"kind":"history_recovery","version":1,
        "through_seq":req.through_seq,"chain_hash":chain_hash,
        "node_ids":selection,"start_seq":req.base_seq.unwrap_or(head)});
    let draft_id = if req.action == Action::Create {
        let draft_id = req
            .request_id
            .ok_or_else(|| bad("Keep one request_id for this recovery and its retries"))?;
        let base = req
            .base_seq
            .ok_or_else(|| bad("Preview the recovery and supply its base_seq"))?;
        // Same document lock serializes retries. Membership is rechecked before
        // returning any receipt, including after a draft has been merged.
        let existing: Option<(Uuid, Uuid, Value)> =
            sqlx::query_as("select document_id,created_by,source from workspace_draft where id=$1")
                .bind(draft_id)
                .fetch_optional(&mut *tx)
                .await?;
        if let Some((document, actor, recorded)) = existing {
            if document != id || actor != auth.identity_id || recorded != source {
                return Err(ApiError::Conflict { reason: "This request_id belongs to a different recovery. Use a new ID for a new selection.".into() });
            }
            return Ok(Json(json!({"id":draft_id,"source":source,"created":false})));
        }
        if base != head {
            return Err(ApiError::Conflict {
                reason:
                    "The team version changed. Preview the recovery again before creating a draft."
                        .into(),
            });
        }
        Some(draft_id)
    } else {
        None
    };
    let historical = crate::reviews::state_through(&mut tx, id, req.through_seq).await?;
    let target = if let Some(ids) = &selection {
        let mut selected = team.clone();
        for id in ids {
            let key = NodeId(id.clone());
            match historical.nodes.get(&key) {
                Some(node) => {
                    selected.nodes.insert(key.clone(), node.clone());
                }
                None if team.nodes.contains_key(&key) => {
                    selected.nodes.remove(&key);
                }
                None => return Err(bad("A selected block is absent from both revisions")),
            }
            selected.removed_choices.remove(&key);
            if historical.removed_choices.contains(&key) {
                selected.removed_choices.insert(key);
            }
        }
        selected
    } else {
        historical
    };
    let ops = workspace_merge::diff(&team, &target);
    if ops.len() > 20_000 {
        return Err(bad(
            "Recover at most 20,000 changes at a time; select fewer blocks",
        ));
    }
    // Preview reports invalid structural selections so callers can include the
    // missing parent/children. Creation must pass exactly the same validation.
    let mut local = team.clone();
    let mut blocked_reason = None;
    for op in &ops {
        if let Err(error) = governance::authorize(Role::Author, op, &local) {
            blocked_reason = Some(format!(
                "{error}. Include dependent blocks or resolve the structure in a draft."
            ));
            break;
        }
        apply_payload(&mut local, op).map_err(|error| ApiError::Internal(error.to_string()))?;
    }
    let Some(draft_id) = draft_id else {
        let changed: BTreeSet<_> = ops.iter().filter_map(|op| op.target_node_id()).collect();
        let blocks: Vec<_> = changed
            .into_iter()
            .filter_map(|id| {
                target
                    .nodes
                    .get(id)
                    .or_else(|| team.nodes.get(id))
                    .map(|node| {
                        let label = node
                            .current_fields
                            .get("label")
                            .or_else(|| node.current_fields.get("text"))
                            .and_then(Value::as_str)
                            .unwrap_or("")
                            .chars()
                            .take(240)
                            .collect::<String>();
                        json!({"id":id,"label":label})
                    })
            })
            .collect();
        return Ok(Json(json!({"through_seq":req.through_seq,"base_seq":head,
            "chain_hash":chain_hash,"node_ids":selection,"ops":ops,
            "blocks":blocks,"blocked_reason":blocked_reason,"can_propose":role != Role::Auditor})));
    };
    if let Some(reason) = blocked_reason {
        return Err(bad(&reason));
    }
    if ops.is_empty() {
        return Err(bad("The selected content already matches that revision"));
    }
    // Explicit ID makes retry atomic without a second, independently committed
    // receipt. A cross-document UUID collision fails without leaking its draft.
    let inserted = sqlx::query("insert into workspace_draft(id,document_id,name,base_seq,base_state,created_by,source,revision,content_changed_at) values($1,$2,$3,$4,$5,$6,$7,1,now()) on conflict(id) do nothing")
        .bind(draft_id).bind(id).bind(format!("Recover content from revision {}", req.through_seq))
        .bind(head).bind(json!(team)).bind(auth.identity_id).bind(&source).execute(&mut *tx).await?;
    if inserted.rows_affected() != 1 {
        return Err(ApiError::Conflict {
            reason: "This request_id is already in use. Use a new ID for this recovery.".into(),
        });
    }
    for op in &ops {
        sqlx::query("insert into draft_edit(draft_id,payload,actor_id) values($1,$2,$3)")
            .bind(draft_id)
            .bind(json!(op))
            .bind(auth.identity_id)
            .execute(&mut *tx)
            .await?;
    }
    crate::product::audit(
        &mut tx,
        auth.identity_id,
        "document.history_recovery_created",
        id,
        json!({"draft_id":draft_id,"source":source,"edits":ops.len()}),
    )
    .await?;
    tx.commit().await?;
    Ok(Json(json!({"id":draft_id,"source":source,"created":true})))
}

#[derive(utoipa::OpenApi)]
#[openapi(paths(recover), components(schemas(RecoveryRequest, Action)))]
pub struct HistoryApi;
