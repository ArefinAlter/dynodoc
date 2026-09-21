//! Discussion projection over canonical events, with optimistic review revisions.
use crate::{
    auth::{store, AuthContext},
    error::ApiError,
    ops::apply,
    AppState,
};
use axum::{
    extract::{Path, Query, State},
    routing::{get, post},
    Json, Router,
};
use engine_core::{governance, log};
use engine_shared::{DocumentId, EventPayload, IdentityId};
use serde::Deserialize;
use serde_json::{json, Value};
use uuid::Uuid;

pub fn router() -> Router<AppState> {
    Router::new()
        .route("/documents/:id/discussions", get(list))
        .route("/documents/:id/discussions/:thread", post(change))
}
#[derive(Deserialize)]
struct Page {
    before: Option<i64>,
    node: Option<String>,
}
#[utoipa::path(get, path="/documents/{id}/discussions", tag="discussions", params(("id" = String, Path, description = "Resource identifier")), security(("paseto" = [])), responses((status=200, description="Successful operation", body=Value),(status=400, description="Validation or confirmation failed"),(status=401, description="Sign in required"),(status=403, description="Insufficient access"),(status=409, description="Refresh the current revision")))]
async fn list(
    State(state): State<AppState>,
    auth: AuthContext,
    Path(id): Path<Uuid>,
    Query(page): Query<Page>,
) -> Result<Json<Value>, ApiError> {
    store::role_for(&state.pool, DocumentId(id), auth.identity_id)
        .await?
        .ok_or(ApiError::Forbidden)?;
    let mut items:Vec<Value>=sqlx::query_scalar("select jsonb_build_object('id',e.id,'seq',e.seq,'node_id',e.target_node_id,'body',e.payload->>'body','actor_id',e.actor_id,'author',coalesce(i.display_name,i.email),'created_at',e.created_at,'resolved',coalesce((select (r.payload->>'resolved')::boolean from event r where r.document_id=$1 and r.type='CommentResolved' and r.payload->>'thread_id'=e.id order by r.seq desc limit 1),false),'revision',coalesce((select max(r.seq) from event r where r.document_id=$1 and r.type in ('CommentReplied','CommentResolved') and r.payload->>'thread_id'=e.id),e.seq),'replies',coalesce((select jsonb_agg(jsonb_build_object('id',r.id,'body',r.payload->>'body','actor_id',r.actor_id,'author',coalesce(a.display_name,a.email),'created_at',r.created_at) order by r.seq) from event r join identity a on a.id=r.actor_id where r.document_id=$1 and r.type='CommentReplied' and r.payload->>'thread_id'=e.id),'[]'::jsonb)) from event e join identity i on i.id=e.actor_id where e.document_id=$1 and e.type='CommentAdded' and ($3::text is null or e.target_node_id=$3) and ($2::bigint is null or e.seq<$2) order by e.seq desc limit 31")
        .bind(id).bind(page.before).bind(page.node).fetch_all(&state.pool).await?;
    let more = items.len() > 30;
    items.truncate(30);
    let next = if more {
        items.last().map(|r| r["seq"].clone())
    } else {
        None
    };
    Ok(Json(json!({"items":items,"next_before":next})))
}
#[derive(Deserialize)]
struct Change {
    revision: i64,
    body: Option<String>,
    resolved: Option<bool>,
}
#[utoipa::path(post, path="/documents/{id}/discussions/{thread}", tag="discussions", request_body=Value, params(("id" = String, Path, description = "Resource identifier"),("thread" = String, Path, description = "Resource identifier")), security(("paseto" = [])), responses((status=200, description="Successful operation", body=Value),(status=400, description="Validation or confirmation failed"),(status=401, description="Sign in required"),(status=403, description="Insufficient access"),(status=409, description="Refresh the current revision")))]
async fn change(
    State(state): State<AppState>,
    auth: AuthContext,
    Path((id, thread)): Path<(Uuid, String)>,
    Json(input): Json<Change>,
) -> Result<Json<Value>, ApiError> {
    let doc = DocumentId(id);
    let actor = IdentityId(auth.identity_id);
    let (mut tx, role, current) = apply::begin_write(&state, doc, actor).await?;
    let root: Option<i64> = sqlx::query_scalar(
        "select seq from event where id=$1 and document_id=$2 and type='CommentAdded'",
    )
    .bind(&thread)
    .bind(id)
    .fetch_optional(&mut *tx)
    .await?;
    let root = root.ok_or(ApiError::NotFound)?;
    let latest:i64=sqlx::query_scalar("select coalesce(max(seq),$3) from event where document_id=$1 and type in ('CommentReplied','CommentResolved') and payload->>'thread_id'=$2").bind(id).bind(&thread).bind(root).fetch_one(&mut *tx).await?;
    if latest != input.revision {
        return Err(ApiError::Conflict {
            reason: "This discussion changed. Refresh it before replying or resolving.".into(),
        });
    }
    let op = match (input.body, input.resolved) {
        (Some(body), None) => {
            let resolved:bool=sqlx::query_scalar("select coalesce((select (payload->>'resolved')::boolean from event where document_id=$1 and type='CommentResolved' and payload->>'thread_id'=$2 order by seq desc limit 1),false)").bind(id).bind(&thread).fetch_one(&mut *tx).await?;
            if resolved {
                return Err(ApiError::Conflict {
                    reason: "Reopen this discussion before replying.".into(),
                });
            }

            let count:i64=sqlx::query_scalar("select count(*) from event where document_id=$1 and type='CommentReplied' and payload->>'thread_id'=$2").bind(id).bind(&thread).fetch_one(&mut *tx).await?;
            if count >= 100 {
                return Err(ApiError::BadRequest{reason:"This discussion has reached 100 replies. Start a new discussion to continue.".into()});
            }
            EventPayload::CommentReplied {
                thread_id: thread,
                body: body.trim().to_string(),
            }
        }
        (None, Some(resolved)) => EventPayload::CommentResolved {
            thread_id: thread,
            resolved,
        },
        _ => {
            return Err(ApiError::BadRequest {
                reason: "Provide a reply or a resolution action.".into(),
            })
        }
    };
    governance::authorize(role, &op, &current)?;
    apply::materialize_node_change(&mut tx, doc, &op).await?;
    let event = log::append_in_tx(&mut tx, doc, &op, actor).await?;
    tx.commit().await?;
    state.subscriptions.publish(doc, event.clone());
    Ok(Json(json!({"events":[event]})))
}

#[derive(utoipa::OpenApi)]
#[openapi(paths(list, change))]
pub struct DiscussionApi;
